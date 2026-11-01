// SPDX-License-Identifier: Apache-2.0
//! The editor's side of the agent connection (PLAN.md, "Agent experience",
//! sections 1, 2, 5 and 10; milestone 7 step 5b-1): `:AgentConnect` with the
//! consent prompt, `:AgentDisconnect`, `:AgentCancel`, the lifecycle states
//! and their backoff, the status indicator's connection states, and
//! `prediction_allowed`.
//!
//! The protocol is `acp.rs`, a pure state machine; the process is
//! `agent_conn.rs`, on worker threads. This module only joins them: it turns
//! [`OutAction`]s into [`ConnCmd`]s, and worker events into `Conn` events. The
//! clock is read only in [`Editor::agent_timer`] and the event handler, and
//! tests pass it in explicitly.
//!
//! One connection at a time for now: `:AgentUse` and several agents are step
//! 5b-2. Not in this step: `:Agent` prompts, the agent log, permission
//! answers (no request is answered, because no capability that causes one is
//! advertised), and file calls.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::Editor;
use crate::acp::{Conn, ConnState, InEvent, Notice, OutAction};
use crate::agent_approval::{Approvals, FILE_NAME};
use crate::agent_conn::{AgentEvent, ConnCmd};
use crate::agent_detect::{AGENTS, AgentSpec};
use crate::key::Key;

/// Seconds to wait before each retry after a lost connection; then Error.
pub const RETRY_DELAYS: [u64; 5] = [1, 2, 4, 8, 16];

/// The lifecycle states of 5b-1 (PLAN.md, "Agent experience", Lifecycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    /// Process started; `initialize`, `authenticate` or `session/new` pending.
    Connecting,
    /// Connected, no turn running.
    Idle,
    /// Connection lost; a retry is scheduled at `retry_at`.
    Disconnected,
    /// Could not connect, or gave up retrying.
    Error,
}

/// One agent connection.
#[derive(Debug)]
pub struct Link {
    pub id: &'static str,
    /// Connection generation; events carry it, and stale ones are ignored.
    pub generation: u64,
    pub conn: Conn,
    pub life: Life,
    /// Retries made since the last successful connect.
    pub retries: u32,
    pub retry_at: Option<Instant>,
    /// The process's exit code, when the reader reported one.
    pub exit: Option<i32>,
    pub cwd: PathBuf,
}

/// Everything the agent connection keeps in the editor.
#[derive(Default)]
pub struct AgentState {
    pub link: Option<Link>,
    /// The agent whose consent prompt is on the message line.
    pub consent: Option<&'static str>,
    pub approvals: Approvals,
    /// The project root: where agents run and what approvals are for.
    pub project: Option<PathBuf>,
    /// Requests for the worker, taken by `take_agent_conn_commands`.
    pub outbox: Vec<ConnCmd>,
    pub next_gen: u64,
    /// Prompt turns, the agent log and the leader keys (`agent_turn.rs`).
    pub(crate) turn: crate::agent_turn::TurnState,
}

fn spec_of(id: &str) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|a| a.id == id)
}

impl Editor {
    /// The project root. `main.rs` sets it to the working directory.
    pub fn set_agent_project(&mut self, root: PathBuf) {
        self.agent.project = Some(root);
    }

    /// The approvals read at startup. A warning (an unreadable file) goes to
    /// the message line; the store then refuses to save.
    pub fn set_agent_approvals(&mut self, approvals: Approvals, warning: Option<String>) {
        self.agent.approvals = approvals;
        if let Some(text) = warning {
            self.error(text);
        }
    }

    /// The worker requests not yet taken. `main.rs` sends each to the manager.
    pub fn take_agent_conn_commands(&mut self) -> Vec<ConnCmd> {
        std::mem::take(&mut self.agent.outbox)
    }

    /// `prediction_allowed`: prediction never reads or sends an agent's
    /// text in this step (PLAN.md, "Prediction and agent coexistence").
    pub fn prediction_allowed(&self) -> bool {
        false
    }

    /// The moment the agent layer next needs a timer (handshake timeout or
    /// retry), for `next_deadline`.
    pub(crate) fn agent_deadline(&self) -> Option<Instant> {
        let leader = self.agent_leader_deadline();
        let Some(link) = self.agent.link.as_ref() else {
            return leader;
        };
        let retry = link.retry_at.filter(|_| link.life == Life::Disconnected);
        [retry, link.conn.next_deadline(), leader]
            .into_iter()
            .flatten()
            .min()
    }

