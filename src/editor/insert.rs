use tinsel::app::Key;

use super::{Completion, Editor, Input, InsertSession, Mode, Range};
use crate::buffer::Pos;
use crate::command::Op;
use crate::motion;
use crate::text::{char_count, class, first_non_blank, indent, is_word, vcol};

impl Editor {
    pub(super) fn begin_insert(&mut self, count: usize, open: Option<bool>) {
        self.set_want();
        self.mode = Mode::Insert;
        self.insert = InsertSession {
            count,
            open,
            ..InsertSession::default()
        };
    }

    pub(super) fn open_line(&mut self, above: bool, count: usize) {
        self.touch();
        self.open_one(above);
        self.begin_insert(count, Some(above));
        self.insert.auto_indent = Some(self.cursor.row);
    }

    pub(super) fn open_one(&mut self, above: bool) {
        let line = self.buf.line(self.cursor.row).to_string();
        let mut ind = if self.options.auto_indent {
            indent(&line).to_string()
        } else {
            String::new()
        };
        let deeper = if above {
            line.trim_start().starts_with(['}', ']', ')'])
        } else {
            line.trim_end().ends_with(['{', '[', '('])
        };
        if self.options.auto_indent && deeper {
            let width = vcol(&ind, char_count(&ind), self.options.tab_stop);
            ind = self.make_indent(width + self.options.shift_width);
        }
        let row = if above { self.cursor.row } else { self.cursor.row + 1 };
        self.buf.insert_lines(row, vec![ind.clone()]);
        self.cursor = Pos::new(row, char_count(&ind));
    }

    pub(super) fn insert_input(&mut self, input: Input) {
        if self.completion_input(&input) {
            return;
        }
        if let Some(b) = self.building.as_mut() {
            b.inputs.push(input.clone());
        }
        if !matches!(input, Input::Key(Key::Esc | Key::Ctrl('c'))) {
            self.insert.typed.push(input.clone());
        }
        let key = match input {
            Input::Paste(text) => {
                self.touch();
                self.insert.auto_indent = None;
                self.cursor = self.buf.insert(self.cursor, &text);
                return;
            }
            Input::Key(k) => k,
        };
        if self.insert.register_next {
            self.insert.register_next = false;
            if let Key::Char(c) = key {
                if let Some(r) = self.fetch(Some(c)) {
                    let mut text = r.text;
                    if r.linewise {
                        text.push('\n');
                    }
                    self.touch();
                    self.cursor = self.buf.insert(self.cursor, &text);
                }
            }
            return;
        }
        match key {
            Key::Esc | Key::Ctrl('c') => self.leave_insert(),
            Key::Char(c) => self.type_paired(c),
            Key::Tab => {
                if self.options.expand_tab {
                    let v = vcol(self.buf.line(self.cursor.row), self.cursor.col, self.options.tab_stop);
                    let sw = self.options.shift_width.max(1);
                    let spaces = " ".repeat(sw - v % sw);
                    self.type_str(&spaces);
                } else {
                    self.type_char('\t');
                }
            }
            Key::Enter | Key::Ctrl('j') | Key::Ctrl('m') => self.newline(),
            Key::Backspace | Key::Ctrl('h') => self.backspace(),
            Key::Delete => {
                let p = self.cursor;
                let len = self.buf.line_len(p.row);
                if p.col < len {
                    self.touch();
                    self.buf.delete(p, Pos::new(p.row, p.col + 1));
                } else if p.row < self.buf.last_row() {
                    self.touch();
                    self.buf.delete(p, Pos::new(p.row + 1, 0));
                }
            }
            Key::Ctrl('w') => self.delete_word_before(),
            Key::Ctrl('u') => {
                let p = self.cursor;
                if p.col == 0 {
                    return self.backspace();
                }
                let fnb = first_non_blank(self.buf.line(p.row));
                let from = if p.col > fnb { fnb } else { 0 };
                self.touch();
                self.buf.delete(Pos::new(p.row, from), p);
                self.cursor.col = from;
            }
            Key::Ctrl('r') => self.insert.register_next = true,
            Key::Ctrl('t') | Key::Ctrl('d') => {
                let row = self.cursor.row;
                let before = self.buf.line_len(row);
                let r = Range::lines(row, row);
                let col = self.cursor.col;
                let op = if key == Key::Ctrl('t') { Op::Indent } else { Op::Dedent };
                if self.buf.line(row).is_empty() && op == Op::Indent {
                    self.touch();
                    let ind = self.make_indent(self.options.shift_width);
                    self.buf.set_line(row, ind);
                } else {
                    self.apply(op, r, None, 1);
                }
                let after = self.buf.line_len(row);
                self.cursor = Pos::new(row, (col + after).saturating_sub(before));
            }
            Key::Left => self.cursor.col = self.cursor.col.saturating_sub(1),
            Key::Right => self.cursor.col += 1,
            Key::Up => {
                if self.cursor.row > 0 {
                    self.cursor.row -= 1;
                    self.cursor.col = self.col_for_want(self.cursor.row);
                }
                return;
            }
            Key::Down => {
                if self.cursor.row < self.buf.last_row() {
                    self.cursor.row += 1;
                    self.cursor.col = self.col_for_want(self.cursor.row);
                }
                return;
            }
            Key::Home => self.cursor.col = 0,
            Key::End => self.cursor.col = self.buf.line_len(self.cursor.row),
            Key::PageUp => self.scroll_page(false, 1),
            Key::PageDown => self.scroll_page(true, 1),
            _ => {}
        }
        self.clamp();
        self.set_want();
    }

