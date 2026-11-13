// SPDX-License-Identifier: Apache-2.0
//! Milestone 7 step 3b (PLAN.md, "Review gutter, step 3b"): the change
//! gutter against the review baseline, the `[base]` scratch document,
//! `:AgentChanges` and the `[deleted]` mark in the status line. All of it is
//! rvim's own (Vim has no counterpart), so it is checked on the Frame and the
//! editor state, not against Vim. With no agent opt-in the frame must be the
//! one Vim would show: no gutter and no mark.

use rvim::Editor;
use rvim::Event;
use rvim::view::{Hl, frame};
use std::fs;
use std::path::PathBuf;

/// `s` padded with spaces to `width` cells, as the Frame rows are.
fn row(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

/// An editor on a file with `text` in a new temp dir, on a 12x5 screen, with
/// the agent opt-in set to `opt_in`.
fn open(dir: &tempfile::TempDir, text: &str, opt_in: bool) -> (Editor, PathBuf) {
    let path = dir.path().join("f.txt");
    fs::write(&path, text).unwrap();
    let mut ed = Editor::open(&path);
    ed.set_agent_enabled(opt_in);
    ed.set_screen_size(12, 5);
    (ed, path)
}

fn message(ed: &Editor) -> String {
    ed.message().map(|m| m.text.clone()).unwrap_or_default()
}

fn cursor(ed: &Editor) -> (usize, usize) {
    let c = ed.cursor();
    (c.line, c.col)
}

#[test]
fn no_opt_in_no_gutter() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", false);
    ed.feed("jix<Esc>");
    let f = frame(&ed);
    assert_eq!(
        f.rows,
        [row("a", 12), row("xb", 12), row("c", 12), row("~", 12)]
    );
    assert_eq!(f.cursor, (0, 1));
    assert!(f.hl.iter().flatten().all(|h| *h != Hl::Changed));
    assert!(ed.review_signs().is_empty());
}

#[test]
fn opt_in_unchanged_text_has_no_gutter() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", true);
    ed.feed("jl");
    let f = frame(&ed);
    assert_eq!(
        f.rows,
        [row("a", 12), row("b", 12), row("c", 12), row("~", 12)]
    );
}

#[test]
fn changed_line_gets_a_sign_left_of_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", true);
    ed.feed("jix<Esc>");
    let f = frame(&ed);
    assert_eq!(
        f.rows,
        [
            row("  a", 12),
            row("~ xb", 12),
            row("  c", 12),
            row("  ~", 12)
        ]
    );
    // The cursor is after the gutter.
    assert_eq!(f.cursor, (2, 1));
    // The sign and its space are the Changed group; the other gutter cells
    // are Normal, and the filler `~` keeps NonText after the gutter.
    assert_eq!(f.hl[1][..2], [Hl::Changed; 2]);
    assert_eq!(f.hl[0][..2], [Hl::Normal; 2]);
    assert_eq!(f.hl[3][..2], [Hl::Normal; 2]);
    assert_eq!(f.hl[3][2], Hl::NonText);
}

#[test]
fn gutter_is_left_of_the_number_column() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", true);
    ed.feed(":set nu<CR>jix<Esc>");
    let f = frame(&ed);
    assert_eq!(
        f.rows,
        [
            row("    1 a", 12),
            row("~   2 xb", 12),
            row("    3 c", 12),
            row("      ~", 12)
        ]
    );
    assert_eq!(f.cursor, (6, 1));
    assert_eq!(f.hl[1][..2], [Hl::Changed; 2]);
    assert_eq!(f.hl[1][2..6], [Hl::CursorLineNr; 4]);
    assert_eq!(f.hl[0][2..6], [Hl::LineNr; 4]);
}

#[test]
fn added_and_deleted_lines() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", true);
    ed.feed("onew<Esc>");
    assert_eq!(ed.review_signs(), [(1, '+')]);
    ed.feed("u");
    assert!(ed.review_signs().is_empty());
    ed.feed("jdd");
    assert_eq!(ed.review_signs(), [(0, '_')]);
    ed.feed("uggdd");
    assert_eq!(ed.review_signs(), [(0, '^')]);
    let f = frame(&ed);
    assert_eq!(f.rows[0], row("^ b", 12));
}

#[test]
fn wrapped_lines_wrap_in_the_narrower_text_area() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\n", true);
    // 12 columns minus the gutter leave 10 for the text.
    ed.feed("A0123456789xy<Esc>");
    let f = frame(&ed);
    assert_eq!(
        f.rows,
        [
            row("~ a012345678", 12),
            row("  9xy", 12),
            row("  b", 12),
            row("  ~", 12)
        ]
    );
}

#[test]
fn write_resets_the_baseline_and_the_gutter_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", true);
    ed.feed("jix<Esc>");
    assert_eq!(ed.review_signs(), [(1, '~')]);
    ed.feed(":w<CR>");
    assert!(ed.review_signs().is_empty());
    assert_eq!(frame(&ed).rows[1], row("xb", 12));
}

