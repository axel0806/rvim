// SPDX-License-Identifier: Apache-2.0
//! 'hlsearch', 'incsearch' and `:nohlsearch` checked against real Vim. The
//! highlight itself is not visible to the harness (it compares text and
//! cursor), so these cases cover what the highlight does to the editing: the
//! option forms, `:noh` and the search that follows it, and an incsearch
//! prompt that is answered. The screen is covered in tests/search_highlight.rs.

mod common;
use common::{check, cross};
use rvim::Editor;
use rvim::view::{Hl, frame};
use std::process::Command;

const TEXT: &str = "foo bar\nqux foo\nlast foo line\n";

#[test]
fn nohlsearch_does_not_move_the_cursor_or_change_text() {
    check(&[
        (TEXT, ":set hls<CR>/foo<CR>:noh<CR>x"),
        (TEXT, ":set hls<CR>/foo<CR>:nohl<CR>nx"),
        (TEXT, ":set hls<CR>/foo<CR>:nohlsearch<CR>Nx"),
        (TEXT, ":set hls<CR>/bar<CR>:noh<CR>:noh<CR>x"),
    ]);
}

#[test]
fn a_search_after_nohlsearch_moves_as_usual() {
    check(&[
        (TEXT, ":set hls<CR>/foo<CR>:noh<CR>nx"),
        (TEXT, ":set hls<CR>:noh<CR>/qux<CR>x"),
        (TEXT, ":set hls<CR>/foo<CR>:noh<CR>*x"),
    ]);
}

#[test]
fn hlsearch_option_forms_do_not_change_text() {
    check(&[
        (TEXT, ":set hls<CR>/qux<CR>x"),
        (TEXT, ":set nohls<CR>/qux<CR>x"),
        (TEXT, ":set invhls<CR>/qux<CR>x"),
        (TEXT, ":set invhls<CR>:set invhls<CR>/qux<CR>x"),
        (TEXT, ":set hls!<CR>:set hls&<CR>/qux<CR>x"),
        (TEXT, ":set hls?<CR>x"),
    ]);
}

#[test]
fn every_text_with_hlsearch_and_incsearch_keys() {
    let texts = [TEXT, "foo\n", "", "ab\n\ncd"];
    let keys = [":set hls<CR>/foo<CR>:noh<CR>x", ":set is hls<CR>/a<CR>x"];
    check(&cross(&texts, &keys));
}

#[test]
fn nohlsearch_keeps_the_desired_column() {
    check(&[("abcdef\nab\nabcdef", ":set hls<CR>llllj:noh<CR>jX")]);
}

#[test]
fn incsearch_forward_accept() {
    check(&[
        (TEXT, ":set is<CR>/qux<CR>x"),
        (TEXT, ":set is<CR>/foo<CR>x"),
        (TEXT, ":set is<CR>/line<CR>x"),
        (TEXT, ":set is<CR>/foo<CR>nx"),
    ]);
}

#[test]
fn incsearch_backward_accept() {
    check(&[
        (TEXT, ":set is<CR>G$?foo<CR>x"),
        (TEXT, ":set is<CR>G$?bar<CR>x"),
    ]);
}

#[test]
fn incsearch_with_no_match_leaves_the_cursor() {
    check(&[(TEXT, ":set is<CR>/zzz<CR>x")]);
}

#[test]
fn incsearch_with_an_operator() {
    check(&[
        (TEXT, ":set is<CR>d/qux<CR>"),
        (TEXT, ":set is<CR>wd/last<CR>"),
    ]);
}

#[test]
fn incsearch_and_hlsearch_together() {
    check(&[
        (TEXT, ":set is hls<CR>/foo<CR>nx"),
        (TEXT, ":set is hls<CR>/foo<CR>:noh<CR>/qux<CR>x"),
    ]);
}

#[test]
fn desired_column_after_a_search_prompt_is_answered() {
    let text = "abcdef\nab\nabcdef\nabcdef";
    check(&[
        (text, "llllj/ab<CR>jX"),
        (text, ":set is<CR>llllj/ab<CR>jX"),
        (text, "llllj?abc<CR>jX"),
        (text, ":set is<CR>llllj?abc<CR>jX"),
        (text, "llllj/zz<CR>jX"),
    ]);
}

