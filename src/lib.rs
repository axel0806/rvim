//! rvim: a Vim-compatible editor core. The library has no terminal
//! dependency; `main.rs` is the crossterm front end.

pub mod buffer;
pub mod editor;
mod ex;
pub mod key;
pub mod motion;
pub mod text;
pub mod view;

pub use editor::{Editor, Mode};
pub use key::Key;
