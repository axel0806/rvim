// SPDX-License-Identifier: Apache-2.0
//! Prompt turns and the agent log (PLAN.md, "Agent experience", sections 3
//! and 4; milestone 7 step 5b-2): `:Agent`, `:AgentLog`, the leader keys
//! `\a`, `\al` and `\ac` (only with the opt-in), the streamed updates written
//! to the log document. Permission requests are `agent_permission.rs`.
//!
//! The log is a scratch document with no file. It is not modifiable: changes
//! typed in it are refused with E21, and `:w` with E382, as Vim refuses them
//! in a `buftype=nofile` buffer. Splits do not exist yet, so the log replaces
//! the window's document, and `:AgentLog!` (or `\al` again) goes back to the
//! document and cursor it replaced.

use crate::Editor;
use crate::acp::{
    ConnState, ContentBlock, InEvent, Notice, PlanEntryStatus, SessionUpdate, StopReason,
    ToolCallInfo, ToolCallStatus,
};
use crate::agent_session::Life;
use crate::buffer::Buffer;
use crate::cmdline::PromptKind;
use crate::document::DocId;
use crate::editor::{Mode, VisualMode};
use crate::key::Key;
use crate::motion::Pos;
use std::time::{Duration, Instant};

/// The leader prefix state (`\` then `a`), with the opt-in only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Leader {
    #[default]
    None,
    /// `\` typed; the next key says what it is.
    Backslash,
    /// `\a` typed; the next key is `l`, `c`, or the start of a prompt.
    Agent,
}

/// What the turn code keeps in the editor.
#[derive(Default)]
pub(crate) struct TurnState {
    /// The agent log document, once it exists.
    log: Option<DocId>,
    /// The document and cursor the log replaced, for `:AgentLog!`.
    back: Option<(DocId, Pos)>,
    /// The log has no text yet: its one empty line is not a line of output.
    fresh: bool,
    /// The last log line is still open (a partial line), with its mark.
    open: Option<&'static str>,
    /// A prompt typed before the connection was ready; sent on connect.
    pub(crate) pending: Option<String>,
    /// The leader prefix state.
    pub(crate) leader: Leader,
    /// When `\a` alone opens the prompt: a key that is not `l` or `c` by then
    /// (Vim's 'timeoutlen', 1000 ms by default).
    pub(crate) leader_until: Option<Instant>,
}

/// Vim's 'timeoutlen' default: how long `\a` waits for `l` or `c`.
const LEADER_TIMEOUT: Duration = Duration::from_millis(1000);

/// The agent log's text, one line per line of the document (for tests and
/// for the view). None until the log has been made.
impl Editor {
    pub fn agent_log_text(&self) -> Option<String> {
        let id = self.agent.turn.log?;
        Some(self.document(id).buf.lines().join("\n"))
    }

    /// `:Agent {text}`: sends `text` as one prompt turn. With no text it opens
    /// the prompt line with `Agent ` typed.
    pub(crate) fn agent_command(&mut self, text: &str) {
        if text.is_empty() {
            self.cmd_start(PromptKind::Ex, Mode::Normal, "Agent ");
        } else {
            self.agent_prompt(text);
        }
    }

    /// Sends a prompt turn to the connected agent. The first prompt, with no
    /// connection yet, starts the connection (consent first if needed); the
    /// text is sent when the connection is ready.
    pub(crate) fn agent_prompt(&mut self, text: &str) {
        let Some(link) = self.agent.link.as_ref() else {
            self.agent.turn.pending = Some(text.to_owned());
            self.agent_connect(None);
            if self.agent.link.is_none() && self.agent.consent.is_none() {
                self.agent.turn.pending = None;
            }
            return;
        };
        let (life, state, generation, id) = (
            link.life,
            link.conn.state().clone(),
            link.generation,
            link.id,
        );
        match life {
            Life::Connecting => {
                self.agent.turn.pending = Some(text.to_owned());
                return self.info(format!(
                    "{id}-acp: connecting; the prompt is sent when connected"
                ));
            }
            Life::Disconnected => {
                return self.error(format!(
                    "{id}-acp: connection lost, retrying; send the prompt again later"
                ));
            }
            Life::Error => {
                self.agent.turn.pending = None;
                return self.error(format!("{id}-acp: not connected; :AgentConnect"));
            }
            Life::Idle => {}
        }
        match state {
            ConnState::Ready => {}
            ConnState::Prompting { .. } | ConnState::Cancelling { .. } => {
                return self.info(format!(
                    "{id}-acp: a turn is already running; :AgentCancel stops it"
                ));
            }
            _ => return self.error(format!("{id}-acp: not connected; :AgentConnect")),
        }
        let actions = match self.agent.link.as_mut() {
            Some(link) => link.conn.on_event(InEvent::Prompt {
                blocks: vec![ContentBlock::Text {
                    text: text.to_owned(),
                }],
            }),
            None => return,
        };
        self.log_line(&format!("> {text}"));
        self.agent_apply(generation, actions, Instant::now());
    }

