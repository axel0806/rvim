// SPDX-License-Identifier: Apache-2.0
//! The config file, `$XDG_CONFIG_HOME/rvim/config.toml` (PLAN.md, "Editing
//! experience", Config). Only the first tables are read; the rest of the
//! plan (profile, mappings, theme, Lua) is later work.
//!
//! ```toml
//! [options]          # any option of the registry, with its `:set` name
//! shiftwidth = 2     # a number for a number option, true or false for a flag
//! expandtab = true
//!
//! [start]
//! menu = true        # the start menu when no flag is given
//!
//! [agent]
//! enabled = true     # agent indicators, and a scan at startup
//! ```
//!
//! Nothing here blocks or touches the terminal. The one file read is
//! [`load`]'s job, and it takes the reader as a parameter, so tests pass a
//! closure. `main.rs` calls it once, before the terminal is set up, and
//! `Editor` never reads the file: it gets the parsed [`Config`] through
//! `Editor::apply_config` (`options.rs`), which uses the `:set` code path.
//! Unknown keys and bad values are warnings, never errors: the file is
//! read as far as it is valid, and the rest keeps the defaults.

use crate::options::{INERT, Options, TABLE, Val};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use toml::{Table, Value};

/// The file name inside the rvim config directory.
pub const FILE_NAME: &str = "config.toml";

/// Larger files are not read (a warning says so). A config is a few lines.
pub const MAX_BYTES: u64 = 256 * 1024;

/// A value for an option of the registry, typed as the option is.
#[derive(Clone, Debug, PartialEq)]
pub enum OptValue {
    Bool(bool),
    Num(i64),
}

/// The parsed config. The default is the config of a missing file: no
/// options, no start menu switch, no agent table, no warnings.
#[derive(Debug, Default, PartialEq)]
pub struct Config {
    /// The `[options]` entries that name an option of the registry and have
    /// its type, in the order of the file's keys (sorted by name, since the
    /// TOML tables are maps), with the registry's name (not a short name).
    /// Vim options that rvim accepts and ignores are dropped silently.
    pub options: Vec<(&'static str, OptValue)>,
    /// `[start] menu`, or None when the file does not set it.
    pub start_menu: Option<bool>,
    /// The `[agent]` table: the opt-in that shows the agent indicators.
    pub agent: AgentConfig,
    /// Each problem, as one line for the message line; each starts with
    /// `config:`.
    pub warnings: Vec<String>,
}

/// The `[agent]` keys read in this version (PLAN.md, "Agent experience",
/// Config). Only `enabled` is read. Its default is off, so with no config
/// nothing agent-related runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentConfig {
    /// `enabled = true`: the agent indicators are shown and a scan runs at
    /// startup (`agent_scan.rs`). It also turns on the conflict prompt and
    /// the write guard (`conflict.rs`). It turns on nothing else.
    pub enabled: bool,
}

impl Config {
    /// Whether `[agent] enabled = true` is set: the agent opt-in (PLAN.md,
    /// "Agent experience"). The `--agent` flag is the same switch for one
    /// run; `main.rs` sets `agent.enabled` for it before the config is applied.
    pub fn agent_enabled(&self) -> bool {
        self.agent.enabled
    }
}

/// The path of `config.toml` for an environment. `env` looks up a variable
/// (None when unset); `windows` selects the Windows rules.
///
/// The directory is the first of: `$XDG_CONFIG_HOME` (only when absolute,
/// as the XDG spec says); on Windows `%APPDATA%`; `$HOME/.config` (on Windows
/// `%USERPROFILE%`, then `$HOME`). The file is `rvim/config.toml` inside it.
/// Empty variables count as unset. None when no base directory is known.
pub fn config_path(env: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty());
    let base = if let Some(x) = var("XDG_CONFIG_HOME").filter(|x| is_absolute(x, windows)) {
        PathBuf::from(x)
    } else if windows && let Some(appdata) = var("APPDATA") {
        PathBuf::from(appdata)
    } else {
        let home = if windows {
            var("USERPROFILE").or_else(|| var("HOME"))
        } else {
            var("HOME")
        };
        PathBuf::from(home?).join(".config")
    };
    Some(base.join("rvim").join(FILE_NAME))
}