#[test]
fn a_reload_from_disk_is_shown_until_the_next_write() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, path) = open(&dir, "a\nb\nc\n", true);
    // Another program (an agent) changes the file; it is reloaded.
    fs::write(&path, "a\nB\nc\nd\n").unwrap();
    ed.feed(":set autoread<CR>:checktime<CR>");
    assert_eq!(ed.buf().lines(), ["a", "B", "c", "d"]);
    assert!(!ed.buf().modified);
    assert_eq!(ed.review_signs(), [(1, '~'), (3, '+')]);
    // `u` takes the reload back, and with it the signs.
    ed.feed("u");
    assert!(ed.review_signs().is_empty());
    ed.feed("<C-r>:w<CR>");
    assert!(ed.review_signs().is_empty());
}

#[test]
fn large_diffs_run_on_a_worker_and_stale_results_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..1000).map(|i| format!("l{i}\n")).collect();
    let (mut ed, _) = open(&dir, &text, true);
    // 600 changed lines: a 360,000-cell table, over the synchronous bound.
    ed.feed(":200,799s/^/x/<CR>");
    assert!(
        ed.review_signs().is_empty(),
        "nothing until the worker is back"
    );
    let mut jobs = ed.take_review_jobs();
    assert_eq!(jobs.len(), 1);
    let first = jobs.pop().unwrap();
    // The text changes before the result arrives: the result is dropped and
    // a new diff is asked for.
    ed.feed("ggx");
    assert!(
        ed.take_review_jobs().is_empty(),
        "one job at a time per document"
    );
    ed.handle_event(first.run());
    assert!(ed.review_signs().is_empty(), "a stale result is not used");
    let mut jobs = ed.take_review_jobs();
    assert_eq!(jobs.len(), 1);
    ed.handle_event(jobs.pop().unwrap().run());
    let signs = ed.review_signs();
    assert_eq!(signs.len(), 601);
    assert_eq!(signs[0], (0, '~'));
    assert_eq!(signs[1], (199, '~'));
    assert_eq!(signs[600], (798, '~'));
}

#[test]
fn small_diffs_need_no_worker() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..1000).map(|i| format!("l{i}\n")).collect();
    let (mut ed, _) = open(&dir, &text, true);
    ed.feed(":200,299s/^/x/<CR>");
    assert!(ed.take_review_jobs().is_empty());
    assert_eq!(ed.review_signs().len(), 100);
}

#[test]
fn base_shows_the_baseline_and_cannot_be_changed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\nc\n", true);
    ed.feed("jix<Esc>:AgentBase<CR>");
    assert_eq!(ed.buf().lines(), ["a", "b", "c"]);
    assert_eq!(
        message(&ed),
        format!(
            "\"[base]\" 3 lines, the baseline of \"{}\"",
            dir.path().join("f.txt").display()
        )
    );
    // [base] has no gutter of its own.
    let f = frame(&ed);
    assert_eq!(
        f.rows,
        [row("a", 12), row("b", 12), row("c", 12), row("~", 12)]
    );
    // Not modifiable: changes are refused as Vim refuses them.
    ed.feed("x");
    assert_eq!(
        message(&ed),
        "E21: Cannot make changes, 'modifiable' is off"
    );
    ed.feed("dd");
    assert_eq!(ed.buf().lines(), ["a", "b", "c"]);
    ed.feed(":s/b/z/<CR>");
    assert_eq!(
        message(&ed),
        "E21: Cannot make changes, 'modifiable' is off"
    );
    // Never written.
    ed.feed(":w<CR>");
    assert_eq!(message(&ed), "E382: Cannot write, 'buftype' option is set");
    assert!(!ed.buf().modified);
    // Its name is [base].
    ed.feed("<C-g>");
    assert!(message(&ed).starts_with("\"[base]\""), "{}", message(&ed));
    // Back to the document, with its change.
    ed.feed(":AgentBase!<CR>");
    assert_eq!(ed.buf().lines(), ["a", "xb", "c"]);
    assert_eq!(ed.review_signs(), [(1, '~')]);
}

#[test]
fn base_follows_writes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\n", true);
    ed.feed("ix<Esc>:w<CR>:AgentBase<CR>");
    assert_eq!(ed.buf().lines(), ["xa", "b"]);
    ed.feed(":AgentBase!<CR>ddiy<Esc>:AgentBase<CR>");
    assert_eq!(ed.buf().lines(), ["xa", "b"]);
}

#[test]
fn base_needs_a_file() {
    let mut ed = Editor::from_text("a\nb");
    ed.feed(":AgentBase<CR>");
    assert_eq!(
        message(&ed),
        "\"[No Name]\" has no baseline: it was not read from or written to a file"
    );
    assert_eq!(ed.buf().lines(), ["a", "b"]);
    ed.feed(":AgentBase!<CR>");
    assert_eq!(message(&ed), "no document to go back to");
}

