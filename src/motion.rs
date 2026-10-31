//! Cursor motions. The word motions are ports of Vim's `fwd_word`,
//! `bck_word` and `end_word` (src/textobject.c) including their quirks, so
//! they walk the buffer exactly like Vim's `inc_cursor`/`dec_cursor`: the
//! position just past a line's last char (Vim's NUL) is a real stop.

use crate::buffer::Buffer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Vim's character class for word motions (`utf_class`): 0 blank,
/// 1 punctuation, 2 keyword ('iskeyword' default `@,48-57,_,192-255`),
/// 3 emoji, and separate classes for scripts such as CJK so that `w` stops
/// where the script changes.
pub fn char_class(c: char) -> u32 {
    let u = c as u32;
    if u < 0x100 {
        return match c {
            ' ' | '\t' | '\u{a0}' => 0,
            c if c.is_ascii_alphanumeric() || c == '_' => 2,
            '\u{c0}'..='\u{ff}' => 2,
            _ => 1,
        };
    }
    if is_emoji(u) {
        return 3;
    }
    match UTF_CLASSES.binary_search_by(|&(first, last, _)| {
        if u < first {
            std::cmp::Ordering::Greater
        } else if u > last {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(i) => UTF_CLASSES[i].2,
        Err(_) => 2,
    }
}

/// Main emoji blocks. Vim uses its full `emoji_all` table; this covers the
/// common ranges (see PLAN.md, known differences).
fn is_emoji(u: u32) -> bool {
    matches!(u, 0x1f300..=0x1f64f | 0x1f680..=0x1f6ff | 0x1f900..=0x1f9ff | 0x1fa70..=0x1faff)
}

/// From Vim's `utf_class_buf` table (src/mbyte.c): sorted, non-overlapping.
const UTF_CLASSES: &[(u32, u32, u32)] = &[
    (0x037e, 0x037e, 1),
    (0x0387, 0x0387, 1),
    (0x055a, 0x055f, 1),
    (0x0589, 0x0589, 1),
    (0x05be, 0x05be, 1),
    (0x05c0, 0x05c0, 1),
    (0x05c3, 0x05c3, 1),
    (0x05f3, 0x05f4, 1),
    (0x060c, 0x060c, 1),
    (0x061b, 0x061b, 1),
    (0x061f, 0x061f, 1),
    (0x066a, 0x066d, 1),
    (0x06d4, 0x06d4, 1),
    (0x0700, 0x070d, 1),
    (0x0964, 0x0965, 1),
    (0x0970, 0x0970, 1),
    (0x0df4, 0x0df4, 1),
    (0x0e4f, 0x0e4f, 1),
    (0x0e5a, 0x0e5b, 1),
    (0x0f04, 0x0f12, 1),
    (0x0f3a, 0x0f3d, 1),
    (0x0f85, 0x0f85, 1),
    (0x104a, 0x104f, 1),
    (0x10fb, 0x10fb, 1),
    (0x1361, 0x1368, 1),
    (0x166d, 0x166e, 1),
    (0x1680, 0x1680, 0),
    (0x169b, 0x169c, 1),
    (0x16eb, 0x16ed, 1),
    (0x1735, 0x1736, 1),
    (0x17d4, 0x17dc, 1),
    (0x1800, 0x180a, 1),
    (0x2000, 0x200b, 0),
    (0x200c, 0x2027, 1),
    (0x2028, 0x2029, 0),
    (0x202a, 0x202e, 1),
    (0x202f, 0x202f, 0),
    (0x2030, 0x205e, 1),
    (0x205f, 0x205f, 0),
    (0x2060, 0x206f, 1),
    (0x2070, 0x207f, 0x2070),
    (0x2080, 0x2094, 0x2080),
    (0x20a0, 0x27ff, 1),
    (0x2800, 0x28ff, 0x2800),
    (0x2900, 0x2998, 1),
    (0x29d8, 0x29db, 1),
    (0x29fc, 0x29fd, 1),
    (0x2e00, 0x2e7f, 1),
    (0x3000, 0x3000, 0),
    (0x3001, 0x3020, 1),
    (0x3030, 0x3030, 1),
    (0x303d, 0x303d, 1),
    (0x3040, 0x309f, 0x3040),
    (0x30a0, 0x30ff, 0x30a0),
    (0x3300, 0x9fff, 0x4e00),
    (0xac00, 0xd7a3, 0xac00),
    (0xf900, 0xfaff, 0x4e00),
    (0xfd3e, 0xfd3f, 1),
    (0xfe30, 0xfe6b, 1),
    (0xff00, 0xff0f, 1),
    (0xff1a, 0xff20, 1),
    (0xff3b, 0xff40, 1),
    (0xff5b, 0xff65, 1),
    (0x1d000, 0x1d24f, 1),
    (0x1d400, 0x1d7ff, 1),
    (0x1f000, 0x1f2ff, 1),
    (0x1f300, 0x1f9ff, 1),
    (0x20000, 0x2a6df, 0x4e00),
    (0x2a700, 0x2b73f, 0x4e00),
    (0x2b740, 0x2b81f, 0x4e00),
    (0x2f800, 0x2fa1f, 0x4e00),
];

/// Char at `p`, or `None` on the end-of-line position (Vim's NUL).
pub fn gchar(buf: &Buffer, p: Pos) -> Option<char> {
    buf.char_at(p.line, p.col)
}

fn is_white(c: Option<char>) -> bool {
    matches!(c, Some(' ' | '\t'))
}

/// Vim's `inc()`: 0 moved within the line, 2 moved onto the end-of-line
/// position, 1 moved to the next line, -1 already at the end of the buffer.
pub fn inc(buf: &Buffer, p: &mut Pos) -> i32 {
    let len = buf.line_len(p.line);
    if p.col < len {
        p.col += 1;
        return if p.col < len { 0 } else { 2 };
    }
    if p.line < buf.last_line() {
        p.line += 1;
        p.col = 0;
        return 1;
    }
    -1
}

/// Vim's `dec()`: 0 within the line, 1 moved to the end-of-line position of
/// the previous line, -1 at the start of the buffer.
pub fn dec(buf: &Buffer, p: &mut Pos) -> i32 {
    if p.col > 0 {
        p.col -= 1;
        return 0;
    }
    if p.line > 0 {
        p.line -= 1;
        p.col = buf.line_len(p.line);
        return 1;
    }
    -1
}

/// `inc()` that skips the end-of-line position.
pub fn incl(buf: &Buffer, p: &mut Pos) -> i32 {
    let mut r = inc(buf, p);
    if r >= 1 && p.col > 0 {
        r = inc(buf, p);
    }
    r
}

/// `dec()` that skips the end-of-line position.
pub fn decl(buf: &Buffer, p: &mut Pos) -> i32 {
    let mut r = dec(buf, p);
    if r == 1 && p.col > 0 {
        r = dec(buf, p);
    }
    r
}

/// A cursor walking the buffer with Vim's word classes, mirroring the
/// global `curwin->w_cursor` + `cls_bigword` state of textobject.c.
pub struct Cur<'a> {
    pub buf: &'a Buffer,
    pub p: Pos,
    pub big: bool,
}

impl<'a> Cur<'a> {
    pub fn new(buf: &'a Buffer, p: Pos, big: bool) -> Self {
        Self { buf, p, big }
    }

    pub fn cls(&self) -> u32 {
        match gchar(self.buf, self.p) {
            None => 0,
            Some(c) => {
                let k = char_class(c);
                if self.big && k != 0 { 1 } else { k }
            }
        }
    }

    pub fn inc(&mut self) -> i32 {
        inc(self.buf, &mut self.p)
    }

    pub fn dec(&mut self) -> i32 {
        dec(self.buf, &mut self.p)
    }

    fn line_empty(&self) -> bool {
        self.buf.line_len(self.p.line) == 0
    }

    /// Moves while the class is `class`. True if the buffer boundary was hit.
    fn skip_chars(&mut self, class: u32, forward: bool) -> bool {
        while self.cls() == class {
            let r = if forward { self.inc() } else { self.dec() };
            if r == -1 {
                return true;
            }
        }
        false
    }

    /// `fwd_word()`. With `eol`, stop at the end of the line on the last
    /// count (operator-pending `w`).
    pub fn fwd_word(&mut self, count: usize, eol: bool) -> bool {
        let mut count = count as i64;
        while {
            count -= 1;
            count >= 0
        } {
            let sclass = self.cls();
            let last_line = self.p.line == self.buf.last_line();
            let i = self.inc();
            if i == -1 || (i >= 1 && last_line) {
                return false;
            }
            if i >= 1 && eol && count == 0 {
                return true;
            }
            if sclass != 0 {
                while self.cls() == sclass {
                    let i = self.inc();
                    if i == -1 || (i >= 1 && eol && count == 0) {
                        return true;
                    }
                }
            }
            while self.cls() == 0 {
                if self.p.col == 0 && self.line_empty() {
                    break;
                }
                let i = self.inc();
                if i == -1 || (i >= 1 && eol && count == 0) {
                    return true;
                }
            }
        }
        true
    }

    /// `bck_word()`. With `stop`, do not move past the start of the word
    /// the cursor is on.
    pub fn bck_word(&mut self, count: usize, mut stop: bool) -> bool {
        'outer: for _ in 0..count {
            let sclass = self.cls();
            if self.dec() == -1 {
                return false;
            }
            if !stop || sclass == self.cls() || sclass == 0 {
                while self.cls() == 0 {
                    if self.p.col == 0 && self.line_empty() {
                        stop = false;
                        continue 'outer;
                    }
                    if self.dec() == -1 {
                        return true;
                    }
                }
                let c = self.cls();
                if self.skip_chars(c, false) {
                    return true;
                }
            }
            self.inc();
            stop = false;
        }
        true
    }

    /// `end_word()`. `stop`: if already at the end of a word, move one word
    /// less. `empty`: stop on an empty line.
    pub fn end_word(&mut self, count: usize, mut stop: bool, empty: bool) -> bool {
        'outer: for _ in 0..count {
            let sclass = self.cls();
            if self.inc() == -1 {
                return false;
            }
            if self.cls() == sclass && sclass != 0 {
                if self.skip_chars(sclass, true) {
                    return false;
                }
            } else if !stop || sclass == 0 {
                while self.cls() == 0 {
                    if empty && self.p.col == 0 && self.line_empty() {
                        stop = false;
                        continue 'outer;
                    }
                    if self.inc() == -1 {
                        return false;
                    }
                }
                let c = self.cls();
                if self.skip_chars(c, true) {
                    return false;
                }
            }
            self.dec();
            stop = false;
        }
        true
    }

    /// `bckend_word()` (`ge`, `gE`).
    pub fn bckend_word(&mut self, count: usize, eol: bool) -> bool {
        for _ in 0..count {
            let sclass = self.cls();
            let i = self.dec();
            if i == -1 {
                return false;
            }
            if eol && i == 1 {
                return true;
            }
            if sclass != 0 {
                while self.cls() == sclass {
                    let i = self.dec();
                    if i == -1 || (eol && i == 1) {
                        return true;
                    }
                }
            }
            while self.cls() == 0 {
                if self.p.col == 0 && self.line_empty() {
                    break;
                }
                let i = self.dec();
                if i == -1 || (eol && i == 1) {
                    return true;
                }
            }
        }
        true
    }

    /// Back to the start of the word or of the white space under the cursor.
    pub fn back_in_line(&mut self) {
        let sclass = self.cls();
        loop {
            if self.p.col == 0 {
                break;
            }
            self.dec();
            if self.cls() != sclass {
                self.inc();
                break;
            }
        }
    }

    /// `oneleft()`: false at column 0.
    pub fn oneleft(&mut self) -> bool {
        if self.p.col == 0 {
            return false;
        }
        self.p.col -= 1;
        true
    }
}

