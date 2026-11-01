// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! Operators, ported from Vim's ops.c and register.c: the
//! `do_pending_operator()` setup, delete, yank, change, shift, case
//! changes, replace, block insert/append, join and put.

use crate::editor::{Editor, InsertKind, LastVisual, Mode, PostInsert, VisualMode, VisualRedo};
use crate::motion::{self, Pos, dec, first_non_blank, gchar, inc, inindent};
use crate::normal::{MInfo, Op};
use crate::register::{RegType, Register};
use crate::text::{char_width, vcol_range};

/// Vim's `oparg_T`: the area an operator works on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OpArg {
    pub op: Op,
    pub reg: Option<char>,
    pub start: Pos,
    pub end: Pos,
    pub linewise: bool,
    pub inclusive: bool,
    pub block: bool,
    pub start_vcol: usize,
    pub end_vcol: usize,
    pub line_count: usize,
    pub is_visual: bool,
    pub empty: bool,
    pub use_reg_one: bool,
    pub force: Option<char>,
    /// Block extends to the end of each line (`$`).
    pub is_max: bool,
}

/// Vim's `struct block_def` for one line of a block.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BlockDef {
    pub startspaces: usize,
    pub endspaces: usize,
    pub textlen: usize,
    pub textcol: usize,
    pub start_vcol: usize,
    pub end_vcol: usize,
    pub is_short: bool,
    pub is_one_char: bool,
    pub pre_whitesp: usize,
    pub pre_whitesp_c: usize,
    pub start_char_vcols: usize,
    pub end_char_vcols: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PutFlags {
    /// `gp`/`gP`: cursor after the new text.
    pub cursend: bool,
    /// Visual line mode: put whole lines.
    pub line: bool,
    /// Visual char mode with a linewise register: split the line.
    pub line_split: bool,
    /// Visual block mode with `p`.
    pub line_forward: bool,
}

fn chars_of(s: &str) -> Vec<char> {
    s.chars().collect()
}

impl Editor {
    pub(crate) fn line_vec(&self, l: usize) -> Vec<char> {
        self.doc().buf.line(l).chars().collect()
    }

    pub(crate) fn set_line_vec(&mut self, l: usize, v: &[char]) {
        self.doc_mut()
            .buf
            .set_line(l, &v.iter().collect::<String>());
    }

    fn msgmore(&mut self, n: i64) {
        if n.unsigned_abs() > 2 {
            let what = if n > 0 { "more lines" } else { "fewer lines" };
            self.info(format!("{} {what}", n.unsigned_abs()));
        }
    }

    fn check_cursor_col(&mut self) {
        let len = self.doc().buf.line_len(self.win().cursor.line);
        let allow_end = matches!(self.mode, Mode::Insert | Mode::Replace);
        if self.win().cursor.col >= len {
            self.win_mut().cursor.col = if allow_end {
                len
            } else {
                len.saturating_sub(1)
            };
        }
    }

    // ---- do_pending_operator --------------------------------------------

    /// Operator after a motion in Normal mode.
    pub(crate) fn pending_operator(
        &mut self,
        op: Op,
        reg: Option<char>,
        start: Pos,
        info: MInfo,
        force: Option<char>,
    ) {
        let mut oa = OpArg {
            op,
            reg,
            start,
            end: start,
            linewise: info.linewise,
            inclusive: info.inclusive,
            block: false,
            start_vcol: 0,
            end_vcol: 0,
            line_count: 0,
            is_visual: false,
            empty: false,
            use_reg_one: info.use_reg_one,
            force,
            is_max: false,
        };
        match force {
            Some('V') => oa.linewise = true,
            Some('v') => {
                if oa.linewise {
                    oa.inclusive = false;
                } else {
                    oa.inclusive = !oa.inclusive;
                }
                oa.linewise = false;
            }
            _ => {}
        }
        if oa.start < self.win().cursor {
            oa.end = self.win().cursor;
            self.win_mut().cursor = oa.start;
        } else {
            oa.end = oa.start;
            oa.start = self.win().cursor;
        }
        oa.line_count = oa.end.line - oa.start.line + 1;
        self.win_mut().keep_curswant = false;
        oa.empty = !oa.linewise
            && (!oa.inclusive || (op == Op::Yank && gchar(&self.doc().buf, oa.end).is_none()))
            && oa.start == oa.end;
        // An exclusive motion ending in column 0 ends at the end of the
        // previous line, and becomes linewise when it starts in the indent.
        if !oa.linewise && !oa.inclusive && !info.no_adj && oa.end.col == 0 && oa.line_count > 1 {
            oa.line_count -= 1;
            oa.end.line -= 1;
            if inindent(&self.doc().buf, self.win().cursor, 0) {
                oa.linewise = true;
            } else {
                oa.end.col = self.doc().buf.line_len(oa.end.line);
                if oa.end.col > 0 {
                    oa.end.col -= 1;
                    oa.inclusive = true;
                }
            }
        }
        self.dispatch_op(oa, 1);
    }

    /// Operator on the Visual area. `key` is the key typed (`X D Y C S R`
    /// are linewise variants).
    pub(crate) fn visual_operator(
        &mut self,
        op: Op,
        key: char,
        reg: Option<char>,
        count: Option<usize>,
    ) {
        let Some(mut vmode) = self.visual_mode() else {
            return;
        };
        let ts = self.opts.tabstop();
        // Vim's v_visop / nv_subst / nv_Replace.
        match key {
            'S' | 'R' => vmode = VisualMode::Line,
            'X' | 'Y' | 'D' | 'C' if vmode != VisualMode::Block => vmode = VisualMode::Line,
            'D' | 'C' => {
                self.win_mut().curswant = usize::MAX;
            }
            _ => {}
        }
        let replay = self.replaying_visual;
        if replay.is_none() {
            self.doc_mut().last_visual = Some(LastVisual {
                start: self.win().vstart,
                end: self.win().cursor,
                mode: vmode,
                curswant: self.win().curswant,
            });
        }
        let mut oa = OpArg {
            op,
            reg,
            start: self.win().vstart,
            end: self.win().vstart,
            linewise: false,
            inclusive: true,
            block: false,
            start_vcol: 0,
            end_vcol: 0,
            line_count: 0,
            is_visual: true,
            empty: false,
            use_reg_one: false,
            force: None,
            is_max: self.win().curswant == usize::MAX,
        };
        if vmode == VisualMode::Line {
            oa.start.col = 0;
        }
        if oa.start < self.win().cursor {
            oa.end = self.win().cursor;
            self.win_mut().cursor = oa.start;
        } else {
            oa.end = oa.start;
            oa.start = self.win().cursor;
        }
        oa.line_count = oa.end.line - oa.start.line + 1;

        if vmode == VisualMode::Block {
            self.get_op_vcol(&mut oa, replay);
        }
        // Remember the size for "1v" and "." (Vim's resel_VIsual_*).
        if replay.is_none() {
            let vcol = if self.win().curswant == usize::MAX {
                usize::MAX
            } else if vmode == VisualMode::Block {
                oa.end_vcol - oa.start_vcol + 1
            } else {
                let end_vcol = vcol_range(self.doc().buf.line(oa.end.line), oa.end.col, ts).1;
                if oa.line_count <= 1 {
                    let start_vcol =
                        vcol_range(self.doc().buf.line(oa.start.line), oa.start.col, ts).0;
                    end_vcol - start_vcol + 1
                } else {
                    end_vcol
                }
            };
            let vr = VisualRedo {
                mode: vmode,
                vcol,
                line_count: oa.line_count,
                count,
            };
            self.resel = Some(vr);
            if op != Op::Yank
                && let Some(r) = &mut self.redo_building
            {
                r.visual = Some(vr);
            }
        }
        if vmode != VisualMode::Block && op != Op::Yank && oa.line_count <= 1 {
            oa.start_vcol = vcol_range(self.doc().buf.line(oa.start.line), oa.start.col, ts).0;
        }
        if vmode != VisualMode::Block {
            oa.end_vcol = vcol_range(self.doc().buf.line(oa.end.line), oa.end.col, ts).1;
        }

        oa.linewise = vmode == VisualMode::Line;
        if vmode == VisualMode::Char && gchar(&self.doc().buf, oa.end).is_none() {
            oa.inclusive = false;
            if !op.on_lines() && oa.end.line < self.doc().buf.last_line() {
                oa.end.line += 1;
                oa.end.col = 0;
                oa.line_count += 1;
            }
        }
        self.mode = Mode::Normal;
        self.win_mut().keep_curswant = false;
        oa.empty = !oa.linewise
            && ((!oa.inclusive && !oa.block)
                || (op == Op::Yank && gchar(&self.doc().buf, oa.end).is_none()))
            && oa.start == oa.end;
        let amount = count.unwrap_or(1).max(1);
        self.dispatch_op(oa, amount);
    }