/// Steps for the `v:hlsearch` comparison: rvim's keys, and the same step as
/// Vim script. `set hls` is run before the first step on both sides.
const FLAG_STEPS: &[(&str, &str)] = &[
    ("/foo<CR>", r#"execute "normal /foo\<CR>""#),
    (":noh<CR>", "noh"),
    (":noh<CR>", "noh"),
    ("n", r#"execute "normal n""#),
    (":noh<CR>", "noh"),
    ("*", r#"execute "normal *""#),
    (":noh<CR>", "noh"),
    ("<C-l>", r#"execute "normal \<C-l>""#),
    ("/foo<CR>", r#"execute "normal /foo\<CR>""#),
    ("<C-l>", r#"execute "normal \<C-l>""#),
    (":s/foo/foo/<CR>", "s/foo/foo/"),
    (":noh<CR>", "noh"),
    (":s/bar/bar/<CR>", "s/bar/bar/"),
    ("/qux<CR>", r#"execute "normal /qux\<CR>""#),
];

/// Vim's `v:hlsearch` after each of `steps`, read by a `-es` script (the
/// same mechanism as the harness). `None` if Vim is unavailable.
fn vim_hlsearch_flags(text: &str, steps: &[(&str, &str)]) -> Option<Vec<bool>> {
    let vim = std::env::var("RVIM_REFERENCE_VIM").unwrap_or_else(|_| "vim".into());
    let dir = tempfile::tempdir().unwrap();
    let mut script = String::from("set hls\nlet g:out = []\n");
    for (_, stmt) in steps {
        script.push_str(&format!("{stmt}\ncall add(g:out, v:hlsearch)\n"));
    }
    script.push_str("call writefile(map(g:out, 'string(v:val)'), 'out.txt')\nqa!\n");
    std::fs::write(dir.path().join("cases.vim"), script).unwrap();
    std::fs::write(dir.path().join("t.txt"), text).unwrap();
    let _ = Command::new(&vim)
        .current_dir(dir.path())
        .args(["-N", "-u", "NONE", "-i", "NONE", "-n", "-es", "t.txt"])
        .args(["-S", "cases.vim"])
        .stdout(std::process::Stdio::null())
        .status();
    let Ok(out) = std::fs::read_to_string(dir.path().join("out.txt")) else {
        let msg = format!("reference Vim `{vim}` failed or is missing");
        if std::env::var("RVIM_REQUIRE_VIM").as_deref() == Ok("1") {
            panic!("{msg}");
        }
        eprintln!("warning: {msg}; skipping the v:hlsearch check");
        return None;
    };
    Some(out.lines().map(|l| l.trim() == "1").collect())
}

/// rvim's hlsearch state after each step, seen as highlighted cells: with
/// 'hlsearch' on and a pattern in the text, a match is drawn exactly when the
/// flag (`no_hlsearch` in rvim, `v:hlsearch` in Vim) is on.
fn rvim_hlsearch_flags(text: &str, steps: &[(&str, &str)]) -> Vec<bool> {
    let mut ed = Editor::from_text(text);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>");
    steps
        .iter()
        .map(|(keys, _)| {
            ed.feed(keys);
            frame(&ed).hl.iter().any(|row| row.contains(&Hl::Search))
        })
        .collect()
}

#[test]
fn v_hlsearch_after_each_step_matches_vim() {
    let Some(want) = vim_hlsearch_flags(TEXT, FLAG_STEPS) else {
        return;
    };
    assert_eq!(want.len(), FLAG_STEPS.len(), "Vim wrote every step");
    let got = rvim_hlsearch_flags(TEXT, FLAG_STEPS);
    for (i, (step, _)) in FLAG_STEPS.iter().enumerate() {
        assert_eq!(got[i], want[i], "v:hlsearch after step {i} ({step:?})");
    }
}
