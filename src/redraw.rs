// SPDX-License-Identifier: Apache-2.0
//! Cell-level redraw. A `Frame` becomes a grid of styled cells, and `diff`
//! compares two grids cell by cell and returns the writes a terminal needs:
//! one `DrawOp` per run of changed cells that share a style. No terminal
//! here; `main.rs` turns the ops into escape sequences.

use crate::view::{Frame, Hl};
use unicode_width::UnicodeWidthChar;

/// Cells taken by the "-- INSERT --" label at the start of the status row.
const INSERT_LABEL_CELLS: usize = 12;

/// How a cell is drawn. `hl` is the highlight group; `error` and `bold` are
/// set only on the status row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub hl: Hl,
    /// The status row's error message (white on dark red).
    pub error: bool,
    /// The "-- INSERT --" label.
    pub bold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// `None` marks the right half of a wide character; the cell to its
    /// left draws the character.
    pub ch: Option<char>,
    pub style: Style,
}

/// A whole screen: `rows.len()` rows of exactly `width` cells. The last row
/// is the status row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub width: usize,
    pub rows: Vec<Vec<Cell>>,
}

/// One write for the terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrawOp {
    /// Prints `text` (wide characters included) from cell (`row`, `col`) in
    /// `style`. The cells it covers are exactly the cells it changes.
    Text {
        row: usize,
        col: usize,
        text: String,
        style: Style,
    },
    /// Blanks cells from (`row`, `col`) to the end of the row, in the
    /// default style. Used where the new row ends in blank default cells.
    ClearToEnd { row: usize, col: usize },
}

fn cell_width(c: char) -> usize {
    c.width().unwrap_or(1).max(1)
}

fn blank() -> Cell {
    Cell {
        ch: Some(' '),
        style: Style::default(),
    }
}

/// Appends `c` as one cell (or two, for a wide character) if it fits in
/// `width`; a character that does not fit is dropped.
fn push_char(cells: &mut Vec<Cell>, c: char, w: usize, style: Style, width: usize) {
    if cells.len() + w > width {
        return;
    }
    cells.push(Cell { ch: Some(c), style });
    for _ in 1..w {
        cells.push(Cell { ch: None, style });
    }
}

/// One row of text as cells, styled by `hl` (one group per cell), padded
/// with blanks to `width`.
fn row_cells(text: &str, hl: Option<&[Hl]>, width: usize) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(width);
    let mut cell = 0;
    for c in text.chars() {
        let w = cell_width(c);
        let group = hl.and_then(|h| h.get(cell)).copied().unwrap_or_default();
        let style = Style {
            hl: group,
            ..Style::default()
        };
        push_char(&mut cells, c, w, style, width);
        cell += w;
    }
    cells.resize(width, blank());
    cells
}

impl Grid {
    /// The grid of a frame. `insert` says whether Insert mode is on, which
    /// bolds the status label (an error message takes its place, as before).
    pub fn from_frame(frame: &Frame, insert: bool) -> Grid {
        let width = frame.width.max(1);
        let mut rows: Vec<Vec<Cell>> = frame
            .rows
            .iter()
            .enumerate()
            .map(|(r, text)| row_cells(text, frame.hl.get(r).map(Vec::as_slice), width))
            .collect();
        let mut status = row_cells(&frame.status, None, width);
        if frame.status_is_error {
            let text_cells: usize = frame.status.trim_end().chars().map(cell_width).sum();
            for c in &mut status[..text_cells.min(width)] {
                c.style.error = true;
            }
        } else if insert {
            for c in &mut status[..INSERT_LABEL_CELLS.min(width)] {
                c.style.bold = true;
            }
        }
        rows.push(status);
        Grid { width, rows }
    }
}

/// Which cells of `new` differ from `old` (all of them without an `old`).
/// A wide character is changed as a whole: if either of its cells changed,
/// both are marked, so a write never starts or ends in the middle of one.
fn changed_cells(old: Option<&[Cell]>, new: &[Cell]) -> Vec<bool> {
    let mut mark: Vec<bool> = (0..new.len())
        .map(|i| old.is_none_or(|o| o[i] != new[i]))
        .collect();
    for i in 0..new.len() {
        if !mark[i] {
            continue;
        }
        if new[i].ch.is_none() && i > 0 {
            mark[i - 1] = true;
        }
        if i + 1 < new.len() && new[i].ch.is_some() && new[i + 1].ch.is_none() {
            mark[i + 1] = true;
        }
    }
    mark
}

