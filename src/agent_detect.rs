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
    /// How the user signs in with the agent's own tool, for the "not logged
    /// in" message (`run ... in a shell`). rvim never signs in itself and never
    /// reads, stores or sends a credential: the agent keeps its own login, and
    /// a subscription (Claude Pro or Max, ChatGPT Plus or Pro, and so on) is
    /// used through that login.
    pub login: &'static str,
    /// ACP `authenticate` method ids that reuse the agent's existing login or
    /// run the agent's own sign-in, which is how a subscription user connects.
    /// They are listed first when the agent asks for authentication. Methods
    /// not named here (an API key from the environment, say) are still offered,
    /// after these; a method the agent marks `terminal` is never offered.
    pub login_methods: &'static [&'static str],
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
        acp: "npx -y @agentclientprotocol/claude-agent-acp@0.107.0",
        login: "run `claude auth login` in a shell",
        login_methods: &[],
        notes: "Native install puts claude in ~/.local/bin (Windows: %USERPROFILE%\\.local\\bin\\claude.exe). \
                The ACP adapter is the package in the ACP registry (the older @zed-industries name is gone); it needs \
                Node 22 or later, and on Windows the launcher npx is tried as npx.cmd. \
                Subscription: the adapter runs the Claude Agent SDK, which runs the claude binary with the login that \
                `claude auth login` stored (Pro, Max, Team and Enterprise plans), so rvim handles no credential. \
                The adapter advertises only terminal auth methods (claude-ai-login, claude-login, console-login), and \
                only to a client that declares terminal auth, which rvim does not yet; so rvim sees none and shows the login hint. \
                Anthropic's terms are not clear-cut for a third-party tool: see PLAN.md, \"Agent SDKs and subscriptions\". \
                The IDE protocol is documented for VS Code only (code.claude.com/docs/en/ide-integrations, read 2026-11-01): \
                a local WebSocket server on 127.0.0.1, a lock file ~/.claude/ide/<port>.lock (or $CLAUDE_CONFIG_DIR/ide/ \
                when set), and the token sent in the header X-Claude-Code-Ide-Authorization. The lock file's JSON schema \
                and the handshake are not documented: UNVERIFIED, and not a supported third-party API. \
                ~/.claude/local and %APPDATA%\\npm are install guesses: UNVERIFIED. CLAUDE_CONFIG_DIR is named only in the IDE docs.",
        sources: "code.claude.com/docs/en/setup and code.claude.com/docs/en/ide-integrations (read 2026-11-01); \
                  code.claude.com/docs/en/authentication and /legal-and-compliance, github.com/agentclientprotocol/claude-agent-acp \
                  src/auth-methods.ts, and the ACP registry entry claude-acp 0.107.0 (read 2026-11-13)",
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
                verified: true,
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
        acp: "npx -y @agentclientprotocol/codex-acp@2.6.0",
        login: "run `codex login` in a shell",
        login_methods: &["chat-gpt", "chat-gpt-device-code"],
        notes: "The CLI is codex (docs). Headless: codex exec (docs). The ACP adapter is the registry package \
                (the older @zed-industries name is gone); -y keeps npx from asking to install it. It starts the Codex \
                app server itself and bundles a compatible @openai/codex. Subscription: the method chat-gpt reuses a working \
                ChatGPT login (the one `codex login` cached in ~/.codex/auth.json or the OS store) and opens the \
                browser only when there is none; chat-gpt-device-code is the same with a one-time code and needs URL \
                elicitation, which rvim does not offer. api-key reads CODEX_API_KEY or OPENAI_API_KEY from the agent's \
                environment; gateway needs client data and is never offered. The method ids are from the adapter's \
                main branch, not its 2.6.0 tag: UNVERIFIED for the release. CODEX_HOME is not on the auth page: UNVERIFIED. \
                The app-server route (codex app-server, JSON-RPC over stdio) is documented but not used: see PLAN.md, \
                \"Agent SDKs and subscriptions\".",
        sources: "learn.chatgpt.com/docs/auth and /docs/app-server, github.com/agentclientprotocol/codex-acp \
                  src/CodexAuthMethod.ts and src/CodexAcpClient.ts, and the ACP registry entry codex-acp 2.6.0 (read 2026-11-13)",
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
                verified: true,
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
        login: "run `agent login` in a shell",
        login_methods: &["cursor_login"],
        notes: "The docs name the executable agent (cursor.com/docs/cli/overview), with install by curl or irm. \
                agent is a generic process name, so it counts only when its path (or its symlink target) contains \
                \"cursor\". cursor-agent is the executable the ACP registry names (read 2026-11-01). The Unix install location is UNVERIFIED. \
                Auth: the one method cursor_login; the docs say to sign in first with agent login, or with CURSOR_API_KEY or \
                CURSOR_AUTH_TOKEN, and do not say whether authenticate reuses that login or opens a browser: UNVERIFIED. \
                No config directory is known. The plan behind the login (Cursor Pro and so on) is not stated on the ACP page.",
        sources: "cursor.com/docs/cli/overview and cursor.com/docs/cli/acp (read 2026-11-13); \
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
        login: "run `kimi`, then /login",
        login_methods: &[],
        notes: "The executable is kimi; @moonshot-ai/kimi-code is the npm package, not the executable. ~/.kimi is the \
                legacy kimi-cli (its repository was archived 2026-09-23) and uses the same executable name. Login is \
                /login inside the UI. Headless -p: whether it auto-approves is not stated: UNVERIFIED, not used. \
                The installer location is not stated, so no install directory is probed.",
        sources: "kimi.com/code/docs (Kimi Code CLI getting started, read 2026-11-01); github.com/MoonshotAI/kimi-cli \
                  (read 2026-11-01); cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-01)",
    },
    AgentSpec {
        id: "gemini",
        display: "Gemini CLI",
        executables: &[Named {
            name: "gemini",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
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
        launch: "gemini",
        acp: "npx -y @google/gemini-cli@0.68.0 --acp",
        login: "run `gemini` and choose Sign in with Google",
        login_methods: &["oauth-personal"],
        notes: "The ACP command is the registry entry (npm @google/gemini-cli, pinned to 0.68.0; the docs name --acp only). \
                Methods advertised (source on main): oauth-personal (Log in with Google; authenticate reuses valid cached \
                credentials, or signs in), gemini-api-key, vertex-ai and gateway (needs client data, never offered). \
                A failed authenticate is -32000. SUBSCRIPTION CAVEAT: since 2026-06-18 Gemini CLI no longer serves Google AI Pro \
                and Ultra or free Code Assist for individuals (they moved to Antigravity CLI, registry id antigravity); it still \
                serves Code Assist Standard and Enterprise licences and paid API keys. Using its Google sign-in from \
                third-party software is called a terms violation on geminicli.com/docs/resources/tos-privacy. \
                No install or config directory is probed: UNVERIFIED.",
        sources: "geminicli.com/docs/cli/acp-mode, /docs/get-started/authentication and /docs/resources/tos-privacy, \
                  developers.googleblog.com (Gemini CLI to Antigravity CLI), github.com/google-gemini/gemini-cli \
                  packages/cli/src/acp/acpRpcDispatcher.ts, and the ACP registry entry gemini 0.68.0 (read 2026-11-13)",
    },
    AgentSpec {
        id: "github-copilot-cli",
        display: "GitHub Copilot CLI",
        executables: &[Named {
            name: "copilot",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
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
        launch: "copilot",
        acp: "npx -y @github/copilot@1.0.110 --acp",
        login: "run `copilot login` in a shell",
        login_methods: &[],
        notes: "The ACP command is the registry entry (npm @github/copilot, pinned to 1.0.110; ACP is a public preview). \
                Install: npm install -g @github/copilot (Node 22+), winget install GitHub.Copilot, brew install --cask copilot-cli. \
                It needs a GitHub Copilot subscription; the login (copilot login, device flow on a remote host) is stored in the \
                OS keychain, and COPILOT_GITHUB_TOKEN, GH_TOKEN or GITHUB_TOKEN override it. The ACP page does not list authMethods: \
                UNVERIFIED what initialize returns. Config locations are not probed.",
        sources: "docs.github.com copilot-cli ACP server, authenticate and install pages, and the ACP registry entry \
                  github-copilot-cli 1.0.110 (read 2026-11-13)",
    },
    AgentSpec {
        id: "opencode",
        display: "OpenCode",
        executables: &[Named {
            name: "opencode",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
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
        launch: "opencode",
        acp: "opencode acp",
        login: "run `opencode`, then /connect",
        login_methods: &[],
        notes: "The registry runs the executable opencode with the acp argument (binary release 2.2.7). Providers are \
                connected with /connect; its docs offer Sign in with ChatGPT (Plus or Pro plan) and GitHub Copilot (device \
                flow). It reads its own stored provider logins, so rvim handles no credential. Install and config locations \
                are not probed: UNVERIFIED. Whether the ACP mode advertises authMethods is UNVERIFIED.",
        sources: "opencode.ai/docs/acp and /docs/providers (read 2026-11-13); cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-13)",
    },
    AgentSpec {
        id: "goose",
        display: "Goose",
        executables: &[Named {
            name: "goose",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
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
        launch: "goose",
        acp: "goose acp",
        login: "run `goose configure` in a shell",
        login_methods: &[],
        notes: "The registry runs the executable goose with the acp argument (binary release 1.59.0). `goose configure` \
                sets up a provider, including ChatGPT Codex (browser OAuth, Plus or Pro plan) and GitHub Copilot (device flow). \
                goose can also use claude-acp and codex-acp as providers over the logged-in claude and codex CLIs. \
                Install and config locations are not probed: UNVERIFIED. Whether `goose acp` advertises authMethods is UNVERIFIED.",
        sources: "goose-docs.ai getting-started/providers and guides/acp-providers (read 2026-11-13); \
                  cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-13)",
    },
    AgentSpec {
        id: "qwen-code",
        display: "Qwen Code",
        executables: &[Named {
            name: "qwen",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
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
        launch: "qwen",
        acp: "npx -y @qwen-code/qwen-code@0.27.3 --acp --experimental-skills",
        login: "run `qwen`, then /auth",
        login_methods: &[],
        notes: "The ACP command is the registry entry (npm @qwen-code/qwen-code, pinned to 0.27.3). The executable is qwen \
                (README). /auth offers a Qwen account with a Plus or Pro subscription, the Alibaba Cloud Coding Plan \
                (a fixed monthly fee, its own endpoint and key) or an API key. The README does not name --acp: the flag is the \
                registry's. Config locations: not probed. Whether ACP mode advertises authMethods is UNVERIFIED.",
        sources: "github.com/QwenLM/qwen-code README and qwenlm.github.io/qwen-code-docs auth page (read 2026-11-13); \
                  cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-13)",
    },
    AgentSpec {
        id: "antigravity",
        display: "Antigravity CLI",
        executables: &[Named {
            name: "agy",
            verified: true,
            path_hint: None,
        }],
        install_dirs: &[],
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
        launch: "agy",
        acp: "agy acp",
        login: "run `agy acp --login` in a shell, or sign in in an interactive `agy`",
        login_methods: &["oauth-personal", "oauth-business"],
        notes: "Google's replacement for Gemini CLI (2026-06-18) and the route for Google AI Pro and Ultra subscribers. \
                `agy acp` is the ACP server (Antigravity docs, no minimum version given: check `agy acp --help`); it reuses \
                the CLI's cached sign-in and answers -32000 `Authentication required` without one. Methods advertised: \
                oauth-personal (Google personal), oauth-business (Gemini Enterprise), gemini-api-key, agent-platform, and \
                terminal-login for clients that support terminal auth. What authenticate does for each is UNVERIFIED. \
                The registry's separate agy_acp_server binary download (antigravity-acp 1.5.0, proprietary) is not used. \
                The install location is not documented, so none is probed.",
        sources: "antigravity.google/docs/cli/acp and /docs/ide/extensions/acp, developers.googleblog.com (Gemini CLI to \
                  Antigravity CLI), cdn.agentclientprotocol.com registry v1.0.0 (read 2026-11-13)",
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

/// What the editor keeps from a scan (milestone 7, step 4b). Plain data, so
/// it can travel in an `Event`: the report itself holds `&'static` registry
/// rows and is not kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Agents with an executable found, by registry id, in registry order.
    pub found: Vec<(&'static str, How)>,
    /// Agents with no executable but with a config directory.
    pub config_only: Vec<&'static str>,
    /// Agents with nothing found.
    pub not_found: Vec<&'static str>,
    /// The scan's warnings, as in [`ScanReport::warnings`].
    pub warnings: Vec<String>,
}

impl ScanReport {
    /// The part of the report the editor keeps.
    pub fn summary(&self) -> ScanSummary {
        let mut out = ScanSummary {
            warnings: self.warnings.clone(),
            ..ScanSummary::default()
        };
        for agent in AGENTS {
            if let Some(f) = self.found.iter().find(|f| std::ptr::eq(f.agent, agent)) {
                out.found.push((agent.id, f.how));
                continue;
            }
            let config = self
                .not_found
                .iter()
                .any(|n| std::ptr::eq(n.agent, agent) && n.config_dir.is_some());
            if config {
                out.config_only.push(agent.id);
            } else {
                out.not_found.push(agent.id);
            }
        }
        out
    }
}

impl ScanSummary {
    /// The message line text for a scan: what was found and how, what has a
    /// config directory only, what was not found, then the warnings. The
    /// message line cuts it to its width.
    pub fn message(&self) -> String {
        let mut parts = Vec::new();
        if !self.found.is_empty() {
            let list: Vec<String> = self
                .found
                .iter()
                .map(|(id, how)| {
                    let how = match how {
                        How::OnPath => "PATH",
                        How::InstallDir => "install dir",
                    };
                    format!("{id} ({how})")
                })
                .collect();
            parts.push(format!("found {}", list.join(", ")));
        }
        if !self.config_only.is_empty() {
            parts.push(format!("config only: {}", self.config_only.join(", ")));
        }
        if !self.not_found.is_empty() {
            parts.push(format!("not found: {}", self.not_found.join(", ")));
        }
        parts.extend(self.warnings.iter().map(|w| format!("warning: {w}")));
        parts.join(" | ")
    }
}

/// The status indicator text of the agent layer (PLAN.md, "Agent experience",
/// Lifecycle), or None when the layer is off. `scan` is the last scan's
/// result. Step 4b has no connections, so these are the states without one:
/// `files only` before any scan, `none` after a scan that found nothing, and
/// `<ids> found` otherwise.
pub fn indicator(active: bool, scan: Option<&ScanSummary>) -> Option<String> {
    if !active {
        return None;
    }
    Some(match scan {
        None => "ai: files only".to_string(),
        Some(s) if s.found.is_empty() => "ai: none".to_string(),
        Some(s) => {
            let ids: Vec<&str> = s.found.iter().map(|(id, _)| *id).collect();
            format!("ai: {} found", ids.join(", "))
        }
    })
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