    /// `\a` in Normal or Visual mode, or the key after it when it is not `l`
    /// or `c`: the prompt line, with `Agent ` (or `'<,'>Agent ` in Visual).
    pub(crate) fn agent_open_prompt(&mut self) {
        let was_visual = self.is_visual();
        let initial = if was_visual {
            let mut start = self.win().vstart.min(self.win().cursor);
            if self.visual_mode() == Some(VisualMode::Line) {
                start.col = 0;
            }
            self.end_visual();
            self.win_mut().cursor = start;
            "'<,'>Agent "
        } else {
            "Agent "
        };
        self.cmd_start(PromptKind::Ex, Mode::Normal, initial);
    }

    /// The leader keys (`\a`, `\al`, `\ac`). Returns true when the key was
    /// used. With the opt-in off, nothing is used: `\` is Vim's key.
    pub(crate) fn agent_leader_key(&mut self, key: Key) -> bool {
        if !self.agent_enabled {
            return false;
        }
        match (self.agent.turn.leader, key) {
            (Leader::None, Key::Char('\\')) => {
                self.agent.turn.leader = Leader::Backslash;
                self.win_mut().keep_curswant = true;
                true
            }
            (Leader::None, _) => false,
            (Leader::Backslash, Key::Char('a')) => {
                self.agent.turn.leader = Leader::Agent;
                self.agent.turn.leader_until = Some(Instant::now() + LEADER_TIMEOUT);
                self.win_mut().keep_curswant = true;
                true
            }
            (Leader::Backslash, Key::Esc | Key::Ctrl('c')) => {
                self.agent.turn.leader = Leader::None;
                self.win_mut().keep_curswant = true;
                true
            }
            (Leader::Backslash, _) => {
                // Vim's `\` does nothing; the key after it is a command of its own.
                self.agent.turn.leader = Leader::None;
                self.beep();
                false
            }
            (Leader::Agent, key) => {
                self.agent.turn.leader = Leader::None;
                self.agent.turn.leader_until = None;
                // Only `\al` moves the view; the others keep the desired
                // column, as `<Esc>` and the Ex commands that do not move the
                // cursor do (CLAUDE.md).
                match key {
                    Key::Char('l') => self.agent_log_toggle(),
                    Key::Char('c') => {
                        self.win_mut().keep_curswant = true;
                        self.agent_cancel();
                    }
                    Key::Esc | Key::Ctrl('c') => self.win_mut().keep_curswant = true,
                    _ => {
                        self.win_mut().keep_curswant = true;
                        self.agent_open_prompt();
                        self.cmdline_key(key);
                    }
                }
                true
            }
        }
    }

    /// The moment `\a` alone opens the prompt, for `next_deadline`.
    pub(crate) fn agent_leader_deadline(&self) -> Option<Instant> {
        match self.agent.turn.leader {
            Leader::Agent => self.agent.turn.leader_until,
            _ => None,
        }
    }

    /// The clock moved: `\a` that waited out the timeout opens the prompt.
    pub(crate) fn agent_leader_timer(&mut self, now: Instant) {
        if self.agent_leader_deadline().is_some_and(|t| now >= t) {
            self.agent.turn.leader = Leader::None;
            self.agent.turn.leader_until = None;
            self.agent_open_prompt();
        }
    }

    /// `:AgentLog` (show) and `:AgentLog!` (back), and `\al` (toggle).
    pub(crate) fn agent_log_toggle(&mut self) {
        let log = self.agent_log_doc();
        if self.win().doc == log {
            self.agent_log_back();
        } else {
            self.agent_log_show();
        }
    }

    pub(crate) fn agent_log_show(&mut self) {
        let log = self.agent_log_doc();
        if self.win().doc == log {
            return;
        }
        let here = (self.win().doc, self.win().cursor);
        self.agent.turn.back = Some(here);
        self.win_mut().doc = log;
        let last = self.document(log).buf.last_line();
        self.win_mut().cursor = Pos { line: last, col: 0 };
        self.win_mut().keep_curswant = false;
        self.update_topline();
    }

    pub(crate) fn agent_log_back(&mut self) {
        let Some((doc, pos)) = self.agent.turn.back.take() else {
            return self.info("no document to go back to");
        };
        self.win_mut().doc = doc;
        let buf = &self.document(doc).buf;
        let line = pos.line.min(buf.last_line());
        let col = pos.col.min(buf.line_len(line).saturating_sub(1));
        self.win_mut().cursor = Pos { line, col };
        self.win_mut().keep_curswant = false;
        self.update_topline();
    }