/// Moves `p` back over white space to the first blank of a run.
pub fn find_first_blank(buf: &Buffer, p: &mut Pos) {
    while decl(buf, p) != -1 {
        if !is_white(gchar(buf, *p)) {
            incl(buf, p);
            break;
        }
    }
}

/// Column of the first non-blank char. On an all-blank line this is the
/// end-of-line position, or the last char when `fix` (Vim's BL_FIX).
pub fn first_non_blank(buf: &Buffer, line: usize, fix: bool) -> usize {
    let l = buf.line(line);
    let len = l.len_chars();
    let mut col = 0;
    while col < len && matches!(l.char(col), ' ' | '\t') {
        if fix && col + 1 == len {
            break;
        }
        col += 1;
    }
    col
}

/// Vim's `inindent(extra)`: the cursor is within the leading white space
/// (or `extra` chars past it).
pub fn inindent(buf: &Buffer, p: Pos, extra: usize) -> bool {
    let l = buf.line(p.line);
    let white = l.chars().take_while(|&c| c == ' ' || c == '\t').count();
    white >= p.col + extra
}

/// The line has only white space ('linewhite').
pub fn linewhite(buf: &Buffer, line: usize) -> bool {
    buf.line(line).chars().all(|c| c == ' ' || c == '\t')
}

