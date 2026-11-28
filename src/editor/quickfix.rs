use super::Editor;
use crate::buffer::Pos;
use crate::quickfix::{Quickfix, Spot};

impl Editor {
    // keeps `spots` as the quickfix list, to go through with ]q and [q
    pub(crate) fn keep_quickfix(&mut self, title: &str, spots: Vec<Spot>) {
        if spots.is_empty() {
            return self.error("nothing to keep");
        }
        let n = spots.len();
        self.quickfix = Some(Quickfix::new(title, spots));
        let places = if n == 1 { "1 place" } else { &format!("{n} places") };
        self.info(format!("{places} in the quickfix list, ]q for the next"));
    }

    // runs the ex command `cmd` at each place in the quickfix list, or with `per_file` once in
    // each file it has places in, stopping at the first error
    pub(crate) fn quickfix_do(&mut self, cmd: &str, per_file: bool) {
        let Some(q) = self.quickfix.clone() else {
            return self.error("E42: No Errors");
        };
        if cmd.trim().is_empty() {
            return self.error("E471: Argument required");
        }
        let mut seen = Vec::new();
        let (back, mut ran) = (self.current, 0);
        // where each place is now, as runs before it put lines in or took them out
        let full: Vec<_> = q.spots.iter().map(|s| crate::paths::absolute(&s.path)).collect();
        let mut rows: Vec<usize> = q.spots.iter().map(|s| s.pos.row).collect();
        self.no_jumps = true;
        for (i, spot) in q.spots.iter().enumerate() {
            let path = &full[i];
            if per_file && seen.contains(path) {
                continue;
            }
            seen.push(path.clone());
            if !self.go_to(&spot.path, Pos::new(rows[i], spot.pos.col)) {
                break;
            }
            if let Some(q) = self.quickfix.as_mut() {
                q.at = Some(i);
            }
            let before = self.buf.lines().to_vec();
            self.ex(cmd);
            ran += 1;
            let same_file = self.file.path.as_deref().map(crate::paths::absolute).as_ref() == Some(path);
            if same_file && self.buf.lines() != before {
                let hunks = crate::diff::hunks(&before, self.buf.lines());
                for j in (i + 1..rows.len()).filter(|&j| full[j] == *path) {
                    rows[j] = crate::diff::follow(&hunks, rows[j]).unwrap_or_else(|r| r);
                }
            }
            if self.message.as_ref().is_some_and(|m| m.error) || self.should_quit() {
                break;
            }
        }
        self.no_jumps = false;
        let failed = self.message.as_ref().is_some_and(|m| m.error);
        if !failed {
            let files = seen.len();
            let others = files > 1 || self.current != back;
            let what = if per_file { "file" } else { "place" };
            let mut said = format!("ran at {ran} {what}{}", if ran == 1 { "" } else { "s" });
            if others && self.docs.iter().any(|d| d.modified()) {
                said.push_str(", :wa saves them all");
            }
            self.info(said);
        }
    }

    // lists the problems language servers see in every open file, this one's first, worst
    // first in each, and keeps them as the quickfix list
    pub(crate) fn list_problems(&mut self) {
        use crate::languages::Severity;
        let others = self.docs.iter().enumerate().filter(|&(i, _)| i != self.current);
        let files = std::iter::once((self.buffer_id, self.file.path.clone()))
            .chain(others.map(|(_, d)| (d.id, d.file.path.clone())))
            .filter_map(|(id, path)| Some((id, path?)));
        let mut spots = Vec::new();
        for (id, path) in files.collect::<Vec<_>>() {
            let mut found = self.problems(id).to_vec();
            found.sort_by_key(|p| (p.severity, p.start));
            spots.extend(found.into_iter().map(|p| {
                let what = match p.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                    Severity::Hint => "hint",
                };
                let first = p.message.lines().next().unwrap_or("");
                Spot {
                    path: path.clone(),
                    pos: p.start,
                    note: format!("{what}: {first}"),
                }
            }));
        }
        if spots.is_empty() {
            return self.info("no problems");
        }
        self.quickfix = Some(Quickfix::new("problems", spots));
        self.open_quickfix();
    }

    // lists the quickfix list in the picker, in its order
    pub(crate) fn open_quickfix(&mut self) {
        let Some(q) = self.quickfix.clone() else {
            return self.error("E42: No Errors");
        };
        self.list_spots(&q.title, "", &q.spots, &Default::default());
    }

    // the place chosen in the picker becomes the one gone to last in the quickfix list, if
    // it's one of its places
    pub(super) fn chose_place(&mut self, path: &std::path::Path, pos: crate::buffer::Pos) {
        let path = crate::paths::absolute(path);
        if let Some(q) = self.quickfix.as_mut() {
            if let Some(i) = q
                .spots
                .iter()
                .position(|s| s.pos == pos && crate::paths::absolute(&s.path) == path)
            {
                q.at = Some(i);
            }
        }
    }

    pub fn quickfix(&self) -> Option<&Quickfix> {
        self.quickfix.as_ref()
    }

    // `by` places on in the quickfix list, or back for less than none
    pub(crate) fn step_quickfix(&mut self, by: isize) {
        let Some(q) = self.quickfix.as_mut() else {
            return self.error("E42: No Errors");
        };
        match q.step(by) {
            Some(i) => self.go_to_quickfix(i),
            None => self.error("E553: No more items"),
        }
    }

    // to the nth place in the quickfix list, counting from 0, or the last for None
    pub(crate) fn quickfix_to(&mut self, nth: Option<usize>) {
        let Some(q) = self.quickfix.as_mut() else {
            return self.error("E42: No Errors");
        };
        let last = q.spots.len() - 1;
        let at = nth.unwrap_or(last).min(last);
        q.at = Some(at);
        self.go_to_quickfix(at);
    }

    fn go_to_quickfix(&mut self, i: usize) {
        let Some(q) = &self.quickfix else { return };
        let (n, spot) = (q.spots.len(), q.spots[i].clone());
        if self.go_to(&spot.path, spot.pos) {
            self.info(format!("({} of {n}) {}", i + 1, spot.note));
        }
    }
}
