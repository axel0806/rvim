//! The window's top line and the scroll commands: `CTRL-E CTRL-Y CTRL-F
//! CTRL-B CTRL-D CTRL-U`, `zt zz zb z<CR> z. z-`, and `H M L`, with
//! 'scrolloff' and wrapped lines. The top line (Vim's `w_topline`) is a field
//! of `Editor`, so the commands and the cursor clamp see the same window the
//! frame is drawn from. Heights are screen rows: a wrapped line takes
//! `view::line_height` rows.
//!
//! Rules follow `scroll.txt` and the probes described in PLAN.md (Known
//! differences). Vim's own C code was not available to port line by line, so
//! the exact centring of an even-height window is not verified.

use crate::editor::Editor;
use crate::view;

/// Scroll commands of Normal and Visual mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollCmd {
    /// CTRL-F.
    PageDown,
    /// CTRL-B.
    PageUp,
    /// CTRL-D.
    HalfDown,
    /// CTRL-U.
    HalfUp,
    /// CTRL-E.
    LineDown,
    /// CTRL-Y.
    LineUp,
    /// `zt` (cursor column kept) and `z<CR>` (cursor to first non-blank).
    Top { first_non_blank: bool },
    /// `zz` and `z.`.
    Middle { first_non_blank: bool },
    /// `zb` and `z-`.
    Bottom { first_non_blank: bool },
}

impl Editor {
    /// Screen rows of buffer line `l` (wrapped).
    fn line_rows(&self, l: usize) -> usize {
        view::line_height(self.buf.line(l), self.screen_cols)
    }

    /// Rows of the text area (the screen minus the command line).
    pub(crate) fn text_rows(&self) -> usize {
        self.screen_rows.saturating_sub(1).max(1)
    }

    /// 'scrolloff' in rows, capped at half the window like Vim.
    fn so_rows(&self) -> usize {
        self.opts.scrolloff().min((self.text_rows() - 1) / 2)
    }

    /// Rows of lines `from..to`, stopping early once the sum exceeds `limit`.
    fn rows_upto(&self, from: usize, to: usize, limit: usize) -> usize {
        let mut sum = 0;
        for l in from..to.min(self.buf.line_count()) {
            sum += self.line_rows(l);
            if sum > limit {
                break;
            }
        }
        sum
    }

    /// Rows of at least `so` lines below `cur` (fewer at the end of the file).
    fn rows_below(&self, cur: usize, so: usize) -> usize {
        let mut sum = 0;
        let mut l = cur + 1;
        while l < self.buf.line_count() && sum < so {
            sum += self.line_rows(l);
            l += 1;
        }
        sum
    }

    /// The first line below the last line fully shown in a window starting
    /// at `top` (Vim's `w_botline`, exclusive). A line taller than the window
    /// counts as shown.
    pub(crate) fn bottom_of(&self, top: usize) -> usize {
        let n = self.buf.line_count();
        let r = self.text_rows();
        let mut used = 0;
        let mut l = top;
        while l < n {
            let h = self.line_rows(l);
            if used + h > r {
                if l == top {
                    l += 1;
                }
                break;
            }
            used += h;
            l += 1;
        }
        l
    }

    /// Top line that puts `cur` at the top, with 'scrolloff' rows above it.
    fn top_for_cursor_top(&self, cur: usize) -> usize {
        let so = self.so_rows();
        let mut top = cur;
        let mut acc = 0;
        while top > 0 && acc < so {
            top -= 1;
            acc += self.line_rows(top);
        }
        top
    }

    /// Top line that puts `cur` at the bottom, with `so` rows below it.
    fn top_for_cursor_bottom(&self, cur: usize, so: usize) -> usize {
        let n = self.buf.line_count();
        let r = self.text_rows();
        let mut used = self.line_rows(cur);
        let mut below = 0;
        let mut end = cur + 1;
        while end < n && below < so {
            let h = self.line_rows(end);
            below += h;
            used += h;
            end += 1;
        }
        if used > r {
            return cur;
        }
        let mut top = cur;
        while top > 0 {
            let h = self.line_rows(top - 1);
            if used + h > r {
                break;
            }
            top -= 1;
            used += h;
        }
        top
    }

    /// Top line that centres `cur`, adding lines below and above alternately
    /// until the window is full (near the end of the file, lines above fill
    /// the rest).
    fn top_for_cursor_middle(&self, cur: usize) -> usize {
        let n = self.buf.line_count();
        let r = self.text_rows();
        let mut top = cur;
        let mut bot = cur + 1;
        let mut used = self.line_rows(cur);
        loop {
            let mut moved = false;
            if bot < n && used + self.line_rows(bot) <= r {
                used += self.line_rows(bot);
                bot += 1;
                moved = true;
            }
            if top > 0 && used + self.line_rows(top - 1) <= r {
                top -= 1;
                used += self.line_rows(top);
                moved = true;
            }
            if !moved {
                break;
            }
        }
        top
    }

