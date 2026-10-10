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
