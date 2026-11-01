// SPDX-License-Identifier: Apache-2.0
//! Everything that can happen to the editor, as one enum. The front end
//! reads the terminal on one thread, workers (file watching, agents,
//! prediction, highlighting) send events too, and the main loop feeds them to
//! [`Editor::handle_event`](crate::Editor::handle_event). The editor itself
//! never blocks and never touches the terminal.

use crate::key::Key;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(Key),
    /// Text from a bracketed paste.
    Paste(String),
    /// New terminal size (columns, rows).
    Resize(u16, u16),
    FocusGained,
    FocusLost,
    /// A timer requested through `Editor::next_deadline` expired.
    Timer,
    /// A signal that ends the session (SIGHUP, SIGTERM; see `signals.rs`).
    /// The front end acts on it; the editor ignores it.
    Signal(i32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    #[test]
    fn events_drive_the_editor() {
        let mut ed = Editor::from_text("ab\n");
        ed.handle_event(Event::Key(Key::Char('x')));
        ed.handle_event(Event::Paste("zz".into()));
        ed.handle_event(Event::Resize(80, 24));
        ed.handle_event(Event::FocusGained);
        assert_eq!(ed.doc.buf.text(), "zzb\n");
        assert!(ed.next_deadline().is_none());
    }
}