    /// Vim's `get_op_vcol()` for a block.
    fn get_op_vcol(&mut self, oa: &mut OpArg, replay: Option<VisualRedo>) {
        let ts = self.opts.tabstop();
        oa.block = true;
        let (s, e) = vcol_range(self.doc().buf.line(oa.start.line), oa.start.col, ts);
        oa.start_vcol = s;
        oa.end_vcol = e;
        if replay.is_none() {
            let (s2, e2) = vcol_range(self.doc().buf.line(oa.end.line), oa.end.col, ts);
            oa.start_vcol = oa.start_vcol.min(s2);
            oa.end_vcol = oa.end_vcol.max(e2);
        }
        if self.win().curswant == usize::MAX {
            oa.end_vcol = 0;
            for l in oa.start.line..=oa.end.line {
                let line = self.doc().buf.line(l);
                let (_, e) = vcol_range(line, line.len_chars(), ts);
                oa.end_vcol = oa.end_vcol.max(e);
            }
        } else if let Some(r) = replay {
            oa.end_vcol = oa.start_vcol + r.vcol - 1;
        }
        let save_mode = self.mode;
        self.mode = Mode::Visual(VisualMode::Block);
        self.win_mut().cursor.line = oa.end.line;
        self.coladvance(oa.end_vcol);
        oa.end = self.win().cursor;
        self.win_mut().cursor = oa.start;
        self.coladvance(oa.start_vcol);
        oa.start = self.win().cursor;
        self.mode = save_mode;
    }

    fn dispatch_op(&mut self, mut oa: OpArg, amount: usize) {
        match oa.op {
            Op::LShift | Op::RShift => self.op_shift(&oa, true, amount),
            Op::Join | Op::JoinNs => {
                oa.line_count = oa.line_count.max(2);
                if self.win().cursor.line + oa.line_count - 1 > self.doc().buf.last_line() {
                    self.beep();
                } else {
                    self.do_join(oa.line_count, oa.op == Op::Join, true);
                }
            }
            Op::Delete => {
                self.op_delete(&mut oa);
            }
            Op::Yank => {
                self.op_yank(&oa, false, true);
                self.check_cursor_col();
            }
            Op::Change => self.op_change(&mut oa),
            Op::Tilde | Op::Upper | Op::Lower | Op::Rot13 => {
                self.op_tilde(&mut oa);
                self.check_cursor_col();
            }
            Op::Insert | Op::Append => self.op_insert(&mut oa),
            Op::Replace(c) => self.op_replace(&mut oa, c),
        }
    }

    // ---- delete and yank --------------------------------------------------

    /// The register contents `op_yank()` would store.
    fn yank_content(&self, oa: &OpArg) -> Register {
        let mut kind = if oa.linewise {
            RegType::Line
        } else {
            RegType::Char
        };
        let mut endline = oa.end.line;
        if !oa.linewise
            && oa.start.col == 0
            && !oa.inclusive
            && !oa.is_visual
            && !oa.block
            && oa.end.col == 0
            && oa.line_count > 1
        {
            kind = RegType::Line;
            endline -= 1;
        }
        if oa.block {
            let mut width = oa.end_vcol - oa.start_vcol;
            if self.win().curswant == usize::MAX && width > 0 {
                width -= 1;
            }
            kind = RegType::Block(width);
        }
        let mut lines = Vec::new();
        for l in oa.start.line..=endline {
            let line = self.line_vec(l);
            let text: String = match kind {
                RegType::Block(_) => {
                    let bd = self.block_prep(oa, l, false);
                    let mut s = " ".repeat(bd.startspaces);
                    s.extend(&line[bd.textcol..(bd.textcol + bd.textlen).min(line.len())]);
                    s.push_str(&" ".repeat(bd.endspaces));
                    s
                }
                RegType::Line => line.iter().collect(),
                RegType::Char => {
                    let startcol = if l == oa.start.line { oa.start.col } else { 0 };
                    let endcol = if l == oa.end.line {
                        oa.end.col + usize::from(oa.inclusive)
                    } else {
                        usize::MAX
                    };
                    let a = startcol.min(line.len());
                    let b = endcol.min(line.len()).max(a);
                    line[a..b].iter().collect()
                }
            };
            lines.push(text);
        }
        Register { kind, lines }
    }

    /// Vim's `op_yank()`. With `deleting`, the register was selected by
    /// `op_delete()`.
    pub(crate) fn op_yank(&mut self, oa: &OpArg, deleting: bool, mess: bool) -> bool {
        if oa.reg == Some('_') {
            return true;
        }
        if !deleting {
            self.regs.select(oa.reg, true);
        }
        let reg = self.yank_content(oa);
        let yanklines = reg.lines.len();
        let kind = reg.kind;
        self.regs.store(reg);
        if mess {
            let n = if kind == RegType::Char && yanklines == 1 {
                0
            } else {
                yanklines
            };
            if n > 2 {
                let into = oa.reg.map_or(String::new(), |r| format!(" into \"{r}"));
                let what = if oa.block { "block of " } else { "" };
                self.info(format!("{what}{n} lines yanked{into}"));
            }
        }
        self.op_start = oa.start;
        self.op_end = oa.end;
        true
    }

