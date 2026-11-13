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
use crate::reload::{DiskState, Stale, Warned};
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
pub struct DocId(pub(crate) usize);

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
    /// `None` until a change or yank sets them, so an unset mark is E20.
    pub(crate) op_start: Option<Pos>,
    pub(crate) op_end: Option<Pos>,
    /// The buffer-local option values (`'autoindent'`, `'shiftwidth'`, ...).
    /// A new document copies the global values, see `options.rs`.
    pub(crate) local: Local,
    /// The file as the text was last read from it or written to it (PLAN.md,
    /// "No echo"). `None` for a buffer with no file, or a file that did not
    /// exist when it was read.
    pub(crate) disk: Option<DiskState>,
    /// The file has changed or gone since the text was read or written, and
    /// nothing has reloaded or written it since (set by `:checktime`).
    pub(crate) stale: Option<Stale>,
    /// What the watcher's warning last said about the file, so that the same
    /// change is warned about once (not for `:checktime`, which always says).
    pub(crate) warned: Option<Warned>,
    /// The text last known to be on disk: read from the file or written to
    /// it (PLAN.md, "Conflict prompt", the `[base]` text). The base of a
    /// three-way merge. `None` with no file on disk.
    pub(crate) baseline: Option<Vec<String>>,
    /// The disk state the conflict prompt's `k` answer was given for. The
    /// prompt does not come back for this state (conflict.rs).
    pub(crate) seen: Option<DiskState>,
    /// The text cannot be changed by the user: the agent log (agent_turn.rs).
    /// Changes typed in it are refused, as Vim refuses them with 'modifiable'
    /// off. Output the editor writes to it is not a user change.
    pub(crate) nomodifiable: bool,
    /// The review baseline when it is not `baseline` (review.rs): `None`
    /// while the two are the same text, which they are after `:e` and `:w`.
    /// A change from outside (a reload, the conflict prompt's `l` and `m`)
    /// moves `baseline` but not the review baseline, so the gutter shows what
    /// it changed until the next `:w`.
    review_own: Option<Option<Vec<String>>>,
    /// Goes up whenever a baseline changes, so a gutter computed for the old
    /// one is not taken as current (review.rs).
    pub(crate) baseline_gen: u64,
    /// The change gutter last computed for this document (review.rs).
    pub(crate) review: crate::review::ReviewCache,
    /// The `[base]` scratch document of this document, once made.
    pub(crate) base_doc: Option<DocId>,
    /// For a `[base]` document: the document it is the baseline of, and the
    /// `baseline_gen` its text was copied at.
    pub(crate) base_of: Option<(DocId, u64)>,
    /// The name of a scratch document with no file (`[base]`), shown where
    /// a file name would be.
    pub(crate) scratch_name: Option<&'static str>,
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
            op_start: None,
            op_end: None,
            local,
            disk: None,
            stale: None,
            warned: None,
            baseline: None,
            seen: None,
            nomodifiable: false,
            review_own: None,
            baseline_gen: 0,
            review: Default::default(),
            base_doc: None,
            base_of: None,
            scratch_name: None,
        }
    }

    /// Sets the text last known to be on disk (`baseline`). With `review`
    /// (`:e`, `:w`) the review baseline becomes the same text; without it (a
    /// change from outside) the review baseline stays what it was.
    pub(crate) fn set_baseline(&mut self, lines: Option<Vec<String>>, review: bool) {
        if review {
            self.review_own = None;
        } else if self.review_own.is_none() {
            self.review_own = Some(self.baseline.take());
        }
        self.baseline = lines;
        self.baseline_gen += 1;
    }

    /// The review baseline: the text last read by `:e` or written by `:w`
    /// (PLAN.md, "Per-hunk review", the `[base]` text). `None` with no file.
    pub(crate) fn review_base(&self) -> Option<&Vec<String>> {
        match &self.review_own {
            Some(own) => own.as_ref(),
            None => self.baseline.as_ref(),
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
        // The `[base]` document and the baseline counter stay: the counter
        // keeps going up, so a gutter for the old text is never current.
        let (base_doc, baseline_gen) = (self.base_doc, self.baseline_gen + 1);
        *self = Self {
            version_base,
            op_start,
            op_end,
            base_doc,
            baseline_gen,
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