    pub fn completion(&self) -> Option<(&str, &[String], Option<usize>)> {
        let c = self.completion.as_ref()?;
        Some((c.prefix.as_str(), c.words.as_slice(), c.at))
    }

    pub(super) fn completion_input(&mut self, input: &Input) -> bool {
        let key = match input {
            Input::Key(k) => Some(*k),
            Input::Paste(_) => None,
        };
        if self.completion.is_some() {
            match key {
                Some(Key::Ctrl(c @ ('n' | 'p'))) => self.cycle_completion(c == 'p'),
                Some(Key::Ctrl('y')) => self.accept_completion(),
                Some(Key::Ctrl('e') | Key::Esc) => {
                    self.pick_completion(None);
                    self.completion = None;
                }
                _ => {
                    self.accept_completion();
                    return false;
                }
            }
            return true;
        }
        if self.mode == Mode::Insert && matches!(key, Some(Key::Ctrl('n' | 'p'))) {
            self.start_completion(key == Some(Key::Ctrl('p')));
            return true;
        }
        false
    }

    pub(super) fn start_completion(&mut self, back: bool) {
        let p = self.cursor;
        let before: Vec<char> = self.buf.line(p.row).chars().take(p.col).collect();
        let len = before.iter().rev().take_while(|c| is_word(**c)).count();
        let prefix: String = before[before.len() - len..].iter().collect();
        let start = Pos::new(p.row, p.col - len);
        let others: Vec<&[String]> = (self.docs.iter().enumerate())
            .filter(|(i, _)| *i != self.current)
            .map(|(_, d)| d.buf.lines())
            .collect();
        let words = crate::complete::candidates(self.buf.lines(), start, &prefix, &others);
        if words.is_empty() {
            return self.error("Pattern not found");
        }
        self.completion = Some(Completion {
            start,
            prefix,
            words,
            at: None,
        });
        self.cycle_completion(back);
    }

    pub(super) fn cycle_completion(&mut self, back: bool) {
        let Some(c) = &self.completion else { return };
        let n = c.words.len();
        let at = match (c.at, back) {
            (None, false) => Some(0),
            (None, true) => Some(n - 1),
            (Some(i), false) => (i + 1 < n).then_some(i + 1),
            (Some(i), true) => i.checked_sub(1),
        };
        self.pick_completion(at);
    }

    pub(super) fn pick_completion(&mut self, at: Option<usize>) {
        let Some(c) = self.completion.as_mut() else { return };
        c.at = at;
        let text = at.map_or(&c.prefix, |i| &c.words[i]).clone();
        let start = c.start;
        self.touch();
        self.buf.delete(start, self.cursor);
        self.cursor = self.buf.insert(start, &text);
        self.insert.auto_indent = None;
        self.set_want();
    }