    /// The log document, made on first use.
    fn agent_log_doc(&mut self) -> DocId {
        if let Some(id) = self.agent.turn.log {
            return id;
        }
        let mut buf = Buffer::from_text("");
        buf.locked = true;
        let id = self.new_document(buf);
        self.document_mut(id).nomodifiable = true;
        self.agent.turn.log = Some(id);
        self.agent.turn.fresh = true;
        id
    }

    /// A complete line in the log (closes a line still open).
    pub(crate) fn log_line(&mut self, text: &str) {
        let log = self.agent_log_doc();
        let follow = self.log_follow(log);
        self.agent.turn.open = None;
        self.log_push(log, text);
        self.log_end(log, follow);
    }

    /// Streamed text in the log: a chunk's text continues the open line, and
    /// each newline ends one. `mark` is put before a line that starts here.
    fn log_text(&mut self, mark: &'static str, text: &str) {
        let log = self.agent_log_doc();
        let follow = self.log_follow(log);
        let parts: Vec<&str> = text.split('\n').collect();
        let n = parts.len();
        for (i, seg) in parts.into_iter().enumerate() {
            if i > 0 {
                self.agent.turn.open = None;
            }
            if seg.is_empty() {
                // A blank line between two newlines is kept.
                if i > 0 && i + 1 < n {
                    self.log_push(log, "");
                }
                continue;
            }
            match self.agent.turn.open {
                Some(open) if open == mark => self.log_extend(log, seg),
                _ => {
                    self.log_push(log, &format!("{mark}{seg}"));
                    self.agent.turn.open = Some(mark);
                }
            }
        }
        self.log_end(log, follow);
    }

    /// Whether the log's cursor is on its last line while the log is shown.
    /// Then the view follows the new output, as `:terminal` does; otherwise
    /// it stays where the user put it.
    fn log_follow(&self, log: DocId) -> bool {
        self.win().doc == log && self.win().cursor.line >= self.document(log).buf.last_line()
    }

    fn log_end(&mut self, log: DocId, follow: bool) {
        if follow && self.win().doc == log {
            let last = self.document(log).buf.last_line();
            self.win_mut().cursor = Pos { line: last, col: 0 };
            self.update_topline();
        }
    }

    /// Runs `f` on the log's text with the lock released: the only writes to
    /// the log are the editor's own output, never a change the user typed.
    fn log_buf<R>(&mut self, log: DocId, f: impl FnOnce(&mut Buffer) -> R) -> R {
        let buf = &mut self.document_mut(log).buf;
        let locked = std::mem::replace(&mut buf.locked, false);
        let out = f(buf);
        buf.locked = locked;
        // Output is not a change the user made: the log is never modified.
        buf.modified = false;
        out
    }

    fn log_push(&mut self, log: DocId, text: &str) {
        let fresh = std::mem::take(&mut self.agent.turn.fresh);
        self.log_buf(log, |buf| {
            if fresh {
                buf.set_line(0, text);
            } else {
                let at = buf.line_count();
                buf.insert_line(at, text);
            }
        });
    }

    fn log_extend(&mut self, log: DocId, seg: &str) {
        self.log_buf(log, |buf| {
            let last = buf.last_line();
            let mut text = buf.line_string(last);
            text.push_str(seg);
            buf.set_line(last, &text);
        });
    }

