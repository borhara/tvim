use tinsel::app::{Key, Rect};

use crate::buffer::Pos;
use crate::editor::{Editor, Mode, View};
use crate::text::vcol;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
    Up,
    Down,
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

    fn min_len(&self, across: bool) -> usize {
        let min = if across { MIN_COLS } else { MIN_ROWS };
        match self {
            Node::Leaf(_) => min,
            Node::Split { across: a, kids } if *a == across => {
                kids.iter().map(|k| k.0.min_len(across)).sum::<usize>() + usize::from(across) * (kids.len() - 1)
            }
            Node::Split { kids, .. } => kids.iter().map(|k| k.0.min_len(across)).max().unwrap_or(min),
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

    fn equalize(&mut self) {
        if let Node::Split { kids, .. } = self {
            for k in kids {
                k.1 = 1;
                k.0.equalize();
            }
        }
    }

    fn resize(&mut self, r: Rect, id: usize, across: bool, by: isize) -> bool {
        let Node::Split { across: a, kids } = self else {
            return false;
        };
        let rects = divide(r, *a, &Self::shares(kids));
        let Some(i) = kids.iter().position(|k| k.0.contains(id)) else {
            return false;
        };
        if kids[i].0.resize(rects[i], id, across, by) {
            return true;
        }
        if *a != across || kids.len() < 2 {
            return false;
        }
        let len = |r: &Rect| if across { r.width } else { r.height };
        let mut sizes: Vec<usize> = rects.iter().map(len).collect();
        let spare = |j: usize, sizes: &[usize]| sizes[j].saturating_sub(kids[j].0.min_len(across));
        if by > 0 {
            let mut want = by as usize;
            for j in (i + 1..kids.len()).chain((0..i).rev()) {
                let give = spare(j, &sizes).min(want);
                sizes[j] -= give;
                sizes[i] += give;
                want -= give;
            }
        } else {
            let take = by.unsigned_abs().min(spare(i, &sizes));
            let j = if i + 1 < kids.len() { i + 1 } else { i - 1 };
            sizes[i] -= take;
            sizes[j] += take;
        }
        for (k, s) in kids.iter_mut().zip(sizes) {
            k.1 = s.max(1);
        }
        true
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

    pub(crate) fn views(&self, area: Rect) -> Vec<View<'_>> {
        self.window_rects(area)
            .into_iter()
            .filter_map(|(id, rect)| {
                if id == self.focus {
                    return Some(View {
                        id,
                        rect,
                        focused: true,
                        cursor: self.cursor,
                        top: self.top.get(),
                        left: self.left.get(),
                        text: self.shown_text(),
                    });
                }
                let w = self.wins.iter().find(|w| w.id == id)?;
                let text = self.text_of(w.buffer);
                let last = text.buf.last_row();
                Some(View {
                    id,
                    rect,
                    focused: false,
                    cursor: Pos::new(w.cursor.row.min(last), w.cursor.col),
                    top: w.top.min(last),
                    left: w.left,
                    text,
                })
            })
            .collect()
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
        self.leave_visual_if_any();
        self.focus = id;
        if w.buffer != self.buffer_id {
            if let Some(i) = self.buffer_at(w.buffer) {
                self.no_jumps = true;
                self.no_jumps = false;
            }
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
        self.sync_windows();
        self.save_window();
        self.enter_window(id);
    }

    pub(crate) fn split_window(&mut self, across: bool, fresh: bool) -> bool {
        self.sync_windows();
        let area = self.region();
        let Some((_, r)) = self.window_rects(area).into_iter().find(|(id, _)| *id == self.focus) else {
            return false;
        };
        let (len, need) = if across {
            (r.width, MIN_COLS * 2 + 1)
        } else {
            (r.height, MIN_ROWS * 2)
        };
        if len < need {
            self.error("E36: Not enough room");
            return false;
        }
        self.save_window();
        let id = self.next_win;
        self.next_win += 1;
        let Some(copy) = self.wins.iter().find(|w| w.id == self.focus).cloned() else {
            return false;
        };
        self.wins.push(Win { id, ..copy });
        self.layout.split(area, self.focus, id, across, false);
        self.focus = id;
        if fresh {
        }
        self.fit_screen();
        self.sync_windows();
        true
    }

    pub(crate) fn close_window(&mut self) -> bool {
        if self.wins.len() < 2 {
            return false;
        }
        self.sync_windows();
        let gone = self.focus;
        let Some(next) = self.layout.remove(gone) else {
            return false;
        };
        self.wins.retain(|w| w.id != gone);
        self.previews.remove(&gone);
        self.enter_window(next);
        true
    }

    pub(crate) fn only_window(&mut self) {
        self.wins.retain(|w| w.id == self.focus);
        self.layout = Node::Leaf(self.focus);
        self.shadow = None;
        self.fit_screen();
    }

    fn cycle_window(&mut self, by: isize) {
        let order = self.order();
        let n = order.len() as isize;
        let at = self.window_index() as isize;
        self.focus_window(order[(at + by).rem_euclid(n) as usize]);
    }

    fn focus_toward(&mut self, dir: Dir) {
        let rects = self.window_rects(self.region());
        let Some(&(_, here)) = rects.iter().find(|(id, _)| *id == self.focus) else {
            return;
        };
        let title = usize::from(rects.len() > 1);
        let y = here.y + title + self.cursor.row.saturating_sub(self.top.get());
        let col = vcol(self.buf.line(self.cursor.row), self.cursor.col, self.options.tab_stop);
        let x = here.x + self.gutter() + col.saturating_sub(self.left.get());
        let mut best: Option<((usize, usize), usize)> = None;
        for &(id, r) in &rects {
            let (ahead, gap, from, to, at) = match dir {
                Dir::Left => (
                    r.x + r.width <= here.x,
                    here.x.saturating_sub(r.x + r.width),
                    r.y,
                    r.y + r.height,
                    y,
                ),
                Dir::Right => (
                    r.x >= here.x + here.width,
                    r.x.saturating_sub(here.x + here.width),
                    r.y,
                    r.y + r.height,
                    y,
                ),
                Dir::Up => (
                    r.y + r.height <= here.y,
                    here.y.saturating_sub(r.y + r.height),
                    r.x,
                    r.x + r.width,
                    x,
                ),
                Dir::Down => (
                    r.y >= here.y + here.height,
                    r.y.saturating_sub(here.y + here.height),
                    r.x,
                    r.x + r.width,
                    x,
                ),
            };
            let (lo, hi) = match dir {
                Dir::Left | Dir::Right => (here.y, here.y + here.height),
                Dir::Up | Dir::Down => (here.x, here.x + here.width),
            };
            if id == self.focus || !ahead || to <= lo || from >= hi {
                continue;
            }
            let miss = if at < from {
                from - at
            } else {
                at.saturating_sub(to - 1)
            };
            if best.is_none_or(|(b, _)| (gap, miss) < b) {
                best = Some(((gap, miss), id));
            }
        }
        if let Some((_, id)) = best {
            self.focus_window(id);
        }
    }

    fn resize_window(&mut self, across: bool, by: isize) {
        let area = self.region();
        self.layout.resize(area, self.focus, across, by);
        self.fit_screen();
    }

    pub(crate) fn equalize_windows(&mut self) {
        self.layout.equalize();
        self.fit_screen();
    }

    pub(crate) fn window_key(&mut self, key: Key, count: Option<usize>) {
        let n = count.unwrap_or(1) as isize;
        match key {
            Key::Char('s' | 'S') | Key::Ctrl('s') => {
                self.split_window(false, false);
            }
            Key::Char('v') | Key::Ctrl('v') => {
                self.split_window(true, false);
            }
            Key::Char('n') | Key::Ctrl('n') => {
                self.split_window(false, true);
            }
            Key::Char('h') | Key::Ctrl('h') | Key::Backspace | Key::Left => self.focus_toward(Dir::Left),
            Key::Char('l') | Key::Ctrl('l') | Key::Right => self.focus_toward(Dir::Right),
            Key::Char('k') | Key::Ctrl('k') | Key::Up => self.focus_toward(Dir::Up),
            Key::Char('j') | Key::Ctrl('j') | Key::Down => self.focus_toward(Dir::Down),
            Key::Char('w') | Key::Ctrl('w') => match count {
                Some(k) => {
                    let order = self.order();
                    if let Some(&id) = order.get(k.saturating_sub(1)).or(order.last()) {
                        self.focus_window(id);
                    }
                }
                None => self.cycle_window(1),
            },
            Key::Char('W') => self.cycle_window(-1),
            Key::Char('q') | Key::Ctrl('q') => self.ex("quit"),
            Key::Char('c') => self.ex("close"),
            Key::Char('o') | Key::Ctrl('o') => self.ex("only"),
            Key::Char('=') => self.equalize_windows(),
            Key::Char('+') => self.resize_window(false, n),
            Key::Char('-') => self.resize_window(false, -n),
            Key::Char('>') => self.resize_window(true, n),
            Key::Char('<') => self.resize_window(true, -n),
            _ => {}
        }
    }

    pub(crate) fn sync_windows(&mut self) {
        let id = self.buffer_id;
        if !self.wins.iter().any(|w| w.id != self.focus && w.buffer == id) {
            self.shadow = None;
            return;
        }
        let new = self.buf.lines();
        if let Some((_, old)) = self.shadow.as_ref().filter(|s| s.0 == id) {
            if old.as_slice() == new {
                return;
            }
            let (same_start, same_end) = crate::diff::common_ends(old, new);
            let (old_end, new_end) = (old.len() - same_end, new.len() - same_end);
            let last = new.len() - 1;
            let shift = |row: usize| {
                let row = if row < same_start {
                    row
                } else if row >= old_end {
                    row + new_end - old_end
                } else {
                    row.min(new_end.saturating_sub(1)).max(same_start)
                };
                row.min(last)
            };
            for w in self.wins.iter_mut().filter(|w| w.id != self.focus && w.buffer == id) {
                w.cursor.row = shift(w.cursor.row);
                w.top = shift(w.top);
            }
        }
        self.shadow = Some((id, new.to_vec()));
    }
}
