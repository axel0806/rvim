// SPDX-License-Identifier: Apache-2.0
//! Tests for `rvim::agent_detect`: the scan over a fake filesystem, the real
//! filesystem through an `Env` wrapper, and the registry pinned to PLAN.md.
//! No Vim and no agent is needed; nothing is executed.

use rvim::agent_detect::{AGENTS, Env, How, MAX_PATH_ENTRIES, MAX_PROBES, Os, RealEnv, scan};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

/// A fake host. Files, executables and directories are sets of paths; symlinks
/// map a path to its canonical target. Every call is recorded.
#[derive(Default)]
struct Fake {
    vars: HashMap<String, String>,
    files: HashSet<PathBuf>,
    execs: HashSet<PathBuf>,
    dirs: HashSet<PathBuf>,
    links: HashMap<PathBuf, PathBuf>,
    calls: RefCell<Vec<(&'static str, PathBuf)>>,
}

impl Fake {
    fn var(mut self, name: &str, value: &str) -> Self {
        self.vars.insert(name.to_string(), value.to_string());
        self
    }
    fn file(mut self, path: &str) -> Self {
        self.files.insert(p(path));
        self
    }
    fn exec(mut self, path: &str) -> Self {
        self.files.insert(p(path));
        self.execs.insert(p(path));
        self
    }
    fn dir(mut self, path: &str) -> Self {
        self.dirs.insert(p(path));
        self
    }
    fn link(mut self, from: &str, to: &str) -> Self {
        self.links.insert(p(from), p(to));
        self
    }
    fn log(&self, kind: &'static str, path: &Path) {
        self.calls.borrow_mut().push((kind, path.to_path_buf()));
    }
    /// The kinds of host query made so far.
    fn kinds(&self) -> HashSet<&'static str> {
        self.calls.borrow().iter().map(|(k, _)| *k).collect()
    }
    /// Filesystem probes made so far (the ones that count against the budget).
    fn probe_count(&self) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|(k, _)| matches!(*k, "exists" | "is_file" | "is_executable"))
            .count()
    }
    fn probed_paths(&self) -> Vec<PathBuf> {
        self.calls
            .borrow()
            .iter()
            .filter(|(k, _)| *k != "var")
            .map(|(_, p)| p.clone())
            .collect()
    }
}