/// Whether `value` is an absolute path on the platform the rules are for.
/// The check is by text so that Windows paths can be tested on any host.
fn is_absolute(value: &OsString, windows: bool) -> bool {
    let Some(s) = value.to_str() else {
        return false;
    };
    let b = s.as_bytes();
    if s.starts_with('/') {
        return true;
    }
    windows
        && (s.starts_with('\\')
            || (b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && (b[2] == b'\\' || b[2] == b'/')))
}

/// Reads at most `limit` bytes of the file at `path`. `Ok(None)` when the
/// file does not exist; other errors are returned. This is the reader
/// `main.rs` passes to [`load`].
pub fn read_capped(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// Reads and parses the config at `path`. `read(path, limit)` is the file
/// reader: it returns at most `limit` bytes, or None for a missing file.
/// A missing file is the default config and no warning. An unreadable file,
/// one over [`MAX_BYTES`] and one that is not UTF-8 are the default config
/// with a warning.
pub fn load(path: &Path, read: impl FnOnce(&Path, u64) -> io::Result<Option<Vec<u8>>>) -> Config {
    let bytes = match read(path, MAX_BYTES + 1) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Config::default(),
        Err(e) => return failed(format!("config: {} cannot be read: {e}", path.display())),
    };
    if bytes.len() as u64 > MAX_BYTES {
        return failed(format!(
            "config: {} is larger than {} KiB; not read",
            path.display(),
            MAX_BYTES / 1024
        ));
    }
    match String::from_utf8(bytes) {
        Ok(text) => parse(&text),
        Err(e) => {
            let at = e.utf8_error().valid_up_to();
            let line = line_of(e.as_bytes(), at);
            failed(format!(
                "config: line {line}: not valid UTF-8; file not read"
            ))
        }
    }
}

fn failed(warning: String) -> Config {
    Config {
        warnings: vec![warning],
        ..Config::default()
    }
}

/// The 1-based line of byte offset `at` in `bytes`.
fn line_of(bytes: &[u8], at: usize) -> usize {
    bytes[..at.min(bytes.len())]
        .iter()
        .filter(|&&c| c == b'\n')
        .count()
        + 1
}

/// Parses config text. A syntax error gives the default config with one
/// warning that names the line; otherwise each table is read on its own and
/// its problems are warnings.
pub fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    let table: Table = match text.parse() {
        Ok(table) => table,
        Err(e) => {
            let line = e.span().map_or(1, |s| line_of(text.as_bytes(), s.start));
            cfg.warnings.push(format!(
                "config: line {line}: {}; file not read",
                e.message()
            ));
            return cfg;
        }
    };
    for (key, value) in table {
        match (key.as_str(), value) {
            ("options", v) => options(v, &mut cfg),
            ("start", v) => start(v, &mut cfg),
            ("agent", v) => agent(v, &mut cfg),
            ("profile" | "lua", _) => cfg
                .warnings
                .push(format!("config: [{key}] is not supported yet; ignored")),
            _ => cfg
                .warnings
                .push(format!("config: unknown key \"{key}\"; ignored")),
        }
    }
    cfg
}

fn options(value: Value, cfg: &mut Config) {
    let Value::Table(table) = value else {
        cfg.warnings
            .push("config: [options] must be a table; ignored".into());
        return;
    };
    for (name, value) in table {
        if INERT.contains(&name.as_str()) {
            continue;
        }
        let Some(i) = Options::find(&name) else {
            cfg.warnings
                .push(format!("config: unknown option \"{name}\"; ignored"));
            continue;
        };
        let def = &TABLE[i];
        let typed = match (&def.default, &value) {
            (Val::Bool(_), Value::Boolean(b)) => Some(OptValue::Bool(*b)),
            (Val::Num(_), Value::Integer(n)) => Some(OptValue::Num(*n)),
            (Val::Bool(_), _) => {
                cfg.warnings.push(format!(
                    "config: option \"{name}\" takes true or false; ignored"
                ));
                None
            }
            (Val::Num(_), _) => {
                cfg.warnings
                    .push(format!("config: option \"{name}\" takes a number; ignored"));
                None
            }
        };
        if let Some(v) = typed {
            cfg.options.push((def.name, v));
        }
    }
}

