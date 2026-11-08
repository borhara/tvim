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
