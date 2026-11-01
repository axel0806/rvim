// SPDX-License-Identifier: Apache-2.0
//! Runs keys on a text and prints the result: `try "text" "keys"`.
use rvim::{Editor, Key, Mode};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let text = a[1].clone();
    let mut ed = Editor::from_text(&text);
    ed.feed(&a[2]);
    if ed.mode() != Mode::Normal || !ed.pending_keys().is_empty() {
        ed.handle_key(Key::Esc);
    }
    let c = ed.cursor();
    println!(
        "{:?} cursor=({}, {})",
        ed.buf().lines(),
        c.line + 1,
        c.col + 1
    );
    if let Some(m) = ed.message() {
        println!("msg: {}", m.text);
    }
}
