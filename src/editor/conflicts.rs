use super::Editor;
use crate::conflict::{self, Conflict};

impl Editor {
    /// The merge conflicts in the buffer, found again only when the text changes.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let key = (self.buffer_id, self.version);
        let mut cache = self.conflict_cache.borrow_mut();
        match &*cache {
            Some((k, found)) if *k == key => found.clone(),
            _ => {
                let found = conflict::find(self.buf.lines());
                *cache = Some((key, found.clone()));
                found
            }
        }
    }

    // ]x and [x: the row of the next or previous conflict's start, n of them on
    pub(super) fn next_conflict(&self, row: usize, forward: bool, n: usize) -> Option<usize> {
        let starts = self.conflicts().into_iter().map(|c| c.start);
        // like ]c: no wrapping, and a big count stops at the last one
        if forward {
            starts.filter(|&r| r > row).take(n).last()
        } else {
            starts.rev().filter(|&r| r < row).take(n).last()
        }
    }
}