    /// When the scheduled retry is due, for tests and the status line.
    pub fn agent_retry_at(&self) -> Option<Instant> {
        self.agent.link.as_ref().and_then(|l| l.retry_at)
    }

    /// The status indicator's text (PLAN.md, Lifecycle), or None when the
    /// agent layer is off. With a connection it shows its state; without one,
    /// the scan states of step 4b.
    pub fn agent_indicator(&self) -> Option<String> {
        if !self.agent_active {
            return None;
        }
        if let Some(link) = &self.agent.link {
            let state = match link.life {
                Life::Connecting => " connecting".to_owned(),
                // A turn shows as working; a queued permission request as
                // PERMISSION n (answering it is step 6).
                Life::Idle => match (link.conn.permissions_waiting(), link.conn.state()) {
                    (n, _) if n > 0 => format!(" PERMISSION {n}"),
                    (_, ConnState::Prompting { .. }) => " working".to_owned(),
                    (_, ConnState::Cancelling { .. }) => " cancelling".to_owned(),
                    _ => String::new(),
                },
                Life::Disconnected => format!(" retry {}/{}", link.retries, RETRY_DELAYS.len()),
                Life::Error => " error".to_owned(),
            };
            return Some(format!("ai: {}{state}", link.id));
        }
        crate::agent_detect::indicator(true, self.agent_scan.as_ref())
    }

    /// `:AgentConnect [name]`. Turns the agent layer on. Asks for consent
    /// when this agent is not approved for this project; nothing is started
    /// before that.
    pub fn agent_connect(&mut self, name: Option<&str>) {
        self.agent_active = true;
        let spec = match self.pick_agent(name) {
            Ok(spec) => spec,
            Err(text) => return self.error(text),
        };
        if spec.acp.is_empty() {
            return self.error(format!(
                "{} has no ACP command in the registry; not connected",
                spec.id
            ));
        }
        if let Some(link) = self.agent.link.as_ref().filter(|l| l.life != Life::Error) {
            {
                let text = if link.id == spec.id {
                    format!("{}-acp: already connected", spec.id)
                } else {
                    format!("{}-acp is connected; :AgentDisconnect first", link.id)
                };
                return self.info(text);
            }
        }
        let Some(root) = self.agent.project.clone() else {
            return self.error("no project directory; not connected");
        };
        let project = root.to_string_lossy().into_owned();
        if !self.agent.approvals.is_approved(spec.id, &project) {
            self.agent.consent = Some(spec.id);
            return self.info(format!(
                "Start {} ({}) in {project}? The agent may use the network under its own policy. [y]es [n]o",
                spec.id, spec.acp
            ));
        }
        self.start_link(spec.id, root, 0);
    }

