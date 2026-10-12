use crate::buffer::{Buffer, Pos};
use crate::text::{Class, class};

// keeps the current line's chars, so long lines aren't rescanned each step
struct Line<'a> {
    b: &'a Buffer,
    row: usize,
    chars: Vec<char>,
}

impl<'a> Line<'a> {
    fn new(b: &'a Buffer) -> Self {
        Line {
            b,
            row: usize::MAX,
            chars: Vec::new(),
        }
    }

    fn row(&mut self, row: usize) -> &[char] {
        if row != self.row {
            self.chars = self.b.line(row).chars().collect();
            self.row = row;
        }
        &self.chars
    }

    fn at(&mut self, p: Pos) -> Option<char> {
        self.row(p.row).get(p.col).copied()
    }

    fn len(&mut self, row: usize) -> usize {
        self.row(row).len()
    }

    fn class(&mut self, p: Pos, big: bool) -> Class {
        self.at(p).map_or(Class::Blank, |c| class(c, big))
    }

    fn next(&mut self, p: Pos) -> Option<Pos> {
        if p.col + 1 < self.len(p.row) {
            return Some(Pos::new(p.row, p.col + 1));
        }
        (p.row + 1..self.b.len())
            .find(|&r| !self.b.line(r).is_empty())
            .map(|r| Pos::new(r, 0))
    }

    fn prev(&mut self, p: Pos) -> Option<Pos> {
        if p.col > 0 {
            return Some(Pos::new(p.row, p.col - 1));
        }
        let row = (0..p.row).rev().find(|&r| !self.b.line(r).is_empty())?;
        Some(Pos::new(row, self.len(row) - 1))
    }

    fn prev_stop(&mut self, p: Pos) -> Option<Pos> {
        if p.col > 0 {
            return Some(Pos::new(p.row, p.col.min(self.len(p.row)) - 1));
        }
        let row = p.row.checked_sub(1)?;
        Some(Pos::new(row, self.len(row).saturating_sub(1)))
    }
}

pub(crate) fn class_at(b: &Buffer, p: Pos, big: bool) -> Option<Class> {
    b.char_at(p).map(|c| class(c, big))
}

pub fn word_forward(b: &Buffer, mut p: Pos, big: bool) -> Pos {
    let mut l = Line::new(b);
    let start = l.class(p, big);
    if start != Class::Blank {
        while l.at(p).is_some_and(|c| class(c, big) == start) {
            p.col += 1;
        }
    }
    loop {
        match l.at(p) {
            Some(c) if class(c, big) == Class::Blank => p.col += 1,
            Some(_) => return p,
            None if p.row + 1 >= b.len() => return p,
            None => {
                p = Pos::new(p.row + 1, 0);
                if l.len(p.row) == 0 {
                    return p;
                }
            }
        }
    }
}

pub fn word_end(b: &Buffer, p: Pos, big: bool) -> Option<Pos> {
    let mut l = Line::new(b);
    let mut p = l.next(p)?;
    while l.class(p, big) == Class::Blank {
        p = l.next(p)?;
    }
    let cls = l.class(p, big);
    while l.class(Pos::new(p.row, p.col + 1), big) == cls && p.col + 1 < l.len(p.row) {
        p.col += 1;
    }
    Some(p)
}

pub fn word_back(b: &Buffer, p: Pos, big: bool) -> Option<Pos> {
    let mut l = Line::new(b);
    let mut p = l.prev_stop(p)?;
    loop {
        match l.at(p) {
            None => return Some(p),
            Some(c) if class(c, big) == Class::Blank => match l.prev_stop(p) {
                Some(q) => p = q,
                None => return Some(p),
            },
            Some(_) => break,
        }
    }
    let cls = l.class(p, big);
    while p.col > 0 && l.class(Pos::new(p.row, p.col - 1), big) == cls {
        p.col -= 1;
    }
    Some(p)
}

pub fn word_end_back(b: &Buffer, p: Pos, big: bool) -> Option<Pos> {
    let mut l = Line::new(b);
    let mut cls = l.class(p, big);
    let mut p = p;
    loop {
        let q = l.prev_stop(p)?;
        if q.row != p.row {
            cls = Class::Blank;
        }
        p = q;
        let Some(c) = l.at(p) else { return Some(p) };
        let k = class(c, big);
        if k != Class::Blank && k != cls {
            return Some(p);
        }
        cls = k;
    }
}

fn blank_line(b: &Buffer, row: usize) -> bool {
    b.line(row).is_empty()
}

pub fn paragraph_forward(b: &Buffer, p: Pos) -> Pos {
    let mut r = p.row;
    while r < b.len() && blank_line(b, r) {
        r += 1;
    }
    while r < b.len() && !blank_line(b, r) {
        r += 1;
    }
    if r >= b.len() { b.end() } else { Pos::new(r, 0) }
}