    /// First line the cursor may use in the current window (after
    /// 'scrolloff' rows above it).
    pub(crate) fn first_visible(&self) -> usize {
        if self.top == 0 {
            return 0;
        }
        let so = self.so_rows();
        let last = self.buf.last_line();
        let mut l = self.top;
        let mut acc = 0;
        while acc < so && l < last {
            acc += self.line_rows(l);
            l += 1;
        }
        l
    }

    /// Last line the cursor may use in the current window (after
    /// 'scrolloff' rows below it).
    pub(crate) fn last_visible(&self) -> usize {
        let bot = self.bottom_of(self.top);
        if bot >= self.buf.line_count() {
            return self.buf.last_line();
        }
        let so = self.so_rows();
        let mut l = bot - 1;
        let mut acc = 0;
        while acc < so && l > self.top {
            acc += self.line_rows(l);
            l -= 1;
        }
        l
    }

    /// Moves the cursor line into the window's usable lines.
    fn clamp_cursor_to_window(&mut self) {
        let first = self.first_visible();
        let last = self.last_visible().max(first);
        self.cursor.line = self.cursor.line.clamp(first, last);
    }

    /// Vim's `update_topline()`: keeps the cursor in the window with
    /// 'scrolloff'. A cursor far outside the window is centred, a close one
    /// scrolls the minimum. Called after every key and on resize.
    pub(crate) fn update_topline(&mut self) {
        let n = self.buf.line_count();
        let r = self.text_rows();
        let so = self.so_rows();
        let cur = self.cursor.line.min(n - 1);
        self.top = self.top.min(n - 1);
        if self.top > 0 && (cur < self.top || self.rows_upto(self.top, cur, so) < so) {
            let halfheight = (r / 2).saturating_sub(1).max(2);
            self.top = if cur < self.top && self.top - cur >= halfheight {
                self.top_for_cursor_middle(cur)
            } else {
                self.top_for_cursor_top(cur)
            };
        }
        let need = self.rows_upto(self.top, cur + 1, r) + self.rows_below(cur, so);
        if need > r {
            let bot = self.bottom_of(self.top);
            self.top = if self.rows_upto(bot, cur + 1, r) <= r {
                self.top_for_cursor_bottom(cur, so)
            } else {
                self.top_for_cursor_middle(cur)
            };
        }
    }

    /// The first buffer line shown in the window (0-based).
    pub fn top_line(&self) -> usize {
        self.top
    }

    /// Sets the window size in cells (the command line is the last row) and
    /// keeps the cursor visible.
    pub fn set_screen_size(&mut self, cols: usize, rows: usize) {
        self.screen_cols = cols.max(1);
        self.screen_rows = rows.max(1);
        self.update_topline();
    }

    /// The first line of the window that Vim's `CTRL-F` leaves at the top:
    /// the last two rows of the old page (at least one line of progress).
    fn page_down_top(&self, bot: usize) -> usize {
        let mut t = bot;
        let mut used = 0;
        while t > self.top + 1 {
            let h = self.line_rows(t - 1);
            if used + h > 2 {
                break;
            }
            t -= 1;
            used += h;
        }
        t
    }

    /// The top line for `CTRL-B`: the first two lines of the old window
    /// become the last two lines of the new one.
    fn page_up_top(&self) -> usize {
        let old = self.top;
        let r = self.text_rows();
        let mut t = (old + 2).min(self.buf.line_count());
        let mut used = 0;
        while t > 0 {
            let h = self.line_rows(t - 1);
            if used + h > r {
                break;
            }
            t -= 1;
            used += h;
        }
        t.min(old - 1)
    }

