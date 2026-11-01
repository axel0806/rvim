// SPDX-License-Identifier: Apache-2.0
//! Command-line parsing and the text printed by `--help` and `--version`.
//! Terminal-free: `main.rs` prints the text and decides what to do.

use crate::VERSION_MESSAGE;
use std::ffi::OsString;

/// What the command line asks `rvim` to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Start the editor on `path` (or an empty buffer). `start_menu` is set
    /// by `--start-menu`; the last of `--start-menu` and `--no-start-menu`
    /// wins.
    Edit {
        start_menu: bool,
        path: Option<OsString>,
    },
    /// Print `help_text()` to stdout and exit 0.
    Help,
    /// Print `version_text()` to stdout and exit 0.
    Version,
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
    let mut start_menu = false;
    let mut path = None;
    let mut options = true;
    for arg in args {
        if options {
            match arg.to_str() {
                Some("--") => {
                    options = false;
                    continue;
                }
                Some("-h" | "--help") => return Action::Help,
                Some("-v" | "--version") => return Action::Version,
                Some("--start-menu") => {
                    start_menu = true;
                    continue;
                }
                Some("--no-start-menu") => {
                    start_menu = false;
                    continue;
                }
                _ => {}
            }
        }
        if path.is_none() {
            path = Some(arg);
        }
    }
    Action::Edit { start_menu, path }
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
  --no-start-menu    Do not show the start menu (the default)
  -h, --help         Print this help and exit
  -v, --version      Print the version and exit

Without a config file, rvim uses Vim's defaults and reads no vimrc.
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
            "-h, --help",
            "-v, --version",
            "--",
        ] {
            assert!(text.contains(flag), "help lacks {flag}");
        }
        // Options rvim does not parse must not be advertised.
        for flag in ["-u ", "-N", "--noplugin", "-R", "-es", "--modern"] {
            assert!(!text.contains(flag), "help mentions unsupported {flag}");
        }
        assert!(!text.contains('\x1b'));
    }
}
