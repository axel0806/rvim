// SPDX-License-Identifier: Apache-2.0
//! Start menu tests (PLAN.md, "Start menu", "Testing"): the switch, the
//! Frame layout and styles, the dismissal table, resize and paste. Without
//! the option the editor must behave as before, so the non-menu screens are
//! compared with a plain editor.

use rvim::view::{self, Frame, Hl};
use rvim::{Editor, Event, Mode, VERSION_MESSAGE};
use std::path::Path;

const HINT: &str = "arrows select  Enter open  Esc close";

/// An empty editor with the menu on, shown at `cols` x `rows`.
fn shown(cols: usize, rows: usize) -> Editor {
    let mut ed = Editor::new();
    ed.enable_start_menu();
    assert!(ed.show_start_menu());
    ed.set_screen_size(cols, rows);
    ed
}

/// The same keys with no menu and the same window.
fn plain(cols: usize, rows: usize, keys: &str) -> Editor {
    let mut ed = Editor::new();
    ed.set_screen_size(cols, rows);
    ed.feed(keys);
    ed
}

fn row_of(f: &Frame, label: &str) -> usize {
    f.rows
        .iter()
        .position(|r| r.trim() == label)
        .unwrap_or_else(|| panic!("no row {label:?} in {:?}", f.rows))
}

/// The trimmed text of the row whose cells are all `MenuSelected`.
fn selected(f: &Frame) -> String {
    f.rows
        .iter()
        .zip(&f.hl)
        .find(|(_, h)| !h.is_empty() && h.iter().all(|x| *x == Hl::MenuSelected))
        .map(|(r, _)| r.trim().to_string())
        .unwrap_or_default()
}

