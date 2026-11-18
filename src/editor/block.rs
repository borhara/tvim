use tinsel::app::Key;

use super::{Editor, Mode, Register};
use crate::buffer::Pos;
use crate::command::{Act, Cmd, Op};
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
            Act::Operate(Op::Delete, _) | Act::Key(Key::Char('x')) => {
                let b = self.take_block();
                self.cut_block(b, cmd.register);
                true
            }
            // to the ends of the lines
            Act::Key(Key::Char('X' | 'D')) => {
                let b = Block {
                    right: usize::MAX,
                    ..self.take_block()
                };
                self.cut_block(b, cmd.register);
                true
            }
            Act::Operate(Op::Yank, _) => {
                let b = self.take_block();
                let text = self.block_text(&b);
                self.store_block(cmd.register, text, true);
                let rows = b.bottom - b.top + 1;
                if rows > 2 {
                    self.info(format!("block of {rows} lines yanked"));
                }
                self.cursor = Pos::new(b.top, self.block_cols(b.top, &b).0);
                self.set_want();
                true
            }
            Act::Key(Key::Char(c @ ('p' | 'P'))) => {
                let Some(reg) = self.fetch(cmd.register) else {
                    return true;
                };
                let b = self.take_block();
                // p keeps what it replaced in "; P leaves registers alone
                self.cut_block(b, if c == 'P' { Some('_') } else { None });
                if reg.block || reg.linewise || reg.text.contains('\n') {
                    self.put_register(&reg, cmd.n(), true);
                } else {
                    // a piece of one line goes on every line of the block
                    for row in b.top..=b.bottom {
                        let col = self.padded_col(row, b.left);
                        self.buf.insert(Pos::new(row, col), &reg.text.repeat(cmd.n()));
                    }
                    self.cursor = Pos::new(b.top, self.block_cols(b.top, &b).0);
                }
                self.set_want();
                true
            }
            _ => false,
        }
    }

    // the block, as visual mode is left
    fn take_block(&mut self) -> Block {
        let b = self.block();
        self.leave_visual();
        b
    }

    fn block_text(&self, b: &Block) -> String {
        (b.top..=b.bottom)
            .map(|row| {
                let (from, to) = self.block_cols(row, b);
                self.buf
                    .line(row)
                    .chars()
                    .skip(from)
                    .take(to - from)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn cut_block(&mut self, b: Block, register: Option<char>) {
        let text = self.block_text(&b);
        self.store_block(register, text, false);
        self.touch();
        for row in b.top..=b.bottom {
            let (from, to) = self.block_cols(row, &b);
            if from < to {
                self.buf.delete(Pos::new(row, from), Pos::new(row, to));
            }
        }
        self.cursor = Pos::new(b.top, self.block_cols(b.top, &b).0);
        self.set_want();
    }

    // stores as store does, then marks the registers it filled as holding a block
    fn store_block(&mut self, register: Option<char>, text: String, yank: bool) {
        self.store(register, text.clone(), false, yank);
        let mut filled = match register {
            Some('_') => return,
            Some(c) if c.is_ascii_uppercase() => vec![c.to_ascii_lowercase()],
            Some(c) if c.is_ascii_lowercase() => vec![c, if yank { '0' } else { '1' }],
            _ => vec![if yank { '0' } else { '1' }],
        };
        filled.push('"');
        if !yank && !text.contains('\n') {
            filled.push('-');
        }
        for k in filled {
            if let Some(r) = self.registers.get_mut(&k) {
                r.block = true;
            }
        }
    }

    /// The column at screen column `v` of `row`, after padding the line with spaces to reach it.
    pub(crate) fn padded_col(&mut self, row: usize, v: usize) -> usize {
        let ts = self.options.tab_stop;
        let line = self.buf.line(row);
        let width = vcol(line, char_count(line), ts);
        if width < v {
            let padded = format!("{line}{}", " ".repeat(v - width));
            self.buf.set_line(row, padded);
            return char_count(self.buf.line(row));
        }
        col_at(line, v, ts)
    }

    /// Puts a block's lines side by side into the lines from the cursor down, adding lines
    /// past the end as needed, and returns where the last line of it ends.
    pub(crate) fn put_block(&mut self, r: &Register, count: usize, before: bool) -> Pos {
        let ts = self.options.tab_stop;
        self.touch();
        let pieces: Vec<&str> = r.text.split('\n').collect();
        let width = pieces.iter().map(|p| vcol(p, char_count(p), ts)).max().unwrap_or(0);
        let p = self.cursor;
        let line = self.buf.line(p.row);
        let at = if before || line.is_empty() {
            vcol(line, p.col, ts)
        } else {
            vcol(line, p.col + 1, ts)
        };
        let mut first = 0;
        let mut end = p;
        for (i, piece) in pieces.iter().enumerate() {
            let row = p.row + i;
            if row > self.buf.last_row() {
                self.buf.insert_lines(self.buf.len(), vec![String::new()]);
            }
            let col = self.padded_col(row, at);
            // pieces are widened to line up with any text after them
            let pad = " ".repeat(width - vcol(piece, char_count(piece), ts));
            let full = format!("{piece}{pad}");
            let text = if col < char_count(self.buf.line(row)) {
                full.repeat(count)
            } else {
                format!("{}{piece}", full.repeat(count - 1))
            };
            end = self.buf.insert(Pos::new(row, col), &text);
            if i == 0 {
                first = col;
            }
        }
        self.cursor = Pos::new(p.row, first);
        end
    }
}
