use crate::buffer::{Buffer, Pos};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
    pub linewise: bool,
}

pub fn object(b: &Buffer, p: Pos, kind: char, inner: bool, count: usize) -> Option<Span> {
    match kind {
        _ => None,
    }
}

pub fn is_object(kind: char) -> bool {
    "wW\"'`()b[]{}B<>p".contains(kind)
}
