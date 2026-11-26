use super::{Editor, Range, Register, single, toggle};
use crate::buffer::Pos;
use crate::command::{Motion, Op, Target};
use crate::motion;
use crate::object::object;
use crate::text::{char_count, first_non_blank, indent, vcol};

impl Editor {
    pub(super) fn store(&mut self, register: Option<char>, text: String, linewise: bool, yank: bool) {
        let r = Register {
            text,
            linewise,
            block: false,
        };
        // @ plays the new text, not an older recording
        if let Some(c) = register.filter(char::is_ascii_alphabetic) {
            self.macros.remove(&c.to_ascii_lowercase());
        }
        match register {
            Some('_') => return,
            Some(c @ ('+' | '*')) => {
                let mut t = r.text.clone();
                if linewise {
                    t.push('\n');
                }
                if !self.clipboard.set(&t) {
                    self.error(format!("E354: No clipboard to copy to (register {c})"));
                }
            }
            Some(c) if c.is_ascii_uppercase() => {
                let entry = self.registers.entry(c.to_ascii_lowercase()).or_default();
                if entry.linewise || linewise && !entry.text.is_empty() {
                    entry.text.push('\n');
                }
                entry.text.push_str(&r.text);
                entry.linewise |= linewise;
                let all = entry.clone();
                self.registers.insert('"', all);
                return;
            }
            Some(c) if c.is_ascii_lowercase() => {
                self.registers.insert(c, r.clone());
            }
            _ => {}
        }
        self.registers.insert(if yank { '0' } else { '1' }, r.clone());
        if !yank && !linewise && !r.text.contains('\n') {
            self.registers.insert('-', r.clone());
        }
        self.registers.insert('"', r);
    }

    pub(super) fn fetch(&mut self, register: Option<char>) -> Option<Register> {
        match register.unwrap_or('"') {
            c @ ('+' | '*') => {
                let Some(text) = self.clipboard.get() else {
                    self.error(format!("E353: Nothing in register {c}"));
                    return None;
                };
                let linewise = text.ends_with('\n');
                Some(Register {
                    text: text.strip_suffix('\n').unwrap_or(&text).to_string(),
                    linewise,
                    block: false,
                })
            }
            '/' => self.last_search.clone().map(|text| Register {
                text,
                ..Register::default()
            }),
            c => {
                let r = self.registers.get(&c.to_ascii_lowercase()).cloned();
                if r.is_none() {
                    self.error(format!("E353: Nothing in register {c}"));
                }
                r
            }
        }
    }

    pub(super) fn operate(&mut self, op: Op, target: Target, count: Option<usize>, register: Option<char>) {
        let n = count.unwrap_or(1);
        // cw acts like ce, see :h cw
        if let (Op::Change, Target::Motion(Motion::WordForward(big))) = (op, target) {
            if self.buf.char_at(self.cursor).is_some_and(|c| !c.is_whitespace()) {
                return self.change_word(big, n, register);
            }
        }
        let strict = matches!(op, Op::Delete | Op::Change | Op::Yank);
        let Some(range) = self.target_range(target, count, strict) else {
            return;
        };
        // 3>> is three lines, not three shifts, and 3!! three lines to filter
        let times = if matches!(op, Op::Indent | Op::Dedent | Op::Filter) {
            1
        } else {
            n
        };
        self.apply(op, range, register, times);
    }

    /// The text an operator's target covers from the cursor. With `strict`, a count of lines
    /// running past the end covers nothing, as it does for d, c and y.
    pub(super) fn target_range(&mut self, target: Target, count: Option<usize>, strict: bool) -> Option<Range> {
        let n = count.unwrap_or(1);
        Some(match target {
            Target::Lines => {
                let last = self.cursor.row + n - 1;
                if last > self.buf.last_row() && n > 1 && strict {
                    return None;
                }
                Range::lines(self.cursor.row, last.min(self.buf.last_row()))
            }
            Target::Object { kind, inner } => {
                let span = object(&self.buf, self.cursor, kind, inner, n)?;
                Range {
                    start: span.start,
                    end: span.end,
                    linewise: span.linewise,
                }
            }
            Target::Motion(m) => {
                let (to, kind) = self.target(m, count, true)?;
                if Self::is_jump(m) {
                    self.jump_from(self.cursor);
                }
                self.range_to(to, kind)
            }
        })
    }

