// SPDX-License-Identifier: Apache-2.0
//! The conflict flow for a file that changed on disk while its buffer is
//! modified (PLAN.md, "Agent experience", "Files changed under rvim" and the
//! conflict prompt). It is opt-in: with the agent opt-in off (`[agent]
//! enabled`, or `--agent`) none of this runs, and the warnings W11, W12 and
//! E211 are Vim's, as before.
//!
//! What it does, with the opt-in on:
//! - A modified buffer whose file changed is not reloaded. The prompt
//!   `[l]oad disk [k]eep mine [d]iff [m]erge` is shown on the message line,
//!   and it takes its keys until one of `l`, `k`, `m` or `<Esc>` is pressed
//!   (`d` is not built yet; other keys beep).
//! - `l` loads the disk text as one undo step tagged `external`.
//! - `k` keeps the buffer. The disk state it was shown is recorded as seen, so
//!   the prompt does not come back for it, but `:w` still refuses to
//!   overwrite a file that differs from the last read or written text.
//! - `m` merges (`merge.rs`, base = the last text known to be on disk): the
//!   result is one undo step tagged `user`, the buffer stays modified, and the
//!   conflicting hunks carry conflict markers. `:w` refuses while such markers
//!   are in the text (`:w!` overrides).
//! - A deleted file keeps the buffer and is reported (`:w` writes it again).
//!
//! The baseline and the disk state are kept per document (`Document::baseline`
//! and `Document::disk`); they are set by every read and write of the file.

use crate::buffer::{Buffer, read_file};
use crate::document::DocId;
use crate::editor::Editor;
use crate::key::Key;
use crate::merge::{MARK_MINE, MARK_THEIRS, merge3};
use crate::reload::{Change, Decision, DiskState, Stale, external_change_decision};
use crate::undo::UndoSource;
use std::path::Path;

/// The open prompt: the document, and the disk state the prompt was shown
/// for (the state `k` records as seen).
#[derive(Clone, Debug)]
pub(crate) struct Conflict {
    pub doc: DocId,
    pub seen: DiskState,
}

/// The prompt's text on the message line.
pub(crate) fn prompt_message(name: &str) -> String {
    format!("\"{name}\" changed on disk and in rvim. [l]oad disk [k]eep mine [d]iff [m]erge")
}

/// The message after a reload (Vim's W11 family is not used for it).
pub(crate) fn reload_message(name: &str, change: Option<Change>) -> String {
    match change {
        None => format!("\"{name}\" changed on disk"),
        Some(c) if c.fallback => {
            format!("\"{name}\" changed on disk; line diff too large, cursor kept on its line")
        }
        Some(c) => format!(
            "\"{name}\" changed on disk: +{} -{} lines",
            c.added, c.removed
        ),
    }
}

impl Editor {
    /// Turns the agent opt-in on or off (`[agent] enabled` in the config, or
    /// `--agent` for one run). Off by default.
    pub fn set_agent_enabled(&mut self, on: bool) {
        self.agent_enabled = on;
    }

    /// The conflict prompt takes the key. Only `l`, `k`, `m`, `<Esc>` and `d`
    /// do anything; every other key beeps.
    pub(crate) fn conflict_key(&mut self, key: Key) {
        let Some(c) = self.conflict.clone() else {
            return;
        };
        match key {
            Key::Char('l') => self.conflict_load(c.doc),
            Key::Char('k') | Key::Esc => self.conflict_keep(c),
            Key::Char('m') => self.conflict_merge(c.doc),
            Key::Char('d') => {
                self.info("The diff view is not built yet; use [l]oad, [k]eep or [m]erge")
            }
            _ => self.beep(),
        }
    }

    /// Opens the prompt for `doc`, whose file is now `seen`.
    pub(crate) fn open_conflict(&mut self, doc: DocId, seen: DiskState) {
        self.conflict = Some(Conflict { doc, seen });
    }

    /// `l`: the disk text replaces the buffer as one undo step tagged
    /// `external`; the user's text is one `u` away.
    fn conflict_load(&mut self, doc: DocId) {
        self.conflict = None;
        let Some(path) = self.document(doc).buf.path.clone() else {
            return;
        };
        let name = path.display().to_string();
        match read_file(&path) {
            Ok(Some((mtime, bytes))) => {
                let state = DiskState::of(mtime, &bytes);
                let (new, _) = Buffer::from_file_bytes(&path, Some(bytes.as_slice()));
                let lines = new.lines();
                let change = self.apply_external_change(doc, new, UndoSource::External);
                self.sync_disk(doc, state, lines);
                self.info(reload_message(&name, change));
            }
            Ok(None) => self.deleted_on_disk(doc, &name),
            Err(e) => self.error(format!("E211: File \"{name}\" no longer available ({e})")),
        }
    }