// ---- paragraphs and sentences (textobject.c) ---------------------------

const PARAGRAPHS: &str = "IPLPPPQPP TPHPLIPpLpItpplpipbp";
const SECTIONS: &str = "SHNHH HUnhsh";

/// `inmacro()`: `s` (the text after a leading '.') is an nroff macro listed
/// in `opt` (pairs of chars).
fn inmacro(opt: &str, s: &[char]) -> bool {
    let o: Vec<char> = opt.chars().collect();
    let s0 = s.first().copied();
    let s1 = s.get(1).copied();
    let mut i = 0;
    while i < o.len() {
        let m0 = o[i];
        let m1 = o.get(i + 1).copied();
        let first = m0 == s0.unwrap_or('\0') || (m0 == ' ' && matches!(s0, None | Some(' ')));
        let second = m1 == s1
            || (matches!(m1, None | Some(' ')) && (s0.is_none() || matches!(s1, None | Some(' '))));
        if first && second {
            return true;
        }
        i += 2;
    }
    false
}

/// `startPS()`: line starts a paragraph or section. `para` is '\0', '{' or '}'.
pub fn start_ps(buf: &Buffer, line: usize, para: char, both: bool) -> bool {
    let s: Vec<char> = buf.line(line).chars().collect();
    // An empty line matches `para == '\0'`: that is what separates
    // paragraphs.
    let first = s.first().copied().unwrap_or('\0');
    if first == para || first == '\x0c' || (both && first == '}') {
        return true;
    }
    if first == '.'
        && (inmacro(SECTIONS, &s[1..]) || (para == '\0' && inmacro(PARAGRAPHS, &s[1..])))
    {
        return true;
    }
    false
}

