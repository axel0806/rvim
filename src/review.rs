// SPDX-License-Identifier: Apache-2.0
//! Reviewing a document against its baseline (PLAN.md, "Reviewing agent
//! changes" and "The edit loop", "Per-hunk review"; milestone 7 step 3b).
//!
//! - The review baseline is the text last read by `:e` or written by `:w`
//!   (`Document::review_base`). A change from outside (a reload, the
//!   conflict prompt's `l` and `m`) does not move it, so the change gutter
//!   shows what an agent or another program changed until the next `:w`.
//! - The change gutter: two cells left of the number column, drawn only with
//!   the agent opt-in and only while the document differs from its review
//!   baseline. `+` added line, `~` changed line, `_` lines were deleted after
//!   this line, `^` lines were deleted above the first line. Group
//!   `Hl::Changed`.
//! - The `[base]` scratch document (`:AgentBase`, `:AgentBase!` goes back):
//!   the review baseline, not modifiable and never written.
//! - `:AgentChanges[!] [N]`: the recent undo steps with their source tags on
//!   the message line, and the jump to the first changed line of step `N`.
//!
//! The diff is made after a key or an event, not while drawing. When the
//! changed part of the text (what is left after the common first and last
//! lines) is at most `SYNC_DIFF_CELLS` cells of LCS table it is made at once
//! on the editor thread; a larger one goes to a worker (`DiffJob`, sent by
//! `main.rs`) and comes back as `Event::ReviewDiff` with the document version
//! and baseline counter it was made for; a result for an older state is
//! dropped. Over `MAX_DP_CELLS` cells the changed part is one hunk, and over
//! `MAX_DIFF_LINES` lines in either text there is no gutter at all.

use crate::buffer::Buffer;
use crate::document::DocId;
use crate::editor::Editor;
use crate::event::Event;
use crate::motion::{Pos, first_non_blank};
use crate::reload::{LineMap, MAX_DIFF_LINES, MAX_DP_CELLS, line_pairs};
use crate::undo::StepInfo;
use std::time::{Duration, Instant};

/// Cells of the change gutter: the sign and a space (Vim's sign column is two
/// cells wide too).
pub const GUTTER_WIDTH: usize = 2;
/// The largest LCS table (cells) diffed on the editor thread. 250,000 cells
/// of u32 is 1 MB and well under a millisecond; a 500-line rewrite fits.
pub const SYNC_DIFF_CELLS: usize = 250_000;
/// The steps `:AgentChanges` lists at most (PLAN.md: the last 50).
pub const CHANGES_SHOWN: usize = 50;

/// One run of lines that differ: `base_count` lines of the baseline from
/// `base_start` are `buf_count` lines of the buffer from `buf_start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub base_start: usize,
    pub base_count: usize,
    pub buf_start: usize,
    pub buf_count: usize,
}

/// The mark of one line in the change gutter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    /// The line is not in the baseline.
    Added,
    /// The line replaces a baseline line.
    Changed,
    /// Baseline lines were deleted after this line.
    Deleted,
    /// Baseline lines were deleted above this line, the first one.
    DeletedAbove,
}

impl Sign {
    pub fn mark(self) -> char {
        match self {
            Sign::Added => '+',
            Sign::Changed => '~',
            Sign::Deleted => '_',
            Sign::DeletedAbove => '^',
        }
    }
}

/// The hunks between the equal-line pairs `pairs` of a baseline of `n` lines
/// and a buffer of `m` lines.
fn hunks_from_pairs(pairs: &[(usize, usize)], n: usize, m: usize) -> Vec<Hunk> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for &(pi, pj) in pairs.iter().chain(std::iter::once(&(n, m))) {
        if pi > i || pj > j {
            out.push(Hunk {
                base_start: i,
                base_count: pi - i,
                buf_start: j,
                buf_count: pj - j,
            });
        }
        i = pi + 1;
        j = pj + 1;
    }
    out
}

