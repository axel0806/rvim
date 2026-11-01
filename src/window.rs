// SPDX-License-Identifier: Apache-2.0
//! The window: what is kept per view of a document (PLAN.md, "Document,
//! Window, Editor"). Step 1 has one window. It holds the cursor and the
//! desired column; the Visual start and the jumplist and scroll position
//! follow in later steps of the split.

use crate::document::DocId;
use crate::jumplist::JumpList;
use crate::motion::Pos;

/// Index of a window in `Editor::windows`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct WinId(pub(crate) usize);

#[derive(Default)]
pub(crate) struct Window {
    /// The document this window shows (Vim's `w_buffer`).
    pub(crate) doc: DocId,
    pub(crate) cursor: Pos,
    /// Desired virtual column for vertical motions; `usize::MAX` after `$`.
    pub(crate) curswant: usize,
    pub(crate) keep_curswant: bool,
    /// The other end of the Visual area (Vim's `VIsual`).
    pub(crate) vstart: Pos,
    /// The jumplist and the context mark (Vim's window state, see jumplist.rs).
    pub(crate) jumps: JumpList,
    /// First buffer line shown in the window (Vim's `w_topline`).
    pub(crate) top: usize,
    /// Text columns and rows of the window, from the screen size (the command
    /// line is not part of it). Set by `Editor::fit_windows`.
    pub(crate) width: usize,
    pub(crate) height: usize,
}