/// `findpar()`: `{` / `}`. Returns the new position and whether the last
/// char is included, or `None` on failure.
pub fn findpar(
    buf: &Buffer,
    cursor: Pos,
    forward: bool,
    count: usize,
    what: char,
    both: bool,
) -> Option<(Pos, bool)> {
    let last = buf.last_line() as i64;
    let mut curr = cursor.line as i64;
    let dir: i64 = if forward { 1 } else { -1 };
    let mut count = count;
    while count > 0 {
        count -= 1;
        let mut did_skip = false;
        let mut first = true;
        loop {
            if buf.line_len(curr as usize) != 0 {
                did_skip = true;
            }
            if !first && did_skip && start_ps(buf, curr as usize, what, both) {
                break;
            }
            curr += dir;
            if curr < 0 || curr > last {
                if count > 0 {
                    return None;
                }
                curr -= dir;
                break;
            }
            first = false;
        }
    }
    if both && buf.char_at(curr as usize, 0) == Some('}') {
        curr += 1;
    }
    let line = curr as usize;
    let mut incl = false;
    let mut col = 0;
    if curr == last && what != '}' && forward {
        let len = buf.line_len(line);
        if len != 0 {
            col = len - 1;
            incl = true;
        }
    }
    Some((Pos::new(line, col), incl))
}

