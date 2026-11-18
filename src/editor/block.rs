use tinsel::app::Key;

use super::{Editor, Mode, Register, single, toggle};
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

/// Text typed on a block's first line, to go on its other lines too once insert mode ends.
#[derive(Clone, Debug)]
pub(crate) struct BlockInsert {
    start: Pos,
    lines: usize,
    rows: std::ops::RangeInclusive<usize>,
    at: Where,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Where {
    // at this screen column, in lines that reach into it
    Column(usize),
    // at this screen column, padding short lines out to it
    Padded(usize),
    End,
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
            Act::Key(Key::Char('I')) => {
                let b = self.take_block();
                self.cursor = Pos::new(b.top, self.block_cols(b.top, &b).0);
                self.block_insert(&b, Where::Column(b.left));
                true
            }
            Act::Key(Key::Char('A')) => {
                let b = self.take_block();
                // padding the first line is part of the change
                self.touch();
                let at = if b.to_end() { Where::End } else { Where::Padded(b.right) };
                self.cursor = match at {
                    Where::End => Pos::new(b.top, char_count(self.buf.line(b.top))),
                    _ => Pos::new(b.top, self.padded_col(b.top, b.right)),
                };
                self.block_insert(&b, at);
                true
            }
            Act::Operate(Op::Change, _) | Act::Key(Key::Char('s' | 'C')) => {
                let mut b = self.take_block();
                if cmd.act == Act::Key(Key::Char('C')) {
                    b.right = usize::MAX;
                }
                self.cut_block(b, cmd.register);
                // a block to the line ends leaves every line ending where it began
                let at = if b.to_end() {
                    Where::Padded(b.left)
                } else {
                    Where::Column(b.left)
                };
                self.block_insert(&b, at);
                true
            }
            Act::With('r', ch) if ch != '\n' => {
                let b = self.take_block();
                self.map_block(&b, |_| ch);
                true
            }
            Act::Operate(op @ (Op::Lower | Op::Upper | Op::ToggleCase), _) => {
                let b = self.take_block();
                self.map_block(&b, |c| match op {
                    Op::Lower => single(c.to_lowercase()).unwrap_or(c),
                    Op::Upper => single(c.to_uppercase()).unwrap_or(c),
                    _ => toggle(c),
                });
                true
            }
            Act::Operate(op @ (Op::Indent | Op::Dedent), _) => {
                let b = self.take_block();
                self.shift_block(&b, op == Op::Indent, cmd.n());
                true
            }
            Act::Key(Key::Ctrl(c @ ('a' | 'x'))) => {
                let b = self.take_block();
                let by = if c == 'a' { cmd.n() as i64 } else { -(cmd.n() as i64) };
                for row in b.top..=b.bottom {
                    let (from, to) = self.block_cols(row, &b);
                    if from < to {
                        self.cursor = Pos::new(row, from);
                        self.increment(by);
                    }
                }
                self.cursor = Pos::new(b.top, self.block_cols(b.top, &b).0);
                true
            }
            _ => false,
        }
    }

    fn map_block(&mut self, b: &Block, f: impl Fn(char) -> char) {
        self.touch();
        for row in b.top..=b.bottom {
            let (from, to) = self.block_cols(row, b);
            let line: String = self
                .buf
                .line(row)
                .chars()
                .enumerate()
                .map(|(i, c)| if (from..to).contains(&i) && c != '\t' { f(c) } else { c })
                .collect();
            self.buf.set_line(row, line);
        }
        self.cursor = Pos::new(b.top, self.block_cols(b.top, b).0);
        self.set_want();
    }

    // > and < move the text from the block's left edge on, not the whole line
    fn shift_block(&mut self, b: &Block, right: bool, times: usize) {
        let ts = self.options.tab_stop;
        let by = self.options.shift_width.max(1) * times;
        self.touch();
        for row in b.top..=b.bottom {
            let line = self.buf.line(row).to_string();
            if vcol(&line, char_count(&line), ts) <= b.left {
                continue;
            }
            let at = col_at(&line, b.left, ts);
            let (head, tail): (String, String) = (line.chars().take(at).collect(), line.chars().skip(at).collect());
            let start = vcol(&line, at, ts);
            let blank = tail.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
            let rest: String = tail.chars().skip(blank).collect();
            let width = vcol(&line, at + blank, ts) - start;
            let width = if right { width + by } else { width.saturating_sub(by) };
            // spaces, as tabs mid-line would shift with the text before them
            self.buf.set_line(row, format!("{head}{}{rest}", " ".repeat(width)));
        }
        self.cursor = Pos::new(b.top, self.block_cols(b.top, b).0);
        self.set_want();
    }

    fn block_insert(&mut self, b: &Block, at: Where) {
        self.touch();
        self.begin_insert(1, None);
        self.insert.block = Some(BlockInsert {
            start: self.cursor,
            lines: self.buf.len(),
            rows: b.top + 1..=b.bottom,
            at,
        });
    }

    /// Copies what was typed on a block's first line to the rest of it, as long as the typing
    /// stayed on that line.
    pub(crate) fn finish_block_insert(&mut self) {
        let Some(bi) = self.insert.block.take() else { return };
        let (start, end) = (bi.start, self.cursor);
        if end.row != start.row || end.col <= start.col || self.buf.len() != bi.lines {
            return;
        }
        let typed: String = self
            .buf
            .line(start.row)
            .chars()
            .skip(start.col)
            .take(end.col - start.col)
            .collect();
        let ts = self.options.tab_stop;
        for row in bi.rows {
            let line = self.buf.line(row);
            let width = vcol(line, char_count(line), ts);
            let col = match bi.at {
                Where::Column(v) if width <= v => continue,
                Where::Column(v) => col_at(line, v, ts),
                Where::Padded(v) => self.padded_col(row, v),
                Where::End => char_count(line),
            };
            self.buf.insert(Pos::new(row, col), &typed);
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