    /// Vim's `op_delete()`.
    pub(crate) fn op_delete(&mut self, oa: &mut OpArg) -> bool {
        if self.doc().buf.has_no_lines() {
            return true;
        }
        if oa.empty {
            self.u_save_cursor();
            return true;
        }
        // Vi: a multi-line characterwise delete that leaves only white space
        // becomes linewise.
        if !oa.linewise
            && !oa.is_visual
            && !oa.block
            && oa.line_count > 1
            && oa.force.is_none()
            && oa.op == Op::Delete
        {
            let line = self.line_vec(oa.end.line);
            let mut i = oa.end.col;
            if i < line.len() {
                i += usize::from(oa.inclusive);
            }
            while i < line.len() && (line[i] == ' ' || line[i] == '\t') {
                i += 1;
            }
            if i >= line.len() && inindent(&self.doc().buf, self.win().cursor, 0) {
                oa.linewise = true;
            }
        }
        if !oa.linewise
            && oa.line_count == 1
            && oa.op == Op::Delete
            && self.doc().buf.line_len(oa.start.line) == 0
        {
            return true;
        }

        if oa.reg != Some('_') {
            if oa.reg.is_some() {
                self.regs.select(oa.reg, true);
                self.op_yank(oa, true, false);
            } else {
                self.regs.reset_append();
            }
            if oa.linewise || oa.line_count > 1 || oa.use_reg_one {
                self.regs.shift_delete();
                let r = self.yank_content(oa);
                self.regs.store(r);
            }
            if oa.reg.is_none() && !oa.linewise && oa.line_count == 1 {
                self.regs.select(Some('-'), true);
                let r = self.yank_content(oa);
                self.regs.store(r);
            }
        }

        let old_count = self.doc().buf.line_count() as i64;
        if oa.block {
            self.u_save(oa.start.line, oa.line_count, true);
            for l in self.win().cursor.line..=oa.end.line {
                let bd = self.block_prep(oa, l, true);
                if bd.textlen == 0 {
                    continue;
                }
                if l == self.win().cursor.line {
                    self.win_mut().cursor.col = bd.textcol + bd.startspaces;
                }
                let line = self.line_vec(l);
                let mut new: Vec<char> = line[..bd.textcol].to_vec();
                new.extend(std::iter::repeat_n(' ', bd.startspaces + bd.endspaces));
                new.extend(&line[(bd.textcol + bd.textlen).min(line.len())..]);
                self.set_line_vec(l, &new);
            }
            self.check_cursor_col();
        } else if oa.linewise {
            if oa.op == Op::Change {
                if oa.line_count > 1 {
                    let line = self.win().cursor.line;
                    self.del_lines(line + 1, oa.line_count - 1);
                    self.win_mut().cursor.line = line;
                }
                self.u_save_cursor();
                if self.opts.autoindent() {
                    // Keep the indent; it is removed again if nothing is typed.
                    let line = self.line_vec(self.win().cursor.line);
                    let w = line
                        .iter()
                        .position(|&c| c != ' ' && c != '\t')
                        .unwrap_or(line.len());
                    self.set_line_vec(self.win().cursor.line, &line[..w]);
                    self.win_mut().cursor.col = w;
                    self.did_ai = w > 0;
                } else {
                    self.win_mut().cursor.col = 0;
                    let l = self.win().cursor.line;
                    self.doc_mut().buf.set_line(l, "");
                }
                if oa.line_count > 1 {
                    self.doc_mut().uline = None;
                }
            } else {
                self.del_lines(self.win().cursor.line, oa.line_count);
                self.win_mut().cursor.col =
                    first_non_blank(&self.doc().buf, self.win().cursor.line, true);
                self.doc_mut().uline = None;
            }
        } else if oa.line_count == 1 {
            self.u_save_cursor();
            let n = oa.end.col + 1 - usize::from(!oa.inclusive) - oa.start.col;
            self.del_bytes(n, true);
        } else {
            self.u_save(self.win().cursor.line, oa.line_count, true);
            // Truncate the first line, delete the middle lines, cut the
            // start of the last line and join.
            let line = self.line_vec(self.win().cursor.line);
            self.set_line_vec(
                self.win().cursor.line,
                &line[..self.win().cursor.col.min(line.len())],
            );
            let first = self.win().cursor.line;
            if oa.line_count > 2 {
                self.doc_mut()
                    .buf
                    .delete_lines(first + 1, oa.line_count - 2);
            }
            let n = oa.end.col + 1 - usize::from(!oa.inclusive);
            let last = self.line_vec(first + 1);
            self.set_line_vec(first + 1, &last[n.min(last.len())..]);
            self.win_mut().cursor.line = first;
            self.do_join(2, false, false);
        }
        self.msgmore(self.doc().buf.line_count() as i64 - old_count);
        self.op_start = oa.start;
        self.op_end = oa.start;
        true
    }

    /// Vim's `del_lines()`: deletes lines with undo, cursor to column 0.
    pub(crate) fn del_lines(&mut self, first: usize, n: usize) {
        let n = n.min(self.doc().buf.line_count() - first);
        if n == 0 {
            return;
        }
        self.u_save(first, n, false);
        self.doc_mut().buf.delete_lines(first, n);
        self.win_mut().cursor.line = first.min(self.doc_mut().buf.last_line());
        self.win_mut().cursor.col = 0;
    }

    /// Vim's `del_bytes()` at the cursor. With `fixpos`, a cursor left past
    /// the end moves back onto the last char.
    fn del_bytes(&mut self, n: usize, fixpos: bool) {
        let Pos { line, col } = self.win().cursor;
        let len = self.doc().buf.line_len(line);
        if col >= len {
            return;
        }
        let n = n.min(len - col);
        if col + n >= len && col > 0 && fixpos && !matches!(self.mode, Mode::Insert | Mode::Replace)
        {
            self.win_mut().cursor.col -= 1;
        }
        self.doc_mut().buf.remove(line, col, n);
    }

    // ---- change -----------------------------------------------------------

    /// Vim's `op_change()`: delete, then Insert mode.
    pub(crate) fn op_change(&mut self, oa: &mut OpArg) {
        let l = if oa.linewise { 0 } else { oa.start.col };
        if self.doc().buf.has_no_lines() {
            self.u_save_cursor();
        } else if !self.op_delete_for_change(oa) {
            return;
        }
        if l > self.win().cursor.col && self.doc().buf.line_len(self.win().cursor.line) > 0 {
            let (doc, win) = self.parts_mut();
            inc(&doc.buf, &mut win.cursor);
        }
        let post = if oa.block {
            PostInsert::BlockChange {
                oa: *oa,
                pre_textlen: self.doc().buf.line_len(oa.start.line),
                textcol: self.win().cursor.col,
            }
        } else {
            PostInsert::None
        };
        self.start_insert(1, InsertKind::Inline, false);
        self.ins_post = post;
    }

    fn op_delete_for_change(&mut self, oa: &mut OpArg) -> bool {
        // op_delete() works in Normal mode; fixpos must apply as there.
        self.op_delete(oa)
    }

    /// Copies the text typed on the first line to the other lines of a
    /// block (end of `op_change()`).
    pub(crate) fn finish_block_change(&mut self, oa: OpArg, pre_textlen: usize, textcol: usize) {
        if oa.start.line == oa.end.line {
            return;
        }
        let first = self.line_vec(oa.start.line);
        let Some(ins_len) = first.len().checked_sub(pre_textlen).filter(|&n| n > 0) else {
            return;
        };
        let ins: Vec<char> =
            first[textcol.min(first.len())..(textcol + ins_len).min(first.len())].to_vec();
        for l in oa.start.line + 1..=oa.end.line {
            let bd = self.block_prep(&oa, l, true);
            if !bd.is_short {
                let mut line = self.line_vec(l);
                let at = bd.textcol.min(line.len());
                line.splice(at..at, ins.iter().copied());
                self.u_save(l, 1, true);
                self.set_line_vec(l, &line);
            }
        }
        self.check_cursor_col();
    }