#[test]
fn base_cursor_and_desired_column() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "abcdefgh\nab\nabcdefgh\n", true);
    // :AgentBase moves the cursor to column 0 of the same line number and
    // resets the desired column: `j` stays in column 0.
    ed.feed("llll:AgentBase<CR>");
    assert_eq!(cursor(&ed), (0, 0));
    ed.feed("j");
    assert_eq!(cursor(&ed), (1, 0));
    // :AgentBase! puts the cursor back where it was (line 0, column 4) and
    // resets the desired column to it: `jj` goes to column 1, then 4.
    ed.feed(":AgentBase!<CR>");
    assert_eq!(cursor(&ed), (0, 4));
    ed.feed("j");
    assert_eq!(cursor(&ed), (1, 1));
    ed.feed("j");
    assert_eq!(cursor(&ed), (2, 4));
}

#[test]
fn deleted_mark_in_the_status_line() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, path) = open(&dir, "a\n", true);
    ed.set_screen_size(40, 5);
    fs::remove_file(&path).unwrap();
    ed.feed(":checktime<CR>");
    let f = frame(&ed);
    // The mark is right of the message and left of the ruler (column 22).
    assert_eq!(&f.status[12..22], "[deleted] ", "{:?}", f.status);
    // `:w` writes the file again and the mark goes.
    ed.feed(":w<CR>");
    assert!(!frame(&ed).status.contains("[deleted]"));
}

#[test]
fn no_opt_in_no_deleted_mark() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, path) = open(&dir, "a\n", false);
    ed.set_screen_size(40, 5);
    fs::remove_file(&path).unwrap();
    ed.feed(":checktime<CR>");
    let f = frame(&ed);
    assert!(!f.status.contains("[deleted]"), "{:?}", f.status);
    assert!(message(&ed).starts_with("E211"), "{}", message(&ed));
}

#[test]
fn agent_changes_lists_the_steps_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, path) = open(&dir, "a\nb\nc\n", false);
    ed.feed(":AgentChanges<CR>");
    assert_eq!(message(&ed), "No changes");
    ed.feed("jddonew<Esc>");
    ed.feed(":AgentChanges<CR>");
    let msg = message(&ed);
    let rows: Vec<&str> = msg.split(" | ").collect();
    assert_eq!(rows.len(), 2, "{msg}");
    let name = path.display().to_string();
    // `#seq age source file +added -removed`; the age is a few seconds.
    let (head, tail) = rows[0].split_once(" ago ").unwrap();
    assert!(head.starts_with("#2 ") && head.ends_with('s'), "{msg}");
    assert_eq!(tail, format!("user {name} +1 -0"));
    let (head, tail) = rows[1].split_once(" ago ").unwrap();
    assert!(head.starts_with("#1 "), "{msg}");
    assert_eq!(tail, format!("user {name} +0 -1"));
}

#[test]
fn agent_changes_shows_external_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, path) = open(&dir, "a\nb\n", true);
    fs::write(&path, "a\nB\nc\n").unwrap();
    ed.feed(":set autoread<CR>:checktime<CR>:AgentChanges<CR>");
    let msg = message(&ed);
    assert!(
        msg.ends_with(&format!("external {} +2 -1", path.display())),
        "{msg}"
    );
}

#[test]
fn agent_changes_bang_lists_every_document() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "a\nb\n", true);
    ed.feed("x:AgentChanges!<CR>");
    assert!(message(&ed).starts_with("#1 "), "{}", message(&ed));
    // The [base] document is not listed (it has no changes of its own).
    ed.feed(":AgentBase<CR>:AgentChanges!<CR>");
    assert_eq!(message(&ed).split(" | ").count(), 1);
}

#[test]
fn agent_changes_jump_and_desired_column() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, _) = open(&dir, "abcdefgh\nab\nabcdefgh\n  xyzwvu\nabcdefgh\n", false);
    // Listing does not move the cursor and keeps the desired column:
    // `llllj` leaves column 1 with column 4 wanted, and the next `j` gets it.
    ed.feed("llllj:AgentChanges<CR>j");
    assert_eq!(cursor(&ed), (2, 4));
    // A change on line 4 (index 3), step 1.
    ed.feed("jx");
    // The jump goes to the first changed line, on its first non-blank, and
    // resets the desired column: `j` from there stays in column 2.
    ed.feed("gglllll:AgentChanges 1<CR>");
    assert_eq!(cursor(&ed), (3, 2));
    ed.feed("j");
    assert_eq!(cursor(&ed), (4, 2));
    // It is a jump: `''` goes back to where it came from.
    ed.feed(":AgentChanges 1<CR>''");
    assert_eq!(cursor(&ed), (4, 0));
    // Errors keep the cursor and the desired column.
    ed.feed("gglllllj:AgentChanges 9<CR>");
    assert_eq!(message(&ed), "E830: Undo number 9 not found");
    ed.feed("j");
    assert_eq!(cursor(&ed), (2, 5));
    ed.feed(":AgentChanges x<CR>");
    assert_eq!(message(&ed), "E474: Invalid argument: x");
}

#[test]
fn review_events_for_unknown_documents_are_ignored() {
    let mut ed = Editor::from_text("a");
    ed.handle_event(Event::ReviewDiff {
        doc: rvim::DocId::default(),
        version: 999,
        base_gen: 999,
        hunks: Vec::new(),
    });
    assert!(ed.review_signs().is_empty());
}
