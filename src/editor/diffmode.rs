use std::rc::Rc;

use super::Editor;
use crate::diff::{self, Hunk};

impl Editor {
    // the buffer window `id` shows
    fn buffer_in(&self, id: usize) -> Option<usize> {
        match id == self.focus {
            true => Some(self.buffer_id),
            false => self.wins.iter().find(|w| w.id == id).map(|w| w.buffer),
        }
    }

    // the two windows compared, the first as the old side, while both are open
    fn diff_pair(&self) -> Option<(usize, usize)> {
        let mut open = self
            .diff_wins
            .iter()
            .filter(|&&id| self.wins.iter().any(|w| w.id == id));
        Some((*open.next()?, *open.next()?))
    }

    // the changes from the old side to the new, worked out again only when either changes
    fn diff_hunks(&self, (old, new): (usize, usize)) -> Option<Rc<[Hunk]>> {
        let (a, b) = (self.buffer_in(old)?, self.buffer_in(new)?);
        let (a, b) = (self.text_of(a), self.text_of(b));
        let key = (a.id, a.version, b.id, b.version);
        let mut cache = self.diff_cache.borrow_mut();
        if let Some((k, hunks)) = &*cache
            && *k == key
        {
            return Some(hunks.clone());
        }
        let hunks: Rc<[Hunk]> = diff::hunks(a.buf.lines(), b.buf.lines()).into();
        *cache = Some((key, hunks.clone()));
        Some(hunks)
    }

    /// For a window being compared: whether it's the old side, and the changes to the new.
    pub(crate) fn diff_view(&self, win: usize) -> Option<(bool, Rc<[Hunk]>)> {
        let pair = self.diff_pair()?;
        let old = match win {
            w if w == pair.0 => true,
            w if w == pair.1 => false,
            _ => return None,
        };
        Some((old, self.diff_hunks(pair)?))
    }

    // :diffthis
    pub(crate) fn diff_this(&mut self) {
        if !self.diff_wins.contains(&self.focus) {
            self.diff_wins.push(self.focus);
        }
        let open = self
            .diff_wins
            .iter()
            .filter(|&&id| self.wins.iter().any(|w| w.id == id))
            .count();
        if open < 2 {
            self.info("comparing this window: :diffthis in another to compare it with");
        }
    }

    // :diffoff, or :diffoff! for every window
    pub(crate) fn diff_off(&mut self, all: bool) {
        match all {
            true => self.diff_wins.clear(),
            false => self.diff_wins.retain(|&id| id != self.focus),
        }
    }
}