fn gc(buf: &Buffer, p: Pos) -> char {
    gchar(buf, p).unwrap_or('\0')
}

/// `findsent()`: `(` / `)`.
pub fn findsent(buf: &Buffer, cursor: Pos, forward: bool, count: usize) -> Option<Pos> {
    let func = |p: &mut Pos| if forward { incl(buf, p) } else { decl(buf, p) };
    let mut pos = cursor;
    let mut count = count as i64;
    let mut noskip = false;
    while count > 0 {
        count -= 1;
        let prev_pos = pos;
        let mut goto_found = false;
        if gc(buf, pos) == '\0' {
            loop {
                if func(&mut pos) == -1 {
                    break;
                }
                if gc(buf, pos) != '\0' {
                    break;
                }
            }
            if forward {
                goto_found = true;
            }
        } else if forward && pos.col == 0 && start_ps(buf, pos.line, '\0', false) {
            if pos.line == buf.last_line() {
                return None;
            }
            pos.line += 1;
            goto_found = true;
        } else if !forward {
            decl(buf, &mut pos);
        }

        if !goto_found {
            let mut found_dot = false;
            loop {
                let c = gc(buf, pos);
                if !(c == ' ' || c == '\t' || ".!?)]\"'".contains(c) && c != '\0') {
                    break;
                }
                let mut tpos = pos;
                if decl(buf, &mut tpos) == -1 || (buf.line_len(tpos.line) == 0 && forward) {
                    break;
                }
                if found_dot {
                    break;
                }
                if ".!?".contains(c) {
                    found_dot = true;
                }
                let tc = gc(buf, tpos);
                if ")]\"'".contains(c) && !(".!?)]\"'".contains(tc) && tc != '\0') {
                    break;
                }
                decl(buf, &mut pos);
            }
            let startlnum = pos.line;
            loop {
                let c = gc(buf, pos);
                if c == '\0' || (pos.col == 0 && start_ps(buf, pos.line, '\0', false)) {
                    if !forward && pos.line != startlnum {
                        pos.line += 1;
                        pos.col = 0;
                    }
                    break;
                }
                if c == '.' || c == '!' || c == '?' {
                    let mut tpos = pos;
                    let mut cc: i64;
                    loop {
                        cc = inc(buf, &mut tpos) as i64;
                        if cc == -1 {
                            break;
                        }
                        let g = gc(buf, tpos);
                        cc = g as i64;
                        if !(")]\"'".contains(g) && g != '\0') {
                            break;
                        }
                    }
                    if cc == -1 || cc == ' ' as i64 || cc == '\t' as i64 || cc == 0 {
                        pos = tpos;
                        if gc(buf, pos) == '\0' {
                            inc(buf, &mut pos);
                        }
                        break;
                    }
                }
                if func(&mut pos) == -1 {
                    if count > 0 {
                        return None;
                    }
                    noskip = true;
                    break;
                }
            }
        }
        // found:
        while !noskip && matches!(gc(buf, pos), ' ' | '\t') {
            if incl(buf, &mut pos) == -1 {
                break;
            }
        }
        if prev_pos == pos {
            if func(&mut pos) == -1 {
                if count > 0 {
                    return None;
                }
                break;
            }
            count += 1;
        }
    }
    Some(pos)
}

// ---- f, t, F, T ----------------------------------------------------------

/// `searchc()` on one line. `stop == false` skips a match right next to the
/// cursor (`;` after `t`).
pub fn searchc(
    buf: &Buffer,
    cursor: Pos,
    c: char,
    forward: bool,
    till: bool,
    count: usize,
    mut stop: bool,
) -> Option<usize> {
    let line = buf.line(cursor.line);
    let len = line.len_chars() as i64;
    let mut col = cursor.col as i64;
    let dir: i64 = if forward { 1 } else { -1 };
    for _ in 0..count {
        loop {
            col += dir;
            if col < 0 || col >= len {
                return None;
            }
            if line.char(col as usize) == c && stop {
                break;
            }
            stop = true;
        }
    }
    if till {
        col -= dir;
    }
    Some(col as usize)
}