/// The hunks of `buf` against `base`: the line diff a reload uses, or one
/// hunk over the part between the common first and last lines when that
/// diff is over its size caps.
pub fn diff_hunks<S: AsRef<str>>(base: &[S], buf: &[S]) -> Vec<Hunk> {
    if let Some(pairs) = line_pairs(base, buf) {
        return hunks_from_pairs(&pairs, base.len(), buf.len());
    }
    let pre = base
        .iter()
        .zip(buf)
        .take_while(|(a, b)| a.as_ref() == b.as_ref())
        .count();
    let suf = base[pre..]
        .iter()
        .rev()
        .zip(buf[pre..].iter().rev())
        .take_while(|(a, b)| a.as_ref() == b.as_ref())
        .count();
    coarse(pre, base.len() - pre - suf, buf.len() - pre - suf)
}

/// One hunk of `a` baseline and `b` buffer lines at line `pre`, or none.
fn coarse(pre: usize, a: usize, b: usize) -> Vec<Hunk> {
    if a == 0 && b == 0 {
        return Vec::new();
    }
    vec![Hunk {
        base_start: pre,
        base_count: a,
        buf_start: pre,
        buf_count: b,
    }]
}

/// The gutter's signs for `hunks`, by buffer line, in line order. A changed
/// run longer than the baseline lines it replaces ends in added lines; a
/// shorter one is all changed lines.
pub fn signs(hunks: &[Hunk]) -> Vec<(usize, Sign)> {
    let mut out = Vec::new();
    for h in hunks {
        if h.buf_count == 0 {
            if h.base_count > 0 {
                out.push(match h.buf_start {
                    0 => (0, Sign::DeletedAbove),
                    s => (s - 1, Sign::Deleted),
                });
            }
        } else {
            for k in 0..h.buf_count {
                let sign = if k < h.base_count {
                    Sign::Changed
                } else {
                    Sign::Added
                };
                out.push((h.buf_start + k, sign));
            }
        }
    }
    out.sort_by_key(|&(l, _)| l);
    out.dedup_by_key(|&mut (l, _)| l);
    out
}

/// The gutter of one document, as last computed.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReviewCache {
    /// The document version and baseline counter `signs` is for.
    for_state: Option<(u64, u64)>,
    /// The state a worker is diffing now (one job at a time per document).
    pending: Option<(u64, u64)>,
    /// Signs by line. While a job is pending these are the previous ones.
    signs: Vec<(usize, Sign)>,
}

/// A line diff for a worker: the changed part of the texts, without their
/// common first and last lines (`offset` of them come first).
pub struct DiffJob {
    pub doc: DocId,
    pub version: u64,
    pub base_gen: u64,
    offset: usize,
    base: Vec<String>,
    buf: Vec<String>,
}

impl DiffJob {
    /// Makes the diff (slow for a large text: run it on a worker) and gives
    /// the event that carries it back to the editor.
    pub fn run(self) -> Event {
        let mut hunks = diff_hunks(&self.base, &self.buf);
        for h in &mut hunks {
            h.base_start += self.offset;
            h.buf_start += self.offset;
        }
        Event::ReviewDiff {
            doc: self.doc,
            version: self.version,
            base_gen: self.base_gen,
            hunks,
        }
    }
}

/// What the editor keeps for review besides the documents.
#[derive(Default)]
pub(crate) struct ReviewState {
    /// Diffs for the worker, not yet taken by `take_review_jobs`.
    jobs: Vec<DiffJob>,
    /// The document and cursor `[base]` replaced, for `:AgentBase!`.
    back: Option<(DocId, Pos)>,
    /// The gutter width last laid out, so the view is scrolled again when
    /// the gutter comes or goes (the text area changes width).
    shown_width: usize,
}

/// `d` as Vim's `:undolist` shows a recent time, shortened: `12s ago`, and
/// minutes or hours for older steps.
fn age(d: Duration) -> String {
    let s = d.as_secs();
    if s < 100 {
        format!("{s}s ago")
    } else if s < 100 * 60 {
        format!("{}m ago", s / 60)
    } else {
        format!("{}h ago", s / 3600)
    }
}

/// Lines added and removed by one undo step, from the region it recorded.
fn step_counts(s: &StepInfo) -> (usize, usize) {
    let (bl, al) = (s.before.line_count(), s.after.line_count());
    let top = s.top.min(bl).min(al);
    let below = s.below.min(bl - top).min(al - top);
    let old: Vec<String> = (top..bl - below)
        .map(|l| s.before.line(l).to_string())
        .collect();
    let new: Vec<String> = (top..al - below)
        .map(|l| s.after.line(l).to_string())
        .collect();
    let map = LineMap::new(&old, &new);
    if map.fallback {
        (new.len(), old.len())
    } else {
        (map.added, map.removed)
    }
}

