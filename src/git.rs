use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

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
    git(dir, args).filter(|(code, _)| *code == 0).and_then(|(_, out)| trimmed(out))
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
