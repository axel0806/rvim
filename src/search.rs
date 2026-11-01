// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! `/ ? n N * #`: a port of Vim's `searchit()` and `do_search()` (search.c),
//! including the 'cpoptions' `c` behaviour of the default Vim.

use crate::buffer::Buffer;
use crate::editor::{Editor, Message, Mode, VisualMode};
use crate::motion::{Pos, char_class, decl, incl};
use crate::regex::{Opts, Regex};

/// The offset part of a search command (`/foo/e+1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SearchOff {
    pub line: bool,
    pub end: bool,
    pub off: i64,
}

/// What `n` repeats.
#[derive(Clone, Debug)]
pub(crate) struct SearchState {
    pub pat: Option<String>,
    pub fwd: bool,
    pub off: SearchOff,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            pat: None,
            fwd: true,
            off: SearchOff::default(),
        }
    }
}

pub(crate) struct Found {
    pub pos: Pos,
    /// Vim's `end_pos`: the match start when `SEARCH_END` is set, else the
    /// position after its last char (exclusive).
    pub other: Pos,
    pub wrapped: bool,
}

/// Where a search starts. `Outside` is Vim's "lnum 0 / past the end":
/// the search then continues at the other end of the buffer.
#[derive(Clone, Copy)]
pub(crate) enum Start {
    At(Pos),
    Outside,
}

pub(crate) struct SearchFlags {
    /// `SEARCH_START`: accept a match at the start position.
    pub start: bool,
    /// `SEARCH_END`: put the position on the last char of the match.
    pub end: bool,
    pub wrapscan: bool,
}

