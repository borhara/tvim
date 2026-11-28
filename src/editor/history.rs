use super::{Choosing, Editor, MAX_UNDO, Moment};
use crate::menu::{Entry, Menu};

// as many of the latest as the list offers
const LISTED: usize = 100;

impl Editor {
    // what the undo history had before this session, as far back as it goes, and with `now`
    // the text as it is
    fn seed_history(&mut self, now: bool) {
        let mut from = None;
        for s in &self.undo {
            self.history.push_back(Moment {
                lines: s.lines.clone(),
                cursor: s.cursor,
                when: None,
                version: s.version,
                from,
            });
            from = Some(s.version);
        }
        if now && from.is_some() {
            self.history.push_back(Moment {
                lines: self.buf.lines().into(),
                cursor: self.cursor,
                when: None,
                version: self.version,
                from,
            });
        }
    }

    // after a change: the text as it left it, and the version it was made to
    pub(super) fn remember(&mut self) {
        if self.history.is_empty() {
            self.seed_history(false);
        }
        self.history.push_back(Moment {
            lines: self.buf.lines().into(),
            cursor: self.cursor,
            when: Some(crate::time::now()),
            version: self.version,
            from: self.undo.last().map(|s| s.version),
        });
        while self.history.len() > MAX_UNDO {
            self.history.pop_front();
        }
    }

    // Space u or :undolist: every state the text has been in, the newest first, to go back to
    pub(crate) fn list_history(&mut self) {
        if self.history.is_empty() {
            self.seed_history(true);
        }
        if self.history.len() < 2 {
            return self.info("no changes yet");
        }
        let now = crate::time::now();
        let current = self.history.iter().rposition(|m| m.version == self.version);
        let picks: Vec<usize> = (0..self.history.len()).rev().take(LISTED).collect();
        let entries = (picks.iter())
            .map(|&i| {
                let m = &self.history[i];
                let when = match m.when {
                    Some(t) => tinsel::format::ago_long(std::time::Duration::from_secs((now - t).max(0) as u64)),
                    None => "before".into(),
                };
                let label = if current == Some(i) {
                    format!("{when} · now")
                } else {
                    when
                };
                let made_to = m.from.and_then(|v| self.history.iter().rfind(|o| o.version == v));
                Entry {
                    label,
                    hint: made_to.map_or_else(String::new, |o| change(&o.lines, &m.lines)),
                    disabled: None,
                }
            })
            .collect();
        let mut menu = Menu::new("undo history", entries);
        menu.selected = current.and_then(|c| picks.iter().position(|&i| i == c)).unwrap_or(0);
        self.menu = Some((menu, Choosing::History(picks)));
    }

    // the text back as it was at moment `i`, as a change of its own that u takes back and
    // the history keeps
    pub(super) fn revisit(&mut self, i: usize) {
        let Some(m) = self.history.get(i).cloned() else { return };
        if *m.lines == *self.buf.lines() || self.refuses_change() {
            return;
        }
        self.touch();
        self.buf.replace_all(m.lines.to_vec());
        self.cursor = m.cursor;
        self.clamp();
        self.end_change();
    }
}

// how many lines a change put in and took out, by the part of the text it changed
fn change(old: &[String], new: &[String]) -> String {
    let (start, end) = crate::diff::common_ends(old, new);
    let (gone, added) = (old.len() - start - end, new.len() - start - end);
    match (added, gone) {
        (0, 0) => String::new(),
        (a, 0) => format!("+{a}"),
        (0, g) => format!("−{g}"),
        (a, g) => format!("+{a} −{g}"),
    }
}
