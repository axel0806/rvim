// SPDX-License-Identifier: Apache-2.0
//! Command-line parsing and the text printed by `--help` and `--version`.
//! Terminal-free: `main.rs` prints the text and decides what to do.

use crate::VERSION_MESSAGE;
use std::ffi::{OsStr, OsString};

/// What the command line asks `rvim` to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the editor on `path` (or an empty buffer). `start_menu` is set
    /// by `--start-menu`; the last of `--start-menu` and `--no-start-menu`
    /// wins. It is false when neither is given (see
    /// [`Invocation::start_menu`], which tells the two apart).
    Edit {
        start_menu: bool,
        path: Option<OsString>,
    },
    /// Print `help_text()` to stdout and exit 0.
    Help,
    /// Print `version_text()` to stdout and exit 0.
    Version,
    /// The command line is wrong: print the message to stderr and exit 2.
    Usage(String),
}

/// Whether the config file is read. `-u NONE` and `--clean` skip it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ConfigUse {
    Read,
    Skip,
}

/// The command line in full: what [`parse`] gives, plus the settings the
/// config file may supply, so the caller can tell "not given" from "false".
#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub action: Action,
    /// `--start-menu` or `--no-start-menu`, the last one given. None when
    /// neither is given, so the config's `[start] menu` decides.
    pub start_menu: Option<bool>,
    pub config: ConfigUse,
}

/// Parses the arguments after the program name, as Vim does: options may
/// come in any order, `--` ends the options (so later arguments are file
/// names even when they start with `-`), and the first file name is the one
/// opened. `-h`/`--help` and `-v`/`--version` act on the first occurrence,
/// even when a file name comes before them.
pub fn parse<I>(args: I) -> Action
where
    I: IntoIterator<Item = OsString>,
{
    parse_invocation(args).action
}

/// [`parse`] with the settings the config file may supply. Besides the
/// start menu flags it accepts `-u NONE` (skip the config file, as Vim's
/// `-u NONE` skips the vimrc), `--clean` (the same) and `-N` (accepted:
/// rvim always behaves as Vim with `-N`). Any other `-u` value is a usage
/// error, because rvim reads no other vimrc.
pub fn parse_invocation<I>(args: I) -> Invocation
where
    I: IntoIterator<Item = OsString>,
{
    let mut start_menu = None;
    let mut config = ConfigUse::Read;
    let mut path = None;
    let mut options = true;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if options {
            match arg.to_str() {
                Some("--") => {
                    options = false;
                    continue;
                }
                Some("-h" | "--help") => return invocation(Action::Help, start_menu, config),
                Some("-v" | "--version") => {
                    return invocation(Action::Version, start_menu, config);
                }
                Some("--start-menu") => {
                    start_menu = Some(true);
                    continue;
                }
                Some("--no-start-menu") => {
                    start_menu = Some(false);
                    continue;
                }
                Some("--clean") => {
                    config = ConfigUse::Skip;
                    continue;
                }
                Some("-N") => continue,
                Some("-u") => {
                    let Some(value) = args.next() else {
                        return usage("-u needs an argument: -u NONE".into());
                    };
                    match use_value(&value) {
                        Ok(c) => config = c,
                        Err(msg) => return usage(msg),
                    }
                    continue;
                }
                Some(s) if s.starts_with("-u") => match use_value(OsStr::new(&s[2..])) {
                    Ok(c) => {
                        config = c;
                        continue;
                    }
                    Err(msg) => return usage(msg),
                },
                _ => {}
            }
        }
        if path.is_none() {
            path = Some(arg);
        }
    }
    invocation(
        Action::Edit {
            start_menu: start_menu.unwrap_or(false),
            path,
        },
        start_menu,
        config,
    )
}

fn invocation(action: Action, start_menu: Option<bool>, config: ConfigUse) -> Invocation {
    Invocation {
        action,
        start_menu,
        config,
    }
}

fn usage(message: String) -> Invocation {
    invocation(Action::Usage(message), None, ConfigUse::Read)
}

/// The value of `-u`: only `NONE` is supported.
fn use_value(value: &OsStr) -> Result<ConfigUse, String> {
    if value == "NONE" {
        Ok(ConfigUse::Skip)
    } else {
        Err(format!(
            "-u {} is not supported; use -u NONE to skip the config file",
            value.to_string_lossy()
        ))
    }
}

/// The `--help` text. It lists only the options `parse` accepts.
pub fn help_text() -> String {
    "\
Usage: rvim [options] [file]

Edit the file, or start with an empty buffer when no file is given.
Only the first file name is opened; later ones are ignored.

Options:
  --                 Only file names after this (for names starting with -)
  --start-menu       Show the start menu when no file is given
  --no-start-menu    Do not show the start menu (overrides [start] menu)
  -u NONE            Do not read the config file
  --clean            The same as -u NONE
  -N                 Accepted; rvim always behaves as Vim with -N
  -h, --help         Print this help and exit
  -v, --version      Print the version and exit

The config file is $XDG_CONFIG_HOME/rvim/config.toml (default
~/.config/rvim/config.toml; on Windows %APPDATA%\\rvim\\config.toml). Without
one, rvim uses Vim's defaults and reads no vimrc.
"
    .to_string()
}