    /// `k`: the buffer is kept. The disk state the prompt showed is recorded
    /// as seen, so the prompt does not come back for it.
    fn conflict_keep(&mut self, c: Conflict) {
        self.conflict = None;
        let name = self.name_of(c.doc);
        self.document_mut(c.doc).seen = Some(c.seen);
        self.info(format!(
            "\"{name}\" kept; :w refuses to overwrite it (add ! to override)"
        ));
    }

    /// `m`: a three-way merge of the baseline, the buffer and the file now, as
    /// one undo step tagged `user`. The buffer stays modified; conflicting
    /// hunks carry markers (`merge.rs`).
    fn conflict_merge(&mut self, doc: DocId) {
        self.conflict = None;
        let Some(path) = self.document(doc).buf.path.clone() else {
            return;
        };
        let name = path.display().to_string();
        let (state, theirs, merged, eol) = match read_file(&path) {
            Ok(Some((mtime, bytes))) => {
                let state = DiskState::of(mtime, &bytes);
                let (theirs_buf, _) = Buffer::from_file_bytes(&path, Some(bytes.as_slice()));
                let theirs = theirs_buf.lines();
                let d = self.document(doc);
                let base = d.baseline.clone().unwrap_or_default();
                let merged = merge3(&base, &d.buf.lines(), &theirs);
                let eol = if d.buf.crlf { "\r\n" } else { "\n" };
                (state, theirs, merged, eol)
            }
            Ok(None) => return self.deleted_on_disk(doc, &name),
            Err(e) => {
                return self.error(format!("E211: File \"{name}\" no longer available ({e})"));
            }
        };
        let mut text = String::new();
        for line in &merged.lines {
            text.push_str(line);
            text.push_str(eol);
        }
        let (new, _) = Buffer::from_file_bytes(&path, Some(text.as_bytes()));
        self.apply_text_change(doc, new, UndoSource::User, true);
        // The file now is the base of the next merge; the buffer still differs
        // from it, so the buffer stays modified.
        self.sync_disk(doc, state, theirs);
        self.info(if merged.conflicts == 0 {
            format!("\"{name}\" merged with disk")
        } else {
            format!(
                "\"{name}\" merged with disk: {} conflict(s) marked",
                merged.conflicts
            )
        });
    }

    /// Records the file state the buffer now matches: the disk state, the
    /// baseline text (the last text known to be on disk), and no prompt or
    /// warning outstanding for it.
    pub(crate) fn sync_disk(&mut self, doc: DocId, disk: DiskState, lines: Vec<String>) {
        let d = self.document_mut(doc);
        d.disk = Some(disk);
        d.baseline = Some(lines);
        d.seen = None;
        d.stale = None;
        d.warned = None;
    }

    /// The file is gone: the buffer is kept and marked, and the user is told.
    pub(crate) fn deleted_on_disk(&mut self, doc: DocId, name: &str) {
        self.document_mut(doc).stale = Some(Stale::Deleted);
        self.info(format!("\"{name}\" was deleted on disk"));
    }