    /// The agent `:AgentConnect` means: the name given, or the only one the
    /// last scan found.
    fn pick_agent(&self, name: Option<&str>) -> Result<&'static AgentSpec, String> {
        let found: Vec<&'static str> = self
            .agent_scan
            .as_ref()
            .map(|s| s.found.iter().map(|(id, _)| *id).collect())
            .unwrap_or_default();
        match name {
            Some(name) => {
                let spec = spec_of(name).ok_or_else(|| format!("unknown agent: {name}"))?;
                if self.agent_scan.is_none() {
                    return Err("run :AgentScan first".to_owned());
                }
                if !found.contains(&spec.id) {
                    return Err(format!("{name} is not installed; :AgentScan to look again"));
                }
                Ok(spec)
            }
            None => match found.as_slice() {
                [] => Err("no agent found; run :AgentScan first".to_owned()),
                [one] => spec_of(one).ok_or_else(|| format!("unknown agent: {one}")),
                many => Err(format!(
                    "several agents found ({}); name one: :AgentConnect <name>",
                    many.join(", ")
                )),
            },
        }
    }

    /// Answers the consent prompt, if one is up. Returns true when the key was
    /// used (the prompt takes every key). `y` approves, saves the approval
    /// and connects; `n` and `<Esc>` do nothing.
    pub(crate) fn agent_consent_key(&mut self, key: Key) -> bool {
        let Some(id) = self.agent.consent else {
            return false;
        };
        match key {
            Key::Char('y') => {
                self.agent.consent = None;
                let Some(root) = self.agent.project.clone() else {
                    return true;
                };
                let project = root.to_string_lossy().into_owned();
                self.agent.approvals.approve(id, &project);
                if self.agent.approvals.is_broken() {
                    self.error(format!(
                        "{FILE_NAME} is unreadable: the approval lasts for this session only"
                    ));
                } else {
                    let text = self.agent.approvals.to_text();
                    self.agent.outbox.push(ConnCmd::SaveApprovals(text));
                }
                self.start_link(id, root, 0);
            }
            Key::Char('n') | Key::Esc | Key::Ctrl('c') => {
                self.agent.consent = None;
                self.agent.turn.pending = None;
                self.info("not connected");
            }
            _ => {}
        }
        true
    }

    /// Starts the process and the handshake. `retries` is kept when this is a
    /// retry after a lost connection.
    fn start_link(&mut self, id: &'static str, root: PathBuf, retries: u32) {
        let Some(spec) = spec_of(id) else {
            return;
        };
        self.agent.next_gen += 1;
        let generation = self.agent.next_gen;
        let mut conn = Conn::new();
        let actions = conn.on_event(InEvent::Connect {
            cwd: root.to_string_lossy().into_owned(),
            now: Instant::now(),
        });
        self.agent.outbox.push(ConnCmd::Start {
            generation,
            command: spec.acp.to_owned(),
            cwd: root.clone(),
        });
        self.agent.link = Some(Link {
            id,
            generation,
            conn,
            life: Life::Connecting,
            retries,
            retry_at: None,
            exit: None,
            cwd: root,
        });
        if retries == 0 {
            self.info(format!("{id}-acp: connecting..."));
        }
        self.agent_apply(generation, actions, Instant::now());
    }

    /// `:AgentDisconnect`. Closes the connection and stops the process, which
    /// rvim started. The approval stays.
    pub fn agent_disconnect(&mut self) {
        self.agent.consent = None;
        self.agent.turn.pending = None;
        match self.agent.link.take() {
            Some(link) => {
                self.agent.outbox.push(ConnCmd::Stop {
                    generation: link.generation,
                });
                self.info(format!("{}-acp: disconnected", link.id));
            }
            None => self.info("no agent connected"),
        }
    }

    /// `:AgentCancel`. Sends `session/cancel` for the running turn, if any.
    pub fn agent_cancel(&mut self) {
        let Some(link) = self.agent.link.as_mut() else {
            return self.info("no agent connected");
        };
        let generation = link.generation;
        let actions = link.conn.on_event(InEvent::Cancel);
        let sent = actions.iter().any(|a| matches!(a, OutAction::Send(_)));
        self.agent_apply(generation, actions, Instant::now());
        if !sent {
            self.info("no turn to cancel");
        }
    }

    /// The clock moved: handshake timeouts and retries. Called by the event
    /// loop on `Event::Timer`; tests pass the fake time.
    pub fn agent_timer(&mut self, now: Instant) {
        self.agent_leader_timer(now);
        let Some(link) = self.agent.link.as_mut() else {
            return;
        };
        let generation = link.generation;
        if link.conn.next_deadline().is_some_and(|d| now >= d) {
            let actions = link.conn.on_event(InEvent::Tick { now });
            self.agent_apply(generation, actions, now);
        }
        let Some(link) = self.agent.link.as_ref() else {
            return;
        };
        if link.life == Life::Disconnected && link.retry_at.is_some_and(|t| now >= t) {
            let (id, root, retries) = (link.id, link.cwd.clone(), link.retries);
            // The approval may have been removed since the connection started:
            // a retry needs it again, as a first connection does.
            let project = root.to_string_lossy().into_owned();
            if !self.agent.approvals.is_approved(id, &project) {
                self.agent.link = None;
                return self.error(format!("{id}-acp: approval removed; not reconnecting"));
            }
            self.start_link(id, root, retries);
        }
    }

    /// An event from the worker. Events of an old connection are dropped.
    pub(crate) fn agent_conn_event(&mut self, generation: u64, event: AgentEvent) {
        let now = Instant::now();
        match event {
            AgentEvent::Note(text) => self.error(text),
            AgentEvent::SpawnFailed(reason) => {
                let Some(link) = self
                    .agent
                    .link
                    .as_mut()
                    .filter(|l| l.generation == generation)
                else {
                    return;
                };
                link.life = Life::Error;
                let text = format!("{}-acp: could not start: {reason}", link.id);
                self.agent.turn.pending = None;
                self.error(text);
            }
            AgentEvent::Bytes(bytes) => {
                let actions = match self
                    .agent
                    .link
                    .as_mut()
                    .filter(|l| l.generation == generation)
                {
                    Some(link) => link.conn.on_event(InEvent::Bytes { bytes, now }),
                    None => return,
                };
                self.agent_apply(generation, actions, now);
            }
            AgentEvent::Ended(code) => {
                let actions = match self
                    .agent
                    .link
                    .as_mut()
                    .filter(|l| l.generation == generation && l.life != Life::Error)
                {
                    Some(link) => {
                        link.exit = code;
                        link.conn.on_event(InEvent::Eof)
                    }
                    None => return,
                };
                self.agent_apply(generation, actions, now);
            }
        }
    }

    /// Turns the connection's actions into worker requests and messages.
    /// Actions that belong to later steps (updates, the log, permission
    /// prompts, file calls) are not shown yet.
    pub(crate) fn agent_apply(&mut self, generation: u64, actions: Vec<OutAction>, now: Instant) {
        for action in actions {
            match action {
                OutAction::Send(bytes) => {
                    self.agent.outbox.push(ConnCmd::Send { generation, bytes })
                }
                OutAction::Notice(notice) => {
                    self.agent_notice(generation, notice.clone(), now);
                    self.turn_notice(generation, &notice);
                }
                OutAction::Update(update) => self.turn_update(update),
                OutAction::AskPermission { request, queued } => {
                    self.turn_permission(&request, queued)
                }
                OutAction::Close => self.agent.outbox.push(ConnCmd::Stop { generation }),
                OutAction::Log(_) | OutAction::ServeRead { .. } | OutAction::ServeWrite { .. } => {}
            }
        }
    }

    fn agent_notice(&mut self, generation: u64, notice: Notice, now: Instant) {
        if let Notice::ConnectionLost { .. } = notice {
            return self.agent_lost(generation, now);
        }
        let shown = {
            let Some(link) = self
                .agent
                .link
                .as_mut()
                .filter(|l| l.generation == generation)
            else {
                return;
            };
            let tag = format!("{}-acp", link.id);
            match notice {
                Notice::Connected { .. } => {
                    link.life = Life::Idle;
                    link.retries = 0;
                    link.retry_at = None;
                    Some((format!("{tag}: connected"), false))
                }
                Notice::AuthRequired { .. } => {
                    link.life = Life::Error;
                    self.agent.outbox.push(ConnCmd::Stop { generation });
                    Some((
                        format!(
                            "{tag}: not logged in. Log in with the agent's own tool, then :AgentConnect"
                        ),
                        true,
                    ))
                }
                Notice::Failed { reason } => {
                    link.life = Life::Error;
                    self.agent.outbox.push(ConnCmd::Stop { generation });
                    Some((format!("{tag}: {reason}"), true))
                }
                Notice::RequestFailed { message, .. } => {
                    Some((format!("{tag}: request failed: {message}"), true))
                }
                Notice::PromptRejected { reason } => {
                    Some((format!("{tag}: prompt rejected: {reason}"), true))
                }
                Notice::CancelSent { .. } => Some((format!("{tag}: cancel sent"), false)),
                Notice::Authenticated
                | Notice::TurnStarted { .. }
                | Notice::TurnEnded { .. }
                | Notice::ConnectionLost { .. } => None,
            }
        };
        if let Some((text, is_error)) = shown {
            if is_error {
                self.error(text);
            } else {
                self.info(text);
            }
        }
    }

    /// The connection ended. A first connect that fails is an Error; a lost
    /// connection, or a retry that fails, is retried with backoff.
    fn agent_lost(&mut self, generation: u64, now: Instant) {
        let Some(link) = self
            .agent
            .link
            .as_mut()
            .filter(|l| l.generation == generation)
        else {
            return;
        };
        if link.life == Life::Error {
            return;
        }
        let tag = format!("{}-acp", link.id);
        let exit = link.exit.map(|c| format!(" {c}")).unwrap_or_default();
        if link.life == Life::Connecting && link.retries == 0 {
            link.life = Life::Error;
            link.retry_at = None;
            return self.error(format!("{tag}: exited{exit}"));
        }
        let max = RETRY_DELAYS.len() as u32;
        if link.retries < max {
            let delay = RETRY_DELAYS[link.retries as usize];
            link.retries += 1;
            link.life = Life::Disconnected;
            link.retry_at = Some(now + Duration::from_secs(delay));
            let text = format!("{tag}: connection lost{exit}; retry {}/{max}", link.retries);
            self.info(text);
        } else {
            link.life = Life::Error;
            link.retry_at = None;
            let text = format!("{tag}: connection lost{exit}; gave up after {max} retries");
            self.error(text);
        }
    }
}