    pub(super) fn change_word(&mut self, big: bool, n: usize, register: Option<char>) {
        let p = self.cursor;
        let at_end = |e: &Self, q: Pos| {
            let c = motion::class_at(&e.buf, q, big);
            let next = motion::class_at(&e.buf, Pos::new(q.row, q.col + 1), big);
            next != c || next.is_none()
        };
        let mut end = p;
        for i in 0..n {
            if i == 0 && at_end(self, end) {
                continue;
            }
            match motion::word_end(&self.buf, end, big) {
                Some(q) => end = q,
                None => break,
            }
        }
        let range = Range {
            start: p,
            end: Pos::new(end.row, (end.col + 1).min(self.buf.line_len(end.row))),
            linewise: false,
        };
        self.apply(Op::Change, range, register, 1);
    }

    pub(crate) fn apply(&mut self, op: Op, r: Range, register: Option<char>, count: usize) {
        match op {
            Op::Yank => {
                let text = self.range_text(r);
                let lines = r.end.row - r.start.row + 1;
                self.store(register, text, r.linewise, true);
                if r.linewise {
                    if lines > 2 {
                        self.info(format!("{lines} lines yanked"));
                    }
                    self.cursor.row = r.start.row.min(self.cursor.row);
                } else {
                    self.cursor = r.start;
                }
            }
            Op::Delete => {
                if r.linewise {
                    let text = self.range_text(r);
                    self.store(register, text, true, false);
                    self.touch();
                    self.buf.delete_lines(r.start.row, r.end.row);
                    let row = r.start.row.min(self.buf.last_row());
                    self.cursor = self.first_non_blank_pos(row);
                    let gone = r.end.row - r.start.row + 1;
                    if gone > 2 {
                        self.info(format!("{gone} fewer lines"));
                    }
                } else {
                    if r.start >= r.end {
                        return;
                    }
                    let text = self.buf.slice(r.start, r.end);
                    self.store(register, text, false, false);
                    self.touch();
                    self.buf.delete(r.start, r.end);
                    self.cursor = r.start;
                }
                self.set_want();
            }
            Op::Change => {
                if r.linewise {
                    let text = self.range_text(r);
                    self.store(register, text, true, false);
                    self.touch();
                    let keep = if self.options.auto_indent {
                        indent(self.buf.line(r.start.row)).to_string()
                    } else {
                        String::new()
                    };
                    self.cursor = Pos::new(r.start.row, char_count(&keep));
                    self.buf.replace_lines(r.start.row, r.end.row, vec![keep]);
                } else {
                    let text = self.buf.slice(r.start, r.end);
                    self.store(register, text, false, false);
                    self.touch();
                    self.buf.delete(r.start, r.end);
                    self.cursor = r.start;
                }
                self.begin_insert(1, None);
            }
            Op::Indent | Op::Dedent => {
                self.touch();
                let times = count.max(1);
                for row in r.start.row..=r.last_row() {
                    let line = self.buf.line(row).to_string();
                    if line.is_empty() {
                        continue;
                    }
                    let width = vcol(&line, first_non_blank(&line), self.options.tab_stop);
                    let sw = self.options.shift_width.max(1);
                    let new = if op == Op::Indent {
                        width + sw * times
                    } else {
                        width.saturating_sub(sw * times)
                    };
                    let rest = line.trim_start_matches([' ', '\t']);
                    self.buf.set_line(row, format!("{}{rest}", self.make_indent(new)));
                }
                self.cursor = self.first_non_blank_pos(r.start.row);
                let lines = r.end.row - r.start.row + 1;
                if lines > 2 {
                    let sign = if op == Op::Indent { '>' } else { '<' };
                    self.info(format!(
                        "{lines} lines {sign}ed {times} time{}",
                        if times == 1 { "" } else { "s" }
                    ));
                }
            }
            Op::Lower | Op::Upper | Op::ToggleCase => {
                self.touch();
                let (from, to) = self.char_span(r);
                let text = self.buf.slice(from, to);
                let changed: String = text
                    .chars()
                    .map(|c| match op {
                        Op::Lower => single(c.to_lowercase()).unwrap_or(c),
                        Op::Upper => single(c.to_uppercase()).unwrap_or(c),
                        _ => toggle(c),
                    })
                    .collect();
                self.buf.delete(from, to);
                self.buf.insert(from, &changed);
                self.cursor = from;
            }
            // asks for the command, with the lines filled in as vim does
            Op::Filter => {
                let (first, last) = (r.start.row, r.last_row().min(self.buf.last_row()));
                self.cursor = self.first_non_blank_pos(first);
                let lines = match last - first {
                    0 => ".".to_string(),
                    n => format!(".,.+{n}"),
                };
                self.begin_cmdline(':', format!("{lines}!"));
            }
            Op::Format | Op::FormatKeep => {
                let (first, last) = (r.start.row, r.last_row().min(self.buf.last_row()));
                let kind = self.format_kind();
                let width = match self.options.text_width {
                    0 => 79,
                    n => n,
                };
                let lines = crate::format::wrap(&self.buf.lines()[first..=last], width, self.options.tab_stop, kind);
                // gw keeps to the text the cursor was on
                let at = self.cursor;
                let word = (first..=last)
                    .contains(&at.row)
                    .then(|| {
                        let p = Pos::new(at.row - first, at.col);
                        crate::format::word_at(&self.buf.lines()[first..=last], kind, p)
                    })
                    .flatten();
                let n = lines.len();
                if lines != self.buf.lines()[first..=last] {
                    self.touch();
                    self.buf.replace_lines(first, last, lines);
                }
                if op == Op::FormatKeep {
                    let found = word.and_then(|w| crate::format::place(&self.buf.lines()[first..first + n], kind, w));
                    self.cursor = match found {
                        Some(p) => Pos::new(first + p.row, p.col),
                        None => {
                            let row = at.row.min(self.buf.last_row());
                            Pos::new(row, at.col.min(self.buf.line_len(row).saturating_sub(1)))
                        }
                    };
                } else {
                    self.cursor = self.first_non_blank_pos(first + n - 1);
                }
            }
            Op::Comment => {
                let name = self.file_name();
                let (open, close) = crate::comment::markers(name);
                let last = r.last_row().min(self.buf.last_row());
                let lines = crate::comment::toggle(&self.buf.lines()[r.start.row..=last], open, close);
                self.touch();
                for (row, line) in (r.start.row..).zip(lines) {
                    self.buf.set_line(row, line);
                }
                self.cursor = self.first_non_blank_pos(r.start.row);
            }
        }
    }

