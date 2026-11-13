// SPDX-License-Identifier: Apache-2.0
//! Answering permission requests (PLAN.md, "Agent experience", section 5;
//! milestone 7 step 6a): the modal prompt on the message line, its keys, the
//! session grants, `:AgentGrants`, `:AgentForget` and the reminder.
//!
//! `acp.rs` keeps the queue and answers only on `InEvent::PermissionAnswer`.
//! This module reads the head of that queue after every batch of actions
//! ([`Editor::permission_sync`]): it shows the prompt for a new head, answers
//! it from a session grant the user made, or takes the prompt down when the
//! queue is empty. Nothing here answers by timeout: after five minutes the
//! message line only reminds the user, every five minutes.
//!
//! Grants live in memory only. They are never written to disk and are gone
//! when rvim exits; `:AgentForget` drops them sooner.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::Editor;
use crate::acp::{
    InEvent, PermissionChoice, PermissionOptionKind, PermissionRequest, ToolCallContent, ToolKind,
};
use crate::agent_approval::FILE_NAME;
use crate::agent_conn::ConnCmd;
use crate::key::Key;

/// How long the prompt waits before the message line reminds the user, and
/// how often the reminder repeats. It never answers.
pub(crate) const REMIND_AFTER: Duration = Duration::from_secs(5 * 60);

/// A session grant: the user's `a` (allow) or `N` (reject) for a tool kind
/// and scope, answering later requests of the same kind and scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Grant {
    pub agent: &'static str,
    pub kind: &'static str,
    /// The file's directory relative to the project (`src/`), or the
    /// command for `execute`.
    pub scope: String,
    pub allow: bool,
}

/// The prompt on the message line.
#[derive(Debug)]
struct Prompt {
    generation: u64,
    agent: &'static str,
    request: PermissionRequest,
    queued: usize,
    remind_at: Instant,
    shown: Shown,
}

/// What the prompt says about a request.
#[derive(Debug, Clone)]
struct Shown {
    kind: &'static str,
    /// For the prompt: `wants to edit src/a.rs (+3 -1)`, `wants to run: cargo test`.
    action: String,
    /// For the log: `edit src/a.rs`.
    what: String,
    /// The grant scope, when the request has one.
    scope: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct PermState {
    prompt: Option<Prompt>,
    pub(crate) grants: Vec<Grant>,
}

/// The wire name of a tool kind (ACP v1 `ToolKind`).
fn kind_word(kind: Option<ToolKind>) -> &'static str {
    match kind {
        Some(ToolKind::Read) => "read",
        Some(ToolKind::Edit) => "edit",
        Some(ToolKind::Delete) => "delete",
        Some(ToolKind::Move) => "move",
        Some(ToolKind::Search) => "search",
        Some(ToolKind::Execute) => "execute",
        Some(ToolKind::Think) => "think",
        Some(ToolKind::Fetch) => "fetch",
        Some(ToolKind::SwitchMode) => "switch_mode",
        Some(ToolKind::Other) | None => "other",
    }
}

/// `path` relative to `root` with `/` separators, or as given when it is
/// outside the project.
fn relative(path: &str, root: Option<&Path>) -> String {
    if let Some(rel) = root.and_then(|r| Path::new(path).strip_prefix(r).ok()) {
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        return parts.join("/");
    }
    path.to_owned()
}

/// The directory part of a shown path, with its separator: `src/`, or `./`
/// for a file at the project root.
fn dir_of(shown: &str) -> String {
    match shown.rfind(['/', '\\']) {
        Some(i) => shown[..=i].to_owned(),
        None => "./".to_owned(),
    }
}

fn lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.lines().map(str::to_owned).collect()
}