    // ---- block helpers ----------------------------------------------------

    /// Vim's `block_prep()` for line `lnum`.
    pub(crate) fn block_prep(&self, oa: &OpArg, lnum: usize, is_del: bool) -> BlockDef {
        let line = self.line_vec(lnum);
        let mut bd = BlockDef::default();
        let mut incr = 0;
        let mut vcol = 0;
        let mut i = 0;
        let mut prev_pstart = 0;
        while vcol < oa.start_vcol && i < line.len() {
            incr = char_width(line[i], vcol, self.opts.tabstop());
            vcol += incr;
            if line[i] == ' ' || line[i] == '\t' {
                bd.pre_whitesp += incr;
                bd.pre_whitesp_c += 1;
            } else {
                bd.pre_whitesp = 0;
                bd.pre_whitesp_c = 0;
            }
            prev_pstart = i;
            i += 1;
        }
        bd.start_vcol = vcol;
        let mut pstart = i;
        bd.start_char_vcols = incr;
        if bd.start_vcol < oa.start_vcol {
            bd.end_vcol = bd.start_vcol;
            bd.is_short = true;
            if !is_del || oa.op == Op::Append {
                bd.endspaces = oa.end_vcol - oa.start_vcol + 1;
            }
        } else {
            bd.startspaces = bd.start_vcol - oa.start_vcol;
            if is_del && bd.startspaces != 0 {
                bd.startspaces = bd.start_char_vcols - bd.startspaces;
            }
            let mut pend = pstart;
            bd.end_vcol = bd.start_vcol;
            if bd.end_vcol > oa.end_vcol {
                bd.is_one_char = true;
                if oa.op == Op::Insert {
                    bd.endspaces = bd.start_char_vcols - bd.startspaces;
                } else if oa.op == Op::Append {
                    bd.startspaces += oa.end_vcol - oa.start_vcol + 1;
                    bd.endspaces = bd.start_char_vcols.saturating_sub(bd.startspaces);
                } else {
                    bd.startspaces = oa.end_vcol - oa.start_vcol + 1;
                    if is_del && oa.op != Op::LShift {
                        bd.startspaces = bd.start_char_vcols - (bd.start_vcol - oa.start_vcol);
                        bd.endspaces = bd.end_vcol - oa.end_vcol - 1;
                    }
                }
            } else {
                let mut v = bd.end_vcol;
                let mut prev_pend = pend;
                while v <= oa.end_vcol && pend < line.len() {
                    prev_pend = pend;
                    incr = char_width(line[pend], v, self.opts.tabstop());
                    v += incr;
                    pend += 1;
                }
                bd.end_vcol = v;
                if bd.end_vcol <= oa.end_vcol
                    && (!is_del || matches!(oa.op, Op::Append | Op::Replace(_)))
                {
                    bd.is_short = true;
                    bd.endspaces = if oa.op == Op::Append {
                        oa.end_vcol - bd.end_vcol + usize::from(oa.inclusive)
                    } else {
                        0
                    };
                } else if bd.end_vcol > oa.end_vcol {
                    bd.endspaces = bd.end_vcol - oa.end_vcol - 1;
                    if !is_del && bd.endspaces != 0 {
                        bd.endspaces = incr - bd.endspaces;
                        if pend != pstart {
                            pend = prev_pend;
                        }
                    }
                }
            }
            bd.end_char_vcols = incr;
            if is_del && bd.startspaces != 0 {
                pstart = prev_pstart;
            }
            bd.textlen = pend - pstart;
        }
        bd.textcol = pstart;
        bd
    }

    // ---- shift --------------------------------------------------------------

    pub(crate) fn get_indent(line: &[char], ts: usize) -> usize {
        let mut vcol = 0;
        for &c in line {
            if c != ' ' && c != '\t' {
                break;
            }
            vcol += char_width(c, vcol, ts);
        }
        vcol
    }

    /// Replaces the indent of `l` with `size` columns of tabs and spaces
    /// ('noexpandtab').
    pub(crate) fn set_indent(&mut self, l: usize, size: usize) {
        let line = self.line_vec(l);
        let rest = line
            .iter()
            .position(|&c| c != ' ' && c != '\t')
            .unwrap_or(line.len());
        let (tabs, spaces) = if self.opts.expandtab() {
            (0, size)
        } else {
            let ts = self.opts.tabstop();
            (size / ts, size % ts)
        };
        let mut new: Vec<char> = std::iter::repeat_n('\t', tabs).collect();
        new.extend(std::iter::repeat_n(' ', spaces));
        new.extend(&line[rest..]);
        if new != line {
            self.set_line_vec(l, &new);
        }
    }

    /// Vim's `shift_line()`: moves line `l` by `amount` shiftwidths. With
    /// 'shiftround' the indent is first rounded to a multiple of
    /// 'shiftwidth' (a left shift of a partial step counts as one step).
    fn shift_line(&mut self, l: usize, left: bool, amount: usize) {
        let sw = self.opts.shiftwidth();
        let count = Self::get_indent(&self.line_vec(l), self.opts.tabstop());
        let new = if self.opts.shiftround() {
            let mut i = (count / sw) as i64;
            let mut amount = amount as i64;
            if !count.is_multiple_of(sw) && left {
                amount -= 1;
            }
            if left {
                i = (i - amount).max(0);
            } else {
                i += amount;
            }
            i as usize * sw
        } else if left {
            count.saturating_sub(sw * amount)
        } else {
            count + sw * amount
        };
        self.set_indent(l, new);
    }

    /// Vim's `op_shift()`.
    pub(crate) fn op_shift(&mut self, oa: &OpArg, curs_top: bool, amount: usize) {
        self.u_save(oa.start.line, oa.line_count, true);
        let block_col = self.win().cursor.col;
        let left = oa.op == Op::LShift;
        for l in oa.start.line..=oa.end.line {
            if self.doc().buf.line_len(l) == 0 {
                continue;
            }
            if oa.block {
                self.shift_block(oa, l, amount);
            } else {
                self.shift_line(l, left, amount);
            }
        }
        // Vim calls changed_lines() even if no line changed.
        self.doc_mut().buf.mark_changed();
        if oa.block {
            self.win_mut().cursor = Pos::new(oa.start.line, block_col);
        } else if curs_top {
            self.win_mut().cursor.line = oa.start.line;
            self.win_mut().cursor.col =
                first_non_blank(&self.doc().buf, self.win().cursor.line, true);
        }
        if oa.line_count > 2 {
            let op = if left { "<" } else { ">" };
            let times = if amount == 1 { "time" } else { "times" };
            self.info(format!("{} lines {op}ed {amount} {times}", oa.line_count));
        }
        self.op_start = oa.start;
        self.op_end = Pos::new(
            oa.end.line,
            self.doc().buf.line_len(oa.end.line).saturating_sub(1),
        );
    }

