use tinsel::app::Key;

use super::{Dot, Editor, Input, Mode, Range, VisualSize};
use crate::buffer::Pos;
use crate::command::{Act, Cmd, Op};
use crate::object::{Span, object};
use crate::text::{col_at, vcol};

impl Editor {
    // gc repeats as gcc, and so isn't here
    pub(super) fn is_visual_change(cmd: &Cmd) -> bool {
        match cmd.act {
            Act::Operate(op, _) => !matches!(op, Op::Yank | Op::Comment | Op::Filter | Op::Tidy),
            Act::Surround(..) => true,
            Act::Key(Key::Char(c)) => "xXDsSCRJpPIA".contains(c),
            Act::Key(Key::Ctrl('a' | 'x')) | Act::With('r', _) => true,
            _ => false,
        }
    }

    pub(super) fn visual_size(&self) -> VisualSize {
        let (a, b) = if self.cursor < self.anchor {
            (self.cursor, self.anchor)
        } else {
            (self.anchor, self.cursor)
        };
        let lines = b.row - a.row + 1;
        let (cols, to_end) = match self.mode {
            Mode::VisualBlock => {
                let blk = self.block();
                (blk.right.saturating_sub(blk.left), blk.to_end())
            }
            Mode::Visual if lines == 1 => (b.col - a.col + 1, false),
            Mode::Visual => (b.col, false),
            _ => (0, false),
        };
        VisualSize {
            mode: self.mode,
            lines,
            cols,
            to_end,
        }
    }

    // selects as much as v held, from the cursor
    pub(super) fn select_again(&mut self, v: VisualSize) {
        let ts = self.options.tab_stop;
        self.anchor = self.cursor;
        let row = (self.cursor.row + v.lines - 1).min(self.buf.last_row());
        let last = |e: &Self, row: usize| e.buf.line_len(row).saturating_sub(1);
        self.cursor = match v.mode {
            Mode::Visual if v.lines == 1 => Pos::new(row, (self.cursor.col + v.cols - 1).min(last(self, row))),
            Mode::Visual => Pos::new(row, v.cols.min(last(self, row))),
            Mode::VisualBlock => {
                let left = vcol(self.buf.line(self.anchor.row), self.anchor.col, ts);
                Pos::new(row, col_at(self.buf.line(row), left + v.cols.max(1) - 1, ts))
            }
            _ => Pos::new(row, self.cursor.col),
        };
        self.mode = v.mode;
        self.set_want();
        if v.to_end {
            self.want = usize::MAX;
        }
    }

    pub(super) fn enter_visual(&mut self, mode: Mode) {
        self.anchor = self.cursor;
        self.mode = mode;
    }

    pub(super) fn leave_visual(&mut self) {
        let r = self.visual_range();
        self.last_visual = Some((self.anchor, self.cursor, self.mode));
        self.marks.insert('<', r.start);
        let last = r.last_row();
        self.marks.insert(
            '>',
            if r.linewise {
                Pos::new(last, 0)
            } else if last < r.end.row {
                Pos::new(last, self.buf.line_len(last).saturating_sub(1))
            } else {
                Pos::new(last, r.end.col.saturating_sub(1))
            },
        );
        self.mode = Mode::Normal;
        self.clamp();
    }

    pub(crate) fn visual_range(&self) -> Range {
        let (a, b) = if self.cursor < self.anchor {
            (self.cursor, self.anchor)
        } else {
            (self.anchor, self.cursor)
        };
        if self.mode == Mode::VisualLine {
            return Range::lines(a.row, b.row);
        }
        if self.mode == Mode::VisualBlock {
            let blk = self.block();
            let (from, _) = self.block_cols(blk.top, &blk);
            let (_, to) = self.block_cols(blk.bottom, &blk);
            return Range {
                start: Pos::new(blk.top, from),
                end: Pos::new(blk.bottom, to),
                linewise: false,
            };
        }
        let len = self.buf.line_len(b.row);
        let end = if len == 0 && b.row < self.buf.last_row() {
            Pos::new(b.row + 1, 0)
        } else {
            Pos::new(b.row, (b.col + 1).min(len))
        };
        Range {
            start: a,
            end,
            linewise: false,
        }
    }