    // dot and counts replay the chosen word as typed text, not as another search
    pub(super) fn accept_completion(&mut self) {
        let Some(c) = self.completion.take() else { return };
        let Some(i) = c.at else { return };
        let rest = c.words[i][c.prefix.len()..].to_string();
        if rest.is_empty() {
            return;
        }
        if let Some(b) = self.building.as_mut() {
            b.inputs.push(Input::Paste(rest.clone()));
        }
        self.insert.typed.push(Input::Paste(rest));
    }

    pub(super) fn type_char(&mut self, c: char) {
        self.touch();
        self.insert.auto_indent = None;
        let p = self.cursor;
        if self.mode == Mode::Replace && p.col < self.buf.line_len(p.row) {
            let old = self.buf.char_at(p);
            self.buf.delete(p, Pos::new(p.row, p.col + 1));
            self.insert.replaced.push(old);
        } else if self.mode == Mode::Replace {
            self.insert.replaced.push(None);
        }
        let mut s = [0u8; 4];
        self.cursor = self.buf.insert(p, c.encode_utf8(&mut s));
        if matches!(c, '}' | ']' | ')') && self.options.auto_indent && self.mode == Mode::Insert {
            self.outdent_closer(c);
        }
    }

    pub(super) fn type_paired(&mut self, c: char) {
        if !self.options.auto_pairs || self.mode != Mode::Insert {
            return self.type_char(c);
        }
        let p = self.cursor;
        let next = self.buf.char_at(p);
        let prev = p
            .col
            .checked_sub(1)
            .and_then(|col| self.buf.char_at(Pos::new(p.row, col)));
        let quote = matches!(c, '"' | '\'' | '`');
        if (quote || matches!(c, ')' | ']' | '}')) && next == Some(c) {
            self.cursor.col += 1;
            return;
        }
        let close = match c {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            _ if quote => c,
            _ => return self.type_char(c),
        };
        if next.is_some_and(is_word) || quote && prev.is_some_and(is_word) {
            return self.type_char(c);
        }
        self.type_char(c);
        self.buf.insert(self.cursor, &close.to_string());
    }

    pub(super) fn outdent_closer(&mut self, c: char) {
        let row = self.cursor.row;
        if self.buf.line(row).trim() != c.encode_utf8(&mut [0; 4]) {
            return;
        }
        let open = match c {
            '}' => '{',
            ']' => '[',
            _ => '(',
        };
        let at = Pos::new(row, self.cursor.col - 1);
        let Some(o) = motion::find_open(&self.buf, at, open, c) else {
            return;
        };
        let ind = indent(self.buf.line(o.row)).to_string();
        self.buf.set_line(row, format!("{ind}{c}"));
        self.cursor.col = char_count(&ind) + 1;
    }