/// The screen rows, trimmed and joined: the version message reads back
/// exactly as `VERSION_MESSAGE` when the rows are joined with spaces.
fn joined(f: &Frame) -> String {
    f.rows
        .iter()
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn text(ed: &Editor) -> String {
    (0..ed.buf().line_count())
        .map(|i| ed.buf().line(i).chars().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn shown_with_the_flag_at_80x24() {
    let ed = shown(80, 24);
    let f = view::frame(&ed);
    assert_eq!(f.rows.len(), 23);
    assert!(f.rows.iter().all(|r| r.chars().count() == 80));
    // The logo sits above the version line, the items below it.
    let logo = row_of(&f, "| rvim |");
    assert_eq!(f.rows[logo - 1].trim(), "+------+");
    assert!(joined(&f).contains(VERSION_MESSAGE));
    let new = row_of(&f, "New file");
    assert_eq!(row_of(&f, "Open file"), new + 1);
    assert_eq!(row_of(&f, "Help"), new + 2);
    assert_eq!(row_of(&f, "Quit"), new + 3);
    assert_eq!(row_of(&f, HINT), new + 5);
    assert!(logo < new);
    // Items share a left edge; the cursor sits on the first letter of the
    // selected item.
    let col = f.rows[new].find("New file").unwrap();
    assert_eq!(f.rows[new + 1].find("Open file"), Some(col));
    assert_eq!(f.cursor, (col, new));
    // Styles: the selected row is all MenuSelected, the unavailable Help
    // row is all Dimmed, the others are Normal.
    assert_eq!(selected(&f), "New file");
    assert!(f.hl[new].iter().all(|h| *h == Hl::MenuSelected));
    assert!(f.hl[new + 1].iter().all(|h| *h == Hl::Normal));
    assert!(f.hl[new + 2].iter().all(|h| *h == Hl::Dimmed));
    // The buffer is untouched.
    assert_eq!(text(&ed), "");
    assert!(ed.cursor() == Editor::new().cursor());
}

#[test]
fn narrow_and_short_windows_hide_the_logo() {
    // 40 columns: the logo is hidden, the version message is still shown.
    let f = view::frame(&shown(40, 20));
    assert_eq!(f.rows.len(), 19);
    assert!(f.rows.iter().all(|r| r.chars().count() == 40));
    assert!(!f.rows.iter().any(|r| r.contains("rvim |")));
    assert!(joined(&f).contains(VERSION_MESSAGE));
    assert_eq!(selected(&f), "New file");
    // 60 columns but 19 rows: still no logo.
    let f = view::frame(&shown(60, 19));
    assert!(!f.rows.iter().any(|r| r.contains("rvim |")));
    // 60 columns and 20 rows: the logo is back.
    let f = view::frame(&shown(60, 20));
    assert!(f.rows.iter().any(|r| r.trim() == "| rvim |"));
    assert!(joined(&f).contains(VERSION_MESSAGE));
}

#[test]
fn off_without_the_flag() {
    let mut ed = Editor::new();
    // No flag: the menu does not show, and the intro message is shown as
    // before.
    assert!(!ed.show_start_menu());
    assert!(!ed.start_menu_shown());
    ed.show_intro();
    ed.set_screen_size(80, 24);
    let f = view::frame(&ed);
    assert!(
        !f.rows
            .iter()
            .any(|r| r.contains("New file") || r.contains("rvim |"))
    );
    assert!(f.status.starts_with("rvim "));
    assert_eq!(f.rows[1].trim_end(), "~");
}

#[test]
fn not_shown_with_a_file_argument() {
    // A missing file opens as an empty buffer with a name.
    let path = Path::new("rvim-start-menu-test-no-such-file.txt");
    let mut ed = Editor::open(path);
    ed.enable_start_menu();
    assert!(!ed.show_start_menu());
    assert!(!ed.start_menu_shown());
}

#[test]
fn not_shown_with_text_or_a_modified_buffer() {
    let mut ed = Editor::from_text("abc");
    ed.enable_start_menu();
    assert!(!ed.show_start_menu());

    // Typed and deleted again: empty, but modified.
    let mut ed = Editor::new();
    ed.enable_start_menu();
    ed.feed("ix<BS><Esc>");
    assert_eq!(text(&ed), "");
    assert!(!ed.show_start_menu());
}

#[test]
fn esc_closes_and_does_nothing_else() {
    let mut ed = shown(80, 24);
    ed.feed("<Down>");
    ed.feed("<Esc>");
    assert!(!ed.start_menu_shown());
    assert!(!ed.should_quit());
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(text(&ed), "");
    let want = view::frame(&plain(80, 24, ""));
    let got = view::frame(&ed);
    assert_eq!(got.rows, want.rows);
    assert_eq!(got.status, want.status);
    assert_eq!(got.cursor, want.cursor);
    assert_eq!(got.hl, want.hl);
}

#[test]
fn up_and_down_move_the_selection_and_skip_unavailable_items() {
    let mut ed = shown(80, 24);
    ed.feed("<Up>");
    assert_eq!(selected(&view::frame(&ed)), "New file");
    ed.feed("<Down>");
    assert_eq!(selected(&view::frame(&ed)), "Open file");
    // Help is dimmed and skipped.
    ed.feed("<Down>");
    assert_eq!(selected(&view::frame(&ed)), "Quit");
    // The last item stays selected.
    ed.feed("<Down><Down>");
    assert_eq!(selected(&view::frame(&ed)), "Quit");
    ed.feed("<Up>");
    assert_eq!(selected(&view::frame(&ed)), "Open file");
    assert!(ed.start_menu_shown());
}

#[test]
fn enter_on_new_file_closes_the_menu() {
    let mut ed = shown(80, 24);
    ed.feed("<CR>");
    assert!(!ed.start_menu_shown());
    assert!(!ed.should_quit());
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(text(&ed), "");
}

#[test]
fn enter_on_open_file_puts_e_on_the_command_line() {
    let mut ed = shown(80, 24);
    ed.feed("<Down><CR>");
    assert!(!ed.start_menu_shown());
    assert_eq!(ed.mode(), Mode::CmdLine);
    assert_eq!(ed.cmdline(), "e ");
    // Esc cancels the command line and leaves the buffer alone.
    ed.feed("<Esc>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(text(&ed), "");
}

#[test]
fn enter_on_quit_quits() {
    let mut ed = shown(80, 24);
    ed.feed("<Down><Down><CR>");
    assert!(!ed.start_menu_shown());
    assert!(ed.should_quit());
}

#[test]
fn other_keys_close_the_menu_and_act_as_without_it() {
    let keys = [
        "j", "3j", "ix<Esc>", ":", "/", "<C-g>", "<Left>", "<F2>", "<Insert>", "u", "p", "gg", "x",
        "<C-l>", "v",
    ];
    for keys in keys {
        let mut with = shown(80, 24);
        with.feed(keys);
        let without = plain(80, 24, keys);
        assert!(!with.start_menu_shown(), "menu still shown after {keys:?}");
        assert_eq!(text(&with), text(&without), "text after {keys:?}");
        assert_eq!(with.cursor(), without.cursor(), "cursor after {keys:?}");
        assert_eq!(with.mode(), without.mode(), "mode after {keys:?}");
        assert_eq!(with.cmdline(), without.cmdline(), "cmdline after {keys:?}");
        assert_eq!(with.should_quit(), without.should_quit());
        let (a, b) = (view::frame(&with), view::frame(&without));
        assert_eq!(a.rows, b.rows, "rows after {keys:?}");
        assert_eq!(a.status, b.status, "status after {keys:?}");
        assert_eq!(a.cursor, b.cursor, "cursor after {keys:?}");
        assert_eq!(a.hl, b.hl, "styles after {keys:?}");
    }
}

#[test]
fn a_count_typed_after_the_menu_starts_its_command() {
    // `3ix<Esc>`: the `3` closes the menu and starts the count.
    let mut ed = shown(80, 24);
    ed.feed("3ix<Esc>");
    assert_eq!(text(&ed), "xxx");
}

#[test]
fn paste_closes_the_menu_and_is_inserted() {
    let mut ed = shown(80, 24);
    ed.feed("<Down>");
    ed.paste("abc");
    assert!(!ed.start_menu_shown());
    assert_eq!(text(&ed), "abc");
}

#[test]
fn resize_keeps_the_menu_and_its_selection() {
    let mut ed = shown(80, 24);
    ed.feed("<Down>");
    ed.handle_event(Event::Resize(40, 20));
    assert!(ed.start_menu_shown());
    let f = view::frame(&ed);
    assert_eq!(f.rows.len(), 19);
    assert!(f.rows.iter().all(|r| r.chars().count() == 40));
    assert!(!f.rows.iter().any(|r| r.contains("rvim |")));
    assert_eq!(selected(&f), "Open file");

    ed.handle_event(Event::Resize(80, 24));
    let f = view::frame(&ed);
    assert!(f.rows.iter().any(|r| r.trim() == "| rvim |"));
    assert_eq!(selected(&f), "Open file");
    assert!(ed.start_menu_shown());
}