    /// A session update from the agent, for the log.
    pub(crate) fn turn_update(&mut self, update: SessionUpdate) {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text { text } = chunk.content {
                    self.log_text("", &text);
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let ContentBlock::Text { text } = chunk.content {
                    self.log_text("[thought] ", &text);
                }
            }
            SessionUpdate::ToolCall(info) | SessionUpdate::ToolCallUpdate(info) => {
                self.log_line(&tool_line(&info));
            }
            SessionUpdate::Plan(plan) => {
                let done = plan
                    .entries
                    .iter()
                    .filter(|e| matches!(e.status, PlanEntryStatus::Completed))
                    .count();
                self.log_line(&format!("[plan] {done}/{} done", plan.entries.len()));
            }
            // The user's own message echoed back, and kinds not shown yet.
            SessionUpdate::UserMessageChunk(_) | SessionUpdate::Other { .. } => {}
        }
    }

    /// The turn's lifecycle notices, for the log and the message line. Only
    /// the current connection's notices are shown.
    pub(crate) fn turn_notice(&mut self, generation: u64, notice: &Notice) {
        let current = self
            .agent
            .link
            .as_ref()
            .is_some_and(|l| l.generation == generation);
        if !current {
            return;
        }
        match notice {
            Notice::Connected { .. } => {
                if let Some(text) = self.agent.turn.pending.take() {
                    self.agent_prompt(&text);
                }
            }
            Notice::CancelSent { .. } => self.log_line("[cancel] session/cancel sent"),
            Notice::TurnEnded { stop_reason, .. } => {
                let word = stop_word(*stop_reason);
                self.log_line(&format!("[stop] {word}"));
                let id = self.agent.link.as_ref().map_or("agent", |l| l.id);
                self.info(format!("{id}-acp: turn ended ({word})"));
            }
            Notice::RequestFailed { message, .. } => {
                self.log_line(&format!("[error] {message}"));
            }
            Notice::PromptRejected { reason } => {
                self.log_line(&format!("[error] prompt rejected: {reason}"));
            }
            Notice::AuthRequired { .. } => {
                self.agent.turn.pending = None;
                self.log_line(
                    "[error] not logged in; log in with the agent's own tool, then :AgentConnect",
                );
            }
            Notice::ConnectionLost { turn } => {
                self.agent.turn.pending = None;
                if turn.is_some() {
                    self.log_line("[error] connection lost; the turn is not resumed");
                } else {
                    self.log_line("[error] connection lost");
                }
            }
            Notice::Failed { reason } => {
                self.agent.turn.pending = None;
                self.log_line(&format!("[error] {reason}"));
            }
            Notice::Authenticated | Notice::TurnStarted { .. } => {}
        }
    }

    /// The commands `:AgentLog` and `:AgentLog!` run, and the refusals of
    /// the log: an Ex command that would change its text or write it gives
    /// Vim's message for a `nofile` buffer (E21, E382).
    pub(crate) fn log_refusal(name: &str) -> Option<&'static str> {
        if name == "wq"
            || abbrev(name, "write", 1)
            || abbrev(name, "xit", 1)
            || abbrev(name, "exit", 3)
        {
            return Some("E382: Cannot write, 'buftype' option is set");
        }
        let changes = abbrev(name, "delete", 1)
            || matches!(name, "&" | "&&" | "~" | "t" | ">" | "<")
            || abbrev(name, "substitute", 1)
            || abbrev(name, "global", 1)
            || abbrev(name, "vglobal", 1)
            || abbrev(name, "normal", 4)
            || abbrev(name, "move", 1)
            || abbrev(name, "copy", 2)
            || abbrev(name, "join", 1)
            || abbrev(name, "put", 2)
            || abbrev(name, "read", 1)
            || abbrev(name, "edit", 1)
            || abbrev(name, "undo", 1)
            || abbrev(name, "redo", 3)
            || abbrev(name, "append", 1)
            || abbrev(name, "insert", 1)
            || abbrev(name, "change", 1)
            || abbrev(name, "sort", 3)
            || abbrev(name, "retab", 3)
            || abbrev(name, "left", 2)
            || abbrev(name, "right", 2)
            || abbrev(name, "center", 2);
        changes.then_some("E21: Cannot make changes, 'modifiable' is off")
    }
}

/// `name` is a prefix of `full` of at least `min` characters (Vim's rule).
fn abbrev(name: &str, full: &str, min: usize) -> bool {
    name.len() >= min && name.len() <= full.len() && full.starts_with(name)
}

fn stop_word(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        StopReason::Other => "unknown",
    }
}

fn status_word(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "pending",
        ToolCallStatus::InProgress => "in progress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        ToolCallStatus::Other => "unknown",
    }
}

/// `[tool] edit src/a.rs: completed` (PLAN.md, "The edit loop").
fn tool_line(info: &ToolCallInfo) -> String {
    let mut what = String::new();
    if let Some(kind) = info.kind {
        what.push_str(&format!("{kind:?}").to_lowercase());
        what.push(' ');
    }
    what.push_str(info.title.as_deref().unwrap_or(&info.tool_call_id));
    match info.status {
        Some(status) => format!("[tool] {what}: {}", status_word(status)),
        None => format!("[tool] {what}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::Editor;

    #[test]
    fn leader_keys_keep_the_cursor_and_the_desired_column() {
        let mut ed = Editor::from_text("abcdef\nab\nabcdef\n");
        ed.set_agent_enabled(true);
        ed.feed("llllj");
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1));
        assert_eq!(ed.win().curswant, 4);
        // `\ac` (no turn: a message) and `\a` then `<Esc>` move nothing and
        // keep the column the `j` before them set.
        ed.feed("\\ac");
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1));
        assert_eq!(ed.win().curswant, 4, "\\ac keeps the desired column");
        ed.feed("\\a<Esc>");
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1));
        assert_eq!(ed.win().curswant, 4, "\\a<Esc> keeps the desired column");
        ed.feed("j");
        assert_eq!((ed.cursor().line, ed.cursor().col), (2, 4));
    }
}
