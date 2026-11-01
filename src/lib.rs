// SPDX-License-Identifier: Apache-2.0
//! rvim: a Vim-compatible editor core. The library has no terminal
//! dependency; `main.rs` is the crossterm front end.

pub mod buffer;
mod cmdline;
pub mod editor;
pub mod event;
pub(crate) mod ex;
mod jumplist;
pub mod key;
mod macros;
pub mod motion;
mod normal;
mod ops;
pub mod options;
pub mod recovery;
pub mod regex;
pub mod register;
mod scroll;
mod search;
mod startmenu;
pub mod text;
pub mod textobject;
mod undo;
pub mod view;

pub use editor::{Editor, Mode, VisualMode};
pub use event::Event;

/// Shown by `:version` and at startup. rvim contains code derived from Vim,
/// and the Vim license (condition II.3) requires telling users that this is
/// a modified version.
pub const VERSION_MESSAGE: &str = concat!(
    "rvim ",
    env!("CARGO_PKG_VERSION"),
    " - a modified version of Vim, reimplemented in Rust (not the official Vim). ",
    "Vim license; report issues at https://github.com/axel0806/rvim/issues"
);
pub use key::Key;