    /// Alt-j and Alt-k: moves lines `first..=last` down or up `n` lines, or as far as they'll
    /// go, quietly, as a key pressed again and again shouldn't fail at the end. Gives how far.
    pub(super) fn shift_lines(&mut self, first: usize, last: usize, down: bool, n: usize) -> usize {
        let by = if down {
            n.min(self.buf.last_row() - last)
        } else {
            n.min(first)
        };
        if by == 0 {
            return 0;
        }
        self.touch();
        let lines = self.buf.lines()[first..=last].to_vec();
        self.buf.delete_lines(first, last);
        self.buf.insert_lines(if down { first + by } else { first - by }, lines);
        by
    }

    pub(super) fn format_kind(&self) -> crate::format::Kind<'static> {
        let name = self.file_name();
        let (open, close) = crate::comment::markers(name);
        crate::format::Kind {
            comment: close.is_empty().then_some(open),
            markdown: matches!(crate::syntax::extension(name).as_str(), "md" | "markdown" | "mdx"),
        }
    }

    // the open file's name, which says how its comments start
    pub(super) fn file_name(&self) -> &str {
        self.file
            .path
            .as_ref()
            .and_then(|p| p.file_name()?.to_str())
            .unwrap_or("")
    }

    pub(super) fn char_span(&self, r: Range) -> (Pos, Pos) {
        if r.linewise {
            (
                Pos::new(r.start.row, 0),
                Pos::new(r.end.row, self.buf.line_len(r.end.row)),
            )
        } else {
            (r.start, r.end)
        }
    }

    pub(super) fn range_text(&self, r: Range) -> String {
        if r.linewise {
            self.buf.lines()[r.start.row..=r.end.row.min(self.buf.last_row())].join("\n")
        } else {
            self.buf.slice(r.start, r.end)
        }
    }

    pub(super) fn make_indent(&self, width: usize) -> String {
        if self.options.expand_tab {
            " ".repeat(width)
        } else {
            let ts = self.options.tab_stop.max(1);
            format!("{}{}", "\t".repeat(width / ts), " ".repeat(width % ts))
        }
    }

    pub(super) fn put(&mut self, register: Option<char>, count: usize, before: bool) {
        let Some(r) = self.fetch(register) else { return };
        let end = self.put_register(&r, count, before);
        if !r.linewise && !r.block && !r.text.contains('\n') {
            self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
        }
        self.set_want();
    }

    pub(super) fn put_register(&mut self, r: &Register, count: usize, before: bool) -> Pos {
        if r.block {
            return self.put_block(r, count, before);
        }
        self.touch();
        if r.linewise {
            let mut lines = Vec::new();
            for _ in 0..count {
                lines.extend(r.text.split('\n').map(str::to_string));
            }
            let added = lines.len();
            let at = if before { self.cursor.row } else { self.cursor.row + 1 };
            self.buf.insert_lines(at, lines);
            self.cursor = self.first_non_blank_pos(at);
            return Pos::new(at + added - 1, 0);
        }
        let text = r.text.repeat(count);
        let len = self.buf.line_len(self.cursor.row);
        let at = if before || len == 0 {
            self.cursor
        } else {
            Pos::new(self.cursor.row, (self.cursor.col + 1).min(len))
        };
        let end = self.buf.insert(at, &text);
        self.cursor = at;
        end
    }

    pub(super) fn paste_normal(&mut self, text: &str) {
        self.touch();
        let len = self.buf.line_len(self.cursor.row);
        let at = Pos::new(self.cursor.row, (self.cursor.col + 1).min(len));
        let end = self.buf.insert(at, text);
        self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
        self.end_change();
    }

    pub(super) fn join(&mut self, row: usize, lines: usize, spaces: bool) {
        if row >= self.buf.last_row() {
            return;
        }
        self.touch();
        let last = (row + lines - 1).min(self.buf.last_row());
        let mut col = 0;
        for _ in row..last {
            let next = self.buf.delete_lines(row + 1, row + 1).remove(0);
            let mut line = self.buf.line(row).to_string();
            let next = if spaces { next.trim_start().to_string() } else { next };
            if spaces {
                if !next.is_empty() && !next.starts_with(')') && !line.ends_with([' ', '\t']) && !line.is_empty() {
                    line.push(' ');
                }
                col = char_count(&line).saturating_sub(usize::from(!next.is_empty() && line.ends_with(' ')));
            } else {
                col = char_count(&line);
            }
            line.push_str(&next);
            self.buf.set_line(row, line);
        }
        self.cursor = Pos::new(row, col);
    }

    pub(super) fn toggle_case_chars(&mut self, n: usize) {
        let len = self.buf.line_len(self.cursor.row);
        if len == 0 {
            return;
        }
        let end = (self.cursor.col + n).min(len);
        let r = Range {
            start: self.cursor,
            end: Pos::new(self.cursor.row, end),
            linewise: false,
        };
        self.apply(Op::ToggleCase, r, None, 1);
        self.cursor.col = end.min(len - 1);
    }

    pub(super) fn replace_chars(&mut self, ch: char, n: usize) {
        let p = self.cursor;
        let len = self.buf.line_len(p.row);
        if p.col + n > len {
            return;
        }
        self.touch();
        self.buf.delete(p, Pos::new(p.row, p.col + n));
        if ch == '\n' {
            self.buf.insert(p, "\n");
            self.cursor = Pos::new(p.row + 1, 0);
        } else {
            let text: String = std::iter::repeat_n(ch, n).collect();
            self.buf.insert(p, &text);
            self.cursor.col = p.col + n - 1;
        }
    }

    pub(super) fn increment(&mut self, by: i64) {
        let line: Vec<char> = self.buf.line(self.cursor.row).chars().collect();
        let mut i = self.cursor.col.min(line.len());
        while i > 0 && i < line.len() && line[i].is_ascii_digit() && line[i - 1].is_ascii_digit() {
            i -= 1;
        }
        let Some(start) = (i..line.len()).find(|&j| line[j].is_ascii_digit()) else {
            return;
        };
        let mut end = start;
        while end < line.len() && line[end].is_ascii_digit() {
            end += 1;
        }
        let negative = start > 0 && line[start - 1] == '-';
        let from = if negative { start - 1 } else { start };
        let digits: String = line[start..end].iter().collect();
        let Ok(value) = digits.parse::<i64>() else { return };
        let value = if negative { -value } else { value };
        let new = value.saturating_add(by);
        let text = if digits.len() > 1 && digits.starts_with('0') {
            let width = digits.len();
            if new < 0 {
                format!("-{:0width$}", -new)
            } else {
                format!("{new:0width$}")
            }
        } else {
            new.to_string()
        };
        self.touch();
        let row = self.cursor.row;
        self.buf.delete(Pos::new(row, from), Pos::new(row, end));
        self.buf.insert(Pos::new(row, from), &text);
        self.cursor = Pos::new(row, from + char_count(&text) - 1);
    }
}