pub fn paragraph_back(b: &Buffer, p: Pos) -> Pos {
    let mut r = p.row;
    while r > 0 && blank_line(b, r) {
        r -= 1;
    }
    while r > 0 && !blank_line(b, r) {
        r -= 1;
    }
    Pos::new(r, 0)
}

const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

pub fn match_bracket(b: &Buffer, p: Pos) -> Option<Pos> {
    let line: Vec<char> = b.line(p.row).chars().collect();
    let (col, c) = line
        .iter()
        .enumerate()
        .skip(p.col)
        .find(|(_, c)| PAIRS.iter().any(|&(o, cl)| **c == o || **c == cl))?;
    let at = Pos::new(p.row, col);
    match PAIRS.iter().find(|&&(o, _)| o == *c) {
        Some(&(open, close)) => find_close(b, at, open, close),
        None => {
            let &(open, close) = PAIRS.iter().find(|&&(_, cl)| cl == *c)?;
            find_open(b, at, open, close)
        }
    }
}

pub fn find_close(b: &Buffer, at: Pos, open: char, close: char) -> Option<Pos> {
    let mut l = Line::new(b);
    let mut depth = 0usize;
    let mut p = at;
    loop {
        p = l.next(p)?;
        match l.at(p) {
            Some(c) if c == open => depth += 1,
            Some(c) if c == close => {
                if depth == 0 {
                    return Some(p);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
}

pub fn find_open(b: &Buffer, at: Pos, open: char, close: char) -> Option<Pos> {
    let mut l = Line::new(b);
    let mut depth = 0usize;
    let mut p = at;
    loop {
        p = l.prev(p)?;
        match l.at(p) {
            Some(c) if c == close => depth += 1,
            Some(c) if c == open => {
                if depth == 0 {
                    return Some(p);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Find {
    pub forward: bool,
    pub till: bool,
    pub ch: char,
}

impl Find {
    pub fn reversed(self) -> Find {
        Find {
            forward: !self.forward,
            ..self
        }
    }
}

pub fn find_in_line(line: &str, col: usize, find: Find, count: usize, repeat: bool) -> Option<usize> {
    let chars: Vec<char> = line.chars().collect();
    let skip = usize::from(find.till && repeat);
    let hit = if find.forward {
        (col + 1 + skip..chars.len())
            .filter(|&i| chars[i] == find.ch)
            .nth(count - 1)?
    } else {
        (0..col.saturating_sub(skip))
            .rev()
            .filter(|&i| chars[i] == find.ch)
            .nth(count - 1)?
    };
    Some(match (find.till, find.forward) {
        (false, _) => hit,
        (true, true) => hit - 1,
        (true, false) => hit + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        Buffer::from_text(s)
    }

    #[test]
    fn words_forward() {
        let b = buf("foo.bar  baz\n\n  qux");
        let w = |r, c| word_forward(&b, Pos::new(r, c), false);
        assert_eq!(w(0, 0), Pos::new(0, 3));
        assert_eq!(w(0, 3), Pos::new(0, 4));
        assert_eq!(w(0, 4), Pos::new(0, 9));
        assert_eq!(w(0, 9), Pos::new(1, 0), "an empty line is a word");
        assert_eq!(w(1, 0), Pos::new(2, 2));
        assert_eq!(w(2, 2), Pos::new(2, 5), "the last word goes to the end");
        assert_eq!(word_forward(&b, Pos::new(0, 0), true), Pos::new(0, 9));
    }

    #[test]
    fn word_ends_and_backs() {
        let b = buf("foo.bar  baz\n\n  qux");
        assert_eq!(word_end(&b, Pos::new(0, 0), false), Some(Pos::new(0, 2)));
        assert_eq!(word_end(&b, Pos::new(0, 2), false), Some(Pos::new(0, 3)));
        assert_eq!(word_end(&b, Pos::new(0, 2), true), Some(Pos::new(0, 6)));
        assert_eq!(word_end(&b, Pos::new(0, 11), false), Some(Pos::new(2, 4)));
        assert_eq!(word_end(&b, Pos::new(2, 4), false), None);
        assert_eq!(word_back(&b, Pos::new(2, 2), false), Some(Pos::new(1, 0)));
        assert_eq!(word_back(&b, Pos::new(1, 0), false), Some(Pos::new(0, 9)));
        assert_eq!(word_back(&b, Pos::new(0, 5), false), Some(Pos::new(0, 4)));
        assert_eq!(word_back(&b, Pos::new(0, 4), false), Some(Pos::new(0, 3)));
        assert_eq!(word_back(&b, Pos::new(0, 9), true), Some(Pos::new(0, 0)));
        assert_eq!(word_back(&b, Pos::new(0, 0), false), None);
        assert_eq!(word_end_back(&b, Pos::new(0, 9), false), Some(Pos::new(0, 6)));
        assert_eq!(word_end_back(&b, Pos::new(0, 4), false), Some(Pos::new(0, 3)));
        assert_eq!(word_end_back(&b, Pos::new(2, 2), false), Some(Pos::new(1, 0)));
    }

}