fn describe(request: &PermissionRequest, root: Option<&Path>) -> Shown {
    let call = &request.tool_call;
    let kind = kind_word(call.kind);
    let title = call.title.clone().filter(|t| !t.is_empty());
    if kind == "execute" {
        return match title {
            Some(cmd) => Shown {
                kind,
                action: format!("wants to run: {cmd}"),
                what: format!("execute {cmd}"),
                scope: Some(cmd),
            },
            None => Shown {
                kind,
                action: format!("wants to run a command ({})", call.tool_call_id),
                what: format!("execute {}", call.tool_call_id),
                scope: None,
            },
        };
    }
    let path = call
        .locations
        .as_ref()
        .and_then(|l| l.first())
        .map(|l| l.path.clone());
    if let Some(path) = path {
        let shown = relative(&path, root);
        let stats = call
            .content
            .iter()
            .flatten()
            .find_map(|c| match c {
                ToolCallContent::Diff {
                    old_text, new_text, ..
                } => Some(crate::merge::line_stats(
                    &lines(old_text.as_deref().unwrap_or("")),
                    &lines(new_text),
                )),
                _ => None,
            })
            .map(|(add, del)| format!(" (+{add} -{del})"))
            .unwrap_or_default();
        let verb = if kind == "other" { "use" } else { kind };
        return Shown {
            kind,
            action: format!("wants to {verb} {shown}{stats}"),
            what: format!("{kind} {shown}"),
            scope: Some(dir_of(&shown)),
        };
    }
    let what = title.unwrap_or_else(|| call.tool_call_id.clone());
    Shown {
        kind,
        action: format!("asks for permission: {what}"),
        what,
        scope: None,
    }
}

/// The prompt's text (PLAN.md, section 5).
fn prompt_text(agent: &str, request: &PermissionRequest, shown: &Shown, queued: usize) -> String {
    let has = |k| request.option_of_kind(k).is_some();
    let scope = shown.scope.as_ref().map(|s| {
        if shown.kind == "execute" {
            " for this command this session".to_owned()
        } else {
            format!(" for {} in {s} this session", shown.kind)
        }
    });
    let mut keys = Vec::new();
    if has(PermissionOptionKind::AllowOnce) {
        keys.push("[y]es".to_owned());
    }
    let always = has(PermissionOptionKind::AllowAlways);
    if always {
        keys.push(format!("[a]lways{}", scope.clone().unwrap_or_default()));
    }
    if has(PermissionOptionKind::RejectOnce) {
        keys.push("[n]o".to_owned());
    }
    if has(PermissionOptionKind::RejectAlways) {
        // The scope is named once: on `a`, or here when there is no `a`.
        let named = if always {
            String::new()
        } else {
            scope.unwrap_or_default()
        };
        keys.push(format!("[N]ever{named}"));
    }
    format!(
        "{agent}-acp {}. {} (1/{queued})",
        shown.action,
        keys.join(" ")
    )
}

