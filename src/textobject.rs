//! Text objects (`iw aw iW aW is as ip ap i( a( ib i{ iB i[ i< i" i' i\``),
//! ported from Vim's textobject.c with 'selection' fixed to "inclusive".
//!
//! Each function mirrors Vim's use of global state: the cursor, and in
//! Visual mode the other end of the selection (`VIsual`) and its mode.

use crate::buffer::Buffer;
use crate::motion::{
    Cur, Pos, decl, find_first_blank, findmatchlimit, findsent, gchar, inc, incl, inindent,
    linewhite, start_ps,
};

/// The selection a text object produced.
pub struct ObjOut {
    /// Operator start (op-pending) or the new Visual start.
    pub start: Pos,
    /// The new cursor (end of the object).
    pub cursor: Pos,
    pub inclusive: bool,
    pub linewise: bool,
}

/// Visual state as text objects see it: the other end and whether the
/// mode is linewise.
#[derive(Clone, Copy)]
pub struct VisualCtx {
    pub start: Pos,
    pub linewise: bool,
}

fn white(c: Option<char>) -> bool {
    matches!(c, Some(' ' | '\t'))
}

pub fn current_word(
    buf: &Buffer,
    cursor: Pos,
    visual: Option<VisualCtx>,
    count: usize,
    include: bool,
    big: bool,
) -> Option<ObjOut> {
    let mut c = Cur::new(buf, cursor, big);
    let mut count = count as i64;
    let mut start_pos = Pos::default();
    let mut inclusive = true;
    let mut include_white = false;
    let mut vis = visual.map(|v| v.start);
    let mut op_start = cursor;

    if visual.is_none() || vis == Some(cursor) {
        c.back_in_line();
        start_pos = c.p;
        if (c.cls() == 0) == include {
            if !c.end_word(1, true, true) {
                return None;
            }
        } else {
            c.fwd_word(1, true);
            if c.p.col == 0 {
                decl(buf, &mut c.p);
            } else {
                c.oneleft();
            }
            if include {
                include_white = true;
            }
        }
        if visual.is_some() {
            vis = Some(start_pos);
        } else {
            op_start = start_pos;
        }
        count -= 1;
    }

    while count > 0 {
        inclusive = true;
        if let Some(v) = vis.filter(|&v| c.p < v) {
            let _ = v;
            if decl(buf, &mut c.p) == -1 {
                return None;
            }
            if include != (c.cls() != 0) {
                if !c.bck_word(1, true) {
                    return None;
                }
            } else {
                if !c.bckend_word(1, true) {
                    return None;
                }
                incl(buf, &mut c.p);
            }
        } else {
            if incl(buf, &mut c.p) == -1 {
                return None;
            }
            if include != (c.cls() == 0) {
                if !c.fwd_word(1, true) && count > 1 {
                    return None;
                }
                if !c.oneleft() {
                    inclusive = false;
                }
            } else if !c.end_word(1, true, true) {
                return None;
            }
        }
        count -= 1;
    }

    if include_white && (c.cls() != 0 || (c.p.col == 0 && !inclusive)) {
        let pos = c.p;
        c.p = start_pos;
        if c.oneleft() {
            c.back_in_line();
            if c.cls() == 0 && c.p.col > 0 {
                if visual.is_some() {
                    vis = Some(c.p);
                } else {
                    op_start = c.p;
                }
            }
        }
        c.p = pos;
    }
    Some(ObjOut {
        start: vis.unwrap_or(op_start),
        cursor: c.p,
        inclusive,
        linewise: false,
    })
}

fn findsent_or(buf: &Buffer, p: Pos, forward: bool) -> Pos {
    findsent(buf, p, forward, 1).unwrap_or(p)
}

/// Skips `count/2` sentences and `count/2` separating white spaces.
fn findsent_forward(buf: &Buffer, cur: &mut Pos, count: i64, at_start_sent: bool) {
    let mut at = at_start_sent;
    let mut count = count;
    while count > 0 {
        count -= 1;
        *cur = findsent_or(buf, *cur, true);
        if at {
            find_first_blank(buf, cur);
        }
        if count == 0 || at {
            decl(buf, cur);
        }
        at = !at;
    }
}

