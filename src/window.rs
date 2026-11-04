use tinsel::app::Rect;

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

fn divide(r: Rect, across: bool, shares: &[usize]) -> Vec<Rect> {
    let n = shares.len();
    let gap = usize::from(across);
    let length = if across { r.width } else { r.height };
    let room = length.saturating_sub(gap * (n - 1));
    let total: usize = shares.iter().map(|s| (*s).max(1)).sum();
    let mut out = Vec::with_capacity(n);
    let (mut sum, mut end) = (0, 0);
    for (i, s) in shares.iter().enumerate() {
        sum += (*s).max(1);
        let start = end;
        end = if i + 1 == n { room } else { room * sum / total };
        let at = start + gap * i;
        out.push(if across {
            Rect::new(r.x + at, r.y, end - start, r.height)
        } else {
            Rect::new(r.x, r.y + at, r.width, end - start)
        });
    }
    out
}

impl Node {
    fn shares(kids: &[(Node, usize)]) -> Vec<usize> {
        kids.iter().map(|k| k.1).collect()
    }

    pub fn place(&self, r: Rect, out: &mut Vec<(usize, Rect)>) {
        match self {
            Node::Leaf(id) => out.push((*id, r)),
            Node::Split { across, kids } => {
                for (kid, rect) in kids.iter().zip(divide(r, *across, &Self::shares(kids))) {
                    kid.0.place(rect, out);
                }
            }
        }
    }

}
