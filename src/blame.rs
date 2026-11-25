use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tinsel::app::Waker;

use crate::editor::dir_of;
use crate::git::git;
use crate::sync::{lock, wait_until};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Commit {
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
}

#[derive(Debug, Default)]
pub struct Blame {
    commits: Vec<Commit>,
    lines: Vec<Option<usize>>,
    pub me: Option<String>,
}

impl Blame {
    pub fn parse(porcelain: &str, me: Option<String>) -> Blame {
        let mut b = Blame { me, ..Blame::default() };
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut at = None;
        for line in porcelain.lines() {
            if line.starts_with('\t') {
                continue;
            }
            let mut words = line.split(' ');
            let first = words.next().unwrap_or("");
            if first.len() == 40 && first.bytes().all(|c| c.is_ascii_hexdigit()) {
                let index = *seen.entry(first).or_insert_with(|| {
                    b.commits.push(Commit::default());
                    b.commits.len() - 1
                });
                at = Some(index);
                if let Some(row) = words.nth(1).and_then(|n| n.parse::<usize>().ok()).filter(|n| *n > 0) {
                    if b.lines.len() < row {
                        b.lines.resize(row, None);
                    }
                    b.lines[row - 1] = Some(index);
                }
                continue;
            }
            let Some(c) = at.map(|i| &mut b.commits[i]) else {
                continue;
            };
            let rest = line.get(first.len() + 1..).unwrap_or("");
            match first {
                "author" => c.author = rest.to_string(),
                "author-mail" => c.email = rest.trim_start_matches('<').trim_end_matches('>').to_string(),
                "author-time" => c.time = rest.parse().unwrap_or(0),
                "summary" => c.summary = rest.to_string(),
                _ => {}
            }
        }
        b
    }

    pub fn line(&self, row: usize) -> Option<&Commit> {
        self.commits.get((*self.lines.get(row)?)?)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

pub fn describe(c: &Commit, me: Option<&str>, now: i64) -> String {
    let who = if me.is_some_and(|m| !m.is_empty() && m.eq_ignore_ascii_case(&c.email)) {
        "you"
    } else {
        c.author.as_str()
    };
    let when = tinsel::format::ago_long(std::time::Duration::from_secs((now - c.time).max(0) as u64));
    if c.summary.is_empty() {
        format!("{who}, {when}")
    } else {
        format!("{who}, {when} · {}", c.summary)
    }
}

type Key = (PathBuf, String);

#[derive(Default)]
struct Shared {
    done: HashMap<Key, Arc<Blame>>,
    running: HashSet<Key>,
}

#[derive(Default)]
pub struct Blamer {
    shared: Arc<Mutex<Shared>>,
}

impl Blamer {
    pub fn get(&self, path: &Path, head: &str) -> Option<Arc<Blame>> {
        let key = (path.to_path_buf(), head.to_string());
        let mut s = lock(&self.shared);
        if let Some(b) = s.done.get(&key) {
            return Some(b.clone());
        }
        if !s.running.insert(key.clone()) {
            return None;
        }
        drop(s);
        let shared = self.shared.clone();
        let waker = Waker::new();
        std::thread::spawn(move || {
            let blame = Arc::new(run(&key.0, &key.1));
            let mut s = lock(&shared);
            s.running.remove(&key);
            s.done.retain(|(p, _), _| *p != key.0);
            s.done.insert(key, blame);
            drop(s);
            waker.wake();
        });
        None
    }

    pub fn busy(&self) -> bool {
        !lock(&self.shared).running.is_empty()
    }

    pub fn wait(&self) {
        wait_until(|| !self.busy());
    }
}

fn run(path: &Path, head: &str) -> Blame {
    let dir = dir_of(path);
    let Some(name) = path.file_name() else {
        return Blame::default();
    };
    let out = |args: &[&std::ffi::OsStr]| match git(dir, args) {
        Some((0, out)) => Some(String::from_utf8_lossy(&out).into_owned()),
        _ => None,
    };
    let me = out(&["config".as_ref(), "user.email".as_ref()]).map(|s| s.trim().to_string());
    let text = out(&[
        "blame".as_ref(),
        "--porcelain".as_ref(),
        head.as_ref(),
        "--".as_ref(),
        name,
    ]);
    Blame::parse(&text.unwrap_or_default(), me)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
1111111111111111111111111111111111111111 1 1 2
author Ada
author-mail <ada@x.org>
author-time 1000
summary first one
filename a.txt
\tline one
1111111111111111111111111111111111111111 2 2
\tline two
2222222222222222222222222222222222222222 3 3 1
author Bo
author-mail <bo@x.org>
author-time 2000
summary second
filename a.txt
\tline three
";

    #[test]
    fn porcelain_is_read_line_by_line() {
        let b = Blame::parse(SAMPLE, Some("bo@x.org".into()));
        assert_eq!(b.len(), 3);
        assert_eq!(b.line(1).unwrap().author, "Ada");
        assert_eq!(b.line(2).unwrap().summary, "second");
        assert_eq!(b.line(3), None);
        assert_eq!(
            describe(b.line(0).unwrap(), b.me.as_deref(), 1000 + 3 * 86_400),
            "Ada, 3 days ago · first one"
        );
        assert_eq!(
            describe(b.line(2).unwrap(), b.me.as_deref(), 2030),
            "you, just now · second"
        );
    }
}
