use super::{Editor, Slice};
use crate::buffer::{Buffer, Pos};
use crate::text::{char_count, col_at, vcol};

impl Editor {
    pub fn set_screen(&self, width: usize, height: usize) {
        self.screen.set((width, height));
    }

    pub fn text_rows(&self) -> usize {
        self.screen.get().1.saturating_sub(2).max(1)
    }

    pub fn gutter(&self) -> usize {
        self.gutter_for(self.buf.len())
    }

    pub(crate) fn gutter_for(&self, lines: usize) -> usize {
        if self.zen() || !(self.options.number || self.options.relative_number) {
            return 0;
        }
        lines.checked_ilog10().map_or(1, |d| d as usize + 1).max(3) + 4
    }

    pub fn text_cols(&self) -> usize {
        self.screen.get().0.saturating_sub(self.gutter() + 1).max(1)
    }

    /// Where a line breaks into screen rows `width` wide: one row unless wrap is on.
    pub(crate) fn wrapped(&self, line: &str, width: usize) -> Vec<(usize, usize)> {
        if self.options.wrap {
            crate::text::wrap_rows(line, width, self.options.tab_stop, self.options.linebreak)
        } else {
            vec![(0, 0)]
        }
    }

    /// The screen rows of a window `height` high showing `buf` from line `top`.
    pub(crate) fn slices(&self, buf: &Buffer, top: usize, left: usize, width: usize, height: usize) -> Vec<Slice> {
        let mut out = Vec::with_capacity(height);
        let mut row = top;
        while out.len() < height {
            if row >= buf.len() {
                out.push(Slice {
                    row,
                    col: 0,
                    left: 0,
                    width,
                    first: true,
                    last: true,
                });
            } else if !self.options.wrap {
                out.push(Slice {
                    row,
                    col: 0,
                    left,
                    width,
                    first: true,
                    last: true,
                });
            } else {
                let rows = self.wrapped(buf.line(row), width);
                for (k, &(col, at)) in rows.iter().enumerate() {
                    let end = rows.get(k + 1).map_or(at + width, |r| r.1);
                    out.push(Slice {
                        row,
                        col,
                        left: at,
                        width: (end - at).min(width),
                        first: k == 0,
                        last: k + 1 == rows.len(),
                    });
                }
            }
            row += 1;
        }
        out.truncate(height);
        out
    }

    pub fn top(&self) -> usize {
        self.top.get()
    }

    pub fn left(&self) -> usize {
        self.left.get()
    }

    pub(super) fn scroll_off(&self) -> usize {
        self.options.scroll_off.min(self.text_rows().saturating_sub(1) / 2)
    }

    pub fn scroll(&self) {
        let rows = self.text_rows();
        let so = self.scroll_off();
        let row = self.cursor.row;
        let mut top = self.top.get().min(self.buf.last_row());
        if row < top + so {
            top = row.saturating_sub(so);
        }
        if self.options.wrap {
            // the screen rows the cursor's line takes down to the cursor, and the lines below
            let line = self.buf.line(row);
            let upto = crate::text::row_holding(&self.wrapped(line, self.text_cols()), self.cursor.col) + 1;
            let below: usize = (row + 1..=(row + so).min(self.buf.last_row()))
                .map(|r| self.line_height(r))
                .sum();
            top = top.max(self.top_with_above(row, rows.saturating_sub(upto + below)));
        } else {
            let below = (row + so).min(self.buf.last_row());
            if below >= top + rows {
                top = below + 1 - rows;
            }
        }
        self.top.set(top);

        let cols = self.text_cols();
        if self.options.wrap {
            self.left.set(0);
            return;
        }
        let line = self.buf.line(row);
        let start = vcol(line, self.cursor.col, self.options.tab_stop);
        let end = vcol(line, self.cursor.col + 1, self.options.tab_stop).max(start + 1);
        let mut left = self.left.get();
        if start < left {
            left = start;
        }
        if end > left + cols {
            left = end - cols;
        }
        self.left.set(left);
    }

    /// How many screen rows line `row` takes.
    pub(crate) fn line_height(&self, row: usize) -> usize {
        if self.options.wrap {
            self.wrapped(self.buf.line(row), self.text_cols()).len()
        } else {
            1
        }
    }

