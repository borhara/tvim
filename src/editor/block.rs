use tinsel::app::Key;

use super::{Editor, Mode};
use crate::buffer::Pos;
use crate::command::{Act, Cmd};
use crate::text::{cell_width, char_count, col_at, vcol};

/// A block visual selection: rows `top..=bottom` and the screen columns `left..right`, where
/// `right` is `usize::MAX` once `$` has taken the block to the end of every line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
}

impl Block {
    pub fn to_end(&self) -> bool {
        self.right == usize::MAX
    }
}

impl Editor {
    pub fn block_selection(&self) -> Option<Block> {
        (self.mode == Mode::VisualBlock).then(|| self.block())
    }

    pub(crate) fn block(&self) -> Block {
        let ts = self.options.tab_stop;
        let span = |p: Pos| {
            let line = self.buf.line(p.row);
            let start = vcol(line, p.col, ts);
            let width = line.chars().nth(p.col).map_or(1, |c| cell_width(c, start, ts));
            (start, start + width.max(1))
        };
        let (a, c) = (span(self.anchor), span(self.cursor));
        Block {
            top: self.anchor.row.min(self.cursor.row),
            bottom: self.anchor.row.max(self.cursor.row),
            left: a.0.min(c.0),
            right: if self.want == usize::MAX {
                usize::MAX
            } else {
                a.1.max(c.1)
            },
        }
    }

    /// The characters of `row` the block covers, as a column range; a tab only partly inside
    /// counts as inside.
    pub(crate) fn block_cols(&self, row: usize, b: &Block) -> (usize, usize) {
        let ts = self.options.tab_stop;
        let line = self.buf.line(row);
        let len = char_count(line);
        let from = col_at(line, b.left, ts);
        let to = if b.to_end() {
            len
        } else {
            (col_at(line, b.right - 1, ts) + 1).min(len)
        };
        (from, to.max(from))
    }

    /// Handles what block mode does its own way, and says whether it did.
    pub(crate) fn run_block(&mut self, cmd: &Cmd) -> bool {
        match cmd.act {
            Act::Key(Key::Char('O')) => {
                let ts = self.options.tab_stop;
                let (a, c) = (self.anchor, self.cursor);
                let a_v = vcol(self.buf.line(a.row), a.col, ts);
                let c_v = vcol(self.buf.line(c.row), c.col, ts);
                self.anchor = Pos::new(a.row, col_at(self.buf.line(a.row), c_v, ts));
                self.cursor = Pos::new(c.row, col_at(self.buf.line(c.row), a_v, ts));
                self.clamp();
                self.set_want();
                true
            }
            _ => false,
        }
    }
}
