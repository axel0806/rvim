// SPDX-License-Identifier: Apache-2.0
//! The modified-version notice on the message line (PLAN.md, "Licensing"):
//! the short notice is complete at 80 and 100 columns with and without config
//! warnings, `:version` shows it, and the full text stays in `--version` and
//! the start menu.

use rvim::view::{self, Frame};
use rvim::{Editor, VERSION_MESSAGE};

/// The message line cannot show more than this many cells: the ruler starts
/// at `width - 18` and the message is cut one cell before it.
fn message_cells(width: usize) -> usize {
    width - 19
}

/// An editor at `width` x 24 with the intro message and `warnings` config
/// warnings noted, as `main.rs` sets them up without a start menu.
fn intro(width: usize, warnings: usize) -> (Editor, Frame) {
    let mut ed = Editor::new();
    ed.set_screen_size(width, 24);
    ed.set_config_warnings(vec![
        "config: line 1: unknown key \"x\"".to_string();
        warnings
    ]);
    ed.show_intro();
    ed.show_config_notice();
    let f = view::frame(&ed);
    (ed, f)
}

#[test]
fn the_short_notice_names_the_program_the_version_and_the_modification() {
    let n = rvim::NOTICE_SHORT;
    assert!(
        n.chars().count() <= 40,
        "{n} is {} cells",
        n.chars().count()
    );
    assert!(n.starts_with("rvim "), "{n}");
    assert!(n.contains(env!("CARGO_PKG_VERSION")), "{n}");
    assert!(n.contains("modified"), "{n}");
    assert!(n.contains("Vim"), "{n}");
    assert!(n.contains("not official"), "{n}");
}

#[test]
fn the_intro_notice_is_complete_on_the_message_row() {
    for width in [80, 100] {
        for warnings in [0, 1, 99] {
            let (ed, f) = intro(width, warnings);
            let expected = if warnings == 0 {
                rvim::NOTICE_SHORT.to_string()
            } else {
                let s = if warnings == 1 { "" } else { "s" };
                format!("{} [{warnings} config warning{s}]", rvim::NOTICE_SHORT)
            };
            let shown = ed.message().expect("a message").text.clone();
            assert_eq!(shown, expected, "width {width}, {warnings} warnings");
            assert!(
                expected.chars().count() <= message_cells(width),
                "width {width}, {warnings} warnings: {} cells",
                expected.chars().count()
            );
            // Not cut: the whole text is at the start of the message row.
            assert!(
                f.status.starts_with(&expected),
                "width {width}, {warnings} warnings: {:?}",
                f.status
            );
            assert_eq!(f.status.chars().count(), width);
        }
    }
}

#[test]
fn the_note_stays_within_the_message_cells_for_up_to_99_warnings() {
    let (ed, _) = intro(80, 99);
    let shown = ed.message().unwrap().text.clone();
    assert_eq!(
        shown,
        format!("{} [99 config warnings]", rvim::NOTICE_SHORT)
    );
    assert_eq!(shown.chars().count(), 60);
    assert!(shown.chars().count() <= message_cells(80));
}

#[test]
fn version_command_shows_the_short_notice_on_the_message_row() {
    for width in [80, 100] {
        let mut ed = Editor::new();
        ed.set_screen_size(width, 24);
        ed.feed(":version<CR>");
        let f = view::frame(&ed);
        assert!(f.status.starts_with(rvim::NOTICE_SHORT), "{:?}", f.status);
        assert!(!f.status_is_error);
    }
}

#[test]
fn the_file_info_message_gets_only_the_short_note() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notice.txt");
    std::fs::write(&path, "abc\n").unwrap();
    let mut ed = Editor::open(&path);
    ed.set_config_warnings(vec!["config: bad".to_string()]);
    let before = ed.message().expect("file info").text.clone();
    ed.show_config_notice();
    let after = ed.message().unwrap().text.clone();
    assert_eq!(after, format!("{before} [1 config warning]"));
}

#[test]
fn version_flag_prints_the_full_notice() {
    let text = rvim::cli::version_text();
    assert_eq!(text, format!("{VERSION_MESSAGE}\n"));
    assert!(text.contains("modified"), "{text}");
    assert!(text.contains("Vim"), "{text}");
    assert!(
        text.contains("https://github.com/axel0806/rvim/issues"),
        "{text}"
    );
}

#[test]
fn the_start_menu_draws_the_full_notice_at_80_and_100_columns() {
    for width in [80, 100] {
        let mut ed = Editor::new();
        ed.enable_start_menu();
        assert!(ed.show_start_menu());
        ed.set_screen_size(width, 24);
        let f = view::frame(&ed);
        let text = f
            .rows
            .iter()
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains(VERSION_MESSAGE), "width {width}: {text}");
    }
}
