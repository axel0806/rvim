// SPDX-License-Identifier: Apache-2.0
//! Milestone 7 step 4b (PLAN.md, "Detecting and connecting to agents",
//! "Agent experience" sections 1 and 2, and milestone 7 step 4): the scan
//! through a fake home and `PATH`, the summary and its message, the
//! `:AgentScan` command, the config opt-in, and the status indicator on the
//! Frame. The scan itself runs on a worker in `main.rs`; here its result is
//! sent to the editor as an event, as the worker does.

use rvim::agent_detect::{self, Env, How, Os, ScanSummary};
use rvim::agent_scan::AgentCmd;
use rvim::config;
use rvim::view;
use rvim::{Editor, Event};
use std::path::{Path, PathBuf};

/// A fake host: variables, existing paths and executables. Nothing is read
/// from the real machine.
#[derive(Default)]
struct FakeHost {
    vars: Vec<(&'static str, &'static str)>,
    existing: Vec<&'static str>,
    executables: Vec<&'static str>,
}

impl Env for FakeHost {
    fn var(&self, name: &str) -> Option<String> {
        self.vars
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string())
    }
    fn exists(&self, path: &Path) -> bool {
        self.existing.iter().any(|p| Path::new(p) == path)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.exists(path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.executables.iter().any(|p| Path::new(p) == path)
    }
    fn canonical(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }
}

/// A Unix host with `claude` on `PATH` and nothing else installed.
fn host_with_claude_on_path() -> FakeHost {
    FakeHost {
        vars: vec![("PATH", "/usr/bin"), ("HOME", "/home/u")],
        existing: vec!["/usr/bin/claude"],
        executables: vec!["/usr/bin/claude"],
    }
}

fn scan_of(host: &FakeHost) -> ScanSummary {
    agent_detect::scan(host, Os::Unix).summary()
}

/// The bottom row of the frame, as text.
fn status(ed: &Editor) -> String {
    view::frame(ed).status
}

fn editor_80x24(text: &str) -> Editor {
    let mut ed = Editor::from_text(text);
    ed.set_screen_size(80, 24);
    ed
}

// ---- detection through the fake host -------------------------------------

#[test]
fn claude_on_path_is_found_and_the_others_are_not() {
    let s = scan_of(&host_with_claude_on_path());
    assert_eq!(s.found, vec![("claude", How::OnPath)]);
    assert!(s.config_only.is_empty());
    assert_eq!(
        s.not_found,
        vec![
            "codex",
            "cursor",
            "kimi",
            "gemini",
            "github-copilot-cli",
            "opencode",
            "goose",
            "qwen-code",
            "antigravity"
        ]
    );
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    assert!(
        s.message()
            .starts_with("found claude (PATH) | not found: codex, cursor, kimi, gemini, github-copilot-cli, opencode, goose, qwen-code, antigravity"),
        "{}",
        s.message()
    );
}

#[test]
fn a_config_directory_alone_is_config_only_not_found() {
    let host = FakeHost {
        vars: vec![("PATH", "/usr/bin"), ("HOME", "/home/u")],
        existing: vec!["/home/u/.claude"],
        executables: vec![],
    };
    let s = scan_of(&host);
    assert!(s.found.is_empty());
    assert_eq!(s.config_only, vec!["claude"]);
    assert!(!s.not_found.contains(&"claude"));
    assert!(
        s.message().contains("config only: claude"),
        "{}",
        s.message()
    );
}

#[test]
fn nothing_installed_gives_an_empty_found_list() {
    let host = FakeHost {
        vars: vec![("PATH", "/usr/bin"), ("HOME", "/home/u")],
        ..FakeHost::default()
    };
    let s = scan_of(&host);
    assert!(s.found.is_empty() && s.config_only.is_empty());
    assert_eq!(s.not_found.len(), 10);
}

#[test]
fn the_scan_is_read_only_and_reads_no_file_contents() {
    // The fake host has no read method at all, so a scan can only check
    // existence and executability; this test documents that the summary
    // comes from those checks alone.
    let s = scan_of(&host_with_claude_on_path());
    assert_eq!(s.found.len(), 1);
}

// ---- the summary and the indicator, as data -------------------------------

#[test]
fn message_lists_warnings_after_the_states() {
    let s = ScanSummary {
        found: vec![("codex", How::InstallDir)],
        config_only: vec!["kimi"],
        not_found: vec!["claude"],
        warnings: vec!["PATH entry skipped".into()],
    };
    assert_eq!(
        s.message(),
        "found codex (install dir) | config only: kimi | not found: claude | warning: PATH entry skipped"
    );
}

