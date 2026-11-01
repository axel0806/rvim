// SPDX-License-Identifier: Apache-2.0
//! Every Rust file under `src/`, `tests/` and `examples/` starts with an SPDX
//! license identifier, as the licensing scheme in PLAN.md ("Licensing")
//! requires. A file derived from Vim also carries the Vim notice on line 2.
//! This test needs no Vim and no terminal.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_rust_file_has_an_spdx_header() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["src", "tests", "examples"] {
        rust_files(&root.join(dir), &mut files);
    }
    assert!(!files.is_empty(), "no .rs files found");

    let mut bad = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        let mut lines = text.lines();
        let first = lines.next().unwrap_or("");
        let second = lines.next().unwrap_or("");
        let ok = match first {
            "// SPDX-License-Identifier: Apache-2.0" => true,
            "// SPDX-License-Identifier: Vim" => second.starts_with("// Derived in part from Vim"),
            _ => false,
        };
        if !ok {
            bad.push(file.display().to_string());
        }
    }
    assert!(
        bad.is_empty(),
        "missing or wrong SPDX header on the first line (a Vim file also needs the \
         Vim notice on line 2):\n{}",
        bad.join("\n")
    );
}
