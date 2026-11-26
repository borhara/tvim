use super::{Editor, Input, Mode, cwd};
use crate::git::Base;
use crate::gitmenu::{GitMenu, Want};

impl Editor {
    pub(super) fn reset_hunk(&mut self) {
        let row = self.cursor.row;
        let Some(h) = self
            .change_hunks()
            .iter()
            .find(|h| h.new.contains(&row) || h.new.is_empty() && h.row() == row)
            .cloned()
        else {
            return;
        };
        let old = self.git.with(|f| match &f.base {
            Base::Head(lines) => lines.get(h.old.clone()).map(<[String]>::to_vec),
            Base::Untracked => Some(Vec::new()),
            Base::Outside => self.saved_lines.get(h.old.clone()).map(<[String]>::to_vec),
        });
        let Some(old) = old else { return };
        self.touch();
        let mut lines = self.buf.lines().to_vec();
        lines.splice(h.new.clone(), old);
        self.buf.replace_all(lines);
        self.cursor = self.first_non_blank_pos(h.new.start.min(self.buf.last_row()));
        self.set_want();
        self.end_change();
    }

    pub fn lens(&self) -> Option<String> {
        let quiet = self.mode != Mode::Normal
            || !self.pending.is_empty()
            || self.explorer_focused()
            || self.preview_focused()
            || self.finder.is_some()
            || self.grep.is_some()
            || self.git_menu.is_some()
            || self.is_blank_slate();
        if !self.options.gitlens || quiet {
            return None;
        }
        let path = self.file.path.as_deref()?;
        let row = self.cursor.row;
        let head = self.git.with(|f| match &f.base {
            Base::Head(_) => f.head.clone().map(Some),
            Base::Untracked => Some(None),
            Base::Outside => None,
        })?;
        let Some(head) = head else {
            return Some("not committed yet".into());
        };
        let mut old = row;
        for h in self.change_hunks().iter() {
            if h.new.contains(&row) {
                return Some("not committed yet".into());
            }
            if h.new.end <= row {
                old = old + h.old.len() - h.new.len();
            }
        }
        let blame = self.blamer.get(path, &head)?;
        let commit = blame.line(old)?;
        Some(crate::blame::describe(commit, blame.me.as_deref(), crate::time::now()))
    }

    pub fn wait_for_blame(&self) {
        self.git.wait();
        let _ = self.lens();
        self.blamer.wait();
    }

    pub fn git_menu(&self) -> Option<&GitMenu> {
        self.git_menu.as_ref()
    }

    pub fn open_git_menu(&mut self, command: Option<&str>) {
        self.reset_for_picker();
        let start = match &self.file.path {
            Some(_) => self.file_dir(),
            None => cwd(),
        };
        let history = std::mem::take(&mut self.git_history);
        self.git_menu = Some(GitMenu::open(&start, history, command.map(str::to_string)));
    }

    pub(super) fn close_git_menu(&mut self) {
        if let Some(m) = self.git_menu.take() {
            self.git_history = m.history;
        }
    }

    pub fn wait_for_git_menu(&mut self) {
        crate::sync::wait_until(|| {
            self.tick();
            !self.git_menu.as_ref().is_some_and(GitMenu::working)
        });
        self.git.wait();
    }

    pub(super) fn git_menu_tick(&mut self) {
        let Some(m) = self.git_menu.as_mut() else { return };
        let changed = m.tick();
        if let Some(why) = m.failed().map(str::to_string) {
            self.close_git_menu();
            self.error(why);
            return;
        }
        if changed {
            self.after_git();
        }
        let early = match self.git_menu.as_mut() {
            Some(m) if m.ready() => std::mem::take(&mut m.early),
            _ => Vec::new(),
        };
        for k in early {
            if self.git_menu.is_some() {
                self.git_menu_input(Input::Key(k));
            }
        }
    }

    // the repo changed under us: redraw the gutter and branch, and reload a clean buffer
    pub(super) fn after_git(&mut self) {
        self.git.probe(self.file.path.as_deref());
        self.refresh_tree();
        if self.is_modified() {
            return;
        }
        let Some(path) = self.file.path.clone() else { return };
        // a picture isn't text to compare, so is read again whenever it's still there
        if self.file.image.is_none() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                return;
            };
            if text == self.contents(None) {
                return;
            }
        } else if !path.exists() {
            return;
        }
        let (cursor, top) = (self.cursor, self.top.get());
        if self.load(&path).is_ok() {
            self.cursor = cursor;
            self.top.set(top);
            self.clamp();
            self.message = None;
        }
    }

    pub(super) fn git_menu_input(&mut self, input: Input) {
        let unsaved = self.is_modified()
            || self
                .docs
                .iter()
                .enumerate()
                .any(|(i, d)| i != self.current && d.modified());
        let Some(m) = self.git_menu.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return m.paste(&text),
            Input::Key(k) => k,
        };
        match m.key(key, unsaved) {
            Want::Nothing => {}
            Want::Close => self.close_git_menu(),
            Want::Open(path) => {
                if self.open_or_report(&path) {
                    self.close_git_menu();
                }
            }
        }
    }
}
