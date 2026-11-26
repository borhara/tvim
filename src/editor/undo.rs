use std::collections::HashMap;

use super::{Editor, MAX_UNDO, Mode, Snapshot};
use crate::buffer::Pos;
use crate::diff;

impl Editor {
    pub(crate) fn touch(&mut self) {
        if !self.changing {
            self.undo.push(self.snapshot());
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.changing = true;
        }
        self.versions += 1;
        self.version = self.versions;
    }

    pub(crate) fn end_change(&mut self) {
        // a picture's empty buffer stays empty, whatever tried to change it
        if self.changing && self.file.image.is_some() {
            self.changing = false;
            if let Some(s) = self.undo.pop() {
                self.buf.replace_all(s.lines);
                self.version = s.version;
                self.cursor = Pos::default();
            }
            self.redo.clear();
            return self.info("images can't be changed");
        }
        if self.changing {
            self.changing = false;
            if self.undo.last().is_some_and(|s| s.lines == self.buf.lines()) {
                let s = self.undo.pop().expect("just checked");
                self.version = s.version;
            } else if let Some(s) = self.undo.last() {
                let before = Some(&s.marks);
                follow_marks(
                    &mut self.marks,
                    &mut self.last_visual,
                    &s.lines,
                    self.buf.lines(),
                    before,
                );
            }
        }
    }

    pub(super) fn undo(&mut self, count: usize) {
        for n in 0..count {
            let Some(s) = self.undo.pop() else {
                if n == 0 {
                    self.info("Already at oldest change");
                }
                return;
            };
            let now = self.snapshot();
            self.redo.push(now);
            self.restore(s);
        }
    }

    pub(super) fn redo(&mut self, count: usize) {
        for n in 0..count {
            let Some(s) = self.redo.pop() else {
                if n == 0 {
                    self.info("Already at newest change");
                }
                return;
            };
            let now = self.snapshot();
            self.undo.push(now);
            self.restore(s);
        }
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.buf.lines().to_vec(),
            cursor: self.cursor,
            version: self.version,
            marks: self.marks.clone(),
        }
    }

    pub(super) fn restore(&mut self, s: Snapshot) {
        let changed = (0..self.buf.len().max(s.lines.len()))
            .find(|&r| self.buf.lines().get(r) != s.lines.get(r))
            .unwrap_or(0);
        follow_marks(&mut self.marks, &mut self.last_visual, self.buf.lines(), &s.lines, None);
        // a mark the change took away with its line comes back with it
        for (k, p) in s.marks {
            self.marks.entry(k).or_insert(p);
        }
        self.buf.replace_all(s.lines);
        self.version = s.version;
        let row = changed.min(self.buf.last_row());
        self.cursor = if s.cursor.row == row {
            s.cursor
        } else {
            self.first_non_blank_pos(row)
        };
        self.mode = Mode::Normal;
    }
}

// marks keep to their lines as lines come and go above them; a letter's mark on a line
// removed goes with it, and the others, like '< and '>, move to the line in its place. Only
// those as they were `before` move, as one set since, like '^ at the end of an insert, is
// already where it belongs.
fn follow_marks<M>(
    marks: &mut HashMap<char, Pos>,
    visual: &mut Option<(Pos, Pos, M)>,
    old: &[String],
    new: &[String],
    before: Option<&HashMap<char, Pos>>,
) {
    if marks.is_empty() && visual.is_none() {
        return;
    }
    let hunks = diff::hunks(old, new);
    let last = new.len().saturating_sub(1);
    let follow = |p: Pos| match diff::follow(&hunks, p.row) {
        Ok(row) => Ok(Pos::new(row, p.col)),
        Err(row) => Err(Pos::new(row.min(last), 0)),
    };
    marks.retain(|&k, p| match follow(*p) {
        _ if before.is_some_and(|b| b.get(&k) != Some(p)) => true,
        Err(_) if k.is_ascii_lowercase() => false,
        Ok(to) | Err(to) => {
            *p = to;
            true
        }
    });
    if let Some((anchor, cursor, _)) = visual {
        *anchor = follow(*anchor).unwrap_or_else(|to| to);
        *cursor = follow(*cursor).unwrap_or_else(|to| to);
    }
}
