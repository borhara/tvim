use std::rc::Rc;

use super::Editor;
use crate::fold;

impl Editor {
    // the closed folds of the buffer with id `id`, outermost only, each as its first and last
    // line, in order
    pub(crate) fn closed_folds(&self, id: usize) -> Rc<[(usize, usize)]> {
        if id == self.buffer_id {
            return self.folded();
        }
        match self.docs.iter().find(|d| d.id == id) {
            Some(d) if !d.closed.is_empty() => outermost(d.buf.lines(), &d.closed, self.options.tab_stop).into(),
            _ => Rc::new([]),
        }
    }

    // this buffer's closed folds, worked out again only when they or the text change
    pub(crate) fn folded(&self) -> Rc<[(usize, usize)]> {
        if self.closed.is_empty() {
            return Rc::new([]);
        }
        let mut cache = self.fold_cache.borrow_mut();
        match &*cache {
            Some((version, closed, folds)) if *version == self.version && *closed == self.closed => folds.clone(),
            _ => {
                let folds: Rc<[_]> = outermost(self.buf.lines(), &self.closed, self.options.tab_stop).into();
                *cache = Some((self.version, self.closed.clone(), folds.clone()));
                folds
            }
        }
    }

    // the closed fold line `row` is in, if it's in one
    pub(crate) fn fold_holding(&self, row: usize) -> Option<(usize, usize)> {
        self.folded()
            .iter()
            .copied()
            .find(|&(start, end)| (start..=end).contains(&row))
    }

    // zc, zo, za, zR and zM
    pub(super) fn fold_key(&mut self, c: char) {
        let ts = self.options.tab_stop;
        let holding = fold::holding(self.buf.lines(), self.cursor.row, ts);
        let closed = |e: &Self, start: usize| e.closed.contains(&start);
        match c {
            'R' => self.closed.clear(),
            'M' => self.closed = fold::all(self.buf.lines(), ts).into_iter().map(|(s, _)| s).collect(),
            // the innermost that's open, or with one closed already, the one around it
            'c' => match holding.iter().find(|&&(s, _)| !closed(self, s)) {
                Some(&(start, _)) => self.closed.push(start),
                None => return self.error("E490: No fold found"),
            },
            // the one that shows, which is the outermost closed
            'o' => match holding.iter().rev().find(|&&(s, _)| closed(self, s)) {
                Some(&(start, _)) => self.closed.retain(|&s| s != start),
                None if holding.is_empty() => return self.error("E490: No fold found"),
                None => {}
            },
            _ => match holding.iter().rev().find(|&&(s, _)| closed(self, s)) {
                Some(&(start, _)) => self.closed.retain(|&s| s != start),
                None => match holding.first() {
                    Some(&(start, _)) => self.closed.push(start),
                    None => return self.error("E490: No fold found"),
                },
            },
        }
        self.closed.sort_unstable();
        self.closed.dedup();
        // to the first line of the fold it's now in
        if let Some((start, _)) = self.fold_holding(self.cursor.row) {
            self.cursor = self.first_non_blank_pos(start);
            self.set_want();
        }
    }

    // moving up and down, and scrolling, lands on a closed fold's first line rather than
    // opening it, as vim's foldopen leaves them
    pub(super) fn onto_fold(&mut self) {
        if let Some((start, _)) = self.fold_holding(self.cursor.row) {
            self.cursor.row = start;
            self.clamp();
        }
    }

    // a closed fold the cursor has gone into, by a search or a mark, say, opens to show it
    pub(super) fn open_folds_at_cursor(&mut self) {
        let row = self.cursor.row;
        if self.fold_holding(row).is_none_or(|(start, _)| start == row) {
            return;
        }
        // every one around it, leaving one that starts on its line, which shows that line
        let around = fold::holding(self.buf.lines(), row, self.options.tab_stop);
        self.closed
            .retain(|&s| s == row || !around.iter().any(|&(a, _)| a == s));
    }

    // j and k: a closed fold is a line of its own, stepped over in one
    pub(super) fn fold_step(&self, row: usize, n: usize, down: bool) -> usize {
        let folds = self.folded();
        let last = self.buf.last_row();
        let mut row = row;
        for _ in 0..n {
            row = if down {
                let end = folds.iter().find(|&&(s, _)| s == row).map_or(row, |&(_, e)| e);
                if end >= last {
                    break;
                }
                end + 1
            } else {
                if row == 0 {
                    break;
                }
                let up = row - 1;
                folds
                    .iter()
                    .find(|&&(s, e)| (s..=e).contains(&up))
                    .map_or(up, |&(s, _)| s)
            };
        }
        row
    }

    // the closed folds' first lines move with the text, and those gone are forgotten
    pub(super) fn follow_folds(&mut self, old: &[String]) {
        if self.closed.is_empty() {
            return;
        }
        let hunks = crate::diff::hunks(old, self.buf.lines());
        self.closed = (self.closed.iter())
            .filter_map(|&row| crate::diff::follow(&hunks, row).ok())
            .collect();
        self.prune_folds();
    }

    // forgets closed folds that no longer start where they were
    pub(super) fn prune_folds(&mut self) {
        let ts = self.options.tab_stop;
        let lines = self.buf.lines();
        self.closed.retain(|&row| fold::fold_at(lines, row, ts).is_some());
    }

    pub fn fold_count(&self) -> usize {
        self.folded().len()
    }
}

// the folds starting at `closed` rows that aren't inside another of them
fn outermost(lines: &[String], closed: &[usize], tab_stop: usize) -> Vec<(usize, usize)> {
    let mut starts = closed.to_vec();
    starts.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for start in starts {
        if out.last().is_some_and(|&(_, end)| start <= end) {
            continue;
        }
        if let Some(end) = fold::fold_at(lines, start, tab_stop) {
            out.push((start, end));
        }
    }
    out
}