    /// The name of document `doc` for messages (its path).
    fn name_of(&self, doc: DocId) -> String {
        self.document(doc)
            .buf
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    /// With the opt-in on, what `:w` (without `!`) refuses to write to `path`,
    /// the file of the buffer: text with conflict markers left by `m`, or a
    /// file that differs from the text last read or written (it changed since
    /// reading). None when the write may go ahead. A file that is gone is
    /// written again; a file that cannot be read is left to the write to report.
    pub(crate) fn write_guard(&self, path: &Path) -> Option<String> {
        let name = path.display();
        if self
            .buf()
            .lines()
            .iter()
            .any(|l| l == MARK_MINE || l == MARK_THEIRS)
        {
            return Some(format!(
                "E13: File \"{name}\" has conflict markers (add ! to override)"
            ));
        }
        let synced = self.doc().disk.as_ref()?;
        let Ok(Some((mtime, bytes))) = read_file(path) else {
            return None;
        };
        let now = DiskState::of(mtime, &bytes);
        match external_change_decision(Some(synced), Some(&now), true, false) {
            Decision::Changed { .. } => Some(format!(
                "W12: Warning: File \"{name}\" has changed since reading (add ! to override)"
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::undo::UndoSource;
    use std::fs;
    use std::path::PathBuf;

    /// An editor on `f.txt` in a new temp dir, with the opt-in on.
    fn open(dir: &tempfile::TempDir, contents: &str) -> (Editor, PathBuf) {
        let path = dir.path().join("f.txt");
        fs::write(&path, contents).unwrap();
        let mut ed = Editor::open(&path);
        ed.set_agent_enabled(true);
        (ed, path)
    }

    fn message_of(ed: &Editor) -> String {
        ed.message().map(|m| m.text.clone()).unwrap_or_default()
    }

    /// Modifies the buffer ("x" at the start) and changes the file on disk.
    fn modify_and_change_disk(ed: &mut Editor, path: &PathBuf, disk: &str) {
        ed.feed("ix<Esc>");
        fs::write(path, disk).unwrap();
        ed.feed(":checktime<CR>");
    }

    fn prompt_open(ed: &Editor) -> bool {
        ed.conflict.is_some()
    }

    #[test]
    fn modified_and_changed_on_disk_opens_the_prompt_without_reloading() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        assert!(prompt_open(&ed));
        assert!(message_of(&ed).contains("[l]oad disk [k]eep mine [d]iff [m]erge"));
        assert_eq!(ed.buf().lines(), ["xone"], "not reloaded");
        assert!(ed.buf().modified);
    }

    #[test]
    fn without_the_opt_in_it_is_w12_and_no_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        ed.set_agent_enabled(false);
        modify_and_change_disk(&mut ed, &path, "two\n");
        assert!(!prompt_open(&ed));
        assert!(message_of(&ed).starts_with("W12"), "{}", message_of(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
    }

    #[test]
    fn l_loads_the_disk_text_as_one_external_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        let seq = ed.doc().undo.seq();
        ed.feed("l");
        assert!(!prompt_open(&ed));
        assert_eq!(ed.buf().lines(), ["two"]);
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().undo.source(seq + 1), Some(UndoSource::External));
        ed.feed("u");
        assert_eq!(ed.buf().lines(), ["xone"], "the user's text is one u away");
        // The disk state is the new one: the write guard lets `:w` through.
        ed.feed("<C-r>");
        ed.feed("ix<Esc>:w<CR>");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xtwo\n");
    }

    #[test]
    fn k_keeps_the_buffer_and_the_prompt_does_not_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        ed.feed("k");
        assert!(!prompt_open(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert!(ed.buf().modified);
        // The same file state again: no prompt, the W12 warning instead.
        ed.feed(":checktime<CR>");
        assert!(!prompt_open(&ed));
        assert!(message_of(&ed).starts_with("W12"), "{}", message_of(&ed));
    }

    #[test]
    fn a_paste_is_ignored_while_the_prompt_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        let beeps = ed.beeps;
        ed.paste("pasted\ntext");
        assert!(prompt_open(&ed), "the paste does not answer the prompt");
        assert_eq!(ed.buf().lines(), ["xone"], "the paste is not inserted");
        assert_eq!(ed.beeps, beeps + 1);
        // The prompt still answers a key afterwards.
        ed.feed("k");
        assert!(!prompt_open(&ed));
    }

    #[test]
    fn escape_is_keep() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        ed.feed("<Esc>");
        assert!(!prompt_open(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
    }

    #[test]
    fn the_prompt_takes_its_keys_and_other_keys_beep() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        // `j`, `x` and `:` are not prompt keys: they do not run as commands.
        let beeps = ed.beeps;
        ed.feed("xj:");
        assert!(prompt_open(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert_eq!(ed.beeps, beeps + 3);
        // `d` says the diff is not built, and keeps the prompt open.
        ed.feed("d");
        assert!(prompt_open(&ed));
        assert!(message_of(&ed).starts_with("The diff view"));
    }

    #[test]
    fn m_merges_disjoint_changes_and_keeps_the_buffer_modified() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\ntwo\nthree\n");
        // Saved as "onex" (the baseline); then mine changes line 1 and disk
        // changes line 3.
        ed.feed("Ax<Esc>:w<CR>");
        ed.feed("ggIy<Esc>");
        fs::write(&path, "onex\ntwo\nTHREE\n").unwrap();
        ed.feed(":checktime<CR>");
        assert!(prompt_open(&ed));
        ed.feed("m");
        assert!(!prompt_open(&ed));
        assert_eq!(ed.buf().lines(), ["yonex", "two", "THREE"]);
        assert!(ed.buf().modified);
        // The merge is one undo step, tagged user.
        let seq = ed.doc().undo.seq();
        assert_eq!(ed.doc().undo.source(seq), Some(UndoSource::User));
        // No markers, so `:w` writes the merged text.
        ed.feed(":w<CR>");
        assert_eq!(fs::read_to_string(&path).unwrap(), "yonex\ntwo\nTHREE\n");
    }

    #[test]
    fn m_writes_conflict_markers_and_w_refuses_until_bang() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        // Saved as "onex" (the baseline); mine and disk then change that line
        // differently.
        ed.feed("Ax<Esc>:w<CR>");
        ed.feed("Amine<Esc>");
        fs::write(&path, "onexdisk\n").unwrap();
        ed.feed(":checktime<CR>");
        assert!(prompt_open(&ed), "{}", message_of(&ed));
        ed.feed("m");
        assert_eq!(
            ed.buf().lines(),
            [
                "<<<<<<< mine",
                "onexmine",
                "=======",
                "onexdisk",
                ">>>>>>> disk"
            ]
        );
        ed.feed(":w<CR>");
        assert!(message_of(&ed).starts_with("E13"), "{}", message_of(&ed));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "onexdisk\n",
            "not written"
        );
        ed.feed(":w!<CR>");
        assert!(fs::read_to_string(&path).unwrap().contains("<<<<<<< mine"));
    }

    #[test]
    fn w_refuses_a_file_that_changed_since_reading_until_bang() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        ed.feed("ix<Esc>");
        fs::write(&path, "two\n").unwrap();
        // The file changed and nothing checked it: `:w` checks itself.
        ed.feed(":w<CR>");
        assert!(message_of(&ed).starts_with("W12"), "{}", message_of(&ed));
        assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
        ed.feed(":w!<CR>");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xone\n");
    }

    #[test]
    fn w_after_k_still_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        modify_and_change_disk(&mut ed, &path, "two\n");
        ed.feed("k:w<CR>");
        assert!(message_of(&ed).starts_with("W12"), "{}", message_of(&ed));
        assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
    }

