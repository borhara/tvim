use std::path::{Path, PathBuf};

use tinsel::app::Key;

use super::{Editor, Input, Mode, cwd, dir_of, listed};
use crate::buffer::Pos;
use crate::explorer::{Action, Explorer};
use crate::finder::{Finder, Pick};
use crate::grep::{Grep, Jump};

impl Editor {
    pub fn grep_picker(&self) -> Option<&Grep> {
        self.grep.as_ref()
    }

    pub fn wait_for_grep(&mut self) {
        if let Some(g) = self.grep.as_mut() {
            g.wait();
        }
    }

    pub fn grep(&mut self, query: Option<&str>) {
        let root = cwd();
        self.grep_in(&root, query.unwrap_or(""));
    }

    pub fn grep_in(&mut self, root: &Path, query: &str) {
        self.reset_for_picker();
        self.grep = Some(Grep::new(root, query));
    }

    pub(super) fn grep_input(&mut self, input: Input) {
        let Some(g) = self.grep.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return g.paste(&text),
            Input::Key(k) => k,
        };
        self.message = None;
        match g.key(key) {
            Jump::Nothing => {}
            Jump::Close => self.grep = None,
            Jump::Keep(title, spots) => {
                self.grep = None;
                self.keep_quickfix(&title, spots);
            }
            Jump::Open { path, row, col, query } => {
                self.chose_place(&path, Pos::new(row, col));
                if self.go_to(&path, Pos::new(row, col)) {
                    self.grep = None;
                    self.last_search = Some(crate::regex::escape(&query));
                    self.search_backward = false;
                    self.search_shown = true;
                }
            }
        }
    }

    pub fn pick_buffer_list(&mut self) {
        let root = cwd();
        let names: Vec<String> = self
            .buffers()
            .into_iter()
            .filter_map(|b| b.path)
            .map(|p| listed(&crate::paths::canonical(&p).unwrap_or(p)))
            .collect();
        self.pending.clear();
        self.finder = Some(Finder::from_list(&root, names, "open files"));
    }

    pub fn finder(&self) -> Option<&Finder> {
        self.finder.as_ref()
    }

    pub fn find_files(&mut self, query: Option<&str>) {
        let root = cwd();
        self.find_files_in(&root, query.unwrap_or(""));
    }

    pub fn find_files_in(&mut self, root: &Path, query: &str) {
        self.reset_for_picker();
        self.finder = Some(Finder::new(root, query));
    }

    pub(super) fn finder_input(&mut self, input: Input) {
        let Some(f) = self.finder.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return f.paste(&text),
            Input::Key(k) => k,
        };
        self.message = None;
        match f.key(key) {
            Pick::Nothing => {}
            Pick::Close => self.finder = None,
            Pick::Open(path) => {
                if self.open_or_report(&path) {
                    self.finder = None;
                }
            }
        }
    }

    pub fn explorer(&self) -> Option<&Explorer> {
        self.explorer.as_ref()
    }

    pub fn explorer_focused(&self) -> bool {
        self.explorer.is_some() && self.explorer_focused && !self.zen()
    }

    pub fn zen(&self) -> bool {
        self.zen.is_some()
    }

    pub(crate) fn leave_zen(&mut self) {
        self.zen = None;
    }

    pub fn toggle_zen(&mut self) {
        match self.zen.take() {
            Some(focused) => self.explorer_focused = focused,
            None => {
                self.zen = Some(self.explorer_focused);
                self.explorer_focused = false;
            }
        }
    }

    pub(crate) fn file_dir(&self) -> PathBuf {
        self.file
            .path
            .as_deref()
            .map_or_else(|| PathBuf::from("."), |p| dir_of(p).to_path_buf())
    }

    // the working folder when the file is inside it, so its tree shows where the file sits
    pub(super) fn sidebar_root(&self) -> PathBuf {
        let dir = self.file_dir();
        let cwd = std::env::current_dir().and_then(|d| crate::paths::canonical(&d)).ok();
        match (cwd, crate::paths::canonical(&dir).ok()) {
            (Some(cwd), Some(d)) if d.starts_with(&cwd) => cwd,
            _ => dir,
        }
    }

    pub(super) fn current_file(&self) -> Option<PathBuf> {
        self.file.path.as_ref().and_then(|p| crate::paths::canonical(p).ok())
    }

    // keeps the open file showing in the sidebar, or moves the sidebar to its folder
    pub(super) fn follow_file(&mut self) {
        let Some(path) = self.current_file() else { return };
        let Some(x) = self.explorer.as_mut() else { return };
        if !x.reveal(&path) {
            let dir = self.file_dir();
            let focused = self.explorer_focused;
            self.explore(&dir);
            self.explorer_focused = focused;
        }
    }

    pub fn toggle_explorer_focus(&mut self) {
        self.leave_zen();
        if self.explorer.is_none() {
            let dir = self.sidebar_root();
            self.explore(&dir);
        } else {
            self.explorer_focused = !self.explorer_focused;
        }
    }

    pub fn toggle_explorer(&mut self) {
        self.leave_zen();
        if self.explorer.take().is_none() {
            let dir = self.sidebar_root();
            self.explore(&dir);
        } else {
            self.explorer_focused = false;
        }
    }

    pub fn explore(&mut self, dir: &Path) {
        let current = self.current_file();
        match Explorer::open(dir, current.as_deref()) {
            Ok(x) => {
                self.explorer = Some(x);
                self.explorer_focused = true;
                self.mode = Mode::Normal;
                self.pending.clear();
                self.message = None;
                self.refresh_tree();
            }
            Err(e) => self.error(format!("\"{}\" {e}", dir.display())),
        }
    }

    pub fn explorer_rows(&self) -> usize {
        // less the sidebar's heading and the gap under it
        self.text_rows().saturating_sub(2).max(1)
    }

    pub(super) fn explorer_input(&mut self, input: Input) {
        let rows = self.explorer_rows();
        let Some(x) = self.explorer.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return x.paste(&text),
            Input::Key(k) => k,
        };
        self.message = None;
        let root = x.root().to_path_buf();
        let action = x.key(key, rows);
        if self.explorer.as_ref().is_some_and(|x| x.root() != root) || matches!(key, Key::Char('R') | Key::Ctrl('l')) {
            self.refresh_tree();
        }
        match action {
            Action::Nothing => {}
            Action::Error(e) => self.error(e),
            Action::Command => self.begin_cmdline(':', String::new()),
            Action::Close => {
                self.explorer = None;
                self.explorer_focused = false;
            }
            Action::Leave => self.explorer_focused = false,
            Action::Open(path) => {
                self.open_or_report(&path);
            }
        }
    }

    pub(super) fn reset_for_picker(&mut self) {
        self.pending.clear();
        self.message = None;
        self.leave_visual_if_any();
        self.mode = Mode::Normal;
    }
}