/// The writes that turn the screen `prev` into `next`. With no `prev`, or
/// one of another size (a resize), every cell is written.
pub fn diff(prev: Option<&Grid>, next: &Grid) -> Vec<DrawOp> {
    let prev = prev.filter(|p| p.width == next.width && p.rows.len() == next.rows.len());
    let width = next.width;
    let mut ops = Vec::new();
    for (row, cells) in next.rows.iter().enumerate() {
        let old = prev.map(|p| p.rows[row].as_slice());
        let changed = changed_cells(old, cells);
        let mut col = 0;
        while col < width {
            if !changed[col] {
                col += 1;
                continue;
            }
            let start = col;
            let style = cells[col].style;
            while col < width && changed[col] && cells[col].style == style {
                col += 1;
            }
            let run = &cells[start..col];
            // A run that ends the row in default blanks: the blanks are
            // cleared, not written.
            let keep = if col == width && style == Style::default() {
                run.iter()
                    .rposition(|c| c.ch != Some(' '))
                    .map_or(0, |k| k + 1)
            } else {
                run.len()
            };
            if keep > 0 {
                let text = run[..keep].iter().filter_map(|c| c.ch).collect();
                ops.push(DrawOp::Text {
                    row,
                    col: start,
                    text,
                    style,
                });
            }
            if keep < run.len() {
                ops.push(DrawOp::ClearToEnd {
                    row,
                    col: start + keep,
                });
            }
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::view;

    /// A grid with the given rows of text (the last one is the status row).
    fn grid(rows: &[&str], width: usize) -> Grid {
        Grid {
            width,
            rows: rows.iter().map(|r| row_cells(r, None, width)).collect(),
        }
    }

    fn text(row: usize, col: usize, s: &str) -> DrawOp {
        DrawOp::Text {
            row,
            col,
            text: s.to_string(),
            style: Style::default(),
        }
    }

    fn visual() -> Style {
        Style {
            hl: Hl::Visual,
            ..Style::default()
        }
    }

    #[test]
    fn identical_frames_give_no_ops() {
        let g = grid(&["abcd", "ef  "], 4);
        assert!(diff(Some(&g), &g.clone()).is_empty());
    }

    #[test]
    fn one_changed_cell_is_one_op() {
        let prev = grid(&["abcd", "    "], 4);
        let next = grid(&["abXd", "    "], 4);
        assert_eq!(diff(Some(&prev), &next), [text(0, 2, "X")]);
    }

    #[test]
    fn style_only_change_is_written() {
        let prev = grid(&["abc", " "], 3);
        let mut next = prev.clone();
        next.rows[0][1].style = visual();
        assert_eq!(
            diff(Some(&prev), &next),
            [DrawOp::Text {
                row: 0,
                col: 1,
                text: "b".into(),
                style: visual(),
            }]
        );
    }

    #[test]
    fn adjacent_changes_with_one_style_are_coalesced() {
        let prev = grid(&["abcdefgh", "        "], 8);
        let next = grid(&["abXYZfgh", "        "], 8);
        assert_eq!(diff(Some(&prev), &next), [text(0, 2, "XYZ")]);
    }

    #[test]
    fn adjacent_changes_split_at_a_style_change() {
        let prev = grid(&["abcdefgh", "        "], 8);
        let mut next = grid(&["abXYZfgh", "        "], 8);
        next.rows[0][3].style = visual();
        assert_eq!(
            diff(Some(&prev), &next),
            [
                text(0, 2, "X"),
                DrawOp::Text {
                    row: 0,
                    col: 3,
                    text: "Y".into(),
                    style: visual(),
                },
                text(0, 4, "Z"),
            ]
        );
    }

    #[test]
    fn wide_char_replaced_by_two_narrow_ones() {
        let prev = grid(&["日xy", "    "], 4);
        let next = grid(&["abxy", "    "], 4);
        assert_eq!(diff(Some(&prev), &next), [text(0, 0, "ab")]);
    }

    #[test]
    fn two_narrow_ones_replaced_by_a_wide_char() {
        let prev = grid(&["abxy", "    "], 4);
        let next = grid(&["日xy", "    "], 4);
        assert_eq!(diff(Some(&prev), &next), [text(0, 0, "日")]);
    }

    #[test]
    fn wide_char_replaced_by_another_wide_char() {
        let prev = grid(&["日x", "  "], 3);
        let next = grid(&["本x", "  "], 3);
        assert_eq!(diff(Some(&prev), &next), [text(0, 0, "本")]);
    }

    #[test]
    fn wide_char_tail_is_cleared_when_row_shortens() {
        let prev = grid(&["日日", "  "], 4);
        let next = grid(&["日  ", "  "], 4);
        assert_eq!(
            diff(Some(&prev), &next),
            [DrawOp::ClearToEnd { row: 0, col: 2 }]
        );
    }

    #[test]
    fn shortened_row_clears_its_tail() {
        let prev = grid(&["abcdef", "      "], 6);
        let next = grid(&["ab    ", "      "], 6);
        assert_eq!(
            diff(Some(&prev), &next),
            [DrawOp::ClearToEnd { row: 0, col: 2 }]
        );
    }

    #[test]
    fn lengthened_row_writes_the_new_tail() {
        let prev = grid(&["ab    ", "      "], 6);
        let next = grid(&["abcdef", "      "], 6);
        assert_eq!(diff(Some(&prev), &next), [text(0, 2, "cdef")]);
    }

    #[test]
    fn styled_blank_tail_is_written_not_cleared() {
        // A blank cell in the Visual style is text to write, not a clear.
        let prev = grid(&["ab  ", "    "], 4);
        let mut next = prev.clone();
        next.rows[0][2].style = visual();
        assert_eq!(
            diff(Some(&prev), &next),
            [DrawOp::Text {
                row: 0,
                col: 2,
                text: " ".into(),
                style: visual(),
            }]
        );
        // The same cell at the end of a row, with a default blank after it.
        let mut next = grid(&["abcd", "    "], 4);
        next.rows[0][3].style = visual();
        let prev = grid(&["abc ", "    "], 4);
        assert_eq!(
            diff(Some(&prev), &next),
            [DrawOp::Text {
                row: 0,
                col: 3,
                text: "d".into(),
                style: visual(),
            }]
        );
    }

    #[test]
    fn size_change_repaints_everything() {
        let prev = grid(&["abcd", "efgh"], 4);
        let next = grid(&["abcde", "fgh  "], 5);
        let ops = diff(Some(&prev), &next);
        assert_eq!(ops, diff(None, &next));
        assert_eq!(
            ops,
            [
                text(0, 0, "abcde"),
                text(1, 0, "fgh"),
                DrawOp::ClearToEnd { row: 1, col: 3 },
            ]
        );
    }

    #[test]
    fn cursor_only_move_gives_no_text_ops() {
        let mut ed = Editor::from_text("abc\ndef");
        ed.set_screen_size(10, 4);
        let before = view::frame(&ed);
        let g1 = Grid::from_frame(&before, false);
        ed.feed("l");
        let after = view::frame(&ed);
        assert_ne!(before.cursor, after.cursor);
        let g2 = Grid::from_frame(&after, false);
        // Only the ruler on the status row changes; no text row is written.
        let status = g2.rows.len() - 1;
        let ops = diff(Some(&g1), &g2);
        assert!(!ops.is_empty());
        assert!(
            ops.iter()
                .all(|op| matches!(op, DrawOp::Text { row, .. } if *row == status))
        );
        // The same frame again, with the cursor moved and nothing else, is
        // not written at all.
        assert!(diff(Some(&g2), &g2).is_empty());
    }

    #[test]
    fn insert_label_is_bold_and_error_is_marked() {
        let mut ed = Editor::from_text("abc");
        ed.set_screen_size(20, 3);
        ed.feed("i");
        let g = Grid::from_frame(&view::frame(&ed), true);
        let status = g.rows.last().unwrap();
        assert!(status[..12].iter().all(|c| c.style.bold));
        assert!(!status[12].style.bold);

        let mut ed = Editor::from_text("abc");
        ed.set_screen_size(40, 3);
        ed.feed(":nosuchcommand<CR>");
        let f = view::frame(&ed);
        assert!(f.status_is_error);
        let g = Grid::from_frame(&f, false);
        let status = g.rows.last().unwrap();
        assert!(status[0].style.error);
        assert!(!status[39].style.error);
    }
}