fn agent(value: Value, cfg: &mut Config) {
    let Value::Table(table) = value else {
        cfg.warnings
            .push("config: [agent] must be a table; ignored".into());
        return;
    };
    for (key, value) in table {
        match (key.as_str(), value) {
            ("enabled", Value::Boolean(on)) => cfg.agent.enabled = on,
            ("enabled", _) => cfg
                .warnings
                .push("config: [agent] enabled takes true or false; ignored".into()),
            // Planned for later steps (connections, logs); not read yet.
            ("shareable" | "log", _) => cfg.warnings.push(format!(
                "config: [agent] {key} is not supported yet; ignored"
            )),
            _ => cfg
                .warnings
                .push(format!("config: unknown key \"agent.{key}\"; ignored")),
        }
    }
}

fn start(value: Value, cfg: &mut Config) {
    let Value::Table(table) = value else {
        cfg.warnings
            .push("config: [start] must be a table; ignored".into());
        return;
    };
    for (key, value) in table {
        match (key.as_str(), value) {
            ("menu", Value::Boolean(on)) => cfg.start_menu = Some(on),
            ("menu", _) => cfg
                .warnings
                .push("config: [start] menu takes true or false; ignored".into()),
            _ => cfg
                .warnings
                .push(format!("config: unknown key \"start.{key}\"; ignored")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: Vec<(String, OsString)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name: &str| map.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    fn sole_warning(cfg: &Config) -> &str {
        assert_eq!(cfg.warnings.len(), 1, "warnings: {:?}", cfg.warnings);
        &cfg.warnings[0]
    }

    // ---- path resolution -------------------------------------------------

    #[test]
    fn xdg_config_home_wins_when_absolute() {
        let p = config_path(env(&[("XDG_CONFIG_HOME", "/x/cfg"), ("HOME", "/h")]), false);
        assert_eq!(p, Some(PathBuf::from("/x/cfg/rvim/config.toml")));
    }

    #[test]
    fn relative_xdg_config_home_is_ignored() {
        let p = config_path(env(&[("XDG_CONFIG_HOME", "rel"), ("HOME", "/h")]), false);
        assert_eq!(p, Some(PathBuf::from("/h/.config/rvim/config.toml")));
    }

    #[test]
    fn empty_variables_count_as_unset() {
        let p = config_path(env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]), false);
        assert_eq!(p, Some(PathBuf::from("/h/.config/rvim/config.toml")));
    }

    #[test]
    fn home_default_on_unix() {
        let p = config_path(env(&[("HOME", "/home/u")]), false);
        assert_eq!(p, Some(PathBuf::from("/home/u/.config/rvim/config.toml")));
        // On Unix, APPDATA and USERPROFILE mean nothing.
        let p = config_path(env(&[("HOME", "/home/u"), ("APPDATA", "C:\\A")]), false);
        assert_eq!(p, Some(PathBuf::from("/home/u/.config/rvim/config.toml")));
    }

    #[test]
    fn appdata_on_windows_then_home() {
        let appdata = PathBuf::from("C:\\Users\\axel\\AppData\\Roaming");
        let p = config_path(
            env(&[
                ("APPDATA", "C:\\Users\\axel\\AppData\\Roaming"),
                ("USERPROFILE", "C:\\Users\\axel"),
            ]),
            true,
        );
        assert_eq!(p, Some(appdata.join("rvim").join("config.toml")));

        let home = PathBuf::from("C:\\Users\\axel").join(".config");
        let p = config_path(env(&[("USERPROFILE", "C:\\Users\\axel")]), true);
        assert_eq!(p, Some(home.join("rvim").join("config.toml")));
        let p = config_path(env(&[("HOME", "C:\\h")]), true);
        assert_eq!(
            p,
            Some(
                PathBuf::from("C:\\h")
                    .join(".config")
                    .join("rvim")
                    .join("config.toml")
            )
        );
    }

    #[test]
    fn windows_xdg_must_be_absolute_in_either_form() {
        let p = config_path(
            env(&[("XDG_CONFIG_HOME", "C:\\cfg"), ("APPDATA", "C:\\A")]),
            true,
        );
        assert_eq!(
            p,
            Some(PathBuf::from("C:\\cfg").join("rvim").join("config.toml"))
        );
        let p = config_path(
            env(&[("XDG_CONFIG_HOME", "cfg"), ("APPDATA", "C:\\A")]),
            true,
        );
        assert_eq!(
            p,
            Some(PathBuf::from("C:\\A").join("rvim").join("config.toml"))
        );
    }

    #[test]
    fn no_base_directory_gives_no_path() {
        assert_eq!(config_path(env(&[]), false), None);
        assert_eq!(config_path(env(&[("APPDATA", "C:\\A")]), false), None);
    }

    // ---- loading ---------------------------------------------------------

    #[test]
    fn missing_file_is_the_default_without_warning() {
        let cfg = load(Path::new("/nope/config.toml"), |_, _| Ok(None));
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn unreadable_file_warns() {
        let cfg = load(Path::new("/d/config.toml"), |_, _| {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        });
        assert_eq!(cfg.options, Vec::new());
        assert!(sole_warning(&cfg).contains("cannot be read: denied"));
    }

    #[test]
    fn oversize_file_is_not_read() {
        let big = vec![b'#'; (MAX_BYTES + 10) as usize];
        let cfg = load(Path::new("c.toml"), |_, limit| {
            Ok(Some(big[..(limit as usize).min(big.len())].to_vec()))
        });
        assert_eq!(cfg.options, Vec::new());
        assert!(sole_warning(&cfg).contains("larger than 256 KiB; not read"));
    }

    #[test]
    fn file_of_exactly_the_limit_is_read() {
        let mut text = b"[options]\nshiftwidth = 2\n".to_vec();
        text.resize(MAX_BYTES as usize, b' ');
        let cfg = load(Path::new("c.toml"), |_, limit| {
            Ok(Some(text[..(limit as usize).min(text.len())].to_vec()))
        });
        assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
        assert_eq!(cfg.options, vec![("shiftwidth", OptValue::Num(2))]);
    }

    #[test]
    fn invalid_utf8_names_the_line() {
        let cfg = load(Path::new("c.toml"), |_, _| {
            Ok(Some(b"a = 1\n\xff\n".to_vec()))
        });
        assert!(sole_warning(&cfg).starts_with("config: line 2: not valid UTF-8"));
    }

    // ---- parsing -----------------------------------------------------------

    #[test]
    fn empty_file_is_the_default() {
        assert_eq!(parse(""), Config::default());
        assert_eq!(parse("# only a comment\n"), Config::default());
    }

    #[test]
    fn valid_options_start_and_agent() {
        let cfg = parse(
            "[options]\nshiftwidth = 2\nexpandtab = true\nsw = 4\nhls = false\n\
             [start]\nmenu = true\n[agent]\nenabled = true\n",
        );
        assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
        // Sorted by key name; the short name `sw` maps to `shiftwidth`.
        assert_eq!(
            cfg.options,
            vec![
                ("expandtab", OptValue::Bool(true)),
                ("hlsearch", OptValue::Bool(false)),
                ("shiftwidth", OptValue::Num(2)),
                ("shiftwidth", OptValue::Num(4)),
            ]
        );
        assert_eq!(cfg.start_menu, Some(true));
        assert!(cfg.agent.enabled);
    }

    #[test]
    fn dotted_start_menu_key_is_the_same_as_the_table() {
        assert_eq!(parse("start.menu = false\n").start_menu, Some(false));
    }

    #[test]
    fn unknown_key_warns_and_keeps_the_rest() {
        let cfg = parse("colour = 1\n[options]\nexpandtab = true\n");
        assert_eq!(cfg.options, vec![("expandtab", OptValue::Bool(true))]);
        assert_eq!(
            sole_warning(&cfg),
            "config: unknown key \"colour\"; ignored"
        );
    }

    #[test]
    fn unknown_option_and_unknown_start_key_warn() {
        let cfg = parse("[options]\nbogus = 1\n[start]\nrecent_files = 3\n");
        assert!(cfg.options.is_empty());
        assert_eq!(cfg.warnings.len(), 2);
        assert!(cfg.warnings[0].contains("unknown option \"bogus\""));
        assert!(cfg.warnings[1].contains("unknown key \"start.recent_files\""));
    }

    #[test]
    fn wrong_types_warn() {
        let cfg = parse("[options]\nshiftwidth = true\nexpandtab = 1\n[start]\nmenu = \"yes\"\n");
        assert!(cfg.options.is_empty());
        assert_eq!(cfg.start_menu, None);
        assert_eq!(cfg.warnings.len(), 3, "{:?}", cfg.warnings);
        let all = cfg.warnings.join("\n");
        assert!(all.contains("\"shiftwidth\" takes a number"));
        assert!(all.contains("\"expandtab\" takes true or false"));
        assert!(all.contains("menu takes true or false"));
        let cfg = parse("options = 3\nstart = []\nagent = 1\n");
        assert_eq!(cfg.warnings.len(), 3);
    }

    #[test]
    fn vim_options_rvim_ignores_are_accepted_silently() {
        let cfg = parse("[options]\nlist = true\nruler = false\n");
        assert!(cfg.options.is_empty());
        assert!(cfg.warnings.is_empty());
    }

    #[test]
    fn agent_enabled_is_the_only_key_read() {
        assert!(!parse("").agent.enabled, "off by default");
        let cfg = parse("[agent]\nenabled = false\n");
        assert!(!cfg.agent.enabled);
        assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
    }

    #[test]
    fn agent_bad_values_and_unread_keys_warn() {
        let cfg = parse("[agent]\nenabled = \"yes\"\n");
        assert!(!cfg.agent.enabled);
        assert!(sole_warning(&cfg).contains("[agent] enabled takes true or false"));

        let cfg = parse("[agent]\nshareable = [\"C:/p\"]\nlog = \"events\"\nbogus = 1\n");
        assert!(!cfg.agent.enabled);
        assert_eq!(cfg.warnings.len(), 3, "{:?}", cfg.warnings);
        let all = cfg.warnings.join("\n");
        assert!(all.contains("[agent] shareable is not supported yet"));
        assert!(all.contains("[agent] log is not supported yet"));
        assert!(all.contains("unknown key \"agent.bogus\""));

        let cfg = parse("agent = 1\n");
        assert_eq!(cfg.warnings.len(), 1);
        assert!(cfg.warnings[0].contains("[agent] must be a table"));
    }

    #[test]
    fn agent_opt_in_starts_the_layer_and_a_startup_scan() {
        use crate::agent_scan::AgentCmd;
        let (mut ed, problems) = applied("[agent]\nenabled = true\n", None);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(ed.agent_active);
        assert_eq!(ed.take_agent_commands(), vec![AgentCmd::Scan]);

        // No opt-in: no layer, no scan, nothing to take.
        let (mut ed, _) = applied("", None);
        assert!(!ed.agent_active);
        assert!(ed.take_agent_commands().is_empty());
    }

    #[test]
    fn planned_tables_warn_as_not_supported_yet() {
        let cfg = parse("[profile]\nmodern = true\n[lua]\nenable = true\n");
        assert_eq!(cfg.warnings.len(), 2, "{:?}", cfg.warnings);
        let all = cfg.warnings.join("\n");
        assert!(all.contains("[profile] is not supported yet"));
        assert!(all.contains("[lua] is not supported yet"));
    }

    #[test]
    fn syntax_error_names_the_line_and_uses_defaults() {
        let cfg = parse("[options]\nshiftwidth = 2\n\nexpandtab = \n");
        assert_eq!(cfg.options, Vec::new());
        let w = sole_warning(&cfg);
        assert!(w.starts_with("config: line 4: "), "{w}");
        assert!(w.ends_with("; file not read"), "{w}");
    }

    #[test]
    fn duplicate_key_is_a_syntax_error() {
        let cfg = parse("a = 1\na = 2\n");
        assert!(sole_warning(&cfg).starts_with("config: line 2: "));
    }

    #[test]
    fn line_of_counts_newlines_before_the_offset() {
        assert_eq!(line_of(b"ab\ncd\nef", 0), 1);
        assert_eq!(line_of(b"ab\ncd\nef", 3), 2);
        assert_eq!(line_of(b"ab\ncd\nef", 100), 3);
    }

    // ---- applying --------------------------------------------------------

    fn applied(text: &str, flag: Option<bool>) -> (Editor, Vec<String>) {
        let cfg = parse(text);
        let mut problems = cfg.warnings.clone();
        let mut ed = Editor::from_text("abc\n");
        problems.extend(ed.apply_config(&cfg, flag));
        (ed, problems)
    }

    #[test]
    fn shiftwidth_and_expandtab_apply_as_set() {
        let (mut ed, problems) = applied("[options]\nshiftwidth = 2\nexpandtab = true\n", None);
        assert!(problems.is_empty(), "{problems:?}");
        ed.feed(">>");
        assert_eq!(ed.buf().text(), "  abc\n");
    }

    #[test]
    fn buffer_local_option_is_effective_in_a_fresh_editor_too() {
        // `:set` writes the global value as well, so a document made after
        // the config copies it (as Vim does for new buffers).
        let (ed, _) = applied("[options]\nshiftwidth = 3\n", None);
        assert_eq!(ed.eff().value(idx("shiftwidth")), Val::Num(3));
        let mut fresh = Editor::new();
        fresh.apply_config(&parse("[options]\nshiftwidth = 3\n"), None);
        assert_eq!(fresh.eff().value(idx("shiftwidth")), Val::Num(3));
        // The global value is the one a new document copies.
        let mut ed = Editor::from_text("x\n");
        ed.apply_config(&parse("[options]\nshiftwidth = 5\n"), None);
        assert_eq!(ed.opts.get(idx("shiftwidth")), &Val::Num(5));
    }

    #[test]
    fn start_menu_from_config_and_the_flag_override_it() {
        let (ed, _) = applied("[start]\nmenu = true\n", None);
        assert!(ed.eff().startmenu());
        let (ed, _) = applied("[start]\nmenu = true\n", Some(false));
        assert!(
            !ed.eff().startmenu(),
            "--no-start-menu wins over the config"
        );
        let (ed, _) = applied("", Some(true));
        assert!(ed.eff().startmenu());
        let (ed, _) = applied("[options]\nstartmenu = true\n", None);
        assert!(ed.eff().startmenu());
    }

    #[test]
    fn no_config_leaves_the_options_at_their_defaults() {
        let ed = Editor::from_text("x\n");
        let defaults = Options::default();
        for (i, def) in TABLE.iter().enumerate() {
            assert_eq!(ed.eff().value(i), *defaults.get(i), "option {}", def.name);
        }
        // And applying the empty config changes nothing.
        let (ed2, problems) = applied("", None);
        assert!(problems.is_empty());
        for (i, def) in TABLE.iter().enumerate() {
            assert_eq!(ed2.eff().value(i), *defaults.get(i), "option {}", def.name);
        }
    }

    #[test]
    fn values_the_editor_refuses_come_back_as_messages() {
        let cfg = Config {
            options: vec![("shiftwidth", OptValue::Num(-1))],
            ..Config::default()
        };
        let mut ed = Editor::new();
        let problems = ed.apply_config(&cfg, None);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("E487"), "{problems:?}");
    }

    fn idx(name: &str) -> usize {
        Options::find(name).expect("option exists")
    }
}
