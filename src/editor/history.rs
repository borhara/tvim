use super::{Choosing, Editor, MAX_UNDO, Moment};
use crate::menu::{Entry, Menu};

// as many of the latest as the list offers
const LISTED: usize = 100;

impl Editor {
    // after a change: the text as it left it, kept with what came before
    pub(super) fn remember(&mut self) {
        if self.revisiting {
            return;
        }
        // what the undo history had before this session, as far back as it goes
        if self.history.is_empty() {
            self.history = (self.undo.iter())
                .map(|s| Moment {
                    lines: s.lines.clone(),
                    cursor: s.cursor,
                    when: None,
                })
                .collect();
        }
        self.history.push(Moment {
            lines: self.buf.lines().to_vec(),
            cursor: self.cursor,
            when: Some(crate::time::now()),
        });
        if self.history.len() > MAX_UNDO {
            self.history.remove(0);
        }
    }

    // Space u or :undolist: every state the text has been in, the newest first, to go back to
    pub(crate) fn list_history(&mut self) {
        if self.history.len() < 2 {
            return self.info("no changes yet");
        }
        let now = crate::time::now();
        let current = self.history.iter().rposition(|m| m.lines == self.buf.lines());
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
                Entry {
                    label,
                    hint: i
                        .checked_sub(1)
                        .map_or_else(String::new, |p| change(&self.history[p].lines, &m.lines)),
                    disabled: None,
                }
            })
            .collect();
        let mut menu = Menu::new("undo history", entries);
        menu.selected = current.and_then(|c| picks.iter().position(|&i| i == c)).unwrap_or(0);
        self.menu = Some((menu, Choosing::History(picks)));
    }

    // the text back as it was at moment `i`, as a change of its own that u takes back
    pub(super) fn revisit(&mut self, i: usize) {
        let Some(m) = self.history.get(i).cloned() else { return };
        if m.lines == self.buf.lines() || self.refuses_change() {
            return;
        }
        self.revisiting = true;
        self.touch();
        self.buf.replace_all(m.lines);
        self.cursor = m.cursor;
        self.clamp();
        self.end_change();
        self.revisiting = false;
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