/// Vim's `searchit()`. `None` if there is no match.
pub(crate) fn searchit(
    buf: &Buffer,
    re: &Regex,
    start: Start,
    fwd: bool,
    count: usize,
    fl: &SearchFlags,
) -> Option<Found> {
    let nlines = buf.line_count();
    let mut outside = matches!(start, Start::Outside);
    let mut pos = match start {
        Start::At(p) => p,
        Start::Outside => Pos::new(0, 0),
    };
    let mut wrapped = false;
    let mut first_match = true;
    let mut remaining = count.max(1);
    loop {
        let mut found = false;
        let start_pos = pos;
        // pos.line may be -1 ("before the buffer") in Vim; we use `lnum0`.
        let start_char_len = 1usize;
        let extra_col: i64 = if fwd {
            if fl.start { 0 } else { start_char_len as i64 }
        } else if fl.start {
            start_char_len as i64
        } else {
            0
        };
        let mut at_first_line = true;
        let mut lnum: i64 = if !fwd && start_pos.col == 0 && !fl.start {
            at_first_line = false;
            start_pos.line as i64 - 1
        } else {
            start_pos.line as i64
        };
        let mut result: Option<(Pos, Pos)> = None;
        let first_pass = if outside {
            outside = false;
            lnum = if fwd { 0 } else { nlines as i64 - 1 };
            at_first_line = false;
            1
        } else {
            0
        };
        'passes: for pass in first_pass..2 {
            while lnum >= 0 && (lnum as usize) < nlines {
                let l = lnum as usize;
                let col = 0;
                if let Some(mut m) = re.match_line(buf, l, col) {
                    let ptr_len = |line: usize| -> usize {
                        if line < nlines { buf.line_len(line) } else { 0 }
                    };
                    let mut match_ok = true;
                    if fwd && at_first_line {
                        // The match must start after the start position;
                        // otherwise continue at its end ('cpo' has `c`).
                        loop {
                            let ml = m.start.0;
                            let in_first = ml == l;
                            let nm1 = m.end.0 == m.start.0;
                            let too_early = if fl.end && first_match {
                                nm1 && (m.end.1 as i64 - 1) < start_pos.col as i64 + extra_col
                            } else {
                                let at_nul = m.start.1 >= ptr_len(ml);
                                (m.start.1 as i64 - at_nul as i64)
                                    < start_pos.col as i64 + extra_col
                            };
                            if !(in_first && too_early) {
                                break;
                            }
                            if m.end.0 != m.start.0 {
                                match_ok = false;
                                break;
                            }
                            let mut matchcol = m.end.1;
                            if matchcol == m.start.1 && matchcol < ptr_len(ml) {
                                matchcol += 1;
                            }
                            if matchcol == 0 && fl.start {
                                break;
                            }
                            if matchcol >= ptr_len(ml) {
                                match_ok = false;
                                break;
                            }
                            match re.match_line(buf, ml, matchcol) {
                                Some(n) => m = n,
                                None => {
                                    match_ok = false;
                                    break;
                                }
                            }
                        }
                        if !match_ok {
                            lnum += 1;
                            at_first_line = false;
                            continue;
                        }
                    }
                    if !fwd {
                        // The last match before the start position.
                        match_ok = false;
                        let mut best = m.clone();
                        loop {
                            let ok = pass == 1
                                || if fl.end {
                                    m.end.0 < start_pos.line
                                        || (m.end.0 == start_pos.line
                                            && (m.end.1 as i64 - 1)
                                                < start_pos.col as i64 + extra_col)
                                } else {
                                    m.start.0 < start_pos.line
                                        || (m.start.0 == start_pos.line
                                            && (m.start.1 as i64)
                                                < start_pos.col as i64 + extra_col)
                                };
                            if !ok {
                                break;
                            }
                            match_ok = true;
                            best = m.clone();
                            if m.end.0 != m.start.0 {
                                break;
                            }
                            let ml = m.start.0;
                            let mut matchcol = m.end.1;
                            if matchcol == m.start.1 && matchcol < ptr_len(ml) {
                                matchcol += 1;
                            }
                            if matchcol >= ptr_len(ml) {
                                break;
                            }
                            match re.match_line(buf, ml, matchcol) {
                                Some(n) => m = n,
                                None => break,
                            }
                        }
                        if !match_ok {
                            lnum -= 1;
                            at_first_line = false;
                            continue;
                        }
                        m = best;
                    }
                    // Position of the match.
                    let (s, e) = (m.start, m.end);
                    if fl.end && s != e {
                        let mut p = e;
                        if p.1 == 0 {
                            if p.0 > 0 {
                                p.0 -= 1;
                                p.1 = ptr_len(p.0);
                            }
                        } else {
                            p.1 -= 1;
                        }
                        result = Some((Pos::new(p.0, p.1), Pos::new(s.0, s.1)));
                    } else {
                        result = Some((Pos::new(s.0, s.1), Pos::new(e.0, e.1)));
                    }
                    found = true;
                    if pass == 1 {
                        wrapped = true;
                    }
                    break 'passes;
                }
                lnum += if fwd { 1 } else { -1 };
                at_first_line = false;
            }
            at_first_line = false;
            if !fl.wrapscan || found || pass == 1 {
                break;
            }
            lnum = if fwd { 0 } else { nlines as i64 - 1 };
        }
        if !found {
            return None;
        }
        let (p, o) = result.unwrap();
        pos = p;
        first_match = false;
        remaining -= 1;
        if remaining == 0 {
            return Some(Found {
                pos,
                other: o,
                wrapped,
            });
        }
    }
}