// ---- % (findmatchlimit, search.c) ----------------------------------------

const MATCHPAIRS: [(char, char); 3] = [('(', ')'), ('{', '}'), ('[', ']')];

/// `find_mps_values()`: for `initc`, the char to find and whether to search
/// backwards. With `switchit`, the roles are swapped (used by text objects).
fn find_mps(initc: char, switchit: bool) -> Option<(char, char, bool)> {
    for (open, close) in MATCHPAIRS {
        if initc == open {
            return Some(if switchit {
                (close, open, true)
            } else {
                (open, close, false)
            });
        }
        if initc == close {
            return Some(if switchit {
                (open, close, false)
            } else {
                (close, open, true)
            });
        }
    }
    None
}

fn starts_with_at(line: &[char], at: usize, s: &str) -> bool {
    let s: Vec<char> = s.chars().collect();
    line.len() >= at + s.len() && line[at..at + s.len()] == s[..]
}

fn skipwhite_idx(line: &[char], mut i: usize) -> usize {
    while i < line.len() && (line[i] == ' ' || line[i] == '\t') {
        i += 1;
    }
    i
}

/// Result of a `%`-style match search.
pub struct MatchResult {
    pub pos: Pos,
    /// The `#if`/`#endif` form, which makes `%` linewise.
    pub linewise: bool,
}

