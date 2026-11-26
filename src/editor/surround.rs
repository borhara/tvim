use super::{Editor, Range};
use crate::buffer::Pos;
use crate::command::Target;
use crate::object::object;
use crate::text::first_non_blank;

/// Whether `c` names a pair to put around text: a bracket, quote, or any other mark, which
/// goes on both sides as it is.
pub(crate) fn can_add(c: char) -> bool {
    "bB".contains(c) || c.is_ascii_punctuation()
}

/// Whether `c` names a pair ds and cs can find around the cursor.
pub(crate) fn can_find(c: char) -> bool {
    "()b[]{}B<>\"'`".contains(c)
}

// what goes either side for `c`; an opening bracket leaves a space inside, as vim-surround does
fn pair(c: char) -> (String, String) {
    let (open, close, spaced) = match c {
        '(' => ('(', ')', true),
        ')' | 'b' => ('(', ')', false),
        '[' => ('[', ']', true),
        ']' => ('[', ']', false),
        '{' => ('{', '}', true),
        '}' | 'B' => ('{', '}', false),
        '<' | '>' => ('<', '>', false),
        c => (c, c, false),
    };
    if spaced {
        (format!("{open} "), format!(" {close}"))
    } else {
        (open.to_string(), close.to_string())
    }
}

impl Editor {
    /// ys: puts the pair for `c` around what `target` covers.
    pub(super) fn surround_target(&mut self, target: Target, count: Option<usize>, c: char) {
        if let Some(r) = self.target_range(target, count, false) {
            self.surround(r, c);
        }
    }

    /// Puts the pair for `c` around `r`: whole lines from their text on, and otherwise
    /// leaving out the blanks it ends with, as ysw would take in.
    pub(super) fn surround(&mut self, r: Range, c: char) {
        let (start, end) = if r.linewise {
            let last = r.end.row.min(self.buf.last_row());
            let first = r.start.row;
            (
                Pos::new(first, first_non_blank(self.buf.line(first))),
                Pos::new(last, self.buf.line_len(last)),
            )
        } else {
            let mut end = r.end;
            if end.col == 0 && end.row > r.start.row {
                end = Pos::new(end.row - 1, self.buf.line_len(end.row - 1));
            }
            let line: Vec<char> = self.buf.line(end.row).chars().collect();
            let floor = if end.row == r.start.row { r.start.col } else { 0 };
            while end.col > floor && line.get(end.col - 1).is_some_and(|c| c.is_whitespace()) {
                end.col -= 1;
            }
            (r.start, end)
        };
        if start >= end {
            return;
        }
        let (open, close) = pair(c);
        self.touch();
        self.buf.insert(end, &close);
        self.buf.insert(start, &open);
        self.cursor = start;
    }

    /// ds: takes away the pair for `c` around the cursor, and with an opening bracket the
    /// blanks just inside it too.
    pub(super) fn unsurround(&mut self, c: char) {
        self.resurround(c, None);
    }

    /// cs, or with no `new`, ds: swaps the pair for `old` around the cursor for `new`'s.
    pub(super) fn resurround(&mut self, old: char, new: Option<char>) {
        let Some((open, close)) = self.find_pair(old) else {
            return;
        };
        let trim = "([{".contains(old);
        let (put_open, put_close) = new.map(pair).unwrap_or_default();
        let line: Vec<char> = self.buf.line(close.row).chars().collect();
        let mut from = close.col;
        let floor = if close.row == open.row { open.col + 1 } else { 0 };
        while trim && from > floor && line[from - 1] == ' ' {
            from -= 1;
        }
        self.touch();
        self.buf
            .delete(Pos::new(close.row, from), Pos::new(close.row, close.col + 1));
        self.buf.insert(Pos::new(close.row, from), &put_close);
        let line: Vec<char> = self.buf.line(open.row).chars().collect();
        let mut to = open.col + 1;
        let ceiling = if close.row == open.row { from } else { line.len() };
        while trim && to < ceiling && line[to] == ' ' {
            to += 1;
        }
        self.buf.delete(open, Pos::new(open.row, to));
        self.buf.insert(open, &put_open);
        self.cursor = open;
    }

    // where the open and close of the pair for `c` around the cursor are
    fn find_pair(&self, c: char) -> Option<(Pos, Pos)> {
        if matches!(c, '"' | '\'' | '`') {
            let inside = object(&self.buf, self.cursor, c, true, 1)?;
            let open = Pos::new(inside.start.row, inside.start.col.checked_sub(1)?);
            return Some((open, inside.end));
        }
        let around = object(&self.buf, self.cursor, c, false, 1)?;
        Some((around.start, Pos::new(around.end.row, around.end.col - 1)))
    }
}
