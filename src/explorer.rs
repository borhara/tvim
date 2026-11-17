use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use tinsel::app::Key;

use crate::finder::follow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub depth: usize,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Open(PathBuf),
    Close,
    Leave,
    Command,
    Error(String),
}

pub struct Explorer {
    root: PathBuf,
    // a folder's entries are read the first time it's expanded
    loaded: HashMap<PathBuf, Vec<(String, bool)>>,
    expanded: HashSet<PathBuf>,
    rows: Vec<(Entry, Vec<usize>)>,
    selected: usize,
    filter: String,
    filtering: bool,
    show_hidden: bool,
    top: Cell<usize>,
}

fn read(dir: &Path) -> io::Result<Vec<(String, bool)>> {
    let mut entries: Vec<(String, bool)> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let kind = e.file_type().ok()?;
            // only links need a stat, to follow them
            let dir = if kind.is_symlink() {
                std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir())
            } else {
                kind.is_dir()
            };
            Some((name, dir))
        })
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    Ok(entries)
}

impl Explorer {
    pub fn open(dir: &Path, select: Option<&Path>) -> io::Result<Explorer> {
        let root = std::fs::canonicalize(dir)?;
        let mut x = Explorer {
            loaded: HashMap::from([(root.clone(), read(&root)?)]),
            root,
            expanded: HashSet::new(),
            rows: Vec::new(),
            selected: 0,
            filter: String::new(),
            filtering: false,
            show_hidden: false,
            top: Cell::new(0),
        };
        x.rebuild();
        if let Some(path) = select {
            x.reveal(path);
        }
        Ok(x)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn filter(&self) -> Option<&str> {
        (self.filtering || !self.filter.is_empty()).then_some(self.filter.as_str())
    }

    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    pub fn shows_hidden(&self) -> bool {
        self.show_hidden
    }

    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    pub fn visible(&self) -> &[(Entry, Vec<usize>)] {
        &self.rows
    }

    fn shown(&self, name: &str) -> bool {
        !name.starts_with('.') || self.show_hidden || self.filter.starts_with('.')
    }

    fn rebuild(&mut self) {
        let mut rows = Vec::new();
        if self.filter.is_empty() {
            self.walk(&self.root, 0, &mut rows);
        } else {
            self.matches(&self.root, 0, &mut rows);
        }
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    fn entry(dir: &Path, name: &str, is_dir: bool, depth: usize) -> Entry {
        Entry {
            name: name.to_string(),
            dir: is_dir,
            depth,
            path: dir.join(name),
        }
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<(Entry, Vec<usize>)>) {
        let Some(kids) = self.loaded.get(dir) else { return };
        for (name, is_dir) in kids {
            if !self.shown(name) {
                continue;
            }
            let entry = Self::entry(dir, name, *is_dir, depth);
            let open = *is_dir && self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            out.push((entry, Vec::new()));
            if open {
                self.walk(&path, depth + 1, out);
            }
        }
    }

    // matches in the expanded folders, each under the folders that lead to it
    fn matches(&self, dir: &Path, depth: usize, out: &mut Vec<(Entry, Vec<usize>)>) -> bool {
        let Some(kids) = self.loaded.get(dir) else { return false };
        let mut any = false;
        for (name, is_dir) in kids {
            if !self.shown(name) {
                continue;
            }
            let entry = Self::entry(dir, name, *is_dir, depth);
            let mut inner = Vec::new();
            let below =
                *is_dir && self.expanded.contains(&entry.path) && self.matches(&entry.path, depth + 1, &mut inner);
            let hits = tinsel::text::fuzzy(name, &self.filter).map(|m| m.hits);
            if hits.is_some() || below {
                out.push((entry, hits.unwrap_or_default()));
                out.append(&mut inner);
                any = true;
            }
        }
        any
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.rows.get(self.selected).map(|(e, _)| e.path.clone())
    }

    fn select_path(&mut self, path: &Path) -> bool {
        match self.rows.iter().position(|(e, _)| e.path == path) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    fn set_filter(&mut self, f: impl FnOnce(&mut String)) {
        f(&mut self.filter);
        self.rebuild();
        // the folders around the matches are there for context, so start on a match
        self.selected = self.rows.iter().position(|(_, hits)| !hits.is_empty()).unwrap_or(0);
        self.top.set(0);
    }

    fn clear_filter(&mut self) {
        self.filtering = false;
        if !self.filter.is_empty() {
            self.filter.clear();
            self.rebuild();
        }
    }

    fn expand(&mut self, dir: &Path) -> io::Result<()> {
        if !self.loaded.contains_key(dir) {
            let entries = read(dir)?;
            self.loaded.insert(dir.to_path_buf(), entries);
        }
        self.expanded.insert(dir.to_path_buf());
        Ok(())
    }

    // false when outside the tree
    pub fn reveal(&mut self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return false;
        };
        let mut dir = self.root.clone();
        let parts: Vec<_> = rel.components().collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            dir.push(part);
            if self.expand(&dir).is_err() {
                return false;
            }
        }
        self.filtering = false;
        self.filter.clear();
        if parts.iter().any(|p| p.as_os_str().to_string_lossy().starts_with('.')) {
            self.show_hidden = true;
        }
        self.rebuild();
        self.select_path(path)
    }

    fn reroot(&mut self, dir: PathBuf) -> Action {
        if !self.loaded.contains_key(&dir) {
            match read(&dir) {
                Ok(entries) => {
                    self.loaded.insert(dir.clone(), entries);
                }
                Err(e) => return Action::Error(format!("can't open that folder: {e}")),
            }
        }
        let old = std::mem::replace(&mut self.root, dir);
        // keep the old root expanded
        if old.starts_with(&self.root) {
            self.expanded.insert(old.clone());
        }
        self.clear_filter();
        self.rebuild();
        if !self.select_path(&old) {
            self.selected = 0;
        }
        Action::Nothing
    }

    fn up(&mut self) -> Action {
        match self.root.parent().map(Path::to_path_buf) {
            Some(parent) => self.reroot(parent),
            None => Action::Nothing,
        }
    }

    fn activate(&mut self, toggle: bool) -> Action {
        let Some((entry, _)) = self.rows.get(self.selected).cloned() else {
            return Action::Nothing;
        };
        let filtered = !self.filter.is_empty();
        self.clear_filter();
        if !entry.dir {
            self.select_path(&entry.path);
            return Action::Open(entry.path);
        }
        let open = self.expanded.contains(&entry.path);
        if open && toggle && !filtered {
            self.expanded.remove(&entry.path);
        } else if let Err(e) = self.expand(&entry.path) {
            self.select_path(&entry.path);
            return Action::Error(format!("can't open that folder: {e}"));
        }
        self.rebuild();
        self.select_path(&entry.path);
        // l on an open folder steps into it
        if open && !toggle && !filtered {
            let child = self
                .rows
                .get(self.selected + 1)
                .is_some_and(|(e, _)| e.depth > entry.depth);
            self.selected += usize::from(child);
        }
        Action::Nothing
    }

    fn collapse(&mut self) {
        let Some((entry, _)) = self.rows.get(self.selected).cloned() else {
            return;
        };
        if entry.dir && self.expanded.contains(&entry.path) && self.filter.is_empty() {
            self.expanded.remove(&entry.path);
            self.rebuild();
            self.select_path(&entry.path);
        } else if let Some(parent) = entry.path.parent().filter(|_| entry.depth > 0) {
            self.select_path(parent);
        }
    }

    fn reload(&mut self) -> Action {
        let keep = self.selected_path();
        let mut fresh = HashMap::new();
        match read(&self.root) {
            Ok(entries) => fresh.insert(self.root.clone(), entries),
            Err(e) => return Action::Error(format!("can't read this folder: {e}")),
        };
        self.expanded.retain(|d| match read(d) {
            Ok(entries) => {
                fresh.insert(d.clone(), entries);
                true
            }
            Err(_) => false,
        });
        self.loaded = fresh;
        self.rebuild();
        if let Some(p) = keep {
            self.select_path(&p);
        }
        Action::Nothing
    }

    pub fn top(&self, rows: usize) -> usize {
        self.top
            .set(self.top.get().min(self.rows.len().saturating_sub(rows.max(1))));
        follow(&self.top, self.selected, rows)
    }

    pub fn key(&mut self, key: Key, rows: usize) -> Action {
        let last = self.rows.len().saturating_sub(1);
        if self.filtering {
            match key {
                Key::Char(c) => self.set_filter(|f| f.push(c)),
                Key::Backspace | Key::Ctrl('h') => {
                    if self.filter.is_empty() {
                        self.filtering = false;
                    } else {
                        self.set_filter(|f| {
                            f.pop();
                        });
                    }
                }
                Key::Ctrl('u') => self.set_filter(String::clear),
                Key::Esc | Key::Ctrl('c') => {
                    let keep = self.selected_path();
                    self.clear_filter();
                    if !keep.is_some_and(|p| self.select_path(&p)) {
                        self.selected = 0;
                    }
                }
                Key::Enter => return self.activate(false),
                Key::Down | Key::Ctrl('n') | Key::Tab => self.selected = (self.selected + 1).min(last),
                Key::Up | Key::Ctrl('p') | Key::BackTab => self.selected = self.selected.saturating_sub(1),
                _ => {}
            }
            return Action::Nothing;
        }
        let page = rows.max(2) / 2;
        match key {
            Key::Char('j') | Key::Down | Key::Ctrl('n') => self.selected = (self.selected + 1).min(last),
            Key::Char('k') | Key::Up | Key::Ctrl('p') => self.selected = self.selected.saturating_sub(1),
            Key::Char('g') | Key::Home => self.selected = 0,
            Key::Char('G') | Key::End => self.selected = last,
            Key::Ctrl('d') | Key::PageDown => self.selected = (self.selected + page).min(last),
            Key::Ctrl('u') | Key::PageUp => self.selected = self.selected.saturating_sub(page),
            Key::Enter | Key::Char('o') => return self.activate(true),
            Key::Char('l') | Key::Right => return self.activate(false),
            Key::Char('h') | Key::Left | Key::Backspace => self.collapse(),
            Key::Char('-') => return self.up(),
            Key::Char('~') => {
                if let Some(home) = std::env::var_os("HOME").and_then(|h| std::fs::canonicalize(h).ok()) {
                    return self.reroot(home);
                }
            }
            Key::Char('.') => {
                let keep = self.selected_path();
                self.show_hidden = !self.show_hidden;
                self.rebuild();
                if let Some(p) = keep {
                    self.select_path(&p);
                }
            }
            Key::Char('/') => {
                self.filtering = true;
                self.set_filter(String::clear);
            }
            Key::Char('R') | Key::Ctrl('l') => return self.reload(),
            Key::Char(':') => return Action::Command,
            Key::Esc if !self.filter.is_empty() => {
                self.clear_filter();
                self.selected = 0;
            }
            Key::Char('q') => return Action::Close,
            Key::Tab | Key::Esc | Key::Ctrl('c') => return Action::Leave,
            _ => {}
        }
        Action::Nothing
    }

    pub fn paste(&mut self, text: &str) {
        if self.filtering {
            self.set_filter(|f| f.push_str(text.lines().next().unwrap_or("")));
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1000.0 && unit < UNITS.len() - 1 {
        size /= 1000.0;
        unit += 1;
    }
    if unit == 0 || size >= 10.0 {
        format!("{} {}", size.round() as u64, UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(812), "812 B");
        assert_eq!(human_size(4200), "4.2 kB");
        assert_eq!(human_size(13_000_000), "13 MB");
    }
}
