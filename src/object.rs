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
