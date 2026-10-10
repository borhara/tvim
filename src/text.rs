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