pub fn current_sent(
    buf: &Buffer,
    cursor: Pos,
    visual: Option<VisualCtx>,
    count: usize,
    include: bool,
) -> Option<ObjOut> {
    let mut count = count as i64;
    let mut start_pos = cursor;
    let mut pos = start_pos;
    let mut cur = findsent_or(buf, cursor, true);

    // The "extend" part: Visual mode with a non-empty selection.
    let extend = |start_pos: Pos, mut pos: Pos, mut cur: Pos, vstart: Pos, mut count: i64| -> Pos {
        if start_pos < vstart {
            let mut at_start_sent = true;
            decl(buf, &mut pos);
            while pos < cur {
                if !white(gchar(buf, pos)) {
                    at_start_sent = false;
                    break;
                }
                incl(buf, &mut pos);
            }
            if !at_start_sent {
                cur = findsent_or(buf, cur, false);
                if cur == start_pos {
                    at_start_sent = true;
                } else {
                    cur = findsent_or(buf, cur, true);
                }
            }
            if include {
                count *= 2;
            }
            while count > 0 {
                count -= 1;
                if at_start_sent {
                    find_first_blank(buf, &mut cur);
                }
                let c = gchar(buf, cur);
                if !at_start_sent || (!include && !white(c)) {
                    cur = findsent_or(buf, cur, false);
                }
                at_start_sent = !at_start_sent;
            }
        } else {
            incl(buf, &mut pos);
            let mut at_start_sent = true;
            if pos != cur {
                at_start_sent = false;
                while pos < cur {
                    if !white(gchar(buf, pos)) {
                        at_start_sent = true;
                        break;
                    }
                    incl(buf, &mut pos);
                }
                if at_start_sent {
                    cur = findsent_or(buf, cur, false);
                } else {
                    cur = start_pos;
                }
            }
            if include {
                count *= 2;
            }
            findsent_forward(buf, &mut cur, count, at_start_sent);
        }
        cur
    };

    if let Some(v) = visual
        && start_pos != v.start
    {
        let cur = extend(start_pos, pos, cur, v.start, count);
        return Some(ObjOut {
            start: v.start,
            cursor: cur,
            inclusive: true,
            linewise: v.linewise,
        });
    }

    while white(gchar(buf, pos)) {
        incl(buf, &mut pos);
    }
    let start_blank;
    if pos == cur {
        start_blank = true;
        find_first_blank(buf, &mut start_pos);
    } else {
        start_blank = false;
        cur = findsent_or(buf, cur, false);
        start_pos = cur;
    }
    let ncount = if include {
        count * 2
    } else {
        count - i64::from(start_blank)
    };
    if ncount > 0 {
        findsent_forward(buf, &mut cur, ncount, true);
    } else {
        decl(buf, &mut cur);
    }
    if include {
        if start_blank {
            find_first_blank(buf, &mut cur);
            if white(gchar(buf, cur)) {
                decl(buf, &mut cur);
            }
        } else if !white(gchar(buf, cur)) {
            find_first_blank(buf, &mut start_pos);
        }
    }
    if let Some(v) = visual {
        if start_pos == cur {
            count = count.max(1);
            let cur = extend(start_pos, pos, cur, v.start, count);
            return Some(ObjOut {
                start: v.start,
                cursor: cur,
                inclusive: true,
                linewise: v.linewise,
            });
        }
        return Some(ObjOut {
            start: start_pos,
            cursor: cur,
            inclusive: true,
            linewise: false,
        });
    }
    let inclusive = incl(buf, &mut cur) == -1;
    Some(ObjOut {
        start: start_pos,
        cursor: cur,
        inclusive,
        linewise: false,
    })
}

