//! Several cursors, as Ctrl-n in vim-visual-multi or Ctrl-d elsewhere: the word under the
//! cursor and each next one like it selected, to change, insert at or delete all at once.

use tinsel::app::Key;

use super::{Editor, Input, Mode};
use crate::buffer::Pos;
use crate::text::{char_count, is_word};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Multi {
    // what's selected each time, matched as a whole word if it was the word under the cursor
    text: String,
    whole: bool,
    // where each selection starts, the one added last being the cursor's
    starts: Vec<Pos>,
    // while typing, what's been typed at each selection's place
    typing: Option<Typing>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Typing {
    // where the typing at each place starts, the cursor's last
    starts: Vec<Pos>,
    // what's there now at each, so far
    typed: String,
    // the cursor's line's length with nothing typed, to tell how much has been
    base: usize,
}

impl Editor {
    pub fn multi_cursors(&self) -> Vec<(Pos, usize)> {
        let Some(m) = &self.multi else { return Vec::new() };
        match &m.typing {
            Some(t) => t
                .starts
                .iter()
                .map(|p| (Pos::new(p.row, p.col + char_count(&t.typed)), 0))
                .collect(),
            None => m.starts.iter().map(|&p| (p, char_count(&m.text))).collect(),
        }
    }

    // Ctrl-n: the word under the cursor, or what's selected on one line, and then the next
    pub(super) fn multi_start(&mut self) {
        let (text, whole, start) = if self.mode == Mode::Visual && self.anchor.row == self.cursor.row {
            let (a, b) = (self.anchor.min(self.cursor), self.anchor.max(self.cursor));
            let text: String = self
                .buf
                .line(a.row)
                .chars()
                .skip(a.col)
                .take(b.col + 1 - a.col)
                .collect();
            (text, false, a)
        } else {
            let Some((col, word)) = self.word_under_cursor() else {
                return self.error("no word under the cursor");
            };
            (word, true, Pos::new(self.cursor.row, col))
        };
        self.leave_visual_if_any();
        self.mode = Mode::Normal;
        self.multi = Some(Multi {
            text,
            whole,
            starts: vec![start],
            typing: None,
        });
        self.cursor = start;
        self.say_multi();
    }

    // keys while there are several cursors, true if they were for them
    pub(super) fn multi_input(&mut self, input: &Input) -> bool {
        let Some(m) = &self.multi else { return false };
        if m.typing.is_some() {
            return false;
        }
        let Input::Key(key) = input else {
            self.multi = None;
            return false;
        };
        match key {
            Key::Ctrl('n') => self.multi_next(false),
            Key::Ctrl('x') => self.multi_next(true),
            Key::Ctrl('p') => {
                if let Some(m) = self.multi.as_mut().filter(|m| m.starts.len() > 1) {
                    m.starts.pop();
                    self.cursor = *m.starts.last().expect("one is left");
                }
                self.say_multi();
            }
            Key::Char(c @ ('c' | 's' | 'i' | 'a' | 'd' | 'x')) => self.multi_edit(*c),
            Key::Char('y') => {
                let text = m.text.clone();
                self.store(None, text, false, true);
                self.multi = None;
            }
            Key::Esc | Key::Ctrl('c') => self.multi = None,
            // anything else leaves them, and goes on as usual
            _ => {
                self.multi = None;
                return false;
            }
        }
        true
    }

    fn say_multi(&mut self) {
        if let Some(m) = &self.multi {
            let n = m.starts.len();
            self.info(format!(
                "{n} selected · Ctrl-n next · Ctrl-x skip · c i a d to edit them all"
            ));
        }
    }

    // the next match after the last added, round past the end, that isn't selected yet; with
    // `skip`, the last added is let go of first
    fn multi_next(&mut self, skip: bool) {
        let Some(m) = self.multi.as_mut() else { return };
        let from = *m.starts.last().expect("never empty");
        if skip && m.starts.len() > 1 {
            m.starts.pop();
        }
        let found = next_match(self.buf.lines(), &m.text, m.whole, from).filter(|p| !m.starts.contains(p));
        match found {
            Some(p) => {
                m.starts.push(p);
                self.cursor = p;
                self.say_multi();
            }
            None => {
                let text = m.text.clone();
                self.error(format!("no more {text}"));
            }
        }
    }

    // c, s, i, a, d and x at every selection, with what's typed then typed at each
    fn multi_edit(&mut self, how: char) {
        let Some(m) = self.multi.clone() else { return };
        let len = char_count(&m.text);
        let delete = matches!(how, 'c' | 's' | 'd' | 'x');
        self.touch();
        let mut sorted = m.starts.clone();
        sorted.sort();
        // from the end, so those before stay put
        if delete {
            for p in sorted.iter().rev() {
                self.buf.delete(*p, Pos::new(p.row, p.col + len));
            }
        }
        // where each is once those before it on its line are gone
        let place = |p: &Pos| {
            let before = sorted.iter().filter(|q| q.row == p.row && q.col < p.col).count();
            let col = if delete { p.col - before * len } else { p.col };
            Pos::new(p.row, col + if how == 'a' { len } else { 0 })
        };
        let starts: Vec<Pos> = m.starts.iter().map(place).collect();
        if matches!(how, 'd' | 'x') {
            self.multi = None;
            self.cursor = *starts.last().expect("never empty");
            self.clamp();
            return self.end_change();
        }
        let at = *starts.last().expect("never empty");
        self.cursor = at;
        self.begin_insert(1, None);
        let base = self.buf.line_len(at.row);
        if let Some(m) = self.multi.as_mut() {
            m.typing = Some(Typing {
                starts,
                typed: String::new(),
                base,
            });
        }
        self.message = None;
    }

    // after a key typed at the cursor, the same at every other place; typing that leaves the
    // cursor's line, or insert mode, leaves the cursors too
    pub(super) fn multi_typed(&mut self) {
        let Some(t) = self.multi.as_ref().and_then(|m| m.typing.clone()) else {
            return;
        };
        let at = *t.starts.last().expect("never empty");
        let grown = self.buf.line_len(at.row) as isize - t.base as isize;
        if self.mode != Mode::Insert || self.cursor.row != at.row || grown < 0 || self.cursor.col < at.col {
            self.multi = None;
            return;
        }
        let typed: String = self
            .buf
            .line(at.row)
            .chars()
            .skip(at.col)
            .take(grown as usize)
            .collect();
        if typed == t.typed {
            return;
        }
        let (old, new) = (char_count(&t.typed), char_count(&typed));
        let others = &t.starts[..t.starts.len() - 1];
        let mut order: Vec<usize> = (0..others.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(others[i]));
        for &i in &order {
            // one right of the cursor on its line has already moved by what was typed there
            let p = match others[i] {
                p if p.row == at.row && p.col > at.col => Pos::new(p.row, p.col + new - old),
                p => p,
            };
            self.buf.delete(p, Pos::new(p.row, p.col + old));
            self.buf.insert(p, &typed);
        }
        // each moves by the change at every place before it on its line
        let shift = |p: Pos, starts: &[Pos], own: usize| {
            let before = (starts.iter().enumerate())
                .filter(|&(j, q)| j != own && q.row == p.row && q.col < p.col)
                .count();
            Pos::new(
                p.row,
                (p.col as isize + before as isize * (new as isize - old as isize)) as usize,
            )
        };
        let starts: Vec<Pos> = (t.starts.iter().enumerate())
            .map(|(i, &p)| shift(p, &t.starts, i))
            .collect();
        let moved = starts.last().expect("never empty").col as isize - at.col as isize;
        self.cursor.col = (self.cursor.col as isize + moved) as usize;
        let mirrored = others.iter().filter(|q| q.row == at.row).count() as isize;
        let base = (t.base as isize + mirrored * (new as isize - old as isize)) as usize;
        if let Some(m) = self.multi.as_mut() {
            m.typing = Some(Typing { starts, typed, base });
        }
    }
}

// where `text` next is after `from`, going round past the end; as a whole word if `whole`
fn next_match(lines: &[String], text: &str, whole: bool, from: Pos) -> Option<Pos> {
    let n = lines.len();
    for k in 0..=n {
        let row = (from.row + k) % n;
        let line = &lines[row];
        let chars: Vec<char> = line.chars().collect();
        for (byte, _) in line.match_indices(text) {
            let col = line[..byte].chars().count();
            if k == 0 && col <= from.col || k == n && col >= from.col {
                continue;
            }
            let end = col + char_count(text);
            let bounded =
                !whole || (col == 0 || !is_word(chars[col - 1])) && chars.get(end).is_none_or(|&c| !is_word(c));
            if bounded {
                return Some(Pos::new(row, col));
            }
        }
    }
    None
}
