use crate::buffer::{Buffer, Pos};
use crate::text::{Class, class};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
    pub linewise: bool,
}

pub fn object(b: &Buffer, p: Pos, kind: char, inner: bool, count: usize) -> Option<Span> {
    match kind {
        'w' => word(b, p, inner, false, count),
        'W' => word(b, p, inner, true, count),
        '"' | '\'' | '`' => quote(b, p, kind, inner),
        _ => None,
    }
}

pub fn is_object(kind: char) -> bool {
    "wW\"'`()b[]{}B<>p".contains(kind)
}

fn word(b: &Buffer, p: Pos, inner: bool, big: bool, count: usize) -> Option<Span> {
    let chars: Vec<char> = b.line(p.row).chars().collect();
    if chars.is_empty() {
        return None;
    }
    let col = p.col.min(chars.len() - 1);
    let cls = |i: usize| class(chars[i], big);
    let run_end = |from: usize| {
        let k = cls(from);
        (from..chars.len()).find(|&i| cls(i) != k).unwrap_or(chars.len())
    };
    let mut start = col;
    while start > 0 && cls(start - 1) == cls(col) {
        start -= 1;
    }
    let mut end = run_end(col);
    if inner {
        for _ in 1..count {
            if end < chars.len() {
                end = run_end(end);
            }
        }
    } else {
        let on_blank = cls(col) == Class::Blank;
        for n in 0..count {
            if on_blank && n == 0 {
                if end < chars.len() {
                    end = run_end(end);
                }
                continue;
            }
            if n > 0 && end < chars.len() && cls(end) != Class::Blank {
                end = run_end(end);
            }
            if end < chars.len() && cls(end) == Class::Blank {
                end = run_end(end);
            } else if n == 0 {
                while start > 0 && cls(start - 1) == Class::Blank {
                    start -= 1;
                }
            }
        }
    }
    Some(Span {
        start: Pos::new(p.row, start),
        end: Pos::new(p.row, end),
        linewise: false,
    })
}

fn quote(b: &Buffer, p: Pos, q: char, inner: bool) -> Option<Span> {
    let chars: Vec<char> = b.line(p.row).chars().collect();
    let mut quotes = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == q {
            quotes.push(i);
        }
        i += 1;
    }
    let (open, close) = quotes
        .chunks_exact(2)
        .map(|w| (w[0], w[1]))
        .find(|&(o, c)| o <= p.col && p.col <= c)
        .or_else(|| quotes.chunks_exact(2).map(|w| (w[0], w[1])).find(|&(o, _)| o > p.col))?;
    let (mut start, mut end) = if inner { (open + 1, close) } else { (open, close + 1) };
    if !inner {
        let blank = |i: usize| chars[i] == ' ' || chars[i] == '\t';
        if end < chars.len() && blank(end) {
            while end < chars.len() && blank(end) {
                end += 1;
            }
        } else {
            while start > 0 && blank(start - 1) {
                start -= 1;
            }
        }
    }
    Some(Span {
        start: Pos::new(p.row, start),
        end: Pos::new(p.row, end),
        linewise: false,
    })
}
