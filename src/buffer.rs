use std::collections::VecDeque;

use crate::text::{byte_at, char_count};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub row: usize,
    pub col: usize,
}

impl Pos {
    pub const fn new(row: usize, col: usize) -> Pos {
        Pos { row, col }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    lines: Vec<String>,
    tracked: Tracked,
}

/// Lines marked to visit one by one, as :normal and :g do, kept to as lines above them come
/// and go; a line taken out is forgotten. One list for each command running, innermost last.
#[derive(Clone, Debug, Default)]
struct Tracked(Vec<VecDeque<Option<usize>>>);

// marked lines don't make two buffers' text any different
impl PartialEq for Tracked {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Tracked {}

impl Default for Buffer {
    fn default() -> Self {
        Buffer::new()
    }
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer {
            lines: vec![String::new()],
            tracked: Tracked::default(),
        }
    }

    pub fn from_lines(mut lines: Vec<String>) -> Buffer {
        if lines.is_empty() {
            lines.push(String::new());
        }
        Buffer {
            lines,
            tracked: Tracked::default(),
        }
    }

    pub fn from_text(text: &str) -> Buffer {
        Buffer::from_lines(text.split('\n').map(str::to_string).collect())
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    pub fn line(&self, row: usize) -> &str {
        self.lines.get(row).map_or("", String::as_str)
    }

    pub fn line_len(&self, row: usize) -> usize {
        char_count(self.line(row))
    }

    pub fn last_row(&self) -> usize {
        self.lines.len() - 1
    }

    pub fn char_at(&self, p: Pos) -> Option<char> {
        self.line(p.row).chars().nth(p.col)
    }

    pub fn set_line(&mut self, row: usize, text: String) {
        self.lines[row] = text;
    }

    pub fn end(&self) -> Pos {
        Pos::new(self.last_row(), self.line_len(self.last_row()))
    }

    pub fn insert(&mut self, at: Pos, text: &str) -> Pos {
        let line = &self.lines[at.row];
        let split = byte_at(line, at.col);
        let tail = line[split..].to_string();
        let mut parts = text.split('\n');
        let first = parts.next().unwrap_or("");
        let head = format!("{}{first}", &line[..split]);
        let mut end = Pos::new(at.row, char_count(&head));
        self.lines[at.row] = head;
        let added = text.matches('\n').count();
        if added > 0 {
            // text put in at the start of a line pushes the line down with it
            let pushed = at.col == 0;
            self.move_tracked(|r| {
                Some(if r > at.row || r == at.row && pushed {
                    r + added
                } else {
                    r
                })
            });
        }
        let mut row = at.row;
        for part in parts {
            row += 1;
            self.lines.insert(row, part.to_string());
            end = Pos::new(row, char_count(part));
        }
        self.lines[row].push_str(&tail);
        end
    }

    pub fn slice(&self, from: Pos, to: Pos) -> String {
        if from >= to {
            return String::new();
        }
        if from.row == to.row {
            let line = self.line(from.row);
            return line[byte_at(line, from.col)..byte_at(line, to.col)].to_string();
        }
        let first = self.line(from.row);
        let mut out = first[byte_at(first, from.col)..].to_string();
        for row in from.row + 1..to.row {
            out.push('\n');
            out.push_str(self.line(row));
        }
        out.push('\n');
        let last = self.line(to.row);
        out.push_str(&last[..byte_at(last, to.col)]);
        out
    }

    pub fn delete(&mut self, from: Pos, to: Pos) -> String {
        let to = to.min(self.end());
        if from >= to {
            return String::new();
        }
        let gone = self.slice(from, to);
        if to.row > from.row {
            // whole lines go, or the last is joined onto the first and goes with the rest
            let whole = from.col == 0 && to.col == 0;
            let n = to.row - from.row;
            self.move_tracked(|r| match r {
                r if r < from.row => Some(r),
                r if r == from.row => (!whole).then_some(r),
                r if r < to.row => None,
                r if r == to.row => whole.then_some(from.row),
                r => Some(r - n),
            });
        }
        let last = &self.lines[to.row];
        let tail = last[byte_at(last, to.col)..].to_string();
        let first = &mut self.lines[from.row];
        first.truncate(byte_at(first, from.col));
        first.push_str(&tail);
        self.lines.drain(from.row + 1..=to.row);
        gone
    }

    pub fn delete_lines(&mut self, first: usize, last: usize) -> Vec<String> {
        let last = last.min(self.last_row());
        self.move_tracked(|r| match r {
            r if r < first => Some(r),
            r if r <= last => None,
            r => Some(r - (last - first + 1)),
        });
        let gone: Vec<String> = self.lines.drain(first..=last).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        gone
    }

    pub fn replace_lines(&mut self, first: usize, last: usize, new: Vec<String>) {
        let last = last.min(self.last_row());
        let (old, kept) = (last - first + 1, new.len());
        self.move_tracked(|r| match r {
            r if r < first => Some(r),
            r if r > last => Some(r + kept - old),
            r => (r - first < kept).then_some(r),
        });
        self.lines.splice(first..=last, new);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
    }

    pub fn insert_lines(&mut self, at: usize, lines: Vec<String>) {
        let at = at.min(self.lines.len());
        let n = lines.len();
        self.move_tracked(|r| Some(if r >= at { r + n } else { r }));
        self.lines.splice(at..at, lines);
    }

    // all new text, so no line marked is there any more
    pub fn replace_all(&mut self, lines: Vec<String>) {
        let mut tracked = std::mem::take(&mut self.tracked);
        tracked.0.iter_mut().for_each(VecDeque::clear);
        *self = Buffer::from_lines(lines);
        self.tracked = tracked;
    }

    /// Marks `rows` to visit, and gives the list's place to pass to `next_tracked`.
    pub fn track(&mut self, rows: impl IntoIterator<Item = usize>) -> usize {
        self.tracked.0.push(rows.into_iter().map(Some).collect());
        self.tracked.0.len() - 1
    }

    /// The next line marked in list `id` that's still there, where it is now.
    pub fn next_tracked(&mut self, id: usize) -> Option<usize> {
        let rows = self.tracked.0.get_mut(id)?;
        while let Some(row) = rows.pop_front() {
            if row.is_some() {
                return row;
            }
        }
        None
    }

    /// Forgets list `id`, and any inside it.
    pub fn untrack(&mut self, id: usize) {
        self.tracked.0.truncate(id);
    }

    fn move_tracked(&mut self, to: impl Fn(usize) -> Option<usize>) {
        for rows in &mut self.tracked.0 {
            for row in rows.iter_mut() {
                *row = row.and_then(&to);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marked_lines_keep_to_their_text() {
        let lines = |n: usize| (0..n).map(|i| i.to_string()).collect::<Vec<_>>();
        let visit = |b: &mut Buffer, id| std::iter::from_fn(|| b.next_tracked(id)).collect::<Vec<_>>();
        let mut b = Buffer::from_lines(lines(6));
        let id = b.track([1, 2, 3, 4, 5]);
        b.delete_lines(2, 2);
        b.insert_lines(0, vec!["new".into()]);
        b.replace_lines(4, 4, vec!["4a".into(), "4b".into()]);
        b.delete(Pos::new(3, 1), Pos::new(4, 0));
        assert_eq!(b.lines(), ["new", "0", "1", "34a", "4b", "5"]);
        assert_eq!(visit(&mut b, id), [2, 3, 5], "2 went, and 4 was joined onto 3");
        b.untrack(id);

        let mut b = Buffer::from_lines(lines(3));
        let id = b.track([0, 1, 2]);
        b.insert(Pos::new(1, 0), "x\n");
        b.delete(Pos::new(0, 0), Pos::new(1, 0));
        assert_eq!(
            visit(&mut b, id),
            [1, 2],
            "whole lines taken out charwise take their mark"
        );
        assert_eq!(b, Buffer::from_lines(b.lines().to_vec()), "marks don't count as text");
    }

    #[test]
    fn inserts_text_with_line_breaks() {
        let mut b = Buffer::from_text("hello world");
        let end = b.insert(Pos::new(0, 5), ",\nbig");
        assert_eq!(b.text(), "hello,\nbig world");
        assert_eq!(end, Pos::new(1, 3));
        assert_eq!(b.insert(Pos::new(1, 0), ""), Pos::new(1, 0));
    }

    #[test]
    fn slices_and_deletes_across_lines() {
        let mut b = Buffer::from_text("one\ntwo\nthree");
        assert_eq!(b.slice(Pos::new(0, 1), Pos::new(2, 2)), "ne\ntwo\nth");
        assert_eq!(b.slice(Pos::new(0, 3), Pos::new(1, 0)), "\n");
        assert_eq!(b.delete(Pos::new(0, 1), Pos::new(2, 2)), "ne\ntwo\nth");
        assert_eq!(b.text(), "oree");
        assert_eq!(b.delete(Pos::new(0, 2), Pos::new(9, 9)), "ee");
        assert_eq!(b.text(), "or");
    }

    #[test]
    fn line_operations_keep_one_line() {
        let mut b = Buffer::from_text("a\nb\nc");
        assert_eq!(b.delete_lines(0, 1), ["a", "b"]);
        assert_eq!(b.delete_lines(0, 5), ["c"]);
        assert!(b.is_empty());
        b.insert_lines(1, vec!["x".into(), "y".into()]);
        assert_eq!(b.text(), "\nx\ny");
    }

    #[test]
    fn wide_characters_count_once() {
        let b = Buffer::from_text("héllo");
        assert_eq!(b.line_len(0), 5);
        assert_eq!(b.char_at(Pos::new(0, 1)), Some('é'));
        assert_eq!(b.char_at(Pos::new(0, 5)), None);
    }
}
