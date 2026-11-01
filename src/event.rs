// SPDX-License-Identifier: Apache-2.0
//! Everything that can happen to the editor, as one enum. The front end
//! reads the terminal on one thread, workers (file watching, agents,
//! prediction, highlighting) send events too, and the main loop feeds them to
//! [`Editor::handle_event`](crate::Editor::handle_event). The editor itself
//! never blocks and never touches the terminal.

use crate::document::DocId;
use crate::key::Key;
use std::time::SystemTime;

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
    /// The file of document `doc` differs from what the buffer last synced
    /// with (sent by the watcher, `watch.rs`, only with the option
    /// `filewatch`). `version` is the document version the watcher was told
    /// about; the editor drops the event if the text has changed since. The
    /// file's contents are `bytes`, and `mtime`, `size` and `hash` describe
    /// them (`hash` as `reload::fnv1a`). A file the buffer had no state for
    /// (no file on disk yet) also comes as this event.
    FileChanged {
        doc: DocId,
        version: u64,
        bytes: Vec<u8>,
        mtime: Option<SystemTime>,
        size: usize,
        hash: u64,
    },
    /// The file that the buffer had on disk is gone (see `FileChanged`).
    FileDeleted {
        doc: DocId,
        version: u64,
    },
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
        assert_eq!(ed.buf().text(), "zzb\n");
        assert!(ed.next_deadline().is_none());
    }
}
