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
}

impl Default for Buffer {
    fn default() -> Self {
        Buffer::new()
    }
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer {
            lines: vec![String::new()],
        }
    }

    pub fn from_lines(mut lines: Vec<String>) -> Buffer {
        if lines.is_empty() {
            lines.push(String::new());
        }
        Buffer { lines }
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
        let last = &self.lines[to.row];
        let tail = last[byte_at(last, to.col)..].to_string();
        let first = &mut self.lines[from.row];
        first.truncate(byte_at(first, from.col));
        first.push_str(&tail);
        self.lines.drain(from.row + 1..=to.row);
        gone
    }

    pub fn delete_lines(&mut self, first: usize, last: usize) -> Vec<String> {
        let gone: Vec<String> = self.lines.drain(first..=last.min(self.last_row())).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        gone
    }

    pub fn replace_lines(&mut self, first: usize, last: usize, new: Vec<String>) {
        let last = last.min(self.last_row());
        self.lines.splice(first..=last, new);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
    }

    pub fn insert_lines(&mut self, at: usize, lines: Vec<String>) {
        let at = at.min(self.lines.len());
        self.lines.splice(at..at, lines);
    }

    pub fn replace_all(&mut self, lines: Vec<String>) {
        *self = Buffer::from_lines(lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