impl Env for Fake {
    fn var(&self, name: &str) -> Option<String> {
        self.log("var", &p(name));
        self.vars.get(name).cloned()
    }
    fn exists(&self, path: &Path) -> bool {
        self.log("exists", path);
        self.files.contains(path) || self.dirs.contains(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.log("is_file", path);
        self.files.contains(path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.log("is_executable", path);
        self.execs.contains(path)
    }
    fn canonical(&self, path: &Path) -> PathBuf {
        self.log("canonical", path);
        self.links
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_path_buf())
    }
}

fn names_found(report: &rvim::agent_detect::ScanReport, id: &str) -> Vec<PathBuf> {
    report
        .found
        .iter()
        .filter(|f| f.agent.id == id)
        .map(|f| f.path.clone())
        .collect()
}

fn not_found_config(report: &rvim::agent_detect::ScanReport, id: &str) -> Option<Option<PathBuf>> {
    report
        .not_found
        .iter()
        .find(|n| n.agent.id == id)
        .map(|n| n.config_dir.clone())
}

#[test]
fn found_on_path() {
    let env = Fake::default()
        .var("PATH", "/usr/bin:/opt/bin")
        .exec("/opt/bin/claude")
        .var("HOME", "/home/u");
    let r = scan(&env, Os::Unix);
    let hits = names_found(&r, "claude");
    assert_eq!(hits, vec![p("/opt/bin/claude")]);
    assert_eq!(r.found[0].how, How::OnPath);
    assert!(r.found.iter().all(|f| f.agent.id == "claude"));
}

#[test]
fn not_found_when_nothing_is_present() {
    let env = Fake::default()
        .var("PATH", "/usr/bin")
        .var("HOME", "/home/u");
    let r = scan(&env, Os::Unix);
    assert!(r.found.is_empty());
    assert_eq!(r.not_found.len(), AGENTS.len());
    assert!(r.not_found.iter().all(|n| n.config_dir.is_none()));
    let lines = r.lines();
    assert!(
        lines.contains(&"Claude Code: not found".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"Kimi Code: not found".to_string()),
        "{lines:?}"
    );
}

#[test]
fn found_only_under_install_dir() {
    let env = Fake::default()
        .var("PATH", "")
        .var("HOME", "/home/u")
        .exec("/home/u/.local/bin/claude");
    let r = scan(&env, Os::Unix);
    assert_eq!(r.found.len(), 1);
    assert_eq!(r.found[0].agent.id, "claude");
    assert_eq!(r.found[0].how, How::InstallDir);
    assert_eq!(r.found[0].path, p("/home/u/.local/bin/claude"));
}

#[test]
fn config_hint_reports_config_present_not_found() {
    let env = Fake::default()
        .var("PATH", "/usr/bin")
        .var("HOME", "/home/u")
        .dir("/home/u/.claude");
    let r = scan(&env, Os::Unix);
    assert!(names_found(&r, "claude").is_empty());
    assert_eq!(
        not_found_config(&r, "claude"),
        Some(Some(p("/home/u/.claude")))
    );
    let line = r
        .lines()
        .into_iter()
        .find(|l| l.starts_with("Claude Code:"))
        .unwrap();
    assert!(line.contains("config present"), "{line}");
    assert!(line.contains("executable not found"), "{line}");
}

#[test]
fn kimi_legacy_and_new_config_dirs_are_both_probed() {
    let env = Fake::default()
        .var("PATH", "")
        .var("HOME", "/home/u")
        .dir("/home/u/.kimi");
    let r = scan(&env, Os::Unix);
    assert_eq!(not_found_config(&r, "kimi"), Some(Some(p("/home/u/.kimi"))));
    let env = Fake::default()
        .var("PATH", "")
        .var("KIMI_CODE_HOME", "/data/kimi")
        .var("HOME", "/home/u")
        .dir("/data/kimi");
    let r = scan(&env, Os::Unix);
    assert_eq!(not_found_config(&r, "kimi"), Some(Some(p("/data/kimi"))));
}

#[test]
fn windows_pathext_finds_cmd_shim_and_exe() {
    // A npm shim: claude.cmd on PATH.
    let env = Fake::default()
        .var("PATH", "C:\\tools")
        .var("USERPROFILE", "C:\\Users\\u")
        .exec("C:\\tools\\claude.cmd");
    let r = scan(&env, Os::Windows);
    let hits = names_found(&r, "claude");
    assert_eq!(hits, vec![p("C:\\tools\\claude.cmd")]);

    // claude.exe in the native install directory.
    let env = Fake::default()
        .var("PATH", "")
        .var("USERPROFILE", "C:\\Users\\u")
        .exec("C:\\Users\\u\\.local\\bin\\claude.exe");
    let r = scan(&env, Os::Windows);
    assert_eq!(names_found(&r, "claude").len(), 1);
    assert_eq!(r.found[0].how, How::InstallDir);
}

#[test]
fn windows_pathext_limits_extensions() {
    // Only .EXE is allowed, so a .cmd shim is not an executable here.
    let env = Fake::default()
        .var("PATH", "C:\\tools")
        .var("PATHEXT", ".EXE")
        .var("USERPROFILE", "C:\\Users\\u")
        .exec("C:\\tools\\claude.cmd");
    let r = scan(&env, Os::Windows);
    assert!(names_found(&r, "claude").is_empty());
}

#[test]
fn relative_path_entry_is_ignored_and_never_probed() {
    let env = Fake::default()
        .var("PATH", "bin:.:/opt/tools")
        .var("HOME", "/home/u")
        .exec("bin/claude")
        .exec("claude");
    let r = scan(&env, Os::Unix);
    assert!(
        names_found(&r, "claude").is_empty(),
        "relative entry was searched"
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("relative")),
        "{:?}",
        r.warnings
    );
    // No probe is made with a relative path (the PATH variable itself is a name, not a probe).
    for path in env.probed_paths() {
        assert!(
            path.to_string_lossy().starts_with('/'),
            "probed relative path {path:?}"
        );
    }
}

#[test]
fn empty_path_entry_is_ignored() {
    let env = Fake::default()
        .var("PATH", "::/opt/tools:")
        .var("HOME", "/home/u")
        .exec("/opt/tools/claude");
    let r = scan(&env, Os::Unix);
    assert_eq!(names_found(&r, "claude"), vec![p("/opt/tools/claude")]);
    for path in env.probed_paths() {
        assert!(!path.as_os_str().is_empty(), "probed an empty path");
        assert!(path.to_string_lossy().starts_with('/'), "probed {path:?}");
    }
}

#[test]
fn symlinked_duplicates_are_reported_once() {
    let env = Fake::default()
        .var("PATH", "/a:/b")
        .var("HOME", "/home/u")
        .exec("/a/claude")
        .exec("/b/claude")
        .exec("/real/claude")
        .link("/a/claude", "/real/claude")
        .link("/b/claude", "/real/claude");
    let r = scan(&env, Os::Unix);
    assert_eq!(names_found(&r, "claude"), vec![p("/a/claude")]);
    let lines = r.lines();
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.starts_with("Claude Code:"))
            .count(),
        1
    );
}