    /// Vim's `shift_block()` for one line.
    fn shift_block(&mut self, oa: &OpArg, l: usize, amount: usize) {
        let bd = self.block_prep(oa, l, true);
        if bd.is_short {
            return;
        }
        let line = self.line_vec(l);
        let total0 = amount * self.opts.shiftwidth();
        let mut textstart = bd.textcol;
        if oa.op == Op::RShift {
            let mut total = total0 + bd.pre_whitesp;
            let ws_vcol = bd.start_vcol - bd.pre_whitesp;
            if bd.startspaces != 0 {
                textstart += 1;
            }
            let mut vcol = bd.start_vcol;
            while textstart < line.len() && (line[textstart] == ' ' || line[textstart] == '\t') {
                let incr = char_width(line[textstart], vcol, self.opts.tabstop());
                total += incr;
                vcol += incr;
                textstart += 1;
            }
            let ts = self.opts.tabstop();
            let tabs = if self.opts.expandtab() {
                0
            } else {
                ((ws_vcol % ts) + total) / ts
            };
            let spaces = if tabs > 0 {
                ((ws_vcol % ts) + total) % ts
            } else {
                total
            };
            let textcol = bd.textcol - (bd.pre_whitesp_c - usize::from(bd.startspaces != 0));
            let mut new: Vec<char> = line[..textcol].to_vec();
            new.extend(std::iter::repeat_n('\t', tabs));
            new.extend(std::iter::repeat_n(' ', spaces));
            new.extend(&line[textstart..]);
            self.set_line_vec(l, &new);
        } else {
            let mut non_white = textstart;
            if bd.startspaces != 0 {
                non_white += 1;
            }
            let mut non_white_col = bd.start_vcol;
            while non_white < line.len() && (line[non_white] == ' ' || line[non_white] == '\t') {
                non_white_col += char_width(line[non_white], non_white_col, self.opts.tabstop());
                non_white += 1;
            }
            let block_space_width = non_white_col - oa.start_vcol;
            let shift = block_space_width.min(total0);
            let destination = non_white_col - shift;
            let mut vw = bd.start_vcol;
            if bd.startspaces != 0 {
                vw -= bd.start_char_vcols;
            }
            let mut end = textstart;
            while vw < destination && end < line.len() {
                let incr = char_width(line[end], vw, self.opts.tabstop());
                if vw + incr > destination {
                    break;
                }
                vw += incr;
                end += 1;
            }
            let fill = destination - vw;
            let mut new: Vec<char> = line[..end].to_vec();
            new.extend(std::iter::repeat_n(' ', fill));
            new.extend(&line[non_white..]);
            self.set_line_vec(l, &new);
        }
    }

    // ---- case changes and replace ----------------------------------------

    /// Vim's `swapchar()` for one position. True if it changed.
    pub(crate) fn swapchar(&mut self, op: Op, p: Pos) -> bool {
        let Some(c) = gchar(&self.doc().buf, p) else {
            return false;
        };
        let single = |mut it: std::iter::Peekable<std::vec::IntoIter<char>>| -> Option<char> {
            let c = it.next()?;
            if it.peek().is_some() { None } else { Some(c) }
        };
        let upper = || single(c.to_uppercase().collect::<Vec<_>>().into_iter().peekable());
        let lower = || single(c.to_lowercase().collect::<Vec<_>>().into_iter().peekable());
        let nc = if op == Op::Rot13 {
            match c {
                'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
                'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
                _ => c,
            }
        } else if c.is_lowercase() {
            if op == Op::Lower {
                c
            } else {
                upper().unwrap_or(c)
            }
        } else if c.is_uppercase() {
            if op == Op::Upper {
                c
            } else {
                lower().unwrap_or(c)
            }
        } else {
            c
        };
        if nc == c {
            return false;
        }
        let mut line = self.line_vec(p.line);
        line[p.col] = nc;
        self.set_line_vec(p.line, &line);
        true
    }

    /// Vim's `op_tilde()` (`~ g~ gu gU g?`).
    fn op_tilde(&mut self, oa: &mut OpArg) {
        self.u_save(oa.start.line, oa.line_count, true);
        if oa.block {
            for l in oa.start.line..=oa.end.line {
                let bd = self.block_prep(oa, l, false);
                for i in 0..bd.textlen {
                    self.swapchar(oa.op, Pos::new(l, bd.textcol + i));
                }
            }
        } else {
            if oa.linewise {
                oa.start.col = 0;
                oa.end.col = self.doc().buf.line_len(oa.end.line).saturating_sub(1);
            } else if !oa.inclusive {
                dec(&self.doc().buf, &mut oa.end);
            }
            let mut pos = oa.start;
            loop {
                if pos > oa.end {
                    break;
                }
                self.swapchar(oa.op, pos);
                if inc(&self.doc().buf, &mut pos) == -1 {
                    break;
                }
            }
        }
        self.op_start = oa.start;
        self.op_end = oa.end;
        if oa.line_count > 2 {
            self.info(format!("{} lines changed", oa.line_count));
        }
    }

    /// Vim's `op_replace()` (Visual `r`).
    fn op_replace(&mut self, oa: &mut OpArg, c: char) {
        if oa.empty {
            return;
        }
        self.u_save(oa.start.line, oa.line_count, true);
        if oa.block {
            for l in oa.start.line..=oa.end.line {
                let bd = self.block_prep(oa, l, true);
                if bd.textlen == 0 && !oa.is_max {
                    continue;
                }
                let mut numc = oa.end_vcol - oa.start_vcol + 1;
                if bd.is_short && oa.is_max {
                    numc = numc.saturating_sub(oa.end_vcol - bd.end_vcol + 1);
                }
                if bd.is_short && !oa.is_max {
                    numc = numc.saturating_sub(oa.end_vcol - bd.end_vcol + 1);
                }
                let line = self.line_vec(l);
                let mut new: Vec<char> = line[..bd.textcol].to_vec();
                new.extend(std::iter::repeat_n(' ', bd.startspaces));
                new.extend(std::iter::repeat_n(c, numc));
                if !bd.is_short {
                    new.extend(std::iter::repeat_n(' ', bd.endspaces));
                    new.extend(&line[(bd.textcol + bd.textlen).min(line.len())..]);
                }
                self.set_line_vec(l, &new);
            }
        } else {
            if oa.linewise {
                oa.start.col = 0;
                oa.end.col = self.doc().buf.line_len(oa.end.line).saturating_sub(1);
            } else if !oa.inclusive {
                dec(&self.doc().buf, &mut oa.end);
            }
            let mut pos = oa.start;
            while pos <= oa.end {
                if gchar(&self.doc().buf, pos).is_some() {
                    let mut line = self.line_vec(pos.line);
                    line[pos.col] = c;
                    self.set_line_vec(pos.line, &line);
                }
                if inc(&self.doc().buf, &mut pos) == -1 {
                    break;
                }
            }
        }
        self.win_mut().cursor = oa.start;
        self.check_cursor_col();
        self.op_start = oa.start;
        self.op_end = oa.end;
    }

    // ---- block insert / append ------------------------------------------

