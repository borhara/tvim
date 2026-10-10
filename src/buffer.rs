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

}