#[test]
fn generic_agent_name_detected_by_path_only() {
    // A tool that happens to be called agent: not Cursor's.
    let env = Fake::default()
        .var("PATH", "/usr/bin")
        .var("HOME", "/home/u")
        .exec("/usr/bin/agent");
    let r = scan(&env, Os::Unix);
    assert!(names_found(&r, "cursor").is_empty());
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("agent is not Cursor CLI") && w.contains("cursor")),
        "{:?}",
        r.warnings
    );

    // Cursor's agent under a cursor path (also reached through a symlink).
    let env = Fake::default()
        .var("PATH", "/home/u/.local/bin")
        .var("HOME", "/home/u")
        .exec("/home/u/.local/bin/agent")
        .exec("/home/u/.local/share/cursor-agent/versions/1/cursor-agent")
        .link(
            "/home/u/.local/bin/agent",
            "/home/u/.local/share/cursor-agent/versions/1/cursor-agent",
        );
    let r = scan(&env, Os::Unix);
    assert_eq!(
        names_found(&r, "cursor"),
        vec![p("/home/u/.local/bin/agent")]
    );
    let line = r
        .lines()
        .into_iter()
        .find(|l| l.starts_with("Cursor CLI:"))
        .unwrap();
    assert!(
        line.contains("found") && line.contains("protocols acp, files-only"),
        "{line}"
    );
}

#[test]
fn path_entry_cap_is_enforced() {
    let dirs: Vec<String> = (0..300).map(|i| format!("/p{i}")).collect();

    // claude in the 256th entry (index 255) is searched.
    let env = Fake::default()
        .var("PATH", &dirs.join(":"))
        .var("HOME", "/home/u")
        .exec("/p255/claude");
    let r = scan(&env, Os::Unix);
    assert_eq!(names_found(&r, "claude"), vec![p("/p255/claude")]);

    // claude in entry 299 is past the cap: not found, and warned about.
    let env = Fake::default()
        .var("PATH", &dirs.join(":"))
        .var("HOME", "/home/u")
        .exec("/p299/claude");
    let r = scan(&env, Os::Unix);
    assert!(names_found(&r, "claude").is_empty());
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains(&format!("more than {MAX_PATH_ENTRIES} entries"))),
        "{:?}",
        r.warnings
    );
}

