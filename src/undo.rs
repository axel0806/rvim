//! Undo tree. Every node holds the full text after its change as a rope
//! snapshot; ropes share structure, so this costs memory proportional to the
//! edits. Each change also records the line region it touched (Vim's undo
//! entry `ue_top`/`ue_bot`) and the cursor when it began (`uh_cursor`), so
//! the cursor after `u` / `<C-r>` follows Vim's `u_undoredo()`. Lowercase
//! marks are kept as Vim's `uh_namedm`: saved when a change starts, restored
//! by undo, and swapped with the current marks on each step.

use crate::buffer::{Marks, Snapshot};
use crate::motion::Pos;

struct Node {
    parent: usize,
    /// Text after this change.
    snap: Snapshot,
    /// Lines above the changed region.
    top: usize,
    /// Lines below the changed region (unchanged by the change).
    below: usize,
    /// Cursor when the change started.
    cursor: Pos,
    /// Child that `<C-r>` goes to (the most recently undone or created).
    redo_child: Option<usize>,
    /// Vim's UH_CHANGED: the 'modified' state to restore when this change
    /// is undone or redone (swapped on each use).
    changed: bool,
    /// The marks when the change started (swapped on each use).
    marks: Marks,
}

struct Open {
    top: usize,
    below: usize,
    cursor: Pos,
    changed: bool,
    marks: Marks,
}

/// What an undo or redo step did, for placing the cursor and the marks.
pub struct Step {
    pub snap: Snapshot,
    /// Text before the step (to compare changed lines).
    pub from: Snapshot,
    pub top: usize,
    pub below: usize,
    pub uh_cursor: Pos,
    /// The 'modified' state after the step.
    pub modified: bool,
    /// Marks to restore (those set when the change started).
    pub marks: Marks,
}

pub struct UndoTree {
    nodes: Vec<Node>,
    cur: usize,
    open: Option<Open>,
}

impl UndoTree {
    pub fn new(initial: Snapshot) -> Self {
        Self {
            nodes: vec![Node {
                parent: 0,
                snap: initial,
                top: 0,
                below: 0,
                cursor: Pos::default(),
                redo_child: None,
                changed: false,
                marks: [None; 26],
            }],
            cur: 0,
            open: None,
        }
    }

    /// Records that lines `first..first + count` of a buffer with `len`
    /// lines are about to change (Vim's `u_save`). Starts a change if none
    /// is open; `cursor`, `modified` and `marks` are remembered for the
    /// first one.
    pub fn save(
        &mut self,
        first: usize,
        count: usize,
        len: usize,
        cursor: Pos,
        modified: bool,
        marks: Marks,
    ) {
        let below = len.saturating_sub(first + count);
        match &mut self.open {
            Some(o) => {
                o.top = o.top.min(first);
                o.below = o.below.min(below);
            }
            None => {
                self.open = Some(Open {
                    top: first,
                    below,
                    cursor,
                    changed: modified,
                    marks,
                });
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Ends the open change (Vim's `u_sync`), recording `snap` as its result.
    pub fn sync(&mut self, snap: Snapshot) {
        let Some(o) = self.open.take() else { return };
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent: self.cur,
            snap,
            top: o.top,
            below: o.below,
            cursor: o.cursor,
            redo_child: None,
            changed: o.changed,
            marks: o.marks,
        });
        self.nodes[self.cur].redo_child = Some(id);
        self.cur = id;
    }

    /// One step back, or `None` at the oldest change. `modified` and
    /// `marks` are the current 'modified' state and marks.
    pub fn undo(&mut self, modified: bool, marks: Marks) -> Option<Step> {
        if self.cur == 0 {
            return None;
        }
        let restored = std::mem::replace(&mut self.nodes[self.cur].changed, modified);
        let saved = std::mem::replace(&mut self.nodes[self.cur].marks, marks);
        let node = &self.nodes[self.cur];
        let parent = node.parent;
        let step = Step {
            snap: self.nodes[parent].snap.clone(),
            from: node.snap.clone(),
            top: node.top,
            below: node.below,
            uh_cursor: node.cursor,
            modified: restored,
            marks: saved,
        };
        self.nodes[parent].redo_child = Some(self.cur);
        self.cur = parent;
        Some(step)
    }

    /// One step forward, or `None` at the newest change.
    pub fn redo(&mut self, modified: bool, marks: Marks) -> Option<Step> {
        let child = self.nodes[self.cur].redo_child?;
        let restored = std::mem::replace(&mut self.nodes[child].changed, modified);
        let saved = std::mem::replace(&mut self.nodes[child].marks, marks);
        let node = &self.nodes[child];
        let step = Step {
            snap: node.snap.clone(),
            from: self.nodes[self.cur].snap.clone(),
            top: node.top,
            below: node.below,
            uh_cursor: node.cursor,
            modified: restored,
            marks: saved,
        };
        self.cur = child;
        Some(step)
    }

    /// The text was written: undoing or redoing any change now makes the
    /// buffer modified (Vim's `u_unchanged()`).
    pub fn mark_saved(&mut self) {
        for n in &mut self.nodes {
            n.changed = true;
        }
    }

    pub fn seq(&self) -> usize {
        self.cur
    }
}