    #[test]
    fn a_deleted_file_is_kept_and_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        ed.feed("ix<Esc>");
        fs::remove_file(&path).unwrap();
        ed.feed(":checktime<CR>");
        assert!(
            message_of(&ed).contains("was deleted on disk"),
            "{}",
            message_of(&ed)
        );
        assert!(!prompt_open(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
        ed.feed(":w<CR>");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xone\n");
    }

    #[test]
    fn without_the_opt_in_a_deleted_file_is_e211() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        ed.set_agent_enabled(false);
        fs::remove_file(&path).unwrap();
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("E211"), "{}", message_of(&ed));
    }

    #[test]
    fn without_the_opt_in_w_is_unchanged_and_overwrites() {
        // The write guard is part of the opt-in: with it off, `:w` writes as
        // it did before (see "Known differences" in PLAN.md).
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        ed.set_agent_enabled(false);
        ed.feed("ix<Esc>");
        fs::write(&path, "two\n").unwrap();
        ed.feed(":w<CR>");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xone\n");
    }

    #[test]
    fn the_opt_in_is_the_agent_enabled_key_of_the_config() {
        use crate::config::parse;
        assert!(!parse("").agent_enabled());
        assert!(!parse("[agent]\nenabled = false\n").agent_enabled());
        assert!(!parse("[agent]\nenabled = \"yes\"\n").agent_enabled());
        assert!(parse("[agent]\nenabled = true\n").agent_enabled());
    }

    #[test]
    fn a_key_sequence_with_no_prompt_is_unchanged_by_the_opt_in() {
        // Keys that never reach the prompt act as they do with the opt-in off.
        let dir = tempfile::tempdir().unwrap();
        let (mut on, _) = open(&dir, "one\ntwo\n");
        let (mut off, _) = open(&dir, "one\ntwo\n");
        off.set_agent_enabled(false);
        for ed in [&mut on, &mut off] {
            ed.feed("jddu");
        }
        assert_eq!(on.buf().lines(), off.buf().lines());
        assert_eq!(on.cursor(), off.cursor());
    }
}
