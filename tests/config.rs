// SPDX-License-Identifier: Apache-2.0
//! The config file end to end, without the terminal: the command line, the
//! path from a temporary XDG_CONFIG_HOME, the one file read, and the
//! options applied to an Editor (PLAN.md, "Config").

use rvim::Editor;
use rvim::cli::{self, Action, ConfigUse};
use rvim::config::{self, Config};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

/// What `main.rs` does with the command line and the environment, minus
/// the terminal: returns the editor the user would start with.
fn start(args: &[&str], xdg: &std::path::Path) -> (Editor, Vec<String>) {
    start_with_text(args, xdg, "abc\n")
}

/// [`start`] with a buffer holding `text` (an empty one for the start menu).
fn start_with_text(args: &[&str], xdg: &std::path::Path, text: &str) -> (Editor, Vec<String>) {
    let inv = cli::parse_invocation(args.iter().map(OsString::from));
    let config_env = xdg.as_os_str().to_os_string();
    let config = match inv.config {
        ConfigUse::Read => {
            let path = config::config_path(
                |k| match k {
                    "XDG_CONFIG_HOME" => Some(config_env.clone()),
                    _ => None,
                },
                cfg!(windows),
            )
            .expect("a path");
            config::load(&path, config::read_capped)
        }
        ConfigUse::Skip => Config::default(),
    };
    let mut problems = config.warnings.clone();
    let mut ed = Editor::from_text(text);
    problems.extend(ed.apply_config(&config, inv.start_menu));
    if !ed.show_start_menu() {
        ed.show_intro();
    }
    ed.show_config_warnings(&problems);
    (ed, problems)
}

fn write_config(xdg: &std::path::Path, text: &str) -> PathBuf {
    let dir = xdg.join("rvim");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn options_from_the_file_change_what_the_keys_do() {
    let dir = tempfile::tempdir().unwrap();
    write_config(dir.path(), "[options]\nshiftwidth = 2\nexpandtab = true\n");
    let (mut ed, problems) = start(&[], dir.path());
    assert!(problems.is_empty(), "{problems:?}");
    ed.feed(">>");
    assert_eq!(ed.buf().text(), "  abc\n");
}

#[test]
fn a_missing_file_gives_vims_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, problems) = start(&[], dir.path());
    assert!(problems.is_empty());
    assert!(!ed.start_menu_shown());
    // The values Vim shows with `-N -u NONE`, read through `:set opt?`.
    for (query, shown) in [
        (":set sw?<CR>", "  shiftwidth=8"),
        (":set ts?<CR>", "  tabstop=8"),
        (":set sts?<CR>", "  softtabstop=0"),
        (":set et?<CR>", "noexpandtab"),
        (":set ai?<CR>", "noautoindent"),
        (":set so?<CR>", "  scrolloff=0"),
        (":set startmenu?<CR>", "nostartmenu"),
    ] {
        ed.feed(query);
        assert_eq!(
            ed.message().map(|m| m.text.as_str()),
            Some(shown),
            "{query}"
        );
    }
}

#[test]
fn u_none_skips_a_file_that_exists() {
    let dir = tempfile::tempdir().unwrap();
    write_config(dir.path(), "[options]\nshiftwidth = 2\nexpandtab = true\n");
    for args in [&["-u", "NONE"][..], &["--clean"][..], &["-uNONE", "f"][..]] {
        let (mut ed, problems) = start(args, dir.path());
        assert!(problems.is_empty());
        ed.feed(">>");
        assert_eq!(ed.buf().text(), "\tabc\n", "args {args:?}");
    }
}

#[test]
fn a_syntax_error_names_the_line_and_keeps_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    write_config(dir.path(), "[options]\nshiftwidth = 2\nexpandtab =\n");
    let (mut ed, problems) = start(&[], dir.path());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("config: line 3: "), "{problems:?}");
    // The message line shows the problem (the intro message is replaced).
    let shown = ed.message().expect("a message").text.clone();
    assert!(shown.starts_with("config: line 3: "), "{shown}");
    // Defaults: no shiftwidth change, so `>>` uses a tab.
    ed.feed(">>");
    assert_eq!(ed.buf().text(), "\tabc\n");
}

#[test]
fn unknown_keys_warn_and_the_valid_ones_apply() {
    let dir = tempfile::tempdir().unwrap();
    write_config(
        dir.path(),
        "frobnicate = 1\n[options]\nexpandtab = true\nshiftwidth = 2\n",
    );
    let (mut ed, problems) = start(&[], dir.path());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("unknown key \"frobnicate\""));
    ed.feed(">>");
    assert_eq!(ed.buf().text(), "  abc\n");
}

#[test]
fn oversize_file_is_not_applied() {
    let dir = tempfile::tempdir().unwrap();
    let mut text = String::from("[options]\nshiftwidth = 2\n");
    text.push_str(&"# padding\n".repeat(30_000));
    write_config(dir.path(), &text);
    assert!(text.len() as u64 > config::MAX_BYTES);
    let (mut ed, problems) = start(&[], dir.path());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("larger than 256 KiB"));
    ed.feed(">>");
    assert_eq!(ed.buf().text(), "\tabc\n");
}

#[test]
fn start_menu_from_the_file_and_the_flags() {
    let dir = tempfile::tempdir().unwrap();
    write_config(dir.path(), "[start]\nmenu = true\n");
    // The menu needs no file name and an empty buffer.
    let (ed, problems) = start_with_text(&[], dir.path(), "");
    assert!(problems.is_empty());
    assert!(ed.start_menu_shown(), "the config turns the menu on");

    let (ed, _) = start_with_text(&["--no-start-menu"], dir.path(), "");
    assert!(!ed.start_menu_shown(), "the flag turns it off again");
}

#[test]
fn an_invalid_option_value_comes_back_as_a_message() {
    let dir = tempfile::tempdir().unwrap();
    write_config(dir.path(), "[options]\nshiftwidth = -1\n");
    let (_, problems) = start(&[], dir.path());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("E487"), "{problems:?}");
}

#[test]
fn bad_u_value_is_a_usage_error() {
    assert!(matches!(
        cli::parse(["-u", "other.vim"].iter().map(OsString::from)),
        Action::Usage(_)
    ));
}
