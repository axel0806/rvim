// SPDX-License-Identifier: Apache-2.0
//! The document: the text and what is kept with it, shared by every window
//! that shows it (PLAN.md, "Document, Window, Editor"). Step 1 moved the text,
//! the undo tree, the undo line and the last Visual area here. Step 2 added
//! the version counter. Marks are still in `Buffer` and `Editor`, and file
//! state is still in `Buffer`.

use crate::buffer::Buffer;
use crate::editor::LastVisual;
use crate::motion::Pos;
use crate::options::{Local, Options};
use crate::undo::UndoTree;

/// The line `U` restores (Vim's `b_u_line_*`).
#[derive(Clone, Debug)]
pub(crate) struct ULine {
    pub line: usize,
    pub text: String,
    pub col: usize,
}

/// Index of a document in `Editor::documents`. Ids stay stable: documents are
/// not removed yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct DocId(pub(crate) usize);

pub(crate) struct Document {
    pub(crate) buf: Buffer,
    pub(crate) undo: UndoTree,
    pub(crate) uline: Option<ULine>,
    /// The last Visual area (Vim's `b_visual`, used by `gv`, `'<` and `'>`).
    pub(crate) last_visual: Option<LastVisual>,
    /// The version at the time the current buffer was loaded. `version()`
    /// adds the buffer's own change count to it.
    version_base: u64,
    /// The `'[` and `']` marks: the start and end of the last change (Vim's
    /// `b_op_start` and `b_op_end`).
    pub(crate) op_start: Pos,
    pub(crate) op_end: Pos,
    /// The buffer-local option values (`'autoindent'`, `'shiftwidth'`, ...).
    /// A new document copies the global values, see `options.rs`.
    pub(crate) local: Local,
}

impl Document {
    /// A document for `buf`, with buffer-local options copied from `opts`.
    pub(crate) fn new(buf: Buffer, opts: &Options) -> Self {
        Self::with_local(buf, Local::new(opts))
    }

    fn with_local(buf: Buffer, local: Local) -> Self {
        let undo = UndoTree::new(buf.snapshot());
        Self {
            buf,
            undo,
            uline: None,
            last_visual: None,
            version_base: 0,
            op_start: Pos::default(),
            op_end: Pos::default(),
            local,
        }
    }

    /// The change counter of the text. It goes up with every change to the
    /// text (insert, delete, `set_line` and the like), with undo and redo,
    /// and with a reload, and never goes down. It does not change on cursor
    /// motion or on setting marks. Asynchronous results carry the version
    /// they were computed for and are dropped when it has moved on.
    pub(crate) fn version(&self) -> u64 {
        self.version_base + self.buf.changedtick
    }

    /// Replaces the text with a freshly loaded buffer (`:e`). The undo
    /// history and the per-document state start over, but the version keeps
    /// counting up, so a result for the old text is never taken as current.
    pub(crate) fn reload(&mut self, buf: Buffer) {
        let version_base = self.version() + 1;
        // The `'[` and `']` marks and the buffer-local options survive a
        // reload, as they did when they were kept in the editor.
        let (op_start, op_end) = (self.op_start, self.op_end);
        let local = self.local.clone();
        *self = Self {
            version_base,
            op_start,
            op_end,
            ..Self::with_local(buf, local)
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;

    fn version(ed: &Editor) -> u64 {
        ed.doc().version()
    }

    #[test]
    fn insert_and_delete_bump_version() {
        let mut ed = Editor::from_text("abc\n");
        let v0 = version(&ed);
        ed.feed("ix<Esc>");
        let v1 = version(&ed);
        assert!(v1 > v0, "insert: {v0} -> {v1}");
        ed.feed("x");
        let v2 = version(&ed);
        assert!(v2 > v1, "delete: {v1} -> {v2}");
    }

    #[test]
    fn undo_and_redo_bump_version() {
        let mut ed = Editor::from_text("abc\n");
        ed.feed("x");
        let v1 = version(&ed);
        ed.feed("u");
        let v2 = version(&ed);
        assert!(v2 > v1, "undo: {v1} -> {v2}");
        ed.feed("<C-r>");
        let v3 = version(&ed);
        assert!(v3 > v2, "redo: {v2} -> {v3}");
    }

    #[test]
    fn reload_keeps_version_going_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        let mut ed = Editor::from_text("abc\n");
        ed.feed("ix<Esc>");
        let before = version(&ed);
        ed.edit_file(&path);
        assert_eq!(ed.buf().lines(), ["one", "two"]);
        assert!(
            version(&ed) > before,
            "reload: {before} -> {}",
            version(&ed)
        );
    }

    #[test]
    fn cursor_motion_keeps_version() {
        let mut ed = Editor::from_text("abc\ndef\n");
        let v0 = version(&ed);
        ed.feed("llj$0wkh");
        assert_eq!(version(&ed), v0);
    }
}
