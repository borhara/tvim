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