#[test]
fn indicator_states_without_a_connection() {
    let found = ScanSummary {
        found: vec![("claude", How::OnPath), ("codex", How::InstallDir)],
        ..ScanSummary::default()
    };
    let none = ScanSummary {
        not_found: vec!["claude"],
        ..ScanSummary::default()
    };
    assert_eq!(agent_detect::indicator(false, None), None);
    assert_eq!(agent_detect::indicator(false, Some(&found)), None);
    assert_eq!(
        agent_detect::indicator(true, None).as_deref(),
        Some("ai: files only")
    );
    assert_eq!(
        agent_detect::indicator(true, Some(&none)).as_deref(),
        Some("ai: none")
    );
    assert_eq!(
        agent_detect::indicator(true, Some(&found)).as_deref(),
        Some("ai: claude, codex found")
    );
}

// ---- the editor: :AgentScan, the opt-in and the Frame ---------------------

#[test]
fn no_opt_in_and_no_command_leaves_the_frame_as_vim_draws_it() {
    let mut ed = editor_80x24("abc\n");
    assert!(!status(&ed).contains("ai:"), "{:?}", status(&ed));
    assert!(ed.take_agent_commands().is_empty());
    // The same frame as an editor that never heard of agents.
    let plain = editor_80x24("abc\n");
    assert_eq!(view::frame(&ed).status, view::frame(&plain).status);
    assert_eq!(view::frame(&ed).rows, view::frame(&plain).rows);
}

#[test]
fn agent_scan_without_opt_in_asks_for_a_scan_and_shows_the_indicator() {
    let mut ed = editor_80x24("abc\n");
    let before = status(&ed);
    ed.feed(":AgentScan<CR>");
    assert_eq!(ed.take_agent_commands(), vec![AgentCmd::Scan]);
    // Before the result: the layer is on, with no scan yet.
    let s = status(&ed);
    assert_eq!(&s[62..], &before[62..], "the ruler is unchanged");
    assert_eq!(s[..62].chars().count(), 62);
    assert!(s[..62].ends_with("ai: files only "), "{s:?}");

    // The worker's result arrives as an event.
    ed.handle_event(Event::AgentScan(scan_of(&host_with_claude_on_path())));
    assert!(
        ed.message()
            .unwrap()
            .text
            .starts_with("found claude (PATH)")
    );
    let s = status(&ed);
    assert_eq!(s[..62].chars().count(), 62);
    assert!(s[..62].ends_with("ai: claude found "), "{s:?}");
    assert!(s[..45].starts_with("found claude (PATH) | not found: codex, curs"));
    assert!(ed.take_agent_commands().is_empty());
}

#[test]
fn agent_scan_with_no_agents_shows_none() {
    let mut ed = editor_80x24("abc\n");
    ed.feed(":AgentScan<CR>");
    ed.handle_event(Event::AgentScan(scan_of(&FakeHost::default())));
    let s = status(&ed);
    assert!(s[..62].ends_with("ai: none "), "{s:?}");
}

#[test]
fn agent_scan_takes_no_argument() {
    let mut ed = editor_80x24("abc\n");
    ed.feed(":AgentScan now<CR>");
    assert!(ed.take_agent_commands().is_empty());
    assert!(ed.message().unwrap().error);
    assert!(!status(&ed).contains("ai:"));
}

#[test]
fn config_opt_in_shows_the_indicator_and_scans_at_startup() {
    let mut ed = editor_80x24("abc\n");
    let cfg = config::parse("[agent]\nenabled = true\n");
    assert!(ed.apply_config(&cfg, None).is_empty());
    assert_eq!(ed.take_agent_commands(), vec![AgentCmd::Scan]);
    assert!(status(&ed).contains("ai: files only"), "{:?}", status(&ed));
    ed.handle_event(Event::AgentScan(scan_of(&FakeHost::default())));
    assert!(status(&ed).contains("ai: none"), "{:?}", status(&ed));
}

#[test]
fn config_with_other_tables_turns_nothing_agent_related_on() {
    let mut ed = editor_80x24("abc\n");
    let cfg = config::parse("[options]\nshiftwidth = 2\n[agent]\nenabled = false\n");
    ed.apply_config(&cfg, None);
    assert!(ed.take_agent_commands().is_empty());
    assert!(!status(&ed).contains("ai:"));
}

#[test]
fn a_narrow_frame_drops_the_indicator_and_keeps_the_ruler() {
    let mut ed = editor_80x24("abc\n");
    ed.set_screen_size(30, 24);
    ed.feed(":AgentScan<CR>");
    let s = status(&ed);
    assert!(!s.contains("ai:"), "{s:?}");
    assert!(s.trim_end().ends_with("All"), "{s:?}");
}

#[test]
fn the_indicator_keeps_the_message_cells_before_it() {
    let mut ed = editor_80x24("abc\n");
    ed.feed(":AgentScan<CR>");
    ed.handle_event(Event::AgentScan(scan_of(&host_with_claude_on_path())));
    // The message is cut before the indicator, never over it.
    let s = status(&ed);
    assert_eq!(s.chars().count(), 80);
    assert!(s[..45].starts_with("found claude (PATH) | not f"), "{s:?}");
}