impl Editor {
    /// Cells of the change gutter in the current window: `GUTTER_WIDTH` with
    /// the agent opt-in while the document differs from its review baseline,
    /// otherwise 0 (so with no opt-in the frame is Vim's). A window too narrow
    /// for it, the number column and one text cell has none.
    pub(crate) fn sign_col_width(&self) -> usize {
        if !self.agent_enabled || self.doc().review.signs.is_empty() {
            return 0;
        }
        if self.number_col_width() + GUTTER_WIDTH + 1 > self.win().width {
            return 0;
        }
        GUTTER_WIDTH
    }

    /// The gutter sign of line `line` of the current document.
    pub(crate) fn sign_at(&self, line: usize) -> Option<Sign> {
        let signs = &self.doc().review.signs;
        signs
            .binary_search_by_key(&line, |&(l, _)| l)
            .ok()
            .map(|i| signs[i].1)
    }

    /// The gutter signs of the current document, for tests.
    pub fn review_signs(&self) -> Vec<(usize, char)> {
        self.doc()
            .review
            .signs
            .iter()
            .map(|&(l, s)| (l, s.mark()))
            .collect()
    }

    /// Brings the gutter of the current document up to date after a key or
    /// an event, and the `[base]` document shown in the window. Cheap when
    /// nothing changed: it compares the version and the baseline counter.
    pub(crate) fn refresh_review(&mut self) {
        self.refresh_base_doc();
        let id = self.win().doc;
        self.update_review(id);
        let width = self.sign_col_width();
        if width != self.review.shown_width {
            self.review.shown_width = width;
            // The text area changed width, so lines wrap differently.
            self.update_topline();
        }
    }

    fn update_review(&mut self, id: DocId) {
        if !self.agent_enabled {
            return;
        }
        let d = self.document(id);
        let state = (d.version(), d.baseline_gen);
        if d.review.for_state == Some(state) || d.review.pending.is_some() {
            return;
        }
        let signs = match d.review_base() {
            None => Vec::new(),
            Some(base) => {
                let buf = &d.buf;
                let (n, m) = (base.len(), buf.line_count());
                if n > MAX_DIFF_LINES || m > MAX_DIFF_LINES {
                    Vec::new()
                } else {
                    // The common first and last lines, compared in place.
                    let pre = (0..n.min(m))
                        .take_while(|&i| buf.line(i) == base[i].as_str())
                        .count();
                    let suf = (0..(n - pre).min(m - pre))
                        .take_while(|&k| buf.line(m - 1 - k) == base[n - 1 - k].as_str())
                        .count();
                    let (a, b) = (n - pre - suf, m - pre - suf);
                    if a == 0 || b == 0 || a.saturating_mul(b) > MAX_DP_CELLS {
                        signs(&coarse(pre, a, b))
                    } else {
                        let base_mid = base[pre..n - suf].to_vec();
                        let buf_mid: Vec<String> =
                            (pre..m - suf).map(|l| buf.line_string(l)).collect();
                        let job = DiffJob {
                            doc: id,
                            version: state.0,
                            base_gen: state.1,
                            offset: pre,
                            base: base_mid,
                            buf: buf_mid,
                        };
                        if a * b > SYNC_DIFF_CELLS {
                            return self.queue_review_job(job);
                        }
                        match job.run() {
                            Event::ReviewDiff { hunks, .. } => signs(&hunks),
                            _ => Vec::new(),
                        }
                    }
                }
            }
        };
        let r = &mut self.document_mut(id).review;
        r.for_state = Some(state);
        r.signs = signs;
    }

    /// Sends a diff too large for the editor thread to the worker. The
    /// previous signs stay drawn until it comes back.
    fn queue_review_job(&mut self, job: DiffJob) {
        self.document_mut(job.doc).review.pending = Some((job.version, job.base_gen));
        self.review.jobs.push(job);
    }

    /// The diffs the front end must run on a worker (`DiffJob::run`), each
    /// sent back as `Event::ReviewDiff`.
    pub fn take_review_jobs(&mut self) -> Vec<DiffJob> {
        std::mem::take(&mut self.review.jobs)
    }

