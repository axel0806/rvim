// SPDX-License-Identifier: Apache-2.0
//! The approval store (PLAN.md, "Agent experience", sections 1, 2 and 5): which
//! agent may be started in which project. It is `agent-approvals.toml` in the
//! config directory, one `[[approval]]` table per (agent id, project root)
//! pair. Pure: [`parse`] and [`Approvals::to_text`] work on strings; [`load`]
//! and [`write`] are the only file access, and the front end calls them (the
//! read at startup, the write on a worker thread).
//!
//! A file that cannot be read or parsed counts as no approvals, with a warning.
//! It is then never overwritten: [`Approvals::is_broken`] makes the editor
//! refuse to save, so a damaged file is left for the user to repair.

use std::io;
use std::path::Path;

use serde::Deserialize;

/// The file name inside the config directory.
pub const FILE_NAME: &str = "agent-approvals.toml";

/// The largest approvals file read; anything bigger is treated as broken.
pub const READ_LIMIT: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    approval: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    agent: String,
    project: String,
}

/// The approved (agent id, project root) pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Approvals {
    entries: Vec<(String, String)>,
    broken: bool,
}

impl Approvals {
    /// An empty store that cannot be saved: the file exists but is unusable.
    pub fn broken() -> Self {
        Approvals {
            entries: Vec::new(),
            broken: true,
        }
    }

    /// Whether `agent` may be started in `project`.
    pub fn is_approved(&self, agent: &str, project: &str) -> bool {
        self.entries.iter().any(|(a, p)| a == agent && p == project)
    }

    /// Adds the pair (in memory; the caller saves it). Returns false when it
    /// was already there.
    pub fn approve(&mut self, agent: &str, project: &str) -> bool {
        if self.is_approved(agent, project) {
            return false;
        }
        self.entries.push((agent.to_owned(), project.to_owned()));
        true
    }

    /// True when the file was unreadable or invalid: nothing may be saved.
    pub fn is_broken(&self) -> bool {
        self.broken
    }

    /// The file text for these approvals.
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# rvim: agents approved to start in a project. Delete a table to revoke.\n",
        );
        for (agent, project) in &self.entries {
            out.push_str("\n[[approval]]\n");
            out.push_str(&format!("agent = {}\n", quote(agent)));
            out.push_str(&format!("project = {}\n", quote(project)));
        }
        out
    }
}

/// A TOML basic string for `s`.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Parses the file text. An entry with an empty agent or project is an error,
/// as is any TOML the parser rejects.
pub fn parse(text: &str) -> Result<Approvals, String> {
    let file: File = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    let mut approvals = Approvals::default();
    for entry in file.approval {
        if entry.agent.is_empty() || entry.project.is_empty() {
            return Err("an [[approval]] needs both agent and project".to_owned());
        }
        approvals.approve(&entry.agent, &entry.project);
    }
    Ok(approvals)
}

/// Reads the store at `path`. A missing file is no approvals and no warning.
/// An unreadable or invalid file is no approvals, a broken store, and a
/// warning for the message line.
pub fn load(path: &Path) -> (Approvals, Option<String>) {
    let bytes = match crate::config::read_capped(path, READ_LIMIT) {
        Ok(None) => return (Approvals::default(), None),
        Ok(Some(bytes)) => bytes,
        Err(e) => return (Approvals::broken(), Some(warning(&e.to_string()))),
    };
    let parsed = String::from_utf8(bytes)
        .map_err(|_| "not UTF-8".to_owned())
        .and_then(|text| parse(&text));
    match parsed {
        Ok(approvals) => (approvals, None),
        Err(e) => (Approvals::broken(), Some(warning(&e))),
    }
}

fn warning(reason: &str) -> String {
    format!(
        "{FILE_NAME} unreadable ({reason}): agents need approval again; the file is not overwritten"
    )
}

/// Writes `text` to `path`, creating the directory. The file is replaced in
/// one rename, so a crash leaves the old file or the new one.
pub fn write(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_pairs_and_escapes() {
        let mut a = Approvals::default();
        assert!(a.approve("claude", "C:\\Users\\me \"proj\""));
        assert!(a.approve("codex", "/home/me/p"));
        assert!(!a.approve("codex", "/home/me/p"));
        let back = parse(&a.to_text()).expect("parses");
        assert_eq!(back, a);
        assert!(back.is_approved("claude", "C:\\Users\\me \"proj\""));
        assert!(!back.is_approved("claude", "/home/me/p"));
    }

    #[test]
    fn invalid_text_is_an_error() {
        assert!(parse("[[approval]]\nagent = 3\n").is_err());
        assert!(parse("[[approval]]\nagent = \"\"\nproject = \"/p\"\n").is_err());
        assert!(parse("not = [toml").is_err());
    }
}
