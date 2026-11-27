use super::{Editor, Input, Mode, cwd};
use crate::diff::Hunk;
use crate::git::Base;
use crate::gitmenu::{GitMenu, Want};

impl Editor {
    // the change under the cursor, and the lines it took the place of
    fn hunk_here(&self) -> Option<(Hunk, Vec<String>)> {
        let row = self.cursor.row;
        let h = self
            .change_hunks()
            .iter()
            .find(|h| h.new.contains(&row) || h.new.is_empty() && h.row() == row)
            .cloned()?;
        let old = self.git.with(|f| match &f.base {
            Base::Head(lines) => lines.get(h.old.clone()).map(<[String]>::to_vec),
            Base::Untracked => Some(Vec::new()),
            Base::Outside => self.saved_lines.get(h.old.clone()).map(<[String]>::to_vec),
        })?;
        Some((h, old))
    }

    pub(super) fn reset_hunk(&mut self) {
        let Some((h, old)) = self.hunk_here() else { return };
        self.touch();
        let mut lines = self.buf.lines().to_vec();
        lines.splice(h.new.clone(), old);
        self.buf.replace_all(lines);
        self.cursor = self.first_non_blank_pos(h.new.start.min(self.buf.last_row()));
        self.set_want();
        self.end_change();
    }

    // Space h p: the change under the cursor, as it was and as it is, by the cursor
    pub(super) fn preview_hunk(&mut self) {
        let Some((h, old)) = self.hunk_here() else {
            return self.info("no change here");
        };
        let was = old.iter().map(|l| format!("-{l}"));
        let is = self.buf.lines()[h.new].iter().map(|l| format!("+{l}"));
        let body: Vec<String> = was.chain(is).collect();
        self.hover = Some(format!("```diff\n{}\n```", body.join("\n")));
    }

    // Space h s: the change under the cursor goes in the index, as the buffer has it
    pub(super) fn stage_hunk(&mut self) {
        let Some(path) = self.file.path.clone() else {
            return self.error("no file to stage");
        };
        if self.git.with(|f| f.base == Base::Outside) {
            return self.error("not in a git repo");
        }
        let staged = crate::git::staged_lines(&path).unwrap_or_default();
        let lines = self.buf.lines();
        let row = self.cursor.row;
        let hunks = crate::diff::hunks(&staged, lines);
        let Some(h) = hunks
            .iter()
            .find(|h| h.new.contains(&row) || h.new.is_empty() && h.row() == row)
        else {
            return self.info("nothing here to stage");
        };
        let mut text = staged;
        text.splice(h.old.clone(), lines[h.new.clone()].iter().cloned());
        let mut text = text.join("\n");
        if !text.is_empty() && self.file.final_newline {
            text.push('\n');
        }
        match crate::git::stage_text(&path, &text) {
            Ok(()) => {
                self.info("staged");
                self.refresh_tree();
            }
            Err(why) => self.error(why),
        }
    }

    pub fn lens(&self) -> Option<String> {
        let quiet = self.mode != Mode::Normal
            || !self.pending.is_empty()
            || self.explorer_focused()
            || self.preview_focused()
            || self.finder.is_some()
            || self.menu.is_some()
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
        self.read_again();
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