    /// Vim's `op_insert()` (Visual `I` and `A`).
    fn op_insert(&mut self, oa: &mut OpArg) {
        self.win_mut().cursor.line = oa.start.line;
        let mut pre_textlen: i64 = 0;
        let mut bd = BlockDef::default();
        if oa.block {
            self.u_save_cursor();
            bd = self.block_prep(oa, oa.start.line, true);
            pre_textlen = self.doc().buf.line_len(oa.start.line) as i64 - bd.textcol as i64;
            if oa.op == Op::Append {
                pre_textlen -= bd.textlen as i64;
            }
        }
        if oa.op == Op::Append {
            if oa.block {
                let len = self.doc().buf.line_len(self.win().cursor.line);
                self.win_mut().cursor.col = (bd.textcol + bd.textlen).min(len);
                if bd.is_short && !oa.is_max {
                    self.u_save_cursor();
                    let at = self.win().cursor.col;
                    let cur_line = self.win().cursor.line;
                    self.doc_mut()
                        .buf
                        .insert(cur_line, at, &" ".repeat(bd.endspaces));
                    self.win_mut().cursor.col += bd.endspaces;
                    bd.textlen += bd.endspaces;
                }
            } else {
                self.win_mut().cursor = oa.end;
                self.check_cursor_col();
                if self.doc().buf.line_len(self.win().cursor.line) > 0
                    && oa.start_vcol != oa.end_vcol
                {
                    let (doc, win) = self.parts_mut();
                    inc(&doc.buf, &mut win.cursor);
                }
            }
        }
        let post = if oa.block {
            PostInsert::BlockInsert {
                oa: *oa,
                pre_textlen,
                bd_textcol: bd.textcol,
                bd_textlen: bd.textlen,
            }
        } else {
            PostInsert::None
        };
        self.start_insert(1, InsertKind::Inline, false);
        self.ins_post = post;
    }

    /// The end of `op_insert()`: copies what was typed to the other lines.
    pub(crate) fn finish_block_insert(
        &mut self,
        mut oa: OpArg,
        mut pre_textlen: i64,
        bd_textcol: usize,
        bd_textlen: usize,
    ) {
        if self.win().cursor.line != oa.start.line {
            return;
        }
        let mut textcol = bd_textcol;
        let mut textlen = bd_textlen;
        let mut bd2 = self.block_prep(&oa, oa.start.line, true);
        if !oa.is_max || bd2.textlen < textlen {
            if oa.op == Op::Append {
                pre_textlen += bd2.textlen as i64 - textlen as i64;
                if bd2.endspaces != 0 {
                    bd2.textlen = bd2.textlen.saturating_sub(1);
                }
            }
            textcol = bd2.textcol;
            textlen = bd2.textlen;
        }
        let first = self.line_vec(oa.start.line);
        let mut add = textcol;
        if oa.op == Op::Append {
            add += textlen;
        }
        let add = add.min(first.len());
        let rest = &first[add..];
        let ins_len = rest.len() as i64 - pre_textlen;
        if pre_textlen >= 0 && ins_len > 0 {
            let text: Vec<char> = rest[..ins_len as usize].to_vec();
            self.u_save(oa.start.line, oa.line_count, true);
            self.block_insert(&mut oa, &text);
            self.win_mut().cursor.col = oa.start.col;
            self.check_cursor_col();
        }
    }

    /// Vim's `block_insert()`: inserts `s` into lines 2.. of the block.
    fn block_insert(&mut self, oa: &mut OpArg, s: &[char]) {
        let b_insert = oa.op == Op::Insert;
        for l in oa.start.line + 1..=oa.end.line {
            let bd = self.block_prep(oa, l, true);
            if bd.is_short && b_insert {
                continue;
            }
            let old = self.line_vec(l);
            let ts_val;
            let spaces;
            let offset;
            if b_insert {
                ts_val = bd.start_char_vcols;
                spaces = bd.startspaces;
                offset = bd.textcol;
            } else {
                ts_val = bd.end_char_vcols;
                if !bd.is_short {
                    spaces = if bd.endspaces != 0 {
                        ts_val - bd.endspaces
                    } else {
                        0
                    };
                    offset = bd.textcol + bd.textlen - usize::from(spaces != 0);
                } else {
                    spaces = if oa.is_max {
                        0
                    } else {
                        oa.end_vcol - bd.end_vcol + 1
                    };
                    offset = bd.textcol + bd.textlen;
                }
            }
            let offset = offset.min(old.len());
            let mut new: Vec<char> = old[..offset].to_vec();
            let mut rest = offset;
            new.extend(std::iter::repeat_n(' ', spaces));
            new.extend_from_slice(s);
            if spaces > 0 && !bd.is_short && old.get(rest) == Some(&'\t') {
                // A tab cut by the block edge becomes spaces around the text.
                new.extend(std::iter::repeat_n(' ', ts_val - spaces));
                rest += 1;
            }
            new.extend(&old[rest..]);
            self.set_line_vec(l, &new);
        }
    }

    // ---- join -----------------------------------------------------------

    /// Vim's `do_join()`: joins `count` lines at the cursor.
    pub(crate) fn do_join(&mut self, count: usize, insert_space: bool, save_undo: bool) -> bool {
        let first = self.win().cursor.line;
        if save_undo {
            self.u_save(first, count, true);
        }
        let mut spaces = vec![0usize; count];
        let mut parts: Vec<Vec<char>> = Vec::with_capacity(count);
        let mut endcurr1: Option<char> = None;
        let mut endcurr2: Option<char> = None;
        let mut sumsize = 0usize;
        let mut currsize = 0usize;
        // Where the spaces before each joined line start in the result, and
        // how much leading white space was removed from it (for the marks on
        // those lines).
        let mut space_starts = vec![0usize; count];
        let mut removed = vec![0usize; count];
        for (t, sp) in spaces.iter_mut().enumerate() {
            let mut curr = self.line_vec(first + t);
            if t == 0 {
                self.op_start = Pos::new(first, curr.len());
            }
            if insert_space && t > 0 {
                let skip = curr.iter().take_while(|&&c| c == ' ' || c == '\t').count();
                removed[t] = skip;
                curr.drain(..skip);
                if !curr.is_empty() && curr[0] != ')' && sumsize != 0 && endcurr1 != Some('\t') {
                    if endcurr1 == Some(' ') {
                        endcurr1 = endcurr2;
                    } else {
                        *sp += 1;
                    }
                    // 'joinspaces' is on.
                    if matches!(endcurr1, Some('.' | '?' | '!')) {
                        *sp += 1;
                    }
                }
            }
            currsize = curr.len();
            space_starts[t] = sumsize;
            sumsize += currsize + *sp;
            endcurr1 = None;
            endcurr2 = None;
            if insert_space && currsize > 0 {
                endcurr1 = curr.last().copied();
                endcurr2 = if currsize > 1 {
                    Some(curr[currsize - 2])
                } else {
                    None
                };
            }
            parts.push(curr);
        }
        let col = sumsize - currsize - spaces[count - 1];
        let mut new = Vec::with_capacity(sumsize);
        for (t, p) in parts.iter().enumerate() {
            new.extend(std::iter::repeat_n(' ', spaces[t]));
            new.extend(p);
        }
        self.set_line_vec(first, &new);
        self.op_end = Pos::new(first, sumsize);
        // Marks on the joined lines move to the joined line, with the column
        // mapping of Vim's do_join(): `amount` shifts columns after the
        // inserted spaces, and columns inside the removed white space go to
        // the start of the spaces.
        for t in 1..count {
            let s = space_starts[t] as isize;
            let r = removed[t] as isize - spaces[t] as isize;
            let amount = s - r;
            self.doc_mut()
                .buf
                .move_marks_to_line(first + t, first, move |k| {
                    let k = k as isize;
                    if amount < 0 && k <= -amount {
                        0
                    } else if k < r {
                        (amount + r) as usize
                    } else {
                        (k + amount) as usize
                    }
                });
        }
        self.doc_mut().buf.delete_lines(first + 1, count - 1);
        self.win_mut().cursor = Pos::new(first, col);
        self.check_cursor_col();
        self.win_mut().keep_curswant = false;
        true
    }