impl Editor {
    /// The permission prompt's keys, while it is up. Returns true when the
    /// key was used: the prompt takes every key, so none reaches Normal,
    /// Insert or command-line mode. `y` `a` `n` `N` answer with the option of
    /// that kind, when the agent offered one; `<Esc>` and `<C-c>` reject once,
    /// or cancel when no `reject_once` was offered. Other keys do nothing.
    pub(crate) fn agent_permission_key(&mut self, key: Key) -> bool {
        let Some(generation) = self.agent.perm.prompt.as_ref().map(|p| p.generation) else {
            return false;
        };
        // Answering moves nothing: the desired column stays (CLAUDE.md).
        self.win_mut().keep_curswant = true;
        let Some(head) = self
            .agent
            .link
            .as_ref()
            .filter(|l| l.generation == generation)
            .and_then(|l| l.conn.permission_head().cloned())
        else {
            self.permission_clear();
            return true;
        };
        let wanted = match key {
            Key::Char('y') => Some(PermissionOptionKind::AllowOnce),
            Key::Char('a') => Some(PermissionOptionKind::AllowAlways),
            Key::Char('n') => Some(PermissionOptionKind::RejectOnce),
            Key::Char('N') => Some(PermissionOptionKind::RejectAlways),
            Key::Esc | Key::Ctrl('c') => None,
            _ => return true,
        };
        let (choice, word) = match wanted {
            Some(kind) => {
                let Some(option) = head.option_of_kind(kind) else {
                    // Not offered: the prompt stays.
                    return true;
                };
                let allow = matches!(
                    kind,
                    PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways
                );
                (
                    PermissionChoice::Select(option.option_id.clone()),
                    if allow { "allowed" } else { "rejected" },
                )
            }
            None => match head.option_of_kind(PermissionOptionKind::RejectOnce) {
                Some(option) => (
                    PermissionChoice::Select(option.option_id.clone()),
                    "rejected",
                ),
                None => (PermissionChoice::Cancelled, "cancelled"),
            },
        };
        let Some(prompt) = self.agent.perm.prompt.take() else {
            return true;
        };
        let shown = prompt.shown;
        if let (
            Some(kind @ (PermissionOptionKind::AllowAlways | PermissionOptionKind::RejectAlways)),
            Some(scope),
        ) = (wanted, shown.scope.clone())
        {
            let grant = Grant {
                agent: prompt.agent,
                kind: shown.kind,
                scope,
                allow: kind == PermissionOptionKind::AllowAlways,
            };
            let grants = &mut self.agent.perm.grants;
            grants
                .retain(|g| (g.agent, g.kind, &g.scope) != (grant.agent, grant.kind, &grant.scope));
            grants.push(grant);
        }
        self.log_line(&format!("[permission] {}: {word}", shown.what));
        self.message = None;
        let actions = match self.agent.link.as_mut() {
            Some(link) => link.conn.on_event(InEvent::PermissionAnswer(choice)),
            None => return true,
        };
        self.agent_apply(generation, actions, Instant::now());
        true
    }

    /// Brings the prompt in line with the connection's queue: the prompt for
    /// a new head (or the head answered from a grant), the count for the same
    /// head, or no prompt when the queue is empty. Called after every batch of
    /// connection actions.
    pub(crate) fn permission_sync(&mut self, now: Instant) {
        let head = self.agent.link.as_ref().and_then(|l| {
            l.conn
                .permission_head()
                .cloned()
                .map(|h| (l.id, l.generation, l.conn.permissions_waiting(), h))
        });
        let Some((agent, generation, queued, head)) = head else {
            return self.permission_clear();
        };
        if let Some(p) = self.agent.perm.prompt.as_mut()
            && p.generation == generation
            && p.request == head
        {
            if p.queued != queued {
                p.queued = queued;
                let text = prompt_text(agent, &p.request, &p.shown, queued);
                self.info(text);
            }
            return;
        }
        let shown = describe(&head, self.agent.project.as_deref());
        if let Some(grant) = self.grant_for(agent, &shown) {
            let (first, second) = if grant.allow {
                (
                    PermissionOptionKind::AllowAlways,
                    PermissionOptionKind::AllowOnce,
                )
            } else {
                (
                    PermissionOptionKind::RejectAlways,
                    PermissionOptionKind::RejectOnce,
                )
            };
            let option = head
                .option_of_kind(first)
                .or_else(|| head.option_of_kind(second));
            if let Some(option) = option {
                let polarity = if grant.allow { "allow" } else { "reject" };
                let word = if grant.allow { "allowed" } else { "rejected" };
                self.log_line(&format!(
                    "[permission] {}: {word}; grant used: {} {} {polarity}",
                    shown.what, grant.kind, grant.scope
                ));
                self.agent.perm.prompt = None;
                let choice = PermissionChoice::Select(option.option_id.clone());
                let actions = match self.agent.link.as_mut() {
                    Some(link) => link.conn.on_event(InEvent::PermissionAnswer(choice)),
                    None => return,
                };
                // The next head, if any, is handled by the sync at its end.
                return self.agent_apply(generation, actions, now);
            }
        }
        self.log_line(&format!("[permission] {}: waiting", shown.what));
        let text = prompt_text(agent, &head, &shown, queued);
        self.agent.perm.prompt = Some(Prompt {
            generation,
            agent,
            request: head,
            queued,
            remind_at: now + REMIND_AFTER,
            shown,
        });
        self.info(text);
    }

