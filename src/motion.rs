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
