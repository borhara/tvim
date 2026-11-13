use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use tinsel::app::{Key, Waker};

use crate::sync::lock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Staged,
    Changes,
    Untracked,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::Staged => "Staged",
            Group::Changes => "Changes",
            Group::Untracked => "Untracked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub group: Group,
    pub letter: char,
    // where a staged rename came from
    pub from: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub root: PathBuf,
    pub branch: String,
    pub upstream: Option<(usize, usize)>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub current: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Files,
    Log,
    Branches,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typing {
    Commit,
    Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub title: String,
    pub lines: Vec<String>,
    pub code: i32,
    pub scroll: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    Nothing,
    Close,
    Open(PathBuf),
}

#[derive(Default)]
struct Done {
    status: Option<Result<Status, String>>,
    log: Option<Vec<Entry>>,
    branches: Option<Vec<Entry>>,
    note: Option<Note>,
    output: Option<Output>,
}

struct Slot<T> {
    shared: Arc<Mutex<(u64, Option<T>)>>,
    ticket: u64,
    waiting: bool,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Slot {
            shared: Arc::new(Mutex::new((0, None))),
            ticket: 0,
            waiting: false,
        }
    }
}

impl<T: Send + 'static> Slot<T> {
    fn run(&mut self, work: impl FnOnce() -> T + Send + 'static) {
        self.ticket += 1;
        self.waiting = true;
        let ticket = self.ticket;
        let shared = self.shared.clone();
        let waker = Waker::new();
        std::thread::spawn(move || {
            let out = work();
            let mut s = lock(&shared);
            if ticket >= s.0 {
                *s = (ticket, Some(out));
            }
            drop(s);
            waker.wake();
        });
    }

    fn take(&mut self) -> Option<T> {
        let mut s = lock(&self.shared);
        if s.0 != self.ticket {
            return None;
        }
        let out = s.1.take()?;
        self.waiting = false;
        Some(out)
    }
}

pub struct GitMenu {
    start: PathBuf,
    status: Option<Status>,
    failed: Option<String>,
    tab: Tab,
    selected: usize,
    log: Vec<Entry>,
    branches: Vec<Entry>,
    peeks: HashMap<String, Vec<String>>,
    peek_scroll: usize,
    typing: Option<(Typing, String, usize)>,
    confirm: Option<Change>,
    output: Option<Output>,
    note: Option<Note>,
    busy: Option<String>,
    job: Slot<Done>,
    peeking: Option<String>,
    pub(crate) history: Vec<String>,
    recall: Option<usize>,
    queued: Option<String>,
    // typed before the first status came back, played once it has
    pub(crate) early: Vec<Key>,
    opened: Option<f32>,
}

impl GitMenu {
    pub fn open(start: &Path, history: Vec<String>, command: Option<String>) -> GitMenu {
        let mut m = GitMenu {
            start: start.to_path_buf(),
            status: None,
            failed: None,
            tab: Tab::Files,
            selected: 0,
            log: Vec::new(),
            branches: Vec::new(),
            peeks: HashMap::new(),
            peek_scroll: 0,
            typing: None,
            confirm: None,
            output: None,
            note: None,
            busy: None,
            job: Slot::default(),
            peek: Slot::default(),
            peeking: None,
            history,
            recall: None,
            queued: command.filter(|c| !c.trim().is_empty()),
            early: Vec::new(),
            opened: None,
        };
        let start = m.start.clone();
        m.busy = Some("reading".into());
        m.job.run(move || Done {
            status: Some(read_status(&start)),
            ..Done::default()
        });
        m
    }

    pub fn ready(&self) -> bool {
        self.status.is_some()
    }

    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    pub fn root(&self) -> Option<&Path> {
        self.status.as_ref().map(|s| s.root.as_path())
    }

