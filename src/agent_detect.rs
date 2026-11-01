// SPDX-License-Identifier: Apache-2.0
//! Read-only detection of the coding agents installed on this machine: the
//! registry of known agents and [`scan`], which `:AgentScan` will run on a
//! worker thread (PLAN.md, "Detecting and connecting to agents", and
//! milestone 7 step 4).
//!
//! Detection checks that things exist and reads their metadata, and nothing
//! more. It never reads file contents, never runs a program (no `--version`),
//! and connects to nothing. It reaches the host only through [`Env`], so a
//! test passes a fake filesystem and this module does no I/O of its own
//! beyond what [`RealEnv`] does.
//!
//! Search order for each agent: the `PATH` entries, then the agent's install
//! directories, then its config directories. A config directory only reports
//! that the agent was set up before; it never counts as an executable.
//!
//! A `PATH` entry that is relative or empty is ignored, so the current
//! directory is never searched. Each executable name is tried in every
//! `PATH` entry; on Windows each name is also tried with each `PATHEXT`
//! extension. A file reached through a symlink is reported once, by its
//! canonical path. The scan stops after [`MAX_PROBES`] filesystem probes and
//! after [`MAX_PATH_ENTRIES`] `PATH` entries, and says so in the warnings.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// `PATH` entries read before the rest are ignored, with a warning.
pub const MAX_PATH_ENTRIES: usize = 256;

/// Filesystem probes (`exists`, `is_file`, `is_executable`) in one scan. A
/// pathological `PATH` stops here instead of stalling the worker.
pub const MAX_PROBES: usize = 4096;

/// The host conventions that matter here: how `PATH` is split, which
/// extensions an executable may have, and where the home directory is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Unix,
    Windows,
}

impl Os {
    /// The host this build runs on.
    pub fn current() -> Os {
        if cfg!(windows) { Os::Windows } else { Os::Unix }
    }
}

/// The host queries detection needs. Every method is an existence check or a
/// metadata read. There is deliberately no method that reads file contents
/// or runs a program.
pub trait Env {
    /// An environment variable, if set.
    fn var(&self, name: &str) -> Option<String>;
    /// True if something (file or directory) exists at `path`.
    fn exists(&self, path: &Path) -> bool;
    /// True if `path` is a regular file.
    fn is_file(&self, path: &Path) -> bool;
    /// True if `path` is a regular file that can be run. On Unix this is the
    /// execute bit; on Windows it is a `PATHEXT` extension.
    fn is_executable(&self, path: &Path) -> bool;
    /// The canonical form of `path` (symlinks resolved). Returns `path` itself
    /// when it cannot be resolved.
    fn canonical(&self, path: &Path) -> PathBuf;
}

/// The real host: `std::env` for variables and `std::fs` metadata.
pub struct RealEnv;

impl Env for RealEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
    }

    fn exists(&self, path: &Path) -> bool {
        fs::metadata(path).is_ok()
    }

    fn is_file(&self, path: &Path) -> bool {
        fs::metadata(path).is_ok_and(|m| m.is_file())
    }

    fn is_executable(&self, path: &Path) -> bool {
        if !self.is_file(path) {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            has_pathext(path, &pathext(self))
        }
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// The agent's connection methods, in order of capability (PLAN.md,
/// "Connection methods").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolKind {
    /// The agent speaks ACP itself (`<cli> acp`).
    AcpNative,
    /// ACP through a separate adapter package.
    AcpAdapter,
    /// The agent's IDE integration, which reads a lock file that rvim writes.
    ClaudeIde,
    /// The CLI runs in a terminal pane and rvim watches the files it edits.
    FilesOnly,
}

impl ProtocolKind {
    pub fn label(self) -> &'static str {
        match self {
            ProtocolKind::AcpNative => "acp",
            ProtocolKind::AcpAdapter => "acp-adapter",
            ProtocolKind::ClaudeIde => "claude-ide",
            ProtocolKind::FilesOnly => "files-only",
        }
    }
}

/// One connection method of an agent, and whether its details were checked
/// against the agent's own documentation or registry.
#[derive(Clone, Copy, Debug)]
pub struct Conn {
    pub kind: ProtocolKind,
    pub verified: bool,
}

