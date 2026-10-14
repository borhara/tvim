use crate::buffer::{Buffer, Pos};
use crate::motion::{find_close, find_open};
use crate::text::{Class, class, first_non_blank};

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
        '(' | ')' | 'b' => bracket(b, p, '(', ')', inner, count),
        '[' | ']' => bracket(b, p, '[', ']', inner, count),
        '{' | '}' | 'B' => bracket(b, p, '{', '}', inner, count),
        '<' | '>' => bracket(b, p, '<', '>', inner, count),
        'p' => paragraph(b, p, inner, count),
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

fn bracket(b: &Buffer, p: Pos, open: char, close: char, inner: bool, count: usize) -> Option<Span> {
    let mut at = p;
    let mut found = None;
    for _ in 0..count {
        let o = if found.is_none() && b.char_at(at) == Some(open) {
            at
        } else {
            find_open(b, at, open, close)?
        };
        let c = find_close(b, o, open, close)?;
        found = Some((o, c));
        at = o;
    }
    let (o, c) = found?;
    if !inner {
        return Some(Span {
            start: o,
            end: Pos::new(c.row, c.col + 1),
            linewise: false,
        });
    }
    let mut start = Pos::new(o.row, o.col + 1);
    let mut end = c;
    if start.col >= b.line_len(o.row) && o.row < c.row {
        start = Pos::new(o.row + 1, 0);
    }
    if c.col <= first_non_blank(b.line(c.row)) && c.row > start.row {
        end = Pos::new(c.row - 1, b.line_len(c.row - 1));
        if start.col == 0 {
            return Some(Span {
                start,
                end: Pos::new(end.row, 0),
                linewise: true,
            });
        }
    }
    Some(Span {
        start,
        end: end.max(start),
        linewise: false,
    })
}

fn paragraph(b: &Buffer, p: Pos, inner: bool, count: usize) -> Option<Span> {
    let blank = |r: usize| b.line(r).trim().is_empty();
    let run_end = |from: usize| {
        let k = blank(from);
        let mut r = from;
        while r + 1 < b.len() && blank(r + 1) == k {
            r += 1;
        }
        r
    };
    let mut start = p.row;
    while start > 0 && blank(start - 1) == blank(p.row) {
        start -= 1;
    }
    let mut end = run_end(p.row);
    for n in 0..count {
        if n > 0 && end + 1 < b.len() {
            end = run_end(end + 1);
        }
        if !inner {
            if end + 1 < b.len() && blank(end + 1) != blank(end) {
                end = run_end(end + 1);
            } else if n == 0 {
                while start > 0 && blank(start - 1) {
                    start -= 1;
                }
            }
        }
    }
    Some(Span {
        start: Pos::new(start, 0),
        end: Pos::new(end, 0),
        linewise: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &Buffer, s: Span) -> String {
        if s.linewise {
            b.lines()[s.start.row..=s.end.row].join("\n")
        } else {
            b.slice(s.start, s.end)
        }
    }

    fn obj(src: &str, p: Pos, kind: char, inner: bool) -> Option<String> {
        let b = Buffer::from_text(src);
        object(&b, p, kind, inner, 1).map(|s| text(&b, s))
    }

    #[test]
    fn words() {
        assert_eq!(obj("foo bar baz", Pos::new(0, 5), 'w', true).as_deref(), Some("bar"));
        assert_eq!(obj("foo bar baz", Pos::new(0, 5), 'w', false).as_deref(), Some("bar "));
        assert_eq!(obj("foo bar", Pos::new(0, 5), 'w', false).as_deref(), Some(" bar"));
        assert_eq!(obj("foo  bar", Pos::new(0, 3), 'w', false).as_deref(), Some("  bar"));
        assert_eq!(obj("a.b c", Pos::new(0, 0), 'W', true).as_deref(), Some("a.b"));
        assert_eq!(obj("", Pos::new(0, 0), 'w', true), None);
        let b = Buffer::from_text("one two three");
        let s = object(&b, Pos::new(0, 0), 'w', false, 2).unwrap();
        assert_eq!(b.slice(s.start, s.end), "one two ");
    }

}