    /// The grant that answers a request like `shown` from `agent`, if any.
    fn grant_for(&self, agent: &str, shown: &Shown) -> Option<Grant> {
        let scope = shown.scope.as_ref()?;
        self.agent
            .perm
            .grants
            .iter()
            .find(|g| g.agent == agent && g.kind == shown.kind && &g.scope == scope)
            .cloned()
    }

    /// Takes the prompt down: the queue emptied without a key (`:AgentCancel`,
    /// the turn ended, a disconnect or a lost connection). The log says the
    /// request was cancelled.
    pub(crate) fn permission_clear(&mut self) {
        let Some(prompt) = self.agent.perm.prompt.take() else {
            return;
        };
        self.log_line(&format!("[permission] {}: cancelled", prompt.shown.what));
        if prompt.queued > 1 {
            self.log_line(&format!(
                "[permission] {} more cancelled",
                prompt.queued - 1
            ));
        }
    }

    /// Whether the permission prompt is up (for paste and tests).
    pub fn permission_prompt_up(&self) -> bool {
        self.agent.perm.prompt.is_some()
    }

    /// The reminder's deadline, for `next_deadline`.
    pub(crate) fn permission_deadline(&self) -> Option<Instant> {
        self.agent.perm.prompt.as_ref().map(|p| p.remind_at)
    }

    /// The clock moved: the reminder, every five minutes while the prompt
    /// is up. It never answers the request.
    pub(crate) fn permission_timer(&mut self, now: Instant) {
        let Some(prompt) = self.agent.perm.prompt.as_mut() else {
            return;
        };
        if now < prompt.remind_at {
            return;
        }
        while prompt.remind_at <= now {
            prompt.remind_at += REMIND_AFTER;
        }
        let text = format!("{}-acp still waiting for y or n", prompt.agent);
        self.info(text);
    }

    /// The number of session grants for `agent`, for the indicator.
    pub(crate) fn grant_count(&self, agent: &str) -> usize {
        self.agent
            .perm
            .grants
            .iter()
            .filter(|g| g.agent == agent)
            .count()
    }

    /// `:AgentGrants`: this session's grants on one message line.
    pub(crate) fn agent_grants_command(&mut self) {
        let grants = &self.agent.perm.grants;
        if grants.is_empty() {
            return self.info("No grants");
        }
        let text = grants
            .iter()
            .map(|g| {
                let polarity = if g.allow { "allow" } else { "reject" };
                format!("{}: {polarity} {} {}", g.agent, g.kind, g.scope)
            })
            .collect::<Vec<_>>()
            .join(" | ");
        self.info(text);
    }

    /// `:AgentForget` (this project) and `:AgentForget!` (every project):
    /// removes this session's grants and the approvals, saves the approval
    /// file, and closes the connection, which now has no approval.
    pub(crate) fn agent_forget(&mut self, all: bool) {
        self.agent_active = true;
        let grants = std::mem::take(&mut self.agent.perm.grants).len();
        let project = self
            .agent
            .project
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        let removed = match (&project, all) {
            (_, true) => self.agent.approvals.forget_all(),
            (Some(project), false) => self.agent.approvals.forget_project(project),
            (None, false) => 0,
        };
        self.agent.consent = None;
        self.agent.turn.pending = None;
        if let Some(link) = self.agent.link.take() {
            self.agent.outbox.push(ConnCmd::Stop {
                generation: link.generation,
            });
        }
        self.permission_clear();
        let saved = if removed == 0 {
            String::new()
        } else if self.agent.approvals.is_broken() {
            format!("; {FILE_NAME} is unreadable and was not changed")
        } else {
            let text = self.agent.approvals.to_text();
            self.agent.outbox.push(ConnCmd::SaveApprovals(text));
            String::new()
        };
        let place = if all { "every project" } else { "this project" };
        let s = |n: usize| if n == 1 { "" } else { "s" };
        self.info(format!(
            "forgot {grants} grant{} and {removed} approval{} for {place}{saved}",
            s(grants),
            s(removed)
        ));
    }
}
