use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use tinsel::app::Waker;

use crate::editor::dir_of;
use crate::sync::{lock, wait_until};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Base {
    #[default]
    Outside,
    Untracked,
    Head(Vec<String>),
}

#[derive(Debug, Default)]
pub struct Found {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub base: Base,
    pub upstream: Option<(usize, usize)>,
    // bumped when these change, to drop cached diffs
    pub seq: u64,
}

#[derive(Default)]
struct Shared {
    found: Found,
    path: Option<PathBuf>,
    ticket: u64,
    busy: bool,
}

#[derive(Default)]
pub struct Git {
    shared: Arc<Mutex<Shared>>,
}

impl Git {
    pub fn with<R>(&self, f: impl FnOnce(&Found) -> R) -> R {
        f(&lock(&self.shared).found)
    }

    pub fn busy(&self) -> bool {
        lock(&self.shared).busy
    }

    pub fn wait(&self) {
        wait_until(|| !self.busy());
    }

    pub fn probe(&self, path: Option<&Path>) {
        let mut s = lock(&self.shared);
        s.ticket += 1;
        if s.path.as_deref() != path {
            let seq = s.found.seq + 1;
            s.found = Found {
                seq,
                ..Found::default()
            };
            s.path = path.map(Path::to_path_buf);
        }
        let Some(path) = path else {
            s.busy = false;
            return;
        };
        s.busy = true;
        let ticket = s.ticket;
        drop(s);
        let shared = self.shared.clone();
        let path = path.to_path_buf();
        let waker = Waker::new();
        std::thread::spawn(move || {
            let (branch, head, base) = look(&path);
            let upstream = upstream(&path);
            let mut s = lock(&shared);
            if s.ticket != ticket {
                return;
            }
            s.busy = false;
            if s.found.upstream != upstream {
                s.found.upstream = upstream;
                waker.wake();
            }
            if s.found.branch != branch || s.found.head != head || s.found.base != base {
                s.found.branch = branch;
                s.found.head = head;
                s.found.base = base;
                s.found.seq += 1;
                drop(s);
                waker.wake();
            }
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    Modified,
    Added,
    Untracked,
    Deleted,
}

impl FileState {
    pub fn letter(self) -> char {
        match self {
            FileState::Modified => 'M',
            FileState::Added => 'A',
            FileState::Untracked => '?',
            FileState::Deleted => 'D',
        }
    }
}

#[derive(Debug, Default)]
pub struct Statuses {
    files: HashMap<PathBuf, FileState>,
    // every folder from a changed path up to the repo's top
    dirs: HashSet<PathBuf>,
}

impl Statuses {
    pub fn file(&self, path: &Path) -> Option<FileState> {
        self.files
            .get(path)
            .copied()
            .or_else(|| self.untracked_above(path).then_some(FileState::Untracked))
    }

    pub fn dir_changed(&self, path: &Path) -> bool {
        self.dirs.contains(path) || self.untracked_above(path)
    }

    // git lists an untracked folder once, not the files in it
    fn untracked_above(&self, path: &Path) -> bool {
        path.ancestors()
            .skip(1)
            .any(|d| self.files.get(d) == Some(&FileState::Untracked))
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn changed_under(&self, dir: &Path) -> usize {
        self.files.keys().filter(|p| p.starts_with(dir)).count()
    }
}

#[derive(Default)]
struct TreeShared {
    found: Statuses,
    ticket: u64,
    busy: bool,
}

#[derive(Default)]
pub struct Tree {
    shared: Arc<Mutex<TreeShared>>,
}

impl Tree {
    pub fn with<R>(&self, f: impl FnOnce(&Statuses) -> R) -> R {
        f(&lock(&self.shared).found)
    }

    pub fn wait(&self) {
        wait_until(|| !lock(&self.shared).busy);
    }

    pub fn refresh(&self, dir: &Path) {
        let mut s = lock(&self.shared);
        s.ticket += 1;
        s.busy = true;
        let ticket = s.ticket;
        drop(s);
        let shared = self.shared.clone();
        let dir = dir.to_path_buf();
        let waker = Waker::new();
        std::thread::spawn(move || {
            let found = statuses(&dir);
            let mut s = lock(&shared);
            if s.ticket != ticket {
                return;
            }
            s.found = found;
            s.busy = false;
            drop(s);
            waker.wake();
        });
    }
}

fn statuses(dir: &Path) -> Statuses {
    let out = Statuses::default();
    let Some(root) =
        git_line(dir, &["rev-parse", "--show-toplevel"]).and_then(|r| crate::paths::canonical(Path::new(&r)).ok())
    else {
        return out;
    };
    match git(dir, &["status", "--porcelain=v1", "-z"]) {
        Some((0, raw)) => parse_status(&root, &raw),
        _ => out,
    }
}

fn parse_status(root: &Path, raw: &[u8]) -> Statuses {
    let mut out = Statuses::default();
    let mut items = raw.split(|b| *b == 0);
    while let Some(item) = items.next() {
        if item.len() < 4 {
            continue;
        }
        let (x, y) = (item[0], item[1]);
        // a rename or copy is followed by its old name
        if x == b'R' || x == b'C' {
            items.next();
        }
        let state = match (x, y) {
            (b'?', b'?') => FileState::Untracked,
            (b'!', b'!') => continue,
            (b'D', _) | (_, b'D') => FileState::Deleted,
            (b'A', _) => FileState::Added,
            _ => FileState::Modified,
        };
        let rel = String::from_utf8_lossy(&item[3..]);
        let path = root.join(rel.trim_end_matches('/'));
        let mut up = path.parent();
        while let Some(d) = up.filter(|d| d.starts_with(root)) {
            if !out.dirs.insert(d.to_path_buf()) {
                break;
            }
            up = d.parent();
        }
        out.files.insert(path, state);
    }
    out
}

fn git(dir: &Path, args: &[&str]) -> Option<(i32, Vec<u8>)> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some((out.status.code().unwrap_or(-1), out.stdout))
}

fn trimmed(out: Vec<u8>) -> Option<String> {
    let s = String::from_utf8(out).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn git_line(dir: &Path, args: &[&str]) -> Option<String> {
    git(dir, args)
        .filter(|(code, _)| *code == 0)
        .and_then(|(_, out)| trimmed(out))
}

fn upstream(path: &Path) -> Option<(usize, usize)> {
    let text = git_line(dir_of(path), &["rev-list", "--left-right", "--count", "HEAD...@{u}"])?;
    let mut n = text.split_whitespace().map(|n| n.parse().ok());
    Some((n.next()??, n.next()??))
}

fn look(path: &Path) -> (Option<String>, Option<String>, Base) {
    let dir = dir_of(path);
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return (None, None, Base::Outside);
    };
    // 0 on a branch, 1 when detached, anything else and we're not in a repo
    let branch = match git(dir, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        Some((0, out)) => trimmed(out),
        Some((1, _)) => git_line(dir, &["rev-parse", "--short", "HEAD"]),
        _ => return (None, None, Base::Outside),
    };
    let head = git_line(dir, &["rev-parse", "-q", "--verify", "HEAD"]);
    // cat-file skips textconv and filters, so this is the committed text as is
    let base = match git(dir, &["cat-file", "blob", &format!("HEAD:./{name}")]) {
        Some((0, bytes)) => match String::from_utf8(bytes) {
            Ok(text) => Base::Head(lines(&text)),
            Err(_) => Base::Outside,
        },
        _ => match git(dir, &["check-ignore", "-q", "--", name]) {
            Some((1, _)) => Base::Untracked,
            _ => Base::Outside,
        },
    };
    (branch, head, base)
}

fn lines(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    let body = text.strip_suffix('\n').unwrap_or(&text);
    body.split('\n').map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_status() {
        let root = Path::new("/r");
        let raw = b" M src/a.rs\0A  b.rs\0?? new/\0 D gone.rs\0R  to.rs\0from.rs\0!! skip\0";
        let s = parse_status(root, raw);
        assert_eq!(s.file(&root.join("src/a.rs")), Some(FileState::Modified));
        assert_eq!(s.file(&root.join("b.rs")), Some(FileState::Added));
        assert_eq!(s.file(&root.join("new")), Some(FileState::Untracked));
        assert_eq!(s.file(&root.join("gone.rs")), Some(FileState::Deleted));
        assert_eq!(s.file(&root.join("to.rs")), Some(FileState::Modified));
        assert_eq!(
            s.file(&root.join("from.rs")),
            None,
            "a rename's old name isn't an entry"
        );
        assert_eq!(s.file(&root.join("skip")), None);
        assert!(s.dir_changed(&root.join("src")) && s.dir_changed(root));
        assert!(!s.dir_changed(&root.join("new")));
        assert_eq!(FileState::Untracked.letter(), '?');
    }
}
