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

fn cls(buf: &Buffer, p: Pos, big: bool) -> u32 {
    match buf.char_at(p.line, p.col) {
        None => 0,
        Some(c) => {
            let k = char_class(c);
            if big && k != 0 { 1 } else { k }
        }
    }
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

fn line_empty(buf: &Buffer, line: usize) -> bool {
    buf.line_len(line) == 0
}

/// Moves while the class is `class`. True if the buffer boundary was hit.
fn skip_chars(buf: &Buffer, p: &mut Pos, class: u32, big: bool, forward: bool) -> bool {
    while cls(buf, *p, big) == class {
        let r = if forward { inc(buf, p) } else { dec(buf, p) };
        if r == -1 {
            return true;
        }
    }
    false
}

/// `w` / `W`. Returns false where Vim would fail (beep).
pub fn fwd_word(buf: &Buffer, p: &mut Pos, count: usize, big: bool) -> bool {
    for _ in 0..count {
        let sclass = cls(buf, *p, big);
        let last_line = p.line == buf.last_line();
        let i = inc(buf, p);
        if i == -1 || (i >= 1 && last_line) {
            return false;
        }
        if sclass != 0 {
            while cls(buf, *p, big) == sclass {
                if inc(buf, p) == -1 {
                    return true;
                }
            }
        }
        while cls(buf, *p, big) == 0 {
            if p.col == 0 && line_empty(buf, p.line) {
                break;
            }
            if inc(buf, p) == -1 {
                return true;
            }
        }
    }
    true
}

/// `b` / `B`.
pub fn bck_word(buf: &Buffer, p: &mut Pos, count: usize, big: bool) -> bool {
    'outer: for _ in 0..count {
        if dec(buf, p) == -1 {
            return false;
        }
        while cls(buf, *p, big) == 0 {
            if p.col == 0 && line_empty(buf, p.line) {
                continue 'outer;
            }
            if dec(buf, p) == -1 {
                return true;
            }
        }
        let c = cls(buf, *p, big);
        if skip_chars(buf, p, c, big, false) {
            return true;
        }
        inc(buf, p);
    }
    true
}

/// `e` / `E`.
pub fn end_word(buf: &Buffer, p: &mut Pos, count: usize, big: bool) -> bool {
    for _ in 0..count {
        let sclass = cls(buf, *p, big);
        if inc(buf, p) == -1 {
            return false;
        }
        if sclass != 0 && cls(buf, *p, big) == sclass {
            if skip_chars(buf, p, sclass, big, true) {
                return false;
            }
        } else {
            while cls(buf, *p, big) == 0 {
                if inc(buf, p) == -1 {
                    return false;
                }
            }
            let c = cls(buf, *p, big);
            if skip_chars(buf, p, c, big, true) {
                return false;
            }
        }
        dec(buf, p);
    }
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(text: &str, f: fn(&Buffer, &mut Pos, usize, bool) -> bool, n: usize) -> Vec<Pos> {
        let b = Buffer::from_text(text);
        let mut p = Pos::default();
        let mut out = Vec::new();
        for _ in 0..n {
            f(&b, &mut p, 1, false);
            out.push(p);
        }
        out
    }

    #[test]
    fn w_stops_on_words_punctuation_and_empty_lines() {
        let got = walk("foo.bar  baz\n\nqux", fwd_word, 5);
        let want = [(0, 3), (0, 4), (0, 9), (1, 0), (2, 0)];
        assert_eq!(got, want.map(|(l, c)| Pos::new(l, c)));
    }

    #[test]
    fn e_and_b() {
        let got = walk("foo.bar  baz", end_word, 3);
        assert_eq!(got, [Pos::new(0, 2), Pos::new(0, 3), Pos::new(0, 6)]);
        let b = Buffer::from_text("foo bar\n\n  baz");
        let mut p = Pos::new(2, 2);
        assert!(bck_word(&b, &mut p, 1, false));
        assert_eq!(p, Pos::new(1, 0));
        assert!(bck_word(&b, &mut p, 1, false));
        assert_eq!(p, Pos::new(0, 4));
    }
}