    /// A worker's diff arrived. It is used only if the document and its
    /// baseline are still what it was computed for; `refresh_review` then
    /// asks for a new one when they have moved on.
    pub(crate) fn review_arrived(
        &mut self,
        doc: DocId,
        version: u64,
        base_gen: u64,
        hunks: Vec<Hunk>,
    ) {
        if doc.0 >= self.document_count() {
            return;
        }
        let d = self.document_mut(doc);
        d.review.pending = None;
        if (d.version(), d.baseline_gen) == (version, base_gen) {
            d.review.for_state = Some((version, base_gen));
            d.review.signs = signs(&hunks);
        }
    }

    /// `:AgentBase`: shows the review baseline of the current document in
    /// the window, as the `[base]` scratch document (not modifiable, never
    /// written). The cursor goes to the same line number, column 0.
    pub(crate) fn agent_base_show(&mut self) -> Result<(), String> {
        if self.doc().base_of.is_some() {
            return Ok(());
        }
        let src = self.win().doc;
        let name = self.display_name();
        let Some(lines) = self.doc().review_base().cloned() else {
            return Err(format!(
                "\"{name}\" has no baseline: it was not read from or written to a file"
            ));
        };
        let gen_ = self.doc().baseline_gen;
        let base = match self.doc().base_doc {
            Some(b) => b,
            None => {
                let b = self.new_document(Buffer::from_text(""));
                self.doc_mut().base_doc = Some(b);
                b
            }
        };
        self.fill_base(base, src, gen_, &lines);
        let here = (src, self.win().cursor);
        self.review.back = Some(here);
        self.win_mut().doc = base;
        let line = here.1.line.min(self.buf().last_line());
        self.win_mut().cursor = Pos::new(line, 0);
        self.win_mut().keep_curswant = false;
        self.update_topline();
        let n = lines.len();
        self.info(format!(
            "\"[base]\" {n} line{}, the baseline of \"{name}\"",
            if n == 1 { "" } else { "s" }
        ));
        Ok(())
    }

    /// `:AgentBase!`: back to the document `[base]` replaced.
    pub(crate) fn agent_base_back(&mut self) {
        let Some((doc, pos)) = self.review.back.take() else {
            return self.info("no document to go back to");
        };
        self.win_mut().doc = doc;
        let buf = &self.document(doc).buf;
        let line = pos.line.min(buf.last_line());
        let col = pos.col.min(buf.line_len(line).saturating_sub(1));
        self.win_mut().cursor = Pos::new(line, col);
        self.win_mut().keep_curswant = false;
        self.update_topline();
    }

    /// Puts `lines`, the review baseline of `src` at counter `gen_`, into the
    /// `[base]` document `base`.
    fn fill_base(&mut self, base: DocId, src: DocId, gen_: u64, lines: &[String]) {
        let mut buf = Buffer::from_text(&lines.join("\n"));
        buf.locked = true;
        let d = self.document_mut(base);
        d.reload(buf);
        d.nomodifiable = true;
        d.scratch_name = Some("[base]");
        d.base_of = Some((src, gen_));
    }

    /// The `[base]` document in the window follows its document's baseline
    /// (a `:w` or `:e` there while `[base]` is shown, from a later split).
    fn refresh_base_doc(&mut self) {
        let base = self.win().doc;
        let Some((src, gen_)) = self.document(base).base_of else {
            return;
        };
        let now = self.document(src).baseline_gen;
        if now == gen_ {
            return;
        }
        let Some(lines) = self.document(src).review_base().cloned() else {
            return;
        };
        self.fill_base(base, src, now, &lines);
        let last = self.buf().last_line();
        let line = self.win().cursor.line.min(last);
        let col = self
            .win()
            .cursor
            .col
            .min(self.buf().line_len(line).saturating_sub(1));
        self.win_mut().cursor = Pos::new(line, col);
    }