/// Finds the end of the pattern in `s` at the first unescaped `delim`
/// outside `[]`, like Vim's `skip_regexp()`. Returns the byte index.
fn skip_regexp(s: &[char], delim: char) -> usize {
    let mut i = 0;
    let mut mymagic = true;
    while i < s.len() {
        let c = s[i];
        if c == delim {
            return i;
        }
        match c {
            '[' if mymagic => {
                if let Some(end) = skip_coll(s, i) {
                    i = end;
                    continue;
                }
            }
            '\\' if i + 1 < s.len() => {
                let d = s[i + 1];
                if d == '['
                    && !mymagic
                    && let Some(end) = skip_coll(s, i + 1)
                {
                    i = end;
                    continue;
                }
                match d {
                    'v' | 'm' => mymagic = true,
                    'V' | 'M' => mymagic = false,
                    _ => {}
                }
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    s.len()
}

/// Index after the `]` of a collection starting at `s[i] == '['`.
fn skip_coll(s: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if s.get(j) == Some(&'^') {
        j += 1;
    }
    if s.get(j) == Some(&']') || s.get(j) == Some(&'-') {
        j += 1;
    }
    while j < s.len() && s[j] != ']' {
        if s[j] == '-' {
            j += 1;
            if j < s.len() && s[j] != ']' {
                j += 1;
            }
        } else if s[j] == '\\' && j + 1 < s.len() && "etrbn\\]^-".contains(s[j + 1]) {
            j += 2;
        } else if s[j] == '[' && s.get(j + 1) == Some(&':') {
            match s[j + 2..].iter().collect::<String>().find(":]") {
                Some(k) => j += 2 + s[j + 2..j + 2 + k].len() + 2,
                None => j += 1,
            }
        } else {
            j += 1;
        }
    }
    (j < s.len()).then_some(j + 1)
}

fn keyword_class(c: char) -> u32 {
    char_class(c)
}

impl Editor {
    pub(crate) fn re_opts(&self) -> Opts {
        Opts {
            ignorecase: self.opts.ignorecase(),
            smartcase: self.opts.smartcase(),
            last_sub: self.last_sub.clone(),
            tabstop: self.opts.tabstop(),
        }
    }

    fn search_error(&mut self, text: String) {
        self.error(text);
    }

    /// Vim's `do_search()` for a typed pattern (`pat` without its leading
    /// `/` or `?`, possibly followed by the delimiter and an offset), or
    /// `None` to repeat the last search. Returns `None` if nothing was
    /// found, else (new cursor, linewise, inclusive).
    pub(crate) fn do_search(
        &mut self,
        mut dirc: Option<char>,
        pat: Option<&str>,
        count: usize,
    ) -> Option<(Pos, bool, bool)> {
        let mut pos = self.win.cursor;
        let reverse = false;
        let dirc_val = match dirc {
            Some(d) => {
                self.search.fwd = d == '/';
                d
            }
            None => {
                if self.search.fwd ^ reverse {
                    '/'
                } else {
                    '?'
                }
            }
        };
        dirc = Some(dirc_val);
        let mut dirc = dirc.unwrap();
        let mut chars: Vec<char> = pat.map(|p| p.chars().collect()).unwrap_or_default();
        let mut delim = dirc;
        let mut have_pat = pat.is_some();
        let mut retval_line = false;
        #[allow(unused_assignments)]
        let mut inclusive = false;
        #[allow(unused_assignments)]
        let mut echo = String::new();
        let mut wrapped_any = false;
        loop {
            let mut searchstr: Option<String> = None;
            if !have_pat || chars.is_empty() || chars[0] == delim {
                // Use the previous pattern.
                if self.search.pat.is_none() {
                    self.error("E35: No previous regular expression");
                    return None;
                }
            } else {
                searchstr = Some(String::new());
            }
            if have_pat && !chars.is_empty() {
                let end = skip_regexp(&chars, delim);
                let text: String = chars[..end].iter().collect();
                let mut p = end;
                if p < chars.len() && chars[p] == delim {
                    p += 1;
                }
                if searchstr.is_some() {
                    searchstr = Some(text);
                }
                let mut off = SearchOff::default();
                match chars.get(p) {
                    Some('+' | '-' | '0'..='9') => off.line = true,
                    Some('e' | 's' | 'b') => {
                        if chars[p] == 'e' {
                            off.end = true;
                        }
                        p += 1;
                    }
                    _ => {}
                }
                if let Some(c) = chars.get(p)
                    && (c.is_ascii_digit() || *c == '+' || *c == '-')
                {
                    if c.is_ascii_digit() || chars.get(p + 1).is_some_and(|d| d.is_ascii_digit()) {
                        let mut j = p;
                        let neg = chars[j] == '-';
                        if chars[j] == '+' || chars[j] == '-' {
                            j += 1;
                        }
                        let mut v: i64 = 0;
                        while let Some(d) = chars.get(j).and_then(|c| c.to_digit(10)) {
                            v = v.saturating_mul(10).saturating_add(d as i64);
                            j += 1;
                        }
                        off.off = if neg { -v } else { v };
                        p = j;
                    } else {
                        off.off = if *c == '-' { -1 } else { 1 };
                        p += 1;
                    }
                }
                self.search.off = off;
                // Pattern text for the new search.
                if let Some(t) = &searchstr {
                    if t.is_empty() {
                        // empty pattern: use the last one
                    } else {
                        self.search.pat = Some(t.clone());
                        self.last_pat = Some(t.clone());
                    }
                }
                chars.drain(..p);
            } else if !have_pat {
                // `n`: reuse the stored pattern and offset.
            }
            let pattern = match self.search.pat.clone() {
                Some(p) => p,
                None => {
                    self.error("E35: No previous regular expression");
                    return None;
                }
            };
            let off = self.search.off;
            // A character offset is subtracted first so that "/pat/e+2"
            // does not get stuck.
            let mut start = Start::At(pos);
            if !off.line && off.off != 0 {
                let mut p = pos;
                let mut left = off.off.abs();
                while left > 0 {
                    let r = if off.off > 0 {
                        decl(&self.doc.buf, &mut p)
                    } else {
                        incl(&self.doc.buf, &mut p)
                    };
                    if r == -1 {
                        break;
                    }
                    left -= 1;
                }
                start = if left > 0 {
                    Start::Outside
                } else {
                    Start::At(p)
                };
            }
            let re = match Regex::new(&pattern, &self.re_opts()) {
                Ok(r) => r,
                Err(e) => {
                    self.error(e);
                    return None;
                }
            };
            let fl = SearchFlags {
                start: false,
                end: off.end,
                wrapscan: self.opts.wrapscan(),
            };
            let found = searchit(&self.doc.buf, &re, start, dirc == '/', count, &fl);
            let Some(f) = found else {
                self.search_error(format!("E486: Pattern not found: {pattern}"));
                return None;
            };
            wrapped_any |= f.wrapped;
            pos = f.pos;
            inclusive = off.end;
            echo = format!("{dirc}{pattern}");
            if off.line {
                let l = pos.line as i64 + off.off;
                pos.line = l.clamp(0, self.doc.buf.last_line() as i64) as usize;
                pos.col = 0;
                retval_line = true;
            } else if off.off > 0 {
                for _ in 0..off.off {
                    if incl(&self.doc.buf, &mut pos) == -1 {
                        break;
                    }
                }
            } else if off.off < 0 {
                for _ in 0..(-off.off) {
                    if decl(&self.doc.buf, &mut pos) == -1 {
                        break;
                    }
                }
            }
            if !have_pat || chars.first() != Some(&';') {
                break;
            }
            let nd = chars.get(1).copied();
            match nd {
                Some(d @ ('/' | '?')) => {
                    dirc = d;
                    delim = d;
                    self.search.fwd = d == '/';
                    chars.drain(..2);
                    have_pat = true;
                }
                _ => {
                    self.error("E386: Expected '?' or '/'  after ';'");
                    return None;
                }
            }
        }
        if wrapped_any {
            let text = if dirc == '/' {
                "search hit BOTTOM, continuing at TOP"
            } else {
                "search hit TOP, continuing at BOTTOM"
            };
            self.message = Some(Message {
                text: text.into(),
                error: true,
            });
        } else {
            self.message = Some(Message {
                text: echo,
                error: false,
            });
        }
        Some((pos, retval_line, inclusive))
    }

    /// Vim's `current_search()` for `gn` (`forward`) and `gN`: selects the
    /// match under the cursor, else the next (previous) one, by starting
    /// Visual mode, or by moving the Visual cursor when Visual mode is
    /// already active. The search trick is to search backwards for the
    /// match ending before the cursor, then forwards from there, so a match
    /// at the cursor is found. Returns false (with the cursor unchanged) if
    /// there is no match. Operators are applied by the caller.
    pub(crate) fn current_search(&mut self, count: usize, forward: bool) -> bool {
        let Some(pat) = self.search.pat.clone() else {
            return false;
        };
        let Ok(re) = Regex::new(&pat, &self.re_opts()) else {
            return false;
        };
        let visual = self.is_visual();
        let orig_cursor = self.win.cursor;
        let orig_vstart = self.win.vstart;
        // Visual forward with the cursor before the anchor: the first
        // backward search would not move the cursor, so it is skipped.
        let skip_first_backward = forward && visual && self.win.cursor < self.win.vstart;

        let mut pos = self.win.cursor;
        if visual {
            if forward {
                incl(&self.doc.buf, &mut pos);
            } else {
                decl(&self.doc.buf, &mut pos);
            }
        }

        // Is the pattern zero-width at the cursor? (Only matters for the
        // SEARCH_END search below.)
        let zero_width = match searchit(
            &self.doc.buf,
            &re,
            Start::At(self.win.cursor),
            true,
            1,
            &SearchFlags {
                start: true,
                end: false,
                wrapscan: true,
            },
        ) {
            Some(f) => f.pos == f.other,
            None => return false,
        };

        let mut from = Start::At(pos);
        let mut end_pos = pos;
        for i in 0..2 {
            // Forward: first a backward search (i == 0), then a forward one.
            // Backward: the other way round.
            let dir_fwd = if forward { i == 1 } else { i == 0 };
            if forward && i == 0 && skip_first_backward {
                continue;
            }
            let fl = SearchFlags {
                start: false,
                // The backward search finds the match by its last char, so
                // it skips a match that contains the cursor.
                end: !dir_fwd && !zero_width,
                // No wrapping in the first search, as in Vim.
                wrapscan: i == 1 && self.opts.wrapscan(),
            };
            let cnt = if i == 1 { count.max(1) } else { 1 };
            match searchit(&self.doc.buf, &re, from, dir_fwd, cnt, &fl) {
                Some(f) => {
                    pos = f.pos;
                    end_pos = f.other;
                    from = Start::At(pos);
                }
                None if i == 1 => {
                    self.win.cursor = orig_cursor;
                    self.win.vstart = orig_vstart;
                    return false;
                }
                None => {
                    // Try again from the other end of the buffer.
                    if forward {
                        from = Start::Outside;
                    } else {
                        let last = self.doc.buf.last_line();
                        pos = Pos::new(last, self.doc.buf.line_len(last));
                        from = Start::At(pos);
                    }
                }
            }
        }

        let start_pos = pos;
        if !visual {
            self.win.vstart = start_pos;
        }
        self.win.cursor = end_pos;
        if self.win.vstart < end_pos && forward {
            if skip_first_backward {
                // Put the cursor on the start of the match.
                self.win.cursor = pos;
            } else {
                // Put the cursor on the last char of the match.
                decl(&self.doc.buf, &mut self.win.cursor);
            }
        } else if visual && self.win.cursor < self.win.vstart && forward {
            self.win.cursor = pos;
        }
        self.mode = Mode::Visual(VisualMode::Char);
        true
    }

    /// The identifier or string under or after the cursor (Vim's
    /// `find_ident_under_cursor` with `FIND_IDENT|FIND_STRING`).
    pub(crate) fn ident_under_cursor(&self) -> Option<(usize, String)> {
        let line: Vec<char> = self.doc.buf.line(self.win.cursor.line).chars().collect();
        let cls = |c: Option<&char>| c.map_or(0, |&c| keyword_class(c));
        let mut col = 0;
        let mut this_class = 0;
        for i in 0..2 {
            col = self.win.cursor.col.min(line.len());
            while col < line.len() {
                let c = cls(line.get(col));
                if c != 0 && (i == 1 || c != 1) {
                    break;
                }
                col += 1;
            }
            this_class = cls(line.get(col));
            while col > 0 && this_class != 0 {
                let prev = cls(line.get(col - 1));
                if this_class != prev {
                    break;
                }
                col -= 1;
            }
            if this_class > 2 {
                this_class = 2;
            }
            if this_class == 2 {
                // An identifier.
                break;
            }
            if i == 1 {
                break;
            }
        }
        if col >= line.len() || (this_class != 2 && this_class == 0) {
            return None;
        }
        let start = col;
        let first = cls(line.get(col));
        let mut end = col;
        if this_class == 2 {
            while end < line.len() && cls(line.get(end)) == first {
                end += 1;
            }
        } else {
            while end < line.len() && cls(line.get(end)) != 0 {
                end += 1;
            }
        }
        Some((start, line[start..end].iter().collect()))
    }
}
