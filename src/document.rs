// SPDX-License-Identifier: Apache-2.0
//! The document: the text and what is kept with it, shared by every window
//! that shows it (PLAN.md, "Document, Window, Editor"). Step 1 moved the text,
//! the undo tree, the undo line and the last Visual area here. Marks are still
//! in `Buffer` and `Editor`, and file state is still in `Buffer`.

use crate::buffer::Buffer;
use crate::editor::LastVisual;
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
}

impl Document {
    pub(crate) fn new(buf: Buffer) -> Self {
        let undo = UndoTree::new(buf.snapshot());
        Self {
            buf,
            undo,
            uline: None,
            last_visual: None,
        }
    }
}