/// The `--version` text: the same line as `:version`, which carries the
/// version from Cargo and the notice that this is a modified Vim.
pub fn version_text() -> String {
    format!("{VERSION_MESSAGE}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn edit(start_menu: bool, path: Option<&str>) -> Action {
        Action::Edit {
            start_menu,
            path: path.map(OsString::from),
        }
    }

    #[test]
    fn no_arguments_starts_an_empty_editor() {
        assert_eq!(parse(args(&[])), edit(false, None));
    }

    #[test]
    fn first_file_name_is_the_path() {
        assert_eq!(parse(args(&["a.txt", "b.txt"])), edit(false, Some("a.txt")));
    }

    #[test]
    fn start_menu_flags_last_one_wins() {
        assert_eq!(parse(args(&["--start-menu"])), edit(true, None));
        assert_eq!(
            parse(args(&["--start-menu", "--no-start-menu"])),
            edit(false, None)
        );
        assert_eq!(
            parse(args(&["--no-start-menu", "--start-menu", "f"])),
            edit(true, Some("f"))
        );
    }

    #[test]
    fn help_and_version_are_recognised_in_any_position() {
        assert_eq!(parse(args(&["--help"])), Action::Help);
        assert_eq!(parse(args(&["-h"])), Action::Help);
        assert_eq!(parse(args(&["f.txt", "--help"])), Action::Help);
        assert_eq!(parse(args(&["--version"])), Action::Version);
        assert_eq!(parse(args(&["-v"])), Action::Version);
        assert_eq!(parse(args(&["--start-menu", "-h"])), Action::Help);
    }

    #[test]
    fn help_is_not_opened_as_a_file() {
        assert_ne!(parse(args(&["--help"])), edit(false, Some("--help")));
    }

    #[test]
    fn double_dash_makes_later_arguments_file_names() {
        assert_eq!(parse(args(&["--", "--help"])), edit(false, Some("--help")));
        assert_eq!(parse(args(&["--", "-v"])), edit(false, Some("-v")));
        assert_eq!(
            parse(args(&["--", "--start-menu"])),
            edit(false, Some("--start-menu"))
        );
        assert_eq!(
            parse(args(&["--start-menu", "--", "x"])),
            edit(true, Some("x"))
        );
    }

    #[test]
    fn second_double_dash_after_the_separator_is_a_file_name() {
        assert_eq!(parse(args(&["--", "--"])), edit(false, Some("--")));
    }

    #[test]
    fn version_text_has_cargo_version_and_modified_notice() {
        let text = version_text();
        assert!(text.starts_with("rvim "));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("a modified version of Vim"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn help_text_lists_supported_options_only() {
        let text = help_text();
        for flag in [
            "--start-menu",
            "--no-start-menu",
            "-u NONE",
            "--clean",
            "-N",
            "-h, --help",
            "-v, --version",
            "--",
            "config.toml",
        ] {
            assert!(text.contains(flag), "help lacks {flag}");
        }
        // Options rvim does not parse must not be advertised.
        for flag in [
            "--noplugin",
            "-R",
            "-es",
            "--modern",
            "-u DEFAULTS",
            "-u NORC",
        ] {
            assert!(!text.contains(flag), "help mentions unsupported {flag}");
        }
        assert!(!text.contains('\x1b'));
    }

    fn invocation(list: &[&str]) -> Invocation {
        parse_invocation(args(list))
    }

    #[test]
    fn u_none_and_clean_skip_the_config() {
        assert_eq!(invocation(&["-u", "NONE"]).config, ConfigUse::Skip);
        assert_eq!(invocation(&["-uNONE"]).config, ConfigUse::Skip);
        assert_eq!(invocation(&["--clean"]).config, ConfigUse::Skip);
        assert_eq!(invocation(&["f.txt"]).config, ConfigUse::Read);
        // `-u` takes its value: NONE is not a file name.
        assert_eq!(
            invocation(&["-u", "NONE", "f.txt"]).action,
            edit(false, Some("f.txt"))
        );
        assert_eq!(invocation(&["-u", "NONE"]).action, edit(false, None));
    }

    #[test]
    fn other_u_values_are_usage_errors() {
        let Action::Usage(msg) = invocation(&["-u", "vimrc"]).action else {
            panic!("expected a usage error");
        };
        assert!(msg.contains("-u vimrc is not supported"), "{msg}");
        assert!(matches!(invocation(&["-u"]).action, Action::Usage(_)));
        assert!(matches!(
            invocation(&["-uDEFAULTS"]).action,
            Action::Usage(_)
        ));
    }

    #[test]
    fn n_is_accepted_and_changes_nothing() {
        assert_eq!(invocation(&["-N", "f"]), invocation(&["f"]));
    }

    #[test]
    fn after_double_dash_u_is_a_file_name() {
        assert_eq!(invocation(&["--", "-u"]).action, edit(false, Some("-u")));
        assert_eq!(invocation(&["--", "-u"]).config, ConfigUse::Read);
    }

    #[test]
    fn start_menu_is_none_when_no_flag_is_given() {
        assert_eq!(invocation(&[]).start_menu, None);
        assert_eq!(invocation(&["--start-menu"]).start_menu, Some(true));
        assert_eq!(
            invocation(&["--start-menu", "--no-start-menu"]).start_menu,
            Some(false)
        );
        // The action still carries a plain bool for `parse` callers.
        assert_eq!(invocation(&[]).action, edit(false, None));
    }
}