    /// Runs a scroll command with its count (Vim's `nv_scroll_line`,
    /// `nv_halfpage`, `nv_zet`, `pagescroll`).
    pub(crate) fn scroll_cmd(&mut self, cmd: ScrollCmd, count: Option<usize>) {
        let n = self.buf.line_count();
        let count1 = count.unwrap_or(1).max(1);
        match cmd {
            ScrollCmd::LineDown => {
                let top = (self.top + count1).min(n - 1);
                if top == self.top {
                    return;
                }
                self.top = top;
                self.clamp_cursor_to_window();
                self.keep_curswant = true;
                self.coladvance(self.curswant);
            }
            ScrollCmd::LineUp => {
                let top = self.top.saturating_sub(count1);
                if top == self.top {
                    return;
                }
                self.top = top;
                self.clamp_cursor_to_window();
                self.keep_curswant = true;
                self.coladvance(self.curswant);
            }
            ScrollCmd::PageDown => {
                let mut moved = false;
                for _ in 0..count1 {
                    let bot = self.bottom_of(self.top);
                    if bot >= n {
                        // The end is shown: the last line goes to the top.
                        if self.top + 1 >= n {
                            break;
                        }
                        self.top = n - 1;
                    } else {
                        self.top = self.page_down_top(bot);
                    }
                    moved = true;
                }
                if !moved {
                    self.beep();
                    return;
                }
                self.cursor.line = self.first_visible();
                self.cursor_fnb();
            }
            ScrollCmd::PageUp => {
                let mut moved = false;
                for _ in 0..count1 {
                    if self.top == 0 {
                        break;
                    }
                    self.top = self.page_up_top();
                    moved = true;
                }
                if !moved {
                    self.beep();
                    return;
                }
                self.cursor.line = self.last_visible();
                self.cursor_fnb();
            }
            ScrollCmd::HalfDown | ScrollCmd::HalfUp => {
                if let Some(c) = count {
                    self.scroll_opt = c;
                }
                let r = self.text_rows();
                let amt = match self.scroll_opt {
                    0 => (r / 2).max(1),
                    v => v,
                };
                let cur = self.cursor.line;
                // Like Vim's halfpage() loop: `amt` is in screen rows, whole
                // lines are scrolled while the rows left stay non-negative,
                // and the cursor moves by the number of lines scrolled.
                let mut left = amt as isize;
                let mut lines = 0usize;
                let mut scrolled = 0isize;
                if cmd == ScrollCmd::HalfDown {
                    if cur == n - 1 {
                        self.beep();
                        return;
                    }
                    // Stop scrolling once the last line is at the bottom.
                    let max_top = self.top_for_cursor_bottom(n - 1, 0);
                    while left > 0 && self.top + lines < max_top {
                        let h = self.line_rows(self.top + lines) as isize;
                        left -= h;
                        if left < 0 && scrolled > 0 {
                            break;
                        }
                        lines += 1;
                        scrolled += h;
                    }
                    self.top += lines;
                    // At the end of the file the cursor still moves by the count.
                    let step = if lines > 0 { lines } else { amt };
                    self.cursor.line = (cur + step).min(n - 1);
                } else {
                    if cur == 0 {
                        self.beep();
                        return;
                    }
                    while left > 0 && self.top > lines {
                        let h = self.line_rows(self.top - 1 - lines) as isize;
                        left -= h;
                        if left < 0 && scrolled > 0 {
                            break;
                        }
                        lines += 1;
                        scrolled += h;
                    }
                    self.top -= lines;
                    let step = if lines > 0 { lines } else { amt };
                    self.cursor.line = cur.saturating_sub(step);
                }
                self.clamp_cursor_to_window();
                self.cursor_fnb();
            }
            ScrollCmd::Top { first_non_blank }
            | ScrollCmd::Middle { first_non_blank }
            | ScrollCmd::Bottom { first_non_blank } => {
                if let Some(c) = count {
                    self.cursor.line = c.saturating_sub(1).min(n - 1);
                }
                let cur = self.cursor.line;
                self.top = match cmd {
                    ScrollCmd::Top { .. } => self.top_for_cursor_top(cur),
                    ScrollCmd::Middle { .. } => self.top_for_cursor_middle(cur),
                    _ => self.top_for_cursor_bottom(cur, self.so_rows()),
                };
                if first_non_blank {
                    self.cursor_fnb();
                } else {
                    self.keep_curswant = true;
                }
            }
        }
    }

    /// `H`, `M` and `L`: a line of the window, `count` from the top (`H`),
    /// the middle, or `count` from the bottom (`L`). Returns the buffer line.
    pub(crate) fn screen_line(&self, which: char, count1: usize) -> usize {
        match which {
            // The count and 'scrolloff' combine with max(), as in Vim's
            // nv_scroll(): the count is counted from the window edge and the
            // margin is the limit it may not pass.
            'H' => (self.top + count1 - 1)
                .max(self.first_visible())
                .min(self.bottom_of(self.top) - 1),
            'L' => {
                let bot = self.bottom_of(self.top);
                (bot - 1)
                    .saturating_sub(count1 - 1)
                    .min(self.last_visible())
                    .max(self.top)
            }
            _ => {
                // The line that splits the shown rows in half.
                let bot = self.bottom_of(self.top);
                let total = self.rows_upto(self.top, bot, usize::MAX);
                let half = total.div_ceil(2);
                let mut acc = 0;
                let mut l = self.top;
                while l + 1 < bot {
                    acc += self.line_rows(l);
                    if acc >= half {
                        break;
                    }
                    l += 1;
                }
                l
            }
        }
    }

    /// Moves to the first non-blank of the cursor line (Vim's `beginline`).
    fn cursor_fnb(&mut self) {
        self.cursor.col = crate::motion::first_non_blank(&self.buf, self.cursor.line, true);
    }
}
