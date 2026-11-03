use crate::buffer::Pos;

// a title row and a line of text
pub(crate) const MIN_ROWS: usize = 2;
pub(crate) const MIN_COLS: usize = 8;

#[derive(Clone, Debug, Default)]
pub(crate) struct Win {
    pub id: usize,
    pub buffer: usize,
    pub cursor: Pos,
    pub want: usize,
    pub top: usize,
    pub left: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum Node {
    Leaf(usize),
    // across: side by side, else stacked; numbers are shares
    Split { across: bool, kids: Vec<(Node, usize)> },
}