    // ---- put --------------------------------------------------------------

    /// Vim's `do_put()`.
    pub(crate) fn do_put(
        &mut self,
        reg: Option<char>,
        before: bool,
        count: usize,
        flags: PutFlags,
    ) {
        self.do_put_inner(reg, before, count, flags, None);
    }

    /// `visual_block_lines`: when putting a characterwise register over a
    /// Visual block, the last line of the block (Vim puts on every line).
    fn do_put_inner(
        &mut self,
        reg: Option<char>,
        before: bool,
        count: usize,
        flags: PutFlags,
        visual_block_end: Option<usize>,
    ) {
        self.op_start = self.win().cursor;
        self.op_end = self.win().cursor;
        self.regs.select(reg, false);
        let Some(r) = self.regs.current().cloned().filter(|r| !r.lines.is_empty()) else {
            let name = reg.unwrap_or('"');
            self.error(format!("E353: Nothing in register {name}"));
            return;
        };
        let mut fwd = !before;
        let mut ytype = r.kind;
        let y: Vec<Vec<char>> = r.lines.iter().map(|s| chars_of(s)).collect();
        let y_size = y.len();
        let mut nr_lines: i64 = 0;

        if ytype == RegType::Line {
            if flags.line_split {
                self.u_save_cursor();
                let line = self.line_vec(self.win().cursor.line);
                let mut p = self.win().cursor.col.min(line.len());
                if fwd && p < line.len() {
                    p += 1;
                }
                let next_line = self.win().cursor.line + 1;
                self.doc_mut()
                    .buf
                    .insert_line(next_line, &line[p..].iter().collect::<String>());
                self.set_line_vec(self.win().cursor.line, &line[..p]);
                nr_lines += 1;
                fwd = true;
            }
            if flags.line_forward
                && let Some(lv) = self.doc().last_visual
            {
                self.win_mut().cursor = lv.end;
                fwd = true;
            }
            self.op_start = self.win().cursor;
            self.op_end = self.win().cursor;
        }
        if flags.line {
            ytype = RegType::Line;
        }

        match ytype {
            RegType::Block(_) => {
                let n = (y_size + 1).min(self.doc().buf.line_count() - self.win().cursor.line);
                self.u_save(self.win().cursor.line, n, false);
            }
            RegType::Line => {
                let at = self.win().cursor.line + usize::from(fwd);
                self.u_save(at, 0, false);
            }
            RegType::Char => self.u_save_cursor(),
        }

        let lnum = self.win().cursor.line;
        let mut col = self.win().cursor.col;

        if let RegType::Block(y_width) = ytype {
            let c = gchar(&self.doc().buf, self.win().cursor);
            let mut colv;
            if fwd && c.is_some() {
                colv = vcol_range(
                    self.doc().buf.line(lnum),
                    self.win().cursor.col,
                    self.opts.tabstop(),
                )
                .1;
                self.win_mut().cursor.col += 1;
                colv += 1;
            } else {
                colv = vcol_range(
                    self.doc().buf.line(lnum),
                    self.win().cursor.col,
                    self.opts.tabstop(),
                )
                .0;
            }
            let mut textcol = 0;
            let mut totlen = 0;
            for (i, yl) in y.iter().enumerate() {
                if self.win().cursor.line > self.doc().buf.last_line() {
                    let n = self.doc().buf.line_count();
                    self.doc_mut().buf.insert_line(n, "");
                    nr_lines += 1;
                }
                let old = self.line_vec(self.win().cursor.line);
                let mut vcol = 0;
                let mut p = 0;
                let mut incr = 0;
                while vcol < colv && p < old.len() {
                    incr = char_width(old[p], vcol, self.opts.tabstop());
                    vcol += incr;
                    p += 1;
                }
                textcol = p;
                let shortline = vcol < colv || (vcol == colv && p >= old.len());
                let mut startspaces = 0;
                let mut endspaces = 0;
                let mut delcount = 0;
                if vcol < colv {
                    startspaces = colv - vcol;
                } else if vcol > colv {
                    endspaces = vcol - colv;
                    startspaces = incr - endspaces;
                    textcol -= 1;
                    delcount = 1;
                    if old[textcol] != '\t' {
                        delcount = 0;
                        endspaces = 0;
                    }
                }
                let mut spaces = (y_width + 1) as i64;
                for &ch in yl {
                    spaces -= char_width(ch, 0, self.opts.tabstop()) as i64;
                }
                let spaces = spaces.max(0) as usize;
                totlen = count * (yl.len() + spaces) + startspaces + endspaces;
                let mut new: Vec<char> = old[..textcol].to_vec();
                new.extend(std::iter::repeat_n(' ', startspaces));
                for j in 0..count {
                    new.extend(yl);
                    if (j + 1 < count || !shortline) && spaces > 0 {
                        new.extend(std::iter::repeat_n(' ', spaces));
                    } else {
                        totlen -= spaces;
                    }
                }
                new.extend(std::iter::repeat_n(' ', endspaces));
                new.extend(&old[(textcol + delcount).min(old.len())..]);
                self.set_line_vec(self.win().cursor.line, &new);
                self.win_mut().cursor.line += 1;
                if i == 0 {
                    self.win_mut().cursor.col += startspaces;
                }
            }
            self.op_start = Pos::new(lnum, self.win().cursor.col);
            self.op_end = Pos::new(
                self.win().cursor.line - 1,
                (textcol + totlen).saturating_sub(1),
            );
            if flags.cursend {
                self.win_mut().cursor = self.op_end;
                self.win_mut().cursor.col += 1;
                let len = self.doc().buf.line_len(self.win().cursor.line);
                self.win_mut().cursor.col = self.win_mut().cursor.col.min(len);
            } else {
                self.win_mut().cursor.line = lnum;
            }
        } else {
            let yanklen = y[0].len();
            let mut after: i64 = lnum as i64;
            if ytype == RegType::Char {
                if fwd && gchar(&self.doc().buf, self.win().cursor).is_some() {
                    col += 1;
                    if yanklen > 0 {
                        self.win_mut().cursor.col += 1;
                        self.op_end.col += 1;
                    }
                }
                self.op_start = self.win().cursor;
            } else if !fwd {
                after -= 1;
            }
            let new_cursor = self.win().cursor;
            if ytype == RegType::Char && y_size == 1 {
                let start_lnum = lnum;
                let end_lnum = visual_block_end.unwrap_or(lnum).max(lnum);
                let vcol = vcol_range(self.doc().buf.line(lnum), col, self.opts.tabstop()).0;
                let mut l = lnum;
                let mut totlen = 0;
                if count > 0 && yanklen > 0 {
                    totlen = count * yanklen;
                    loop {
                        let old = self.line_vec(l);
                        let mut c = col;
                        if l > start_lnum {
                            c = text_col_at_vcol(&old, vcol, self.opts.tabstop())
                                .unwrap_or(usize::MAX);
                        }
                        if visual_block_end.is_some() && c > old.len() {
                            l += 1;
                            if l > end_lnum {
                                break;
                            }
                            continue;
                        }
                        let c = c.min(old.len());
                        let mut new: Vec<char> = old[..c].to_vec();
                        for _ in 0..count {
                            new.extend(&y[0]);
                        }
                        new.extend(&old[c..]);
                        self.set_line_vec(l, &new);
                        if l == self.win().cursor.line {
                            self.win_mut().cursor.col += totlen - 1;
                        }
                        l += 1;
                        if visual_block_end.is_none() || l > end_lnum {
                            break;
                        }
                    }
                }
                self.op_end = self.win().cursor;
                if totlen > 0 && flags.cursend {
                    self.win_mut().cursor.col += 1;
                }
            } else {
                let mut new_lnum = new_cursor.line as i64;
                let mut lnum_i = after;
                for cnt in 1..=count {
                    let mut i = 0;
                    if ytype == RegType::Char {
                        lnum_i = new_cursor.line as i64;
                        let l = lnum_i as usize;
                        let old = self.line_vec(l);
                        let c = col.min(old.len());
                        let mut last: Vec<char> = y[y_size - 1].clone();
                        last.extend(&old[c..]);
                        self.doc_mut()
                            .buf
                            .insert_line(l + 1, &last.iter().collect::<String>());
                        new_lnum += 1;
                        let mut first: Vec<char> = old[..c].to_vec();
                        first.extend(&y[0]);
                        self.set_line_vec(l, &first);
                        self.win_mut().cursor.line = l;
                        i = 1;
                    }
                    while i < y_size {
                        if ytype != RegType::Char || i < y_size - 1 {
                            let at = (lnum_i + 1) as usize;
                            self.doc_mut()
                                .buf
                                .insert_line(at, &y[i].iter().collect::<String>());
                            new_lnum += 1;
                        }
                        lnum_i += 1;
                        nr_lines += 1;
                        i += 1;
                    }
                    if cnt == 1 {
                        new_lnum = lnum_i;
                    }
                }
                if ytype == RegType::Line {
                    self.op_start.col = 0;
                    if fwd {
                        self.op_start.line += 1;
                    }
                }
                let end_l = new_lnum.max(0) as usize;
                let lastlen = y[y_size - 1].len();
                self.op_end = Pos::new(end_l, lastlen.saturating_sub(1));
                if flags.cursend {
                    if ytype == RegType::Line {
                        let l = lnum_i.max(0) as usize;
                        self.win_mut().cursor = Pos::new(
                            if l >= self.doc().buf.last_line() {
                                self.doc().buf.last_line()
                            } else {
                                l + 1
                            },
                            0,
                        );
                    } else {
                        self.win_mut().cursor = Pos::new(end_l, lastlen);
                        self.op_end = self.win().cursor;
                        if lastlen > 1 {
                            self.op_end.col = lastlen - 1;
                        }
                    }
                } else if ytype == RegType::Line {
                    self.win_mut().cursor.col = 0;
                    if fwd {
                        self.win_mut().cursor.line += 1;
                    }
                    self.win_mut().cursor.col =
                        first_non_blank(&self.doc().buf, self.win().cursor.line, true);
                } else {
                    self.win_mut().cursor = new_cursor;
                }
            }
        }
        self.msgmore(nr_lines);
        self.win_mut().keep_curswant = false;
        let len = self.doc().buf.line_len(self.win().cursor.line);
        if self.win().cursor.col > len {
            self.win_mut().cursor.col = len;
        }
    }