    pub(super) fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            self.type_char(c);
        }
    }

    pub(super) fn newline(&mut self) {
        self.touch();
        // clear an unused autoindent but carry it to the next line
        let kept = self.clear_unused_indent();
        let p = self.cursor;
        let left: String = self.buf.line(p.row).chars().take(p.col).collect();
        let mut ind = match kept {
            Some(k) => k,
            None if self.options.auto_indent => indent(&left).to_string(),
            None => String::new(),
        };
        if self.options.auto_indent && left.trim_end().ends_with(['{', '[', '(', ':']) {
            ind = self.make_indent(vcol(&ind, char_count(&ind), self.options.tab_stop) + self.options.shift_width);
        }
        if self.mode == Mode::Replace {
            self.insert.replaced.clear();
        }
        let keep = char_count(left.trim_end_matches([' ', '\t']));
        if keep < p.col && !left.trim().is_empty() {
            self.buf.delete(Pos::new(p.row, keep), p);
            self.cursor.col = keep;
        }
        let end = self.buf.insert(self.cursor, &format!("\n{ind}"));
        let rest = self.buf.line(end.row).to_string();
        let tail = &rest[crate::text::byte_at(&rest, end.col)..];
        let trimmed = tail.trim_start_matches([' ', '\t']);
        let closer = trimmed.starts_with(['}', ']', ')']);
        if trimmed.len() != tail.len() {
            let head: String = rest.chars().take(end.col).collect();
            self.buf.set_line(end.row, format!("{head}{trimmed}"));
        }
        self.cursor = end;
        self.insert.auto_indent = Some(end.row);
        if closer && self.options.auto_indent && left.trim_end().ends_with(['{', '[', '(']) {
            let outer = indent(&left).to_string();
            let line = self.buf.line(end.row).to_string();
            let head: String = line.chars().take(end.col).collect();
            let tail: String = line.chars().skip(end.col).collect();
            self.buf.set_line(end.row, head);
            self.buf.insert_lines(end.row + 1, vec![format!("{outer}{tail}")]);
        }
    }

    pub(super) fn clear_unused_indent(&mut self) -> Option<String> {
        let row = self.cursor.row;
        let line = self.buf.line(row);
        if self.insert.auto_indent != Some(row) || !line.trim().is_empty() || line.is_empty() {
            return None;
        }
        let line = line.to_string();
        self.buf.set_line(row, String::new());
        self.cursor.col = 0;
        Some(line)
    }

    pub(super) fn backspace(&mut self) {
        let p = self.cursor;
        if self.mode == Mode::Replace {
            if p.col == 0 {
                return;
            }
            let back = Pos::new(p.row, p.col - 1);
            if let Some(old) = self.insert.replaced.pop() {
                self.touch();
                self.buf.delete(back, p);
                if let Some(c) = old {
                    self.buf.insert(back, &c.to_string());
                }
            }
            self.cursor = back;
            return;
        }
        if p.col == 0 {
            if p.row == 0 {
                return;
            }
            self.touch();
            let len = self.buf.line_len(p.row - 1);
            self.buf.delete(Pos::new(p.row - 1, len), p);
            self.cursor = Pos::new(p.row - 1, len);
            return;
        }
        self.touch();
        let pair = |a, b| {
            matches!(
                (a, b),
                ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('`', '`')
            )
        };
        if let (true, Some(a), Some(b)) = (
            self.options.auto_pairs,
            self.buf.char_at(Pos::new(p.row, p.col - 1)),
            self.buf.char_at(p),
        ) {
            if pair(a, b) {
                self.buf.delete(Pos::new(p.row, p.col - 1), Pos::new(p.row, p.col + 1));
                self.cursor.col -= 1;
                return;
            }
        }
        let mut before = self.buf.line(p.row).chars().take(p.col);
        let blank = before.next() == Some(' ') && before.all(|c| c == ' ');
        let from = if self.options.expand_tab && blank {
            let sw = self.options.shift_width.max(1);
            p.col - (if p.col % sw == 0 { sw } else { p.col % sw })
        } else {
            p.col - 1
        };
        self.buf.delete(Pos::new(p.row, from), p);
        self.cursor.col = from;
    }

    pub(super) fn delete_word_before(&mut self) {
        let p = self.cursor;
        if p.col == 0 {
            return self.backspace();
        }
        let chars: Vec<char> = self.buf.line(p.row).chars().collect();
        let mut i = p.col;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let k = class(chars[i - 1], false);
            while i > 0 && class(chars[i - 1], false) == k {
                i -= 1;
            }
        }
        self.touch();
        self.buf.delete(Pos::new(p.row, i), p);
        self.cursor.col = i;
    }

    pub(super) fn leave_insert(&mut self) {
        self.completion = None;
        self.finish_block_insert();
        let count = self.insert.count;
        if count > 1 && !self.insert_replay {
            let typed = std::mem::take(&mut self.insert.typed);
            let open = self.insert.open;
            let building = self.building.take();
            self.insert_replay = true;
            for _ in 1..count {
                if open.is_some() {
                    self.open_one(false);
                }
                for i in &typed {
                    self.insert_input(i.clone());
                }
            }
            self.insert_replay = false;
            self.building = building;
        }
        self.clear_unused_indent();
        self.marks.insert('^', self.cursor);
        self.mode = Mode::Normal;
        self.cursor.col = self.cursor.col.saturating_sub(1);
        self.clamp();
        self.set_want();
        if let Some(b) = self.building.take() {
            self.dot = Some(b);
        }
        self.end_change();
    }
}