#[test]
fn probe_budget_stops_a_huge_search() {
    // 256 entries on Windows with the default PATHEXT: many probes per entry.
    let dirs: Vec<String> = (0..256).map(|i| format!("C:\\d{i}")).collect();
    let env = Fake::default()
        .var("PATH", &dirs.join(";"))
        .var("USERPROFILE", "C:\\Users\\u");
    let r = scan(&env, Os::Windows);
    assert!(
        env.probe_count() <= MAX_PROBES,
        "{} probes",
        env.probe_count()
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("stopped after")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn scan_only_checks_existence_and_metadata() {
    // A rich host: every agent is present somewhere, and symlinks are involved.
    let env = Fake::default()
        .var("PATH", "/usr/bin:/home/u/.local/bin")
        .var("HOME", "/home/u")
        .var("KIMI_CODE_HOME", "/home/u/.kimi-code")
        .exec("/usr/bin/codex")
        .exec("/home/u/.local/bin/claude")
        .link("/home/u/.local/bin/claude", "/real/claude")
        .exec("/real/claude")
        .file("/usr/bin/kimi")
        .dir("/home/u/.kimi-code");
    let r = scan(&env, Os::Unix);
    let kinds = env.kinds();
    let allowed: HashSet<&str> = ["var", "exists", "is_file", "is_executable", "canonical"]
        .into_iter()
        .collect();
    assert!(
        kinds.is_subset(&allowed),
        "unexpected host queries: {kinds:?}"
    );
    assert!(!kinds.is_empty());
    assert_eq!(names_found(&r, "codex"), vec![p("/usr/bin/codex")]);
    assert_eq!(
        names_found(&r, "claude"),
        vec![p("/home/u/.local/bin/claude")]
    );
    // kimi is a plain file, not executable: not found, and warned about.
    assert!(names_found(&r, "kimi").is_empty());
    assert!(r.warnings.iter().any(|w| w.contains("not executable")));
}

#[test]
fn lines_are_plain_data() {
    let env = Fake::default()
        .var("PATH", "/usr/bin")
        .var("HOME", "/home/u")
        .exec("/usr/bin/claude")
        .dir("/home/u/.codex");
    let r = scan(&env, Os::Unix);
    let lines = r.lines();
    assert_eq!(lines.len(), AGENTS.len(), "{lines:?}");
    let claude = p("/usr/bin").join("claude");
    let codex_dir = p("/home/u").join(".codex");
    assert!(
        lines[0].starts_with(&format!(
            "Claude Code: found {} (on PATH); protocols acp-adapter, claude-ide (unverified), files-only",
            claude.display()
        )),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with(&format!(
            "Codex: config present ({}), executable not found",
            codex_dir.display()
        )),
        "{}",
        lines[1]
    );
    assert_eq!(lines[2], "Cursor CLI: not found");
    for line in &lines {
        assert!(!line.contains('\u{1b}'), "terminal code in {line:?}");
    }
}

#[test]
fn registry_matches_plan() {
    // Names only: the ids, display names and executables must match the table
    // in PLAN.md, "Detecting and connecting to agents".
    let expected: Vec<(&str, Vec<&str>)> = vec![
        ("claude", vec!["claude"]),
        ("codex", vec!["codex"]),
        ("cursor", vec!["agent", "cursor-agent"]),
        ("kimi", vec!["kimi"]),
    ];
    let got: Vec<(&str, Vec<&str>)> = AGENTS
        .iter()
        .map(|a| (a.id, a.executables.iter().map(|n| n.name).collect()))
        .collect();
    assert_eq!(got, expected);

    let plan = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/PLAN.md")).unwrap();
    let start = plan
        .find("### Detecting and connecting to agents")
        .expect("detection section");
    let end = plan[start..]
        .find("### Files changed under rvim")
        .map(|i| start + i)
        .expect("end of detection section");
    let section = &plan[start..end];
    for (_, names) in &expected {
        for name in names {
            assert!(
                section.contains(&format!("`{name}`")),
                "`{name}` is not named in PLAN.md's detection section"
            );
        }
    }
}

#[test]
fn source_has_no_process_or_content_reads() {
    // Detection must not run programs or read file contents. The module's
    // only host access is the Env trait; this guards the source against a
    // direct process or file-content call.
    let src = include_str!("../src/agent_detect.rs");
    for forbidden in [
        "process::",
        "Command::",
        "read_to_string",
        "fs::read(",
        "File::open",
        "read_dir",
    ] {
        assert!(
            !src.contains(forbidden),
            "agent_detect.rs contains {forbidden}"
        );
    }
}

/// Wraps the real host and overrides some variables, so a test can point
/// PATH and the home directories at a temporary directory.
struct Overlay<'a> {
    inner: &'a dyn Env,
    set: Vec<(&'static str, Option<String>)>,
}

impl Env for Overlay<'_> {
    fn var(&self, name: &str) -> Option<String> {
        match self.set.iter().find(|(k, _)| *k == name) {
            Some((_, v)) => v.clone(),
            None => self.inner.var(name),
        }
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.inner.is_file(path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.inner.is_executable(path)
    }
    fn canonical(&self, path: &Path) -> PathBuf {
        self.inner.canonical(path)
    }
}

fn overlay_for(dir: &Path) -> Overlay<'static> {
    let d = dir.to_string_lossy().into_owned();
    static REAL: RealEnv = RealEnv;
    Overlay {
        inner: &REAL,
        set: vec![
            ("PATH", Some(d.clone())),
            ("HOME", Some(d.clone())),
            ("USERPROFILE", Some(d.clone())),
            ("APPDATA", Some(d.clone())),
            ("CLAUDE_CONFIG_DIR", None),
            ("KIMI_CODE_HOME", None),
            ("CODEX_HOME", None),
        ],
    }
}

#[test]
fn real_filesystem_finds_a_named_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let file_name = match Os::current() {
        Os::Windows => "claude.exe",
        Os::Unix => "claude",
    };
    let path = tmp.path().join(file_name);
    std::fs::write(&path, b"not really an agent; never run").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let env = overlay_for(tmp.path());
    let r = scan(&env, Os::current());
    let hits = names_found(&r, "claude");
    assert_eq!(hits.len(), 1, "{:?}", r.lines());
    assert_eq!(hits[0], path);
}

#[cfg(unix)]
#[test]
fn unix_needs_the_execute_bit() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("codex");
    std::fs::write(&path, b"plain file").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let env = overlay_for(tmp.path());
    let r = scan(&env, Os::Unix);
    assert!(names_found(&r, "codex").is_empty());
    assert!(
        r.warnings.iter().any(|w| w.contains("not executable")),
        "{:?}",
        r.warnings
    );

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let r = scan(&env, Os::Unix);
    assert_eq!(names_found(&r, "codex"), vec![path]);
}