    /// The highest line that can be at the top with no more than `above` screen rows between
    /// it and line `row`.
    pub(crate) fn top_with_above(&self, row: usize, above: usize) -> usize {
        let (mut top, mut used) = (row, 0);
        while top > 0 && used + self.line_height(top - 1) <= above {
            top -= 1;
            used += self.line_height(top);
        }
        top
    }

    /// The last line shown in full with `top` at the top.
    pub(crate) fn last_shown(&self, top: usize) -> usize {
        let rows = self.text_rows();
        let top = top.min(self.buf.last_row());
        let (mut row, mut used) = (top, self.line_height(top));
        while row < self.buf.last_row() && used + self.line_height(row + 1) <= rows {
            row += 1;
            used += self.line_height(row);
        }
        row
    }

    // gj and gk on wrapped lines: down or up n screen rows, keeping the place along the row
    pub(super) fn screen_row_move(&self, p: Pos, n: usize, down: bool) -> Option<Pos> {
        let (ts, width) = (self.options.tab_stop, self.text_cols());
        let rows_of = |row: usize| self.wrapped(self.buf.line(row), width);
        let mut rows = rows_of(p.row);
        let mut seg = crate::text::row_holding(&rows, p.col);
        let along = vcol(self.buf.line(p.row), p.col, ts) - rows[seg].1;
        let (mut row, start) = (p.row, (p.row, seg));
        for _ in 0..n {
            if down && seg + 1 < rows.len() {
                seg += 1;
            } else if down && row < self.buf.last_row() {
                row += 1;
                rows = rows_of(row);
                seg = 0;
            } else if !down && seg > 0 {
                seg -= 1;
            } else if !down && row > 0 {
                row -= 1;
                rows = rows_of(row);
                seg = rows.len() - 1;
            }
        }
        if (row, seg) == start {
            return None;
        }
        let line = self.buf.line(row);
        let col = col_at(line, rows[seg].1 + along, ts);
        // short of the next row's first character, and of the end in normal mode
        let end = rows
            .get(seg + 1)
            .map_or(self.max_col(char_count(line)), |r| r.0.saturating_sub(1));
        Some(Pos::new(row, col.min(end).max(rows[seg].0)))
    }

    pub(super) fn scroll_half(&mut self, down: bool, count: Option<usize>) {
        let n = count.unwrap_or(self.text_rows() / 2).max(1);
        let max_top = self.buf.last_row();
        let top = self.top.get();
        if down {
            if self.cursor.row >= self.buf.last_row() {
                return;
            }
            self.top
                .set((top + n).min(max_top.saturating_sub(self.text_rows().saturating_sub(1)).max(top)));
            self.cursor.row = (self.cursor.row + n).min(self.buf.last_row());
        } else {
            if self.cursor.row == 0 {
                return;
            }
            self.top.set(top.saturating_sub(n));
            self.cursor.row = self.cursor.row.saturating_sub(n);
        }
        self.cursor = self.first_non_blank_pos(self.cursor.row);
        self.set_want();
    }

    pub(super) fn scroll_page(&mut self, down: bool, n: usize) {
        let page = self.text_rows().saturating_sub(2).max(1) * n;
        let top = self.top.get();
        let so = self.scroll_off();
        if down {
            let new = (top + page).min(self.buf.last_row());
            self.top.set(new);
            self.cursor.row = (new + so).min(self.buf.last_row());
        } else {
            let new = top.saturating_sub(page);
            self.top.set(new);
            self.cursor.row = (new + self.text_rows()).saturating_sub(1 + so).min(self.buf.last_row());
        }
        self.cursor = self.first_non_blank_pos(self.cursor.row);
        self.set_want();
    }

    pub(super) fn scroll_lines(&mut self, by: isize) {
        let top = self.top.get();
        let new = if by > 0 {
            (top + by as usize).min(self.buf.last_row())
        } else {
            top.saturating_sub((-by) as usize)
        };
        self.top.set(new);
        let so = self.scroll_off();
        let rows = self.text_rows();
        let low = if new == 0 { 0 } else { new + so };
        let high = (new + rows).saturating_sub(1 + so).max(low);
        let row = self
            .cursor
            .row
            .clamp(low.min(self.buf.last_row()), high.min(self.buf.last_row()));
        if row != self.cursor.row {
            self.cursor.row = row;
            self.cursor.col = self.col_for_want(row);
        }
    }
}