    /// `:AgentChanges[!]`: the recent undo steps of the current document
    /// (`!`: of every document), newest first, at most `CHANGES_SHOWN`, on
    /// the message line separated by ` | `: `#seq age source file +added
    /// -removed`. `:AgentChanges N` jumps to the first changed line of step
    /// `N` of the current document; it does not undo.
    pub(crate) fn agent_changes(&mut self, bang: bool, arg: &str) -> Result<(), String> {
        let arg = arg.trim();
        if !arg.is_empty() {
            let seq: usize = arg
                .parse()
                .map_err(|_| format!("E474: Invalid argument: {arg}"))?;
            return self.agent_changes_jump(seq);
        }
        let now = Instant::now();
        let docs: Vec<DocId> = if bang {
            (0..self.document_count())
                .map(DocId)
                .filter(|&id| {
                    let d = self.document(id);
                    !d.nomodifiable && d.base_of.is_none()
                })
                .collect()
        } else {
            vec![self.win().doc]
        };
        let mut rows: Vec<(Instant, usize, String)> = Vec::new();
        for id in docs {
            let d = self.document(id);
            let name = match (d.scratch_name, &d.buf.path) {
                (Some(n), _) => n.to_string(),
                (None, Some(p)) => p.display().to_string(),
                (None, None) => "[No Name]".to_string(),
            };
            for seq in (1..=d.undo.last_seq()).rev().take(CHANGES_SHOWN) {
                let Some(s) = d.undo.step_info(seq) else {
                    continue;
                };
                let (added, removed) = step_counts(&s);
                rows.push((
                    s.time,
                    seq,
                    format!(
                        "#{seq} {} {} {name} +{added} -{removed}",
                        age(now.saturating_duration_since(s.time)),
                        s.source
                    ),
                ));
            }
        }
        if rows.is_empty() {
            self.info("No changes");
            return Ok(());
        }
        rows.sort_by(|x, y| y.0.cmp(&x.0).then(y.1.cmp(&x.1)));
        rows.truncate(CHANGES_SHOWN);
        let text: Vec<String> = rows.into_iter().map(|r| r.2).collect();
        self.info(text.join(" | "));
        Ok(())
    }

    /// `:AgentChanges N`: the cursor goes to the first line step `N` changed
    /// (its line number then, clipped to the text now), on its first
    /// non-blank, as a jump (`''` comes back). The desired column is reset.
    fn agent_changes_jump(&mut self, seq: usize) -> Result<(), String> {
        let Some(top) = self.doc().undo.step_info(seq).map(|s| s.top) else {
            return Err(format!("E830: Undo number {seq} not found"));
        };
        let line = top.min(self.buf().last_line());
        self.set_pcmark();
        let col = first_non_blank(self.buf(), line, true);
        self.win_mut().cursor = Pos::new(line, col);
        self.win_mut().keep_curswant = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<&str> {
        s.split('\n').collect()
    }

    fn marks(base: &str, buf: &str) -> Vec<(usize, char)> {
        signs(&diff_hunks(&lines(base), &lines(buf)))
            .into_iter()
            .map(|(l, s)| (l, s.mark()))
            .collect()
    }

    #[test]
    fn equal_texts_have_no_hunks() {
        assert!(diff_hunks(&lines("a\nb"), &lines("a\nb")).is_empty());
    }

    #[test]
    fn added_changed_and_deleted_lines() {
        assert_eq!(marks("a\nb", "a\nx\nb"), [(1, '+')]);
        assert_eq!(marks("a\nb\nc", "a\nB\nc"), [(1, '~')]);
        assert_eq!(marks("a\nb\nc", "a\nc"), [(0, '_')]);
        assert_eq!(marks("a\nb\nc", "b\nc"), [(0, '^')]);
        assert_eq!(marks("a\nb\nc", "a\nb"), [(1, '_')]);
        // A replacement longer than what it replaces ends in added lines.
        assert_eq!(marks("a\nb\nc", "a\nX\nY\nc"), [(1, '~'), (2, '+')]);
        // A shorter one is all changed.
        assert_eq!(marks("a\nb\nc\nd", "a\nX\nd"), [(1, '~')]);
    }

    #[test]
    fn hunks_carry_both_sides() {
        assert_eq!(
            diff_hunks(&lines("a\nb\nc"), &lines("a\nX\nY\nc")),
            [Hunk {
                base_start: 1,
                base_count: 1,
                buf_start: 1,
                buf_count: 2
            }]
        );
    }

    #[test]
    fn ages_are_short() {
        assert_eq!(age(Duration::from_secs(5)), "5s ago");
        assert_eq!(age(Duration::from_secs(150)), "2m ago");
        assert_eq!(age(Duration::from_secs(7300)), "2h ago");
    }
}
