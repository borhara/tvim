use tinsel::app::Rect;

use crate::buffer::Pos;
use crate::editor::{Editor, Mode};

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

    pub fn leaves(&self, out: &mut Vec<usize>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { kids, .. } => kids.iter().for_each(|k| k.0.leaves(out)),
        }
    }

    fn first(&self) -> usize {
        match self {
            Node::Leaf(id) => *id,
            Node::Split { kids, .. } => kids[0].0.first(),
        }
    }

    fn last(&self) -> usize {
        match self {
            Node::Leaf(id) => *id,
            Node::Split { kids, .. } => kids[kids.len() - 1].0.last(),
        }
    }

    fn contains(&self, id: usize) -> bool {
        match self {
            Node::Leaf(n) => *n == id,
            Node::Split { kids, .. } => kids.iter().any(|k| k.0.contains(id)),
        }
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

    pub(crate) fn split(&mut self, r: Rect, at: usize, new: usize, across: bool, after: bool) {
        match self {
            Node::Leaf(id) if *id == at => {
                let mut kids = vec![(Node::Leaf(new), 1), (Node::Leaf(at), 1)];
                if after {
                    kids.reverse();
                }
                *self = Node::Split { across, kids };
            }
            Node::Leaf(_) => {}
            Node::Split { across: a, kids } => {
                let rects = divide(r, *a, &Self::shares(kids));
                let Some(i) = kids.iter().position(|k| k.0.contains(at)) else {
                    return;
                };
                if *a == across && matches!(kids[i].0, Node::Leaf(_)) {
                    for (k, rect) in kids.iter_mut().zip(&rects) {
                        k.1 = if across { rect.width } else { rect.height };
                    }
                    let size = kids[i].1;
                    kids[i].1 = size - size / 2;
                    kids.insert(i + usize::from(after), (Node::Leaf(new), (size / 2).max(1)));
                } else {
                    kids[i].0.split(rects[i], at, new, across, after);
                }
            }
        }
    }

    pub(crate) fn remove(&mut self, id: usize) -> Option<usize> {
        let Node::Split { across, kids } = self else {
            return None;
        };
        let i = kids.iter().position(|k| k.0.contains(id))?;
        let next = if matches!(kids[i].0, Node::Leaf(_)) {
            let (_, share) = kids.remove(i);
            let (j, next) = if i < kids.len() {
                (i, kids[i].0.first())
            } else {
                (i - 1, kids[i - 1].0.last())
            };
            kids[j].1 += share;
            next
        } else {
            let next = kids[i].0.remove(id)?;
            // a kid split the same way joins this one
            let merge = match &kids[i].0 {
                Node::Split { across: a, kids: inner } if *a == *across => Some(inner.clone()),
                _ => None,
            };
            if let Some(inner) = merge {
                let share = kids[i].1;
                let total: usize = inner.iter().map(|k| k.1.max(1)).sum();
                let parts: Vec<(Node, usize)> = inner
                    .into_iter()
                    .map(|(n, s)| (n, (share * s.max(1) / total).max(1)))
                    .collect();
                kids.splice(i..=i, parts);
            }
            next
        };
        if kids.len() == 1 {
            *self = kids.remove(0).0;
        }
        Some(next)
    }

}

impl Editor {
    pub fn window_count(&self) -> usize {
        self.wins.len()
    }

    pub fn window_index(&self) -> usize {
        self.order().iter().position(|&id| id == self.focus).unwrap_or(0)
    }

    fn order(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.layout.leaves(&mut out);
        out
    }

    pub(crate) fn set_region(&self, width: usize, height: usize) {
        self.region.set(Some((width, height)));
    }

    pub(crate) fn region(&self) -> Rect {
        let (w, h) = self.region.get().unwrap_or_else(|| {
            let (w, h) = self.screen.get();
            (w, h.saturating_sub(2))
        });
        Rect::new(0, 0, w, h)
    }

    pub(crate) fn window_rects(&self, area: Rect) -> Vec<(usize, Rect)> {
        let mut out = Vec::new();
        self.layout.place(area, &mut out);
        out
    }

    pub(crate) fn fit_screen(&self) {
        if self.region.get().is_none() {
            let r = self.region();
            self.set_region(r.width, r.height);
        }
        let rects = self.window_rects(self.region());
        if let Some((_, r)) = rects.iter().find(|(id, _)| *id == self.focus) {
            let title = usize::from(rects.len() > 1);
            self.set_screen(r.width, r.height.saturating_sub(title) + 2);
        }
    }

    pub(crate) fn save_window(&mut self) {
        let (cursor, want, top, left, buffer) =
            (self.cursor, self.want, self.top.get(), self.left.get(), self.buffer_id);
        if let Some(w) = self.wins.iter_mut().find(|w| w.id == self.focus) {
            *w = Win {
                id: w.id,
                buffer,
                cursor,
                want,
                top,
                left,
            };
        }
    }

    fn enter_window(&mut self, id: usize) {
        let Some(w) = self.wins.iter().find(|w| w.id == id).cloned() else {
            return;
        };
        self.focus = id;
        if w.buffer != self.buffer_id {
        }
        self.mode = Mode::Normal;
        self.completion = None;
        self.cursor = w.cursor;
        self.want = w.want;
        self.top.set(w.top);
        self.left.set(w.left);
        self.clamp();
        self.fit_screen();
    }

    pub(crate) fn focus_window(&mut self, id: usize) {
        if id == self.focus || !self.wins.iter().any(|w| w.id == id) {
            return;
        }
        self.save_window();
        self.enter_window(id);
    }

}