pub fn current_block(
    buf: &Buffer,
    cursor: Pos,
    visual: Option<VisualCtx>,
    count: usize,
    include: bool,
    what: char,
    other: char,
) -> Option<ObjOut> {
    let old_pos = cursor;
    let mut old_end = cursor;
    let mut old_start = cursor;
    let mut cur = cursor;
    let vis_active = visual.is_some();
    match visual {
        Some(v) if v.start != cursor => {
            if v.start < cursor {
                old_start = v.start;
                cur = v.start;
            } else {
                old_end = v.start;
            }
        }
        _ => {
            if what == '{' {
                while inindent(buf, cur, 1) {
                    if inc(buf, &mut cur) != 0 {
                        break;
                    }
                }
            }
            if gchar(buf, cur) == Some(what) {
                cur.col += 1;
            }
        }
    }
    let _ = old_pos;

    let find = |p: Pos, c: char| findmatchlimit(buf, p, Some(c), None, true).map(|m| m.pos);
    let mut start_pos = cur;
    let mut found = None;
    if find(cur, what).is_some() {
        for _ in 0..count {
            found = find(cur, what);
            match found {
                Some(p) => {
                    cur = p;
                    start_pos = p;
                }
                None => break,
            }
        }
    } else {
        for _ in 0..count {
            found = findmatchlimit(buf, cur, Some(what), Some(true), true).map(|m| m.pos);
            match found {
                Some(p) => {
                    cur = p;
                    start_pos = p;
                }
                None => break,
            }
        }
    }
    found?;
    let mut end_pos = find(cur, other)?;
    cur = end_pos;

    let mut sol = false;
    #[allow(clippy::while_immutable_condition)] // do-while in Vim; exits via break/return
    while !include {
        incl(buf, &mut start_pos);
        sol = cur.col == 0;
        decl(buf, &mut cur);
        while inindent(buf, cur, 1) {
            sol = true;
            if decl(buf, &mut cur) != 0 {
                break;
            }
        }
        if start_pos == end_pos && vis_active {
            return None;
        }
        if vis_active && !(start_pos < old_start) && !(old_end < cur) && start_pos != cur {
            // The area did not grow: select the enclosing block instead.
            cur = old_start;
            decl(buf, &mut cur);
            let p = find(cur, what)?;
            start_pos = p;
            cur = p;
            end_pos = find(cur, other)?;
            cur = end_pos;
        } else {
            break;
        }
    }

    if vis_active {
        if sol && gchar(buf, cur).is_some() {
            inc(buf, &mut cur);
        }
        return Some(ObjOut {
            start: start_pos,
            cursor: cur,
            inclusive: true,
            linewise: false,
        });
    }
    let mut inclusive = false;
    if sol {
        incl(buf, &mut cur);
    } else if start_pos <= cur {
        inclusive = true;
    } else {
        cur = start_pos;
    }
    Some(ObjOut {
        start: start_pos,
        cursor: cur,
        inclusive,
        linewise: false,
    })
}

pub fn current_par(
    buf: &Buffer,
    cursor: Pos,
    visual: Option<VisualCtx>,
    count: usize,
    include: bool,
) -> Option<ObjOut> {
    let last = buf.last_line();
    let mut start_lnum = cursor.line;

    let extend = |mut start_lnum: usize, vstart: Pos| -> (usize, bool) {
        let backward = start_lnum < vstart.line;
        let limit = if backward { 0 } else { last };
        let step = |l: usize| if backward { l - 1 } else { l + 1 };
        let mut ok = true;
        for _ in 0..count {
            if start_lnum == limit {
                ok = false;
                break;
            }
            let mut prev_start_is_white: Option<bool> = None;
            for _ in 0..2 {
                start_lnum = step(start_lnum);
                let start_is_white = linewhite(buf, start_lnum);
                if prev_start_is_white == Some(start_is_white) {
                    start_lnum = if backward {
                        start_lnum + 1
                    } else {
                        start_lnum - 1
                    };
                    break;
                }
                loop {
                    if start_lnum == limit {
                        break;
                    }
                    let next = step(start_lnum);
                    if start_is_white != linewhite(buf, next)
                        || (!start_is_white
                            && start_ps(buf, if backward { start_lnum } else { next }, '\0', false))
                    {
                        break;
                    }
                    start_lnum = next;
                }
                if !include || start_lnum == limit {
                    break;
                }
                prev_start_is_white = Some(start_is_white);
            }
        }
        (start_lnum, ok)
    };

    if let Some(v) = visual
        && start_lnum != v.start.line
    {
        let (l, ok) = extend(start_lnum, v.start);
        if !ok {
            return None;
        }
        return Some(ObjOut {
            start: v.start,
            cursor: Pos::new(l, 0),
            inclusive: true,
            linewise: v.linewise,
        });
    }

    let white_in_front = linewhite(buf, start_lnum);
    while start_lnum > 0 {
        if white_in_front {
            if !linewhite(buf, start_lnum - 1) {
                break;
            }
        } else if linewhite(buf, start_lnum - 1) || start_ps(buf, start_lnum, '\0', false) {
            break;
        }
        start_lnum -= 1;
    }
    let mut end_lnum = start_lnum;
    while end_lnum <= last && linewhite(buf, end_lnum) {
        end_lnum += 1;
    }
    // Vim decrements unconditionally; end_lnum is at least start_lnum here
    // unless start_lnum was not white.
    let mut end_lnum = end_lnum as i64 - 1;
    let mut i = count as i64;
    if !include && white_in_front {
        i -= 1;
    }
    while i > 0 {
        i -= 1;
        if end_lnum == last as i64 {
            return None;
        }
        let do_white = if !include {
            linewhite(buf, (end_lnum + 1) as usize)
        } else {
            false
        };
        if include || !do_white {
            end_lnum += 1;
            while end_lnum < last as i64
                && !linewhite(buf, (end_lnum + 1) as usize)
                && !start_ps(buf, (end_lnum + 1) as usize, '\0', false)
            {
                end_lnum += 1;
            }
        }
        if i == 0 && white_in_front && include {
            break;
        }
        if include || do_white {
            while end_lnum < last as i64 && linewhite(buf, (end_lnum + 1) as usize) {
                end_lnum += 1;
            }
        }
    }
    let end_lnum = end_lnum.max(0) as usize;
    if !white_in_front && !linewhite(buf, end_lnum) && include {
        while start_lnum > 0 && linewhite(buf, start_lnum - 1) {
            start_lnum -= 1;
        }
    }
    if let Some(v) = visual {
        if v.linewise && start_lnum == cursor.line {
            let (l, ok) = extend(cursor.line, v.start);
            if !ok {
                return None;
            }
            return Some(ObjOut {
                start: v.start,
                cursor: Pos::new(l, 0),
                inclusive: true,
                linewise: v.linewise,
            });
        }
        let start = if v.start.line != start_lnum {
            Pos::new(start_lnum, 0)
        } else {
            v.start
        };
        return Some(ObjOut {
            start,
            cursor: Pos::new(end_lnum, 0),
            inclusive: true,
            linewise: true,
        });
    }
    Some(ObjOut {
        start: Pos::new(start_lnum, 0),
        cursor: Pos::new(end_lnum, 0),
        inclusive: true,
        linewise: true,
    })
}