/// The root a [`Location`] is relative to.
#[derive(Clone, Copy, Debug)]
pub enum Base {
    /// The user's home directory (`USERPROFILE` then `HOME` on Windows, `HOME` elsewhere).
    Home,
    /// The value of an environment variable, which must be an absolute path.
    /// Unset or relative means the location is skipped.
    Var(&'static str),
}

/// A directory or file to probe, `rel` below `base`.
#[derive(Clone, Copy, Debug)]
pub struct Location {
    pub base: Base,
    /// Below the base, with `/` separators. Empty means the base itself.
    pub rel: &'static str,
    /// Only on this OS; `None` for both.
    pub os: Option<Os>,
    /// Whether the location was checked against the agent's documentation.
    pub verified: bool,
}

/// An executable name to look for.
#[derive(Clone, Copy, Debug)]
pub struct Named {
    /// The name without an extension (`claude`, not `claude.exe`).
    pub name: &'static str,
    /// Whether the name was checked against the agent's documentation.
    pub verified: bool,
    /// A generic name (such as `agent`) is accepted only when the file's own
    /// path, or the path it resolves to through symlinks, contains this text
    /// (case-insensitive). Detection by name alone would match other tools.
    pub path_hint: Option<&'static str>,
}

/// One known agent: a row of the registry table in PLAN.md.
#[derive(Debug)]
pub struct AgentSpec {
    /// The registry id. Used in approvals, the config and logs.
    pub id: &'static str,
    pub display: &'static str,
    pub executables: &'static [Named],
    pub install_dirs: &'static [Location],
    pub config_dirs: &'static [Location],
    /// Connection methods, best first. `FilesOnly` is always available, and is
    /// listed for every agent.
    pub protocols: &'static [Conn],
    /// The command that starts the CLI in a terminal pane.
    pub launch: &'static str,
    /// The command or adapter package for ACP, or `""` when there is none.
    pub acp: &'static str,
    /// Notes, including what is UNVERIFIED.
    pub notes: &'static str,
    /// The sources read, with the date.
    pub sources: &'static str,
}

const ANY_OS: Option<Os> = None;

/// The registry of known agents. Keep it in step with PLAN.md, "Detecting and
/// connecting to agents"; the test `registry_matches_plan` checks the names.
pub static AGENTS: &[AgentSpec] = &[
    AgentSpec {
        id: "claude",
        display: "Claude Code",
        executables: &[Named {
            name: "claude",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[
            Location {
                base: Base::Home,
                rel: ".local/bin",
                os: ANY_OS,
                verified: true,
            },
            Location {
                base: Base::Home,
                rel: ".claude/local",
                os: ANY_OS,
                verified: false,
            },
            Location {
                base: Base::Var("APPDATA"),
                rel: "npm",
                os: Some(Os::Windows),
                verified: false,
            },
        ],
        config_dirs: &[
            Location {
                base: Base::Home,
                rel: ".claude",
                os: ANY_OS,
                verified: true,
            },
            Location {
                base: Base::Home,
                rel: ".claude.json",
                os: ANY_OS,
                verified: true,
            },
            Location {
                base: Base::Var("CLAUDE_CONFIG_DIR"),
                rel: "",
                os: ANY_OS,
                verified: false,
            },
        ],
        protocols: &[
            Conn {
                kind: ProtocolKind::AcpAdapter,
                verified: true,
            },
            Conn {
                kind: ProtocolKind::ClaudeIde,
                verified: false,
            },
            Conn {
                kind: ProtocolKind::FilesOnly,
                verified: true,
            },
        ],
        launch: "claude",
        acp: "npx @agentclientprotocol/claude-agent-acp@0.99.1",
        notes: "Native install puts claude in ~/.local/bin (Windows: %USERPROFILE%\\.local\\bin\\claude.exe). \
                The ACP adapter is the package in the ACP registry (the older @zed-industries name is gone). \
                The IDE protocol (lock file, WebSocket) comes from a third-party write-up: UNVERIFIED. \
                ~/.claude/local and %APPDATA%\\npm are install guesses: UNVERIFIED. CLAUDE_CONFIG_DIR is not on the setup page: UNVERIFIED.",
        sources: "code.claude.com/docs/en/setup (read 2026-11-01); agentclientprotocol.com/get-started/agents and \
                  cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-01)",
    },
    AgentSpec {
        id: "codex",
        display: "Codex",
        executables: &[Named {
            name: "codex",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[Location {
            base: Base::Var("APPDATA"),
            rel: "npm",
            os: Some(Os::Windows),
            verified: false,
        }],
        config_dirs: &[
            Location {
                base: Base::Home,
                rel: ".codex",
                os: ANY_OS,
                verified: false,
            },
            Location {
                base: Base::Var("CODEX_HOME"),
                rel: "",
                os: ANY_OS,
                verified: false,
            },
        ],
        protocols: &[
            Conn {
                kind: ProtocolKind::AcpAdapter,
                verified: true,
            },
            Conn {
                kind: ProtocolKind::FilesOnly,
                verified: true,
            },
        ],
        launch: "codex",
        acp: "npx @agentclientprotocol/codex-acp@2.5.0",
        notes: "The CLI is codex (docs). Headless: codex exec (docs). The ACP adapter is the registry package \
                (the older @zed-industries name is gone). ~/.codex and CODEX_HOME are not on the CLI page: UNVERIFIED. \
                The app-server launch command is UNVERIFIED and not used.",
        sources: "learn.chatgpt.com/docs/codex/cli (read 2026-11-01); cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-01)",
    },
    AgentSpec {
        id: "cursor",
        display: "Cursor CLI",
        executables: &[
            Named {
                name: "agent",
                verified: true,
                path_hint: Some("cursor"),
            },
            Named {
                name: "cursor-agent",
                verified: false,
                path_hint: Some("cursor"),
            },
        ],
        install_dirs: &[Location {
            base: Base::Home,
            rel: ".local/bin",
            os: Some(Os::Unix),
            verified: false,
        }],
        config_dirs: &[],
        protocols: &[
            Conn {
                kind: ProtocolKind::AcpNative,
                verified: true,
            },
            Conn {
                kind: ProtocolKind::FilesOnly,
                verified: true,
            },
        ],
        launch: "agent",
        acp: "agent acp",
        notes: "The docs name the executable agent (cursor.com/docs/cli/overview), with install by curl or irm. \
                agent is a generic process name, so it counts only when its path (or its symlink target) contains \
                \"cursor\". cursor-agent as a PATH name and the Unix install location are UNVERIFIED. \
                Auth: cursor_login (agent login, CURSOR_API_KEY or CURSOR_AUTH_TOKEN). No config directory is known.",
        sources: "cursor.com/docs/cli/overview and cursor.com/docs/cli/acp (read 2026-11-01); \
                  cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-01)",
    },
    AgentSpec {
        id: "kimi",
        display: "Kimi Code",
        executables: &[Named {
            name: "kimi",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
        config_dirs: &[
            Location {
                base: Base::Var("KIMI_CODE_HOME"),
                rel: "",
                os: ANY_OS,
                verified: true,
            },
            Location {
                base: Base::Home,
                rel: ".kimi-code",
                os: ANY_OS,
                verified: true,
            },
            Location {
                base: Base::Home,
                rel: ".kimi",
                os: ANY_OS,
                verified: true,
            },
        ],
        protocols: &[
            Conn {
                kind: ProtocolKind::AcpNative,
                verified: true,
            },
            Conn {
                kind: ProtocolKind::FilesOnly,
                verified: true,
            },
        ],
        launch: "kimi",
        acp: "kimi acp",
        notes: "The executable is kimi; @moonshot-ai/kimi-code is the npm package, not the executable. ~/.kimi is the \
                legacy kimi-cli (its repository was archived 2026-09-23) and uses the same executable name. Login is \
                /login inside the UI. Headless -p: whether it auto-approves is not stated: UNVERIFIED, not used. \
                The installer location is not stated, so no install directory is probed.",
        sources: "kimi.com/code/docs (Kimi Code CLI getting started, read 2026-11-01); github.com/MoonshotAI/kimi-cli \
                  (read 2026-11-01); cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-01)",
    },
];

/// How an executable was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// In a `PATH` directory.
    OnPath,
    /// In one of the agent's install directories.
    InstallDir,
}

/// An executable found for an agent.
#[derive(Debug)]
pub struct Found {
    pub agent: &'static AgentSpec,
    /// The path as it was found (not the canonical one).
    pub path: PathBuf,
    pub how: How,
}

/// An agent with no executable found. `config_dir` is set when one of its
/// config directories exists, which means it was set up before.
#[derive(Debug)]
pub struct NotFound {
    pub agent: &'static AgentSpec,
    pub config_dir: Option<PathBuf>,
}

/// The result of one scan.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub found: Vec<Found>,
    pub not_found: Vec<NotFound>,
    /// Things the scan skipped and why (relative `PATH` entries, files that
    /// are not executable, a generic `agent` that is not Cursor's, limits).
    pub warnings: Vec<String>,
}

impl ScanReport {
    /// One line per agent, in registry order, then the warnings prefixed with
    /// `warning: `. Plain text, no terminal codes; the caller decides where
    /// the lines go.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for agent in AGENTS {
            let hits: Vec<&Found> = self
                .found
                .iter()
                .filter(|f| std::ptr::eq(f.agent, agent))
                .collect();
            if let Some(first) = hits.first() {
                let how = match first.how {
                    How::OnPath => "on PATH",
                    How::InstallDir => "in an install directory",
                };
                let more = if hits.len() > 1 {
                    format!(" (+{} more)", hits.len() - 1)
                } else {
                    String::new()
                };
                out.push(format!(
                    "{}: found {} ({}); protocols {}{}",
                    agent.display,
                    first.path.display(),
                    how,
                    protocol_list(agent),
                    more
                ));
                continue;
            }
            let config = self
                .not_found
                .iter()
                .find(|n| std::ptr::eq(n.agent, agent))
                .and_then(|n| n.config_dir.as_ref());
            match config {
                Some(dir) => out.push(format!(
                    "{}: config present ({}), executable not found",
                    agent.display,
                    dir.display()
                )),
                None => out.push(format!("{}: not found", agent.display)),
            }
        }
        out.extend(self.warnings.iter().map(|w| format!("warning: {w}")));
        out
    }
}

fn protocol_list(agent: &AgentSpec) -> String {
    agent
        .protocols
        .iter()
        .map(|c| {
            if c.verified {
                c.kind.label().to_string()
            } else {
                format!("{} (unverified)", c.kind.label())
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Searches `env` for the agents in [`AGENTS`]. Read-only: see the module docs.
pub fn scan(env: &dyn Env, os: Os) -> ScanReport {
    let mut s = Scanner {
        env,
        os,
        exts: pathext(env),
        probes: 0,
        exhausted: false,
        seen: HashSet::new(),
        out: ScanReport::default(),
    };
    let all: Vec<usize> = (0..AGENTS.len()).collect();
    s.scan_path(&all);
    for (i, agent) in AGENTS.iter().enumerate() {
        for loc in agent.install_dirs {
            if let Some(dir) = s.resolve(loc) {
                s.check_dir(&dir, How::InstallDir, &[i]);
            }
        }
    }
    for agent in AGENTS {
        if s.out.found.iter().any(|f| std::ptr::eq(f.agent, agent)) {
            continue;
        }
        let dirs: Vec<String> = agent
            .config_dirs
            .iter()
            .filter_map(|loc| s.resolve(loc))
            .collect();
        let mut config_dir = None;
        for dir in dirs {
            if s.spend() && env.exists(Path::new(&dir)) {
                config_dir = Some(PathBuf::from(dir));
                break;
            }
        }
        s.out.not_found.push(NotFound { agent, config_dir });
    }
    s.out
}

struct Scanner<'a> {
    env: &'a dyn Env,
    os: Os,
    exts: Vec<String>,
    probes: usize,
    exhausted: bool,
    /// (agent index, canonical path) already reported.
    seen: HashSet<(usize, PathBuf)>,
    out: ScanReport,
}

impl Scanner<'_> {
    /// Counts one probe. False once the budget is spent (warned once).
    fn spend(&mut self) -> bool {
        if self.probes >= MAX_PROBES {
            if !self.exhausted {
                self.exhausted = true;
                self.out.warnings.push(format!(
                    "detection stopped after {MAX_PROBES} filesystem probes; some locations were not checked"
                ));
            }
            return false;
        }
        self.probes += 1;
        true
    }

    fn scan_path(&mut self, agents: &[usize]) {
        let sep = match self.os {
            Os::Unix => ':',
            Os::Windows => ';',
        };
        let raw = self.env.var("PATH").unwrap_or_default();
        let mut dirs_seen: HashSet<String> = HashSet::new();
        for (index, entry) in raw.split(sep).enumerate() {
            if index == MAX_PATH_ENTRIES {
                self.out.warnings.push(format!(
                    "PATH has more than {MAX_PATH_ENTRIES} entries; the rest were not searched"
                ));
                break;
            }
            let entry = entry.trim().trim_matches('"');
            if entry.is_empty() {
                continue;
            }
            if !is_absolute(self.os, entry) {
                self.out.warnings.push(format!(
                    "PATH entry \"{entry}\" is relative and was ignored (the current directory is never searched)"
                ));
                continue;
            }
            let dir = entry.to_string();
            if dirs_seen.insert(dir.clone()) {
                self.check_dir(&dir, How::OnPath, agents);
            }
        }
    }

    fn check_dir(&mut self, dir: &str, how: How, agents: &[usize]) {
        for &i in agents {
            let agent: &'static AgentSpec = &AGENTS[i];
            for named in agent.executables {
                for candidate in self.candidates(dir, named.name) {
                    if !self.spend() {
                        return;
                    }
                    if !self.env.is_file(Path::new(&candidate)) {
                        continue;
                    }
                    if !self.spend() {
                        return;
                    }
                    if !self.env.is_executable(Path::new(&candidate)) {
                        self.out
                            .warnings
                            .push(format!("{candidate} exists but is not executable; ignored"));
                        continue;
                    }
                    let canon = self.env.canonical(Path::new(&candidate));
                    let canon_text = canon.to_string_lossy().into_owned();
                    if let Some(hint) = named.path_hint {
                        let matches =
                            contains_ci(&canon_text, hint) || contains_ci(&candidate, hint);
                        if !matches {
                            self.out.warnings.push(format!(
                                "{candidate} is not {}: its path has no \"{hint}\"; ignored",
                                agent.display
                            ));
                            continue;
                        }
                    }
                    if self.seen.insert((i, canon)) {
                        self.out.found.push(Found {
                            agent,
                            path: PathBuf::from(&candidate),
                            how,
                        });
                    }
                }
            }
        }
    }

    fn candidates(&self, dir: &str, name: &str) -> Vec<String> {
        match self.os {
            Os::Unix => vec![join(self.os, dir, name)],
            Os::Windows => self
                .exts
                .iter()
                .map(|ext| join(self.os, dir, &format!("{name}{ext}")))
                .collect(),
        }
    }

    fn resolve(&self, loc: &Location) -> Option<String> {
        if loc.os.is_some_and(|o| o != self.os) {
            return None;
        }
        let base = match loc.base {
            Base::Home => home(self.env, self.os)?,
            Base::Var(name) => self.env.var(name).filter(|v| !v.trim().is_empty())?,
        };
        if !is_absolute(self.os, &base) {
            return None;
        }
        if loc.rel.is_empty() {
            Some(base)
        } else {
            Some(join(self.os, &base, loc.rel))
        }
    }
}

/// Joins `rel` (written with `/` in the table) onto `base` with the separator
/// of `os`. The result depends only on `os`, not on the host, so a fake
/// environment with `Os::Windows` sees backslash paths on any machine.
fn join(os: Os, base: &str, rel: &str) -> String {
    let sep = match os {
        Os::Unix => '/',
        Os::Windows => '\\',
    };
    let rel: String = rel
        .chars()
        .map(|c| {
            if os == Os::Windows && c == '/' {
                '\\'
            } else {
                c
            }
        })
        .collect();
    let base_has_sep = base.ends_with(sep) || (os == Os::Windows && base.ends_with('/'));
    if base_has_sep {
        format!("{base}{rel}")
    } else {
        format!("{base}{sep}{rel}")
    }
}

fn home(env: &dyn Env, os: Os) -> Option<String> {
    let names: &[&str] = match os {
        Os::Windows => &["USERPROFILE", "HOME"],
        Os::Unix => &["HOME"],
    };
    names
        .iter()
        .find_map(|n| env.var(n).filter(|v| !v.trim().is_empty()))
}

/// Whether a `PATH` entry or a base directory is absolute on `os`. The host
/// OS does not matter: a Unix path is checked as Unix even on Windows.
fn is_absolute(os: Os, p: &str) -> bool {
    match os {
        Os::Unix => p.starts_with('/'),
        Os::Windows => {
            let b = p.as_bytes();
            let drive = b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && (b[2] == b'\\' || b[2] == b'/');
            drive || p.starts_with("\\\\") || p.starts_with("//")
        }
    }
}

/// The `PATHEXT` extensions, lowercase with the dot. The Windows default is
/// used when the variable is unset.
fn pathext(env: &dyn Env) -> Vec<String> {
    let raw = env
        .var("PATHEXT")
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
    raw.split(';')
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

#[cfg(not(unix))]
fn has_pathext(path: &Path, exts: &[String]) -> bool {
    path.extension().is_some_and(|e| {
        let dotted = format!(".{}", e.to_string_lossy().to_ascii_lowercase());
        exts.contains(&dotted)
    })
}

fn contains_ci(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}