    pub(super) fn run_visual(&mut self, cmd: Cmd) {
        let n = cmd.n();
        if self.mode == Mode::VisualBlock && self.run_block(&cmd) {
            return;
        }
        match cmd.act {
            Act::Move(m) => self.move_cursor(m, cmd.count),
            Act::Select { kind, inner } => {
                let Some(Span { start, end, linewise }) = object(&self.buf, self.cursor, kind, inner, n) else {
                    return;
                };
                if linewise {
                    self.mode = Mode::VisualLine;
                    self.anchor = start;
                    self.cursor = Pos::new(end.row, 0);
                } else if start < end {
                    self.anchor = start;
                    self.cursor = if end.col == 0 && end.row > start.row {
                        Pos::new(end.row - 1, self.buf.line_len(end.row - 1))
                    } else {
                        Pos::new(end.row, end.col - 1)
                    };
                }
            }
            Act::Operate(op, _) => {
                let r = self.visual_range();
                self.leave_visual();
                self.apply(op, r, cmd.register, n);
                if op == Op::Yank {
                    self.cursor = r.start;
                }
                // repeats as gcc over as many lines
                if op == Op::Comment && !self.replaying {
                    self.dot = Some(Dot {
                        count: Some(r.last_row() - r.start.row + 1),
                        register: None,
                        inputs: crate::keys::parse("gcc").into_iter().map(Input::Key).collect(),
                        visual: None,
                    });
                }
            }
            Act::Surround(_, c) => {
                let r = self.visual_range();
                self.leave_visual();
                self.surround(r, c);
            }
            // the lines selected move, and stay selected
            Act::Key(Key::Alt(c)) => {
                let (top, bottom) = (
                    self.anchor.row.min(self.cursor.row),
                    self.anchor.row.max(self.cursor.row),
                );
                let by = self.shift_lines(top, bottom, c == 'j', n);
                for p in [&mut self.anchor, &mut self.cursor] {
                    p.row = if c == 'j' { p.row + by } else { p.row - by };
                }
            }
            Act::Key(Key::Char(c)) => self.visual_key(c, cmd.register, n),
            Act::Key(Key::Ctrl('v' | 'q')) => self.switch_visual(Mode::VisualBlock),
            Act::Key(Key::Ctrl(c @ ('a' | 'x'))) => {
                let r = self.visual_range();
                self.leave_visual();
                for row in r.start.row..=r.last_row() {
                    self.cursor = Pos::new(
                        row,
                        if row == r.start.row && !r.linewise {
                            r.start.col
                        } else {
                            0
                        },
                    );
                    self.increment(if c == 'a' { n as i64 } else { -(n as i64) });
                }
                self.cursor = r.start;
            }
            Act::With('r', ch) => {
                let r = self.visual_range();
                self.leave_visual();
                self.touch();
                let (from, to) = self.char_span(r);
                let text = self.buf.slice(from, to);
                let new: String = text.chars().map(|c| if c == '\n' { c } else { ch }).collect();
                self.buf.delete(from, to);
                self.buf.insert(from, &new);
                self.cursor = from;
            }
            Act::With(c, ch) => self.normal_with(c, ch, n),
            Act::G(c) => self.normal_g(c, n, cmd.register),
            Act::Z(k) => self.normal_z(k, cmd.count),
            Act::Code('a') => self.code_actions(),
            _ => {}
        }
    }

    // v in visual mode leaves it, V switches to lines, and so on
    pub(super) fn switch_visual(&mut self, want: Mode) {
        if self.mode == want {
            self.leave_visual();
        } else {
            self.mode = want;
        }
    }

    pub(super) fn visual_key(&mut self, c: char, register: Option<char>, n: usize) {
        let r = self.visual_range();
        let lines = Range::lines(r.start.row, r.last_row());
        match c {
            'v' => self.switch_visual(Mode::Visual),
            'V' => self.switch_visual(Mode::VisualLine),
            'o' | 'O' => std::mem::swap(&mut self.anchor, &mut self.cursor),
            'x' => {
                self.leave_visual();
                self.apply(Op::Delete, r, register, n);
            }
            'X' | 'D' => {
                self.leave_visual();
                self.apply(Op::Delete, lines, register, n);
            }
            'Y' => {
                self.leave_visual();
                self.apply(Op::Yank, lines, register, n);
            }
            's' => {
                self.leave_visual();
                self.apply(Op::Change, r, register, n);
            }
            'S' | 'R' | 'C' => {
                self.leave_visual();
                self.apply(Op::Change, lines, register, n);
            }
            'J' => {
                self.leave_visual();
                self.join(r.start.row, (r.last_row() - r.start.row + 1).max(2), true);
            }
            'p' | 'P' => {
                let Some(reg) = self.fetch(register) else { return };
                self.leave_visual();
                // p puts the replaced text in "; P leaves registers alone
                self.apply(Op::Delete, r, if c == 'P' { Some('_') } else { None }, 1);
                let text = if reg.linewise {
                    vec![reg.text.as_str(); n].join("\n")
                } else {
                    reg.text.repeat(n)
                };
                if r.linewise {
                    let row = r.start.row;
                    let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
                    let emptied = self.buf.is_empty();
                    self.buf.insert_lines(row.min(self.buf.len()), lines);
                    if emptied {
                        let last = self.buf.last_row();
                        self.buf.delete_lines(last, last);
                    }
                    self.cursor = self.first_non_blank_pos(row.min(self.buf.last_row()));
                } else if reg.linewise {
                    let at = r.start;
                    self.buf.insert(at, &format!("\n{text}\n"));
                    self.cursor = Pos::new(at.row + 1, 0);
                } else {
                    let end = self.buf.insert(r.start, &text);
                    self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
                }
            }
            ':' => {
                self.leave_visual();
                self.begin_cmdline(':', "'<,'>".to_string());
            }
            'I' => {
                self.leave_visual();
                self.cursor = if r.linewise {
                    self.first_non_blank_pos(r.start.row)
                } else {
                    r.start
                };
                self.begin_insert(1, None);
            }
            'A' => {
                self.leave_visual();
                self.cursor = if r.linewise || r.last_row() < r.end.row {
                    Pos::new(r.last_row(), self.buf.line_len(r.last_row()))
                } else {
                    r.end
                };
                self.begin_insert(1, None);
            }
            _ => {}
        }
    }

    pub(crate) fn leave_visual_if_any(&mut self) {
        if self.mode.is_visual() {
            self.leave_visual();
        }
    }
}
