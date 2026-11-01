// SPDX-License-Identifier: Apache-2.0
//! rvim: a Vim-compatible editor core. The library has no terminal
//! dependency; `main.rs` is the crossterm front end.

pub mod acp;
pub mod agent_approval;
pub mod agent_conn;
pub mod agent_detect;
pub mod agent_scan;
pub mod agent_session;
pub mod buffer;
pub mod cli;
mod cmdline;
pub mod config;
pub(crate) mod conflict;
pub(crate) mod document;
pub mod editor;
pub mod event;
pub(crate) mod ex;
pub mod expr;
mod jumplist;
pub mod key;
mod macros;
mod marks;
mod merge;
pub mod motion;
mod normal;
mod ops;
pub mod options;
pub mod recovery;
pub mod redraw;
pub mod regex;
pub mod register;
pub(crate) mod reload;
mod scroll;
mod search;
pub mod signals;
mod startmenu;
pub mod text;
pub mod textobject;
mod undo;
pub mod view;
pub mod watch;
pub(crate) mod window;

pub use document::DocId;
pub use editor::{Editor, Mode, VisualMode};
pub use event::Event;

/// Short modified-version notice for the message line: the intro message and
/// `:version`. The message line shows at most `width - 19` cells (the ruler
/// starts at `width - 18`), so 80 columns show 61 characters. At most 40
/// characters, so that a config note (`[N config warnings]`, up to 21
/// characters) still fits. Vim license condition II.3; the owner should
/// confirm the wording (PLAN.md, "Licensing").
pub const NOTICE_SHORT: &str = concat!(
    "rvim ",
    env!("CARGO_PKG_VERSION"),
    ": modified Vim (not official)"
);

/// The full modified-version notice: `--version` (stdout) and the start menu
/// body, where it is not cut. rvim contains code derived from Vim, and the
/// Vim license (condition II.3) requires telling users that this is a
/// modified version.
pub const VERSION_MESSAGE: &str = concat!(
    "rvim ",
    env!("CARGO_PKG_VERSION"),
    " - a modified version of Vim, reimplemented in Rust (not the official Vim). ",
    "Vim license; report issues at https://github.com/axel0806/rvim/issues"
);
pub use key::Key;
