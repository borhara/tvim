use tinsel::text::char_width;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Blank,
    Word,
    Punct,
}

pub fn class(c: char, big: bool) -> Class {
    if c.is_whitespace() {
        Class::Blank
    } else if big || is_word(c) {
        Class::Word
    } else {
        Class::Punct
    }
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

pub fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
}

pub fn indent(s: &str) -> &str {
    &s[..s.len() - s.trim_start_matches([' ', '\t']).len()]
}

pub fn first_non_blank(s: &str) -> usize {
    char_count(indent(s))
}

pub fn cell_width(c: char, vcol: usize, tabstop: usize) -> usize {
    match c {
        '\t' => tabstop - vcol % tabstop,
        c if c.is_control() => 2,
        c => char_width(c),
    }
}

pub fn vcol(line: &str, col: usize, tabstop: usize) -> usize {
    line.chars().take(col).fold(0, |v, c| v + cell_width(c, v, tabstop))
}

pub fn col_at(line: &str, target: usize, tabstop: usize) -> usize {
    let mut v = 0;
    for (i, c) in line.chars().enumerate() {
        let w = cell_width(c, v, tabstop);
        if v + w > target {
            return i;
        }
        v += w;
    }
    char_count(line)
}

pub fn expand(line: &str, tabstop: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut v = 0;
    for c in line.chars() {
        let w = cell_width(c, v, tabstop);
        match c {
            '\t' => out.extend(std::iter::repeat_n(' ', w)),
            c if c.is_control() => {
                out.push('^');
                out.push(caret(c));
            }
            c => out.push(c),
        }
        v += w;
    }
    out
}

pub(crate) fn caret(c: char) -> char {
    match c as u32 {
        n @ 0..=0x1f => char::from_u32(n + 0x40).unwrap_or('?'),
        _ => '?',
    }
}
