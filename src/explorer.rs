use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

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
        }
        Ok(x)
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

}