/// Next `quotechar` at or after `col`, skipping chars escaped with `\` when
/// `escape`. -1 if none.
fn find_next_quote(line: &[char], mut col: usize, quotechar: char, escape: bool) -> i64 {
    loop {
        let Some(&c) = line.get(col) else { return -1 };
        if escape && c == '\\' {
            col += 1;
            if col >= line.len() {
                return -1;
            }
        } else if c == quotechar {
            return col as i64;
        }
        col += 1;
    }
}

/// Previous `quotechar` before `col_start`; 0 if none (the caller checks).
fn find_prev_quote(line: &[char], mut col_start: usize, quotechar: char, escape: bool) -> usize {
    while col_start > 0 {
        col_start -= 1;
        let mut n = 0;
        if escape {
            while col_start - n > 0 && line[col_start - n - 1] == '\\' {
                n += 1;
            }
        }
        if n & 1 == 1 {
            col_start -= n;
        } else if line.get(col_start) == Some(&quotechar) {
            break;
        }
    }
    col_start
}

pub fn current_quote(
    buf: &Buffer,
    cursor: Pos,
    visual: Option<VisualCtx>,
    count: usize,
    include: bool,
    quotechar: char,
) -> Option<ObjOut> {
    let line: Vec<char> = buf.line(cursor.line).chars().collect();
    let at = |i: i64| -> char {
        if i < 0 {
            '\0'
        } else {
            line.get(i as usize).copied().unwrap_or('\0')
        }
    };
    let mut col_start = cursor.col as i64;
    let mut col_end: i64;
    let mut vis_empty = true;
    let mut vis_bef_curs = false;
    let mut inside_quotes = false;
    let mut selected_quote = false;
    let mut vstart = cursor;

    if let Some(v) = visual {
        if v.start.line != cursor.line {
            return None;
        }
        vstart = v.start;
        vis_bef_curs = v.start < cursor;
        vis_empty = v.start == cursor;
    }

    if !vis_empty {
        let mut i;
        if vis_bef_curs {
            inside_quotes = vstart.col > 0
                && at(vstart.col as i64 - 1) == quotechar
                && at(cursor.col as i64) != '\0'
                && at(cursor.col as i64 + 1) == quotechar;
            i = vstart.col as i64;
            col_end = cursor.col as i64;
        } else {
            inside_quotes = cursor.col > 0
                && at(cursor.col as i64 - 1) == quotechar
                && at(vstart.col as i64) != '\0'
                && at(vstart.col as i64 + 1) == quotechar;
            i = cursor.col as i64;
            col_end = vstart.col as i64;
        }
        while i <= col_end {
            if at(i) == '\0' {
                break;
            }
            let c = at(i);
            i += 1;
            if c == quotechar {
                selected_quote = true;
                break;
            }
        }
    }

    if !vis_empty && at(col_start) == quotechar {
        if vis_bef_curs {
            col_start = find_next_quote(&line, (col_start + 1) as usize, quotechar, false);
            if col_start < 0 {
                return None;
            }
            col_end = find_next_quote(&line, (col_start + 1) as usize, quotechar, true);
            if col_end < 0 {
                col_end = col_start;
                col_start = cursor.col as i64;
            }
        } else {
            col_end = find_prev_quote(&line, col_start as usize, quotechar, false) as i64;
            if at(col_end) != quotechar {
                return None;
            }
            col_start = find_prev_quote(&line, col_end as usize, quotechar, true) as i64;
            if at(col_start) != quotechar {
                col_start = col_end;
                col_end = cursor.col as i64;
            }
        }
    } else if at(col_start) == quotechar || !vis_empty {
        let mut first_col = col_start;
        if !vis_empty {
            first_col = if vis_bef_curs {
                find_next_quote(&line, col_start as usize, quotechar, false)
            } else {
                find_prev_quote(&line, col_start as usize, quotechar, false) as i64
            };
        }
        col_start = 0;
        loop {
            col_start = find_next_quote(&line, col_start as usize, quotechar, false);
            if col_start < 0 || col_start > first_col {
                return None;
            }
            col_end = find_next_quote(&line, (col_start + 1) as usize, quotechar, true);
            if col_end < 0 {
                return None;
            }
            if col_start <= first_col && first_col <= col_end {
                break;
            }
            col_start = col_end + 1;
        }
    } else {
        col_start = find_prev_quote(&line, col_start as usize, quotechar, true) as i64;
        if at(col_start) != quotechar {
            col_start = find_next_quote(&line, col_start as usize, quotechar, false);
            if col_start < 0 {
                return None;
            }
        }
        col_end = find_next_quote(&line, (col_start + 1) as usize, quotechar, true);
        if col_end < 0 {
            return None;
        }
    }

    if include {
        if matches!(at(col_end + 1), ' ' | '\t') {
            while matches!(at(col_end + 1), ' ' | '\t') {
                col_end += 1;
            }
        } else {
            while col_start > 0 && matches!(at(col_start - 1), ' ' | '\t') {
                col_start -= 1;
            }
        }
    }
    if !include && count < 2 && (vis_empty || !inside_quotes) {
        col_start += 1;
    }
    let mut cur = Pos::new(cursor.line, col_start as usize);
    let mut new_vstart = vstart;
    let mut op_start = cur;
    if visual.is_some() {
        if vis_empty
            || (vis_bef_curs
                && !selected_quote
                && (inside_quotes
                    || (at(vstart.col as i64) != quotechar
                        && (vstart.col == 0 || at(vstart.col as i64 - 1) != quotechar))))
        {
            new_vstart = cur;
        }
    } else {
        op_start = cur;
    }
    cur.col = col_end as usize;
    let mut inclusive = false;
    if (include || count > 1 || (!vis_empty && inside_quotes)) && inc(buf, &mut cur) == 2 {
        inclusive = true;
    }
    if visual.is_some() {
        if vis_empty || vis_bef_curs {
            crate::motion::dec(buf, &mut cur);
        } else {
            if inside_quotes
                || (!selected_quote
                    && at(vstart.col as i64) != quotechar
                    && (at(vstart.col as i64) == '\0' || at(vstart.col as i64 + 1) != quotechar))
            {
                crate::motion::dec(buf, &mut cur);
                new_vstart = cur;
            }
            cur.col = col_start as usize;
        }
        return Some(ObjOut {
            start: new_vstart,
            cursor: cur,
            inclusive: true,
            linewise: false,
        });
    }
    Some(ObjOut {
        start: op_start,
        cursor: cur,
        inclusive,
        linewise: false,
    })
}