/// Port of `findmatchlimit()` without Lisp and comment skipping.
/// `initc`: `None` for `%` (use the char under or after the cursor).
/// `dir`: `Some(true)` forward, `Some(false)` backward. `cpo_match`: Vi
/// compatible matching ('cpoptions' has `%`), as text objects use.
pub fn findmatchlimit(
    buf: &Buffer,
    cursor: Pos,
    initc: Option<char>,
    dir: Option<bool>,
    cpo_match: bool,
) -> Option<MatchResult> {
    let mut pos = cursor;
    let line_chars = |l: usize| -> Vec<char> { buf.line(l).chars().collect() };
    let mut linep = line_chars(pos.line);
    let at = |linep: &Vec<char>, i: i64| -> char {
        if i < 0 {
            '\0'
        } else {
            linep.get(i as usize).copied().unwrap_or('\0')
        }
    };
    let mut findc = '\0';
    let mut initc_v = '\0';
    let mut backwards = false;
    let mut hash_dir = 0i32;
    let mut comment_dir = 0i32;
    let mut match_escaped = 0;
    let cpo_bsl = false;
    let mut linewise = false;

    if let Some(ic) = initc {
        let (i2, f2, b2) = find_mps(ic, true)?;
        initc_v = i2;
        findc = f2;
        backwards = b2;
        if let Some(d) = dir {
            backwards = !d;
        }
    } else {
        if !cpo_match {
            let ws = skipwhite_idx(&linep, 0);
            if at(&linep, ws as i64) == '#' && pos.col <= ws {
                let p = skipwhite_idx(&linep, ws + 1);
                if starts_with_at(&linep, p, "if")
                    || starts_with_at(&linep, p, "endif")
                    || starts_with_at(&linep, p, "el")
                {
                    hash_dir = 1;
                }
            } else if at(&linep, pos.col as i64) == '/' {
                if at(&linep, pos.col as i64 + 1) == '*' {
                    comment_dir = 1;
                    backwards = false;
                    pos.col += 1;
                } else if pos.col > 0 && at(&linep, pos.col as i64 - 1) == '*' {
                    comment_dir = -1;
                    backwards = true;
                    pos.col -= 1;
                }
            } else if at(&linep, pos.col as i64) == '*' {
                if at(&linep, pos.col as i64 + 1) == '/' {
                    comment_dir = -1;
                    backwards = true;
                } else if pos.col > 0 && at(&linep, pos.col as i64 - 1) == '/' {
                    comment_dir = 1;
                    backwards = false;
                }
            }
        }
        if hash_dir == 0 && comment_dir == 0 {
            if at(&linep, pos.col as i64) == '\0' && pos.col > 0 {
                pos.col -= 1;
            }
            loop {
                let c = at(&linep, pos.col as i64);
                if c == '\0' {
                    break;
                }
                if let Some((i2, f2, b2)) = find_mps(c, false) {
                    initc_v = i2;
                    findc = f2;
                    backwards = b2;
                    break;
                }
                pos.col += 1;
            }
            if findc == '\0' {
                let ws = skipwhite_idx(&linep, 0);
                if !cpo_match && at(&linep, ws as i64) == '#' {
                    hash_dir = 1;
                } else {
                    return None;
                }
            } else if !cpo_bsl {
                let mut bslcnt = 0;
                let mut col = pos.col as i64;
                while col > 0 && at(&linep, col - 1) == '\\' {
                    bslcnt += 1;
                    col -= 1;
                }
                match_escaped = bslcnt & 1;
            }
        }
        if hash_dir != 0 {
            linewise = true;
            let ws = skipwhite_idx(&linep, 0);
            let p = skipwhite_idx(&linep, ws + 1);
            if starts_with_at(&linep, p, "if") || starts_with_at(&linep, p, "el") {
                hash_dir = 1;
            } else if starts_with_at(&linep, p, "endif") {
                hash_dir = -1;
            } else {
                return None;
            }
            pos.col = 0;
            let mut count = 0;
            loop {
                if hash_dir > 0 {
                    if pos.line == buf.last_line() {
                        break;
                    }
                    pos.line += 1;
                } else {
                    if pos.line == 0 {
                        break;
                    }
                    pos.line -= 1;
                }
                let l = line_chars(pos.line);
                let ws = skipwhite_idx(&l, 0);
                if at(&l, ws as i64) != '#' {
                    continue;
                }
                pos.col = ws;
                let p = skipwhite_idx(&l, ws + 1);
                if hash_dir > 0 {
                    if starts_with_at(&l, p, "if") {
                        count += 1;
                    } else if starts_with_at(&l, p, "el") {
                        if count == 0 {
                            return Some(MatchResult { pos, linewise });
                        }
                    } else if starts_with_at(&l, p, "endif") {
                        if count == 0 {
                            return Some(MatchResult { pos, linewise });
                        }
                        count -= 1;
                    }
                } else if starts_with_at(&l, p, "if") {
                    if count == 0 {
                        return Some(MatchResult { pos, linewise });
                    }
                    count -= 1;
                } else if starts_with_at(&l, p, "endif") {
                    count += 1;
                }
            }
            return None;
        }
    }

    let mut do_quotes: i64 = -1;
    let mut start_in_quotes: i32 = 2; // MAYBE
    let mut inquote = false;
    let mut count = 0i64;
    loop {
        // Move one char.
        if backwards {
            if pos.col == 0 {
                if pos.line == 0 {
                    break;
                }
                pos.line -= 1;
                linep = line_chars(pos.line);
                pos.col = linep.len();
                do_quotes = -1;
            } else {
                pos.col -= 1;
            }
        } else if at(&linep, pos.col as i64) == '\0' {
            if pos.line == buf.last_line() {
                break;
            }
            pos.line += 1;
            linep = line_chars(pos.line);
            pos.col = 0;
            do_quotes = -1;
        } else {
            pos.col += 1;
        }

        let col = pos.col as i64;
        if comment_dir != 0 {
            if comment_dir > 0 {
                if at(&linep, col) == '*' && at(&linep, col + 1) == '/' {
                    pos.col += 1;
                    return Some(MatchResult { pos, linewise });
                }
            } else if col > 0 && at(&linep, col - 1) == '/' && at(&linep, col) == '*' {
                pos.col -= 1;
                return Some(MatchResult { pos, linewise });
            }
            continue;
        }

        if cpo_match {
            do_quotes = 0;
        } else if do_quotes == -1 {
            let mut at_start = do_quotes;
            let mut i = 0usize;
            let n = linep.len();
            while i < n {
                if i as i64 == col + i64::from(backwards) {
                    at_start = do_quotes & 1;
                }
                if linep[i] == '"'
                    && (i == 0 || linep[i - 1] != '\'' || at(&linep, i as i64 + 1) != '\'')
                {
                    do_quotes += 1;
                }
                if linep[i] == '\\' && i + 1 < n {
                    i += 1;
                }
                i += 1;
            }
            do_quotes &= 1;
            if do_quotes == 0 {
                inquote = false;
                if n > 0 && linep[n - 1] == '\\' {
                    do_quotes = 1;
                    if start_in_quotes == 2 {
                        inquote = at_start != 0;
                        if inquote {
                            start_in_quotes = 1;
                        }
                    } else if backwards {
                        inquote = true;
                    }
                }
                if pos.line > 0 {
                    let prev = buf.line(pos.line - 1);
                    let pl = prev.len_chars();
                    if pl > 0 && prev.char(pl - 1) == '\\' {
                        do_quotes = 1;
                        if start_in_quotes == 2 {
                            inquote = at_start != 0;
                            if inquote {
                                start_in_quotes = 1;
                            }
                        } else if !backwards {
                            inquote = true;
                        }
                    }
                }
            }
        }
        if start_in_quotes == 2 {
            start_in_quotes = 0;
        }

        let c = at(&linep, pos.col as i64);
        let col = pos.col as i64;
        match c {
            '\0' => {
                if col == 0 || at(&linep, col - 1) != '\\' {
                    inquote = false;
                    start_in_quotes = 0;
                }
                continue;
            }
            '"' => {
                if do_quotes != 0 {
                    let mut k = col - 1;
                    while k >= 0 && at(&linep, k) == '\\' {
                        k -= 1;
                    }
                    if ((col - 1 - k) & 1) == 0 {
                        inquote = !inquote;
                        start_in_quotes = 0;
                    }
                }
                continue;
            }
            '\'' if !cpo_match && initc_v != '\'' && findc != '\'' => {
                if backwards {
                    if col > 1 {
                        if at(&linep, col - 2) == '\'' {
                            pos.col -= 2;
                            continue;
                        } else if at(&linep, col - 2) == '\\'
                            && col > 2
                            && at(&linep, col - 3) == '\''
                        {
                            pos.col -= 3;
                            continue;
                        }
                    }
                } else if at(&linep, col + 1) != '\0' {
                    if at(&linep, col + 1) == '\\'
                        && at(&linep, col + 2) != '\0'
                        && at(&linep, col + 3) == '\''
                    {
                        pos.col += 3;
                        continue;
                    } else if at(&linep, col + 2) == '\'' {
                        pos.col += 2;
                        continue;
                    }
                }
            }
            _ => {}
        }
        if (!inquote || start_in_quotes == 1) && (c == initc_v || c == findc) {
            let mut bslcnt = 0;
            if !cpo_bsl {
                let mut k = col;
                while k > 0 && at(&linep, k - 1) == '\\' {
                    bslcnt += 1;
                    k -= 1;
                }
            }
            if cpo_bsl || (bslcnt & 1) == match_escaped {
                if c == initc_v {
                    count += 1;
                } else {
                    if count == 0 {
                        return Some(MatchResult { pos, linewise });
                    }
                    count -= 1;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(text: &str, f: fn(&mut Cur) -> bool, n: usize) -> Vec<Pos> {
        let b = Buffer::from_text(text);
        let mut c = Cur::new(&b, Pos::default(), false);
        let mut out = Vec::new();
        for _ in 0..n {
            f(&mut c);
            out.push(c.p);
        }
        out
    }

    #[test]
    fn w_stops_on_words_punctuation_and_empty_lines() {
        let got = walk("foo.bar  baz\n\nqux", |c| c.fwd_word(1, false), 5);
        let want = [(0, 3), (0, 4), (0, 9), (1, 0), (2, 0)];
        assert_eq!(got, want.map(|(l, c)| Pos::new(l, c)));
    }

    #[test]
    fn matches_brackets() {
        let b = Buffer::from_text("a (b [c] \")\" d) e");
        let m = findmatchlimit(&b, Pos::new(0, 0), None, None, false).unwrap();
        assert_eq!(m.pos, Pos::new(0, 14));
    }
}