    pub fn repo_name(&self) -> String {
        self.root()
            .and_then(Path::file_name)
            .map_or_else(|| "git".into(), |n| n.to_string_lossy().into_owned())
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn changes(&self) -> &[Change] {
        self.status.as_ref().map_or(&[], |s| &s.changes)
    }

    pub fn entries(&self) -> &[Entry] {
        match self.tab {
            Tab::Log => &self.log,
            _ => &self.branches,
        }
    }

    pub fn note(&self) -> Option<&Note> {
        self.note.as_ref()
    }

    pub fn busy(&self) -> Option<&str> {
        self.busy.as_deref()
    }

    pub fn working(&self) -> bool {
        self.job.waiting || self.peek.waiting
    }

    pub fn opened(&self) -> Option<f32> {
        self.opened
    }

    fn len(&self) -> usize {
        match self.tab {
            Tab::Files => self.changes().len(),
            Tab::Log => self.log.len(),
            Tab::Branches => self.branches.len(),
        }
    }

    pub fn current(&self) -> Option<&Change> {
        (self.tab == Tab::Files)
            .then(|| self.changes().get(self.selected))
            .flatten()
    }

    pub fn tick(&mut self) -> bool {
        if let Some(done) = self.job.take() {
            self.busy = None;
            match done.status {
                Some(Ok(s)) => {
                    if self.status.is_none() {
                        self.opened = Some(tinsel::app::time());
                    }
                    if self.status.as_ref() != Some(&s) {
                        self.peeks.clear();
                    }
                    self.status = Some(s);
                }
                Some(Err(e)) if self.status.is_none() => self.failed = Some(e),
                Some(Err(e)) => self.note = Some(error(e)),
                None => {}
            }
            if let Some(log) = done.log {
                self.log = log;
            }
            if let Some(b) = done.branches {
                self.branches = b;
            }
            if let Some(n) = done.note {
                self.note = Some(n);
            }
            if let Some(o) = done.output {
                self.output = Some(o);
            }
            self.selected = self.selected.min(self.len().saturating_sub(1));
            if let Some(cmd) = self.queued.take().filter(|_| self.status.is_some()) {
            }
        }
        if let Some((key, lines)) = self.peek.take() {
            self.peeking = None;
            self.peeks.insert(key, lines);
        }
    }

}

fn info(text: impl Into<String>) -> Note {
    Note {
        text: text.into(),
        error: false,
    }
}

fn error(text: impl Into<String>) -> Note {
    Note {
        text: text.into(),
        error: true,
    }
}

fn first_line(s: &str) -> String {
    let s = s.trim();
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    line.strip_prefix("error: ")
        .or_else(|| line.strip_prefix("fatal: "))
        .unwrap_or(line)
        .to_string()
}

fn git(root: &Path, args: &[&str]) -> (i32, String, String) {
    let mut cmd = Command::new("git");
    #[cfg(unix)]
    without_terminal(&mut cmd);
    let out = cmd
        .arg("-C")
        .arg(root)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=never"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "true")
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(e) => (-1, String::new(), format!("can't run git: {e}")),
    }
}

// ssh prompts on the terminal directly; without one it fails instead of hanging
#[cfg(unix)]
fn without_terminal(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn setsid() -> i32;
    }
    // SAFETY: setsid is async-signal-safe, so it's fine between fork and exec
    unsafe {
        cmd.pre_exec(|| {
            setsid();
            Ok(())
        });
    }
}

fn read_status(start: &Path) -> Result<Status, String> {
    let (code, top, _) = git(start, &["rev-parse", "--show-toplevel"]);
    if code != 0 || top.trim().is_empty() {
        return Err("not in a git repository".into());
    }
    let root = PathBuf::from(top.trim());
    let (code, out, err) = git(
        &root,
        &["status", "--porcelain=v1", "-b", "-z", "--untracked-files=all"],
    );
    if code != 0 {
        return Err(first_line(&err));
    }
    let (mut branch, upstream, changes) = parse_status(&out);
    if branch.is_empty() {
        branch = git(&root, &["rev-parse", "--short", "HEAD"]).1.trim().to_string();
    }
    Ok(Status {
        root,
        branch,
        upstream,
        changes,
    })
}

fn parse_status(out: &str) -> (String, Option<(usize, usize)>, Vec<Change>) {
    let mut branch = String::new();
    let mut upstream = None;
    let mut changes = Vec::new();
    let mut parts = out.split('\0');
    while let Some(part) = parts.next() {
        if let Some(head) = part.strip_prefix("## ") {
            let head = head.strip_prefix("No commits yet on ").unwrap_or(head);
            let (names, counts) = head.split_once(" [").unwrap_or((head, ""));
            match names.split_once("...") {
                Some((b, _)) => {
                    branch = b.to_string();
                    let count = |what: &str| {
                        counts
                            .split([',', ']'])
                            .find_map(|c| c.trim().strip_prefix(what)?.trim().parse().ok())
                            .unwrap_or(0)
                    };
                    upstream = (!counts.starts_with("gone")).then(|| (count("ahead"), count("behind")));
                }
                None if names.starts_with("HEAD (") => {}
                None => branch = names.to_string(),
            }
            continue;
        }
        if part.len() < 4 {
            continue;
        }
        let (x, y) = (part.as_bytes()[0] as char, part.as_bytes()[1] as char);
        let path = part[3..].to_string();
        let from = matches!(x, 'R' | 'C').then(|| parts.next()).flatten();
        if x == '?' {
            changes.push(Change {
                path,
                group: Group::Untracked,
                letter: 'U',
                from: None,
            });
            continue;
        }
        if x != ' ' && x != '!' {
            changes.push(Change {
                path: path.clone(),
                group: Group::Staged,
                letter: x,
                from: from.filter(|_| x == 'R').map(str::to_string),
            });
        }
        if y != ' ' && y != '!' {
            changes.push(Change {
                path,
                group: Group::Changes,
                letter: y,
                from: None,
            });
        }
    }
    changes.sort_by(|a, b| (a.group, &a.path).cmp(&(b.group, &b.path)));
    (branch, upstream, changes)
}
