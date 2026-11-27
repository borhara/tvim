use super::Editor;
use crate::conflict::{self, Conflict, Keep};

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

    // Space h o, t, b and n: the conflict under the cursor, down to what's kept of it
    pub(super) fn resolve_conflict(&mut self, keep: Keep) {
        let row = self.cursor.row;
        let Some(c) = self.conflicts().into_iter().find(|c| c.contains(row)) else {
            return self.info("no conflict here");
        };
        self.touch();
        let mut lines = self.buf.lines().to_vec();
        let kept = c.resolve(&lines, keep);
        lines.splice(c.start..=c.end, kept);
        if lines.is_empty() {
            lines.push(String::new());
        }
        self.buf.replace_all(lines);
        self.cursor = self.first_non_blank_pos(c.start.min(self.buf.last_row()));
        self.set_want();
        self.end_change();
    }
}