    /// Visual `p` / `P` (Vim's `nv_put_opt()` for Visual mode).
    pub(crate) fn visual_put(&mut self, reg: Option<char>, before: bool, count: usize) {
        let Some(vmode) = self.visual_mode() else {
            return;
        };
        let regname = reg;
        let keep_registers = before;
        let saved = if matches!(regname, None | Some('"' | '0'..='9' | '-')) {
            self.regs.select(regname, false);
            Some(self.regs.current().cloned())
        } else {
            None
        };
        let vstart = self.win().vstart;
        let vend = self.win().cursor;
        self.redo_building = Some(crate::editor::Redo {
            reg,
            count: None,
            keys: vec![crate::key::Key::Char(if before { 'P' } else { 'p' })],
            visual: None,
        });
        self.visual_operator(
            Op::Delete,
            'd',
            if keep_registers { Some('_') } else { None },
            None,
        );
        let empty = self.doc().buf.has_no_lines();
        let mut reg2 = None;
        if let Some(r1) = saved {
            self.regs.select(regname, false);
            reg2 = Some(self.regs.current().cloned());
            self.regs.set(regname, r1);
        }
        let mut flags = PutFlags::default();
        match vmode {
            VisualMode::Line => flags.line = true,
            VisualMode::Char => flags.line_split = true,
            VisualMode::Block => {
                if !before {
                    flags.line_forward = true;
                }
            }
        }
        let mut dir_before = true;
        if (vmode != VisualMode::Line && self.win().cursor.col < self.op_start.col)
            || (vmode == VisualMode::Line && self.win().cursor.line < self.op_start.line)
        {
            dir_before = false;
        }
        let block_end = if vmode == VisualMode::Block {
            Some(vstart.line.max(vend.line))
        } else {
            None
        };
        self.do_put_inner(regname, dir_before, count, flags, block_end);
        if let Some(r2) = reg2 {
            self.regs.set(regname, r2);
        }
        self.doc_mut().last_visual = Some(LastVisual {
            start: self.op_start,
            end: self.op_end,
            mode: vmode,
            curswant: self.win().curswant,
        });
        if empty
            && self.doc().buf.line_len(self.doc().buf.last_line()) == 0
            && self.doc().buf.line_count() > 1
        {
            let last = self.doc().buf.last_line();
            self.doc_mut().buf.delete_lines(last, 1);
            if self.win().cursor.line > self.doc().buf.last_line() {
                self.win_mut().cursor.line = self.doc_mut().buf.last_line();
                self.coladvance(usize::MAX);
            }
        }
        // Visual put is repeated as a plain put of the same size... Vim does
        // not support "." for it beyond the delete; keep the delete redo.
        self.redo_building = None;
    }

    /// Insert-mode text at the start of Visual block I/A, for unused
    /// variable warnings.
    #[allow(dead_code)]
    fn unused(&self) {
        let _ = motion::incl;
    }
}

/// Char index whose cells cover virtual column `vcol` (Vim's `getvpos`),
/// or `None` if the line is shorter.
fn text_col_at_vcol(line: &[char], vcol: usize, ts: usize) -> Option<usize> {
    let mut v = 0;
    for (i, &c) in line.iter().enumerate() {
        let w = char_width(c, v, ts);
        if v + w > vcol {
            return Some(i);
        }
        v += w;
    }
    if v == vcol { Some(line.len()) } else { None }
}
