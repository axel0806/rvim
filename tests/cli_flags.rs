// SPDX-License-Identifier: Apache-2.0
//! Runs the built `rvim` binary with `--help` and `--version`. Both print to
//! stdout and exit 0 without a terminal (no raw mode, no escape codes). The
//! parsing rules, including `--`, are unit-tested in `src/cli.rs`.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rvim"))
        .args(args)
        .output()
        .expect("rvim binary runs")
}

#[test]
fn help_prints_usage_and_exits_zero() {
    for flag in ["--help", "-h"] {
        let out = run(&[flag]);
        assert!(out.status.success(), "{flag}: {:?}", out.status);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Usage: rvim"), "{flag}: {stdout}");
        assert!(stdout.contains("--start-menu"), "{flag}: {stdout}");
        assert!(stdout.contains("--no-start-menu"), "{flag}: {stdout}");
        assert!(stdout.contains("--version"), "{flag}: {stdout}");
        assert!(!stdout.contains('\x1b'), "{flag} wrote escape codes");
    }
}

#[test]
fn version_prints_version_and_modified_notice_and_exits_zero() {
    for flag in ["--version", "-v"] {
        let out = run(&[flag]);
        assert!(out.status.success(), "{flag}: {:?}", out.status);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(env!("CARGO_PKG_VERSION")),
            "{flag}: {stdout}"
        );
        assert!(
            stdout.contains("a modified version of Vim"),
            "{flag}: {stdout}"
        );
        assert!(!stdout.contains('\x1b'), "{flag} wrote escape codes");
    }
}
