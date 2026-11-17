use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use tinsel::app::{Key, Waker};

use crate::sync::lock;

const MAX_LINES: usize = 3000;
const LOG_LENGTH: usize = 30;

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
    changed: bool,
}

type Peek = (String, Vec<String>);

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
    peek: Slot<Peek>,
    peeking: Option<String>,
    pub(crate) history: Vec<String>,
    recall: Option<usize>,
    queued: Option<String>,
    // typed before the first status came back, played once it has
    pub(crate) early: Vec<Key>,
    opened: Option<f64>,
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

    pub fn typing(&self) -> Option<(Typing, &str, usize)> {
        self.typing.as_ref().map(|(t, s, c)| (*t, s.as_str(), *c))
    }

    pub fn confirming(&self) -> Option<&Change> {
        self.confirm.as_ref()
    }

    pub fn output(&self) -> Option<&Output> {
        self.output.as_ref()
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

    pub fn opened(&self) -> Option<f64> {
        self.opened
    }

    pub fn peek_scroll(&self) -> usize {
        self.peek_scroll
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

    fn peek_key(&self) -> Option<String> {
        match self.tab {
            Tab::Files => self.current().map(|c| format!("{:?}:{}", c.group, c.path)),
            Tab::Log => self.log.get(self.selected).map(|e| format!("commit:{}", e.id)),
            Tab::Branches => self.branches.get(self.selected).map(|e| format!("branch:{}", e.id)),
        }
    }

    pub fn peek(&self) -> Option<&[String]> {
        self.peeks.get(&self.peek_key()?).map(Vec::as_slice)
    }

    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(done) = self.job.take() {
            self.busy = None;
            changed = done.changed;
            if changed {
                self.peeks.clear();
            }
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
                self.command(&cmd);
            }
        }
        if let Some((key, lines)) = self.peek.take() {
            self.peeking = None;
            self.peeks.insert(key, lines);
        }
        self.want_peek();
        changed
    }

    fn want_peek(&mut self) {
        let Some(key) = self.peek_key() else { return };
        if self.peeks.contains_key(&key) || self.peeking.as_ref() == Some(&key) {
            return;
        }
        let Some(root) = self.root().map(Path::to_path_buf) else {
            return;
        };
        let what = match self.tab {
            Tab::Files => match self.current() {
                Some(c) => Peeking::File(c.clone()),
                None => return,
            },
            Tab::Log => Peeking::Commit(self.log[self.selected].id.clone()),
            Tab::Branches => Peeking::Branch(self.branches[self.selected].id.clone()),
        };
        self.peeking = Some(key.clone());
        self.peek.run(move || (key, peek(&root, what)));
    }

    fn start(&mut self, label: &str, work: impl FnOnce(&Path) -> Done + Send + 'static) -> bool {
        if self.job.waiting {
            self.note = Some(error("still busy, one moment"));
            return false;
        }
        let Some(root) = self.root().map(Path::to_path_buf) else {
            return false;
        };
        let tab = self.tab;
        self.busy = Some(label.to_string());
        self.note = None;
        self.job.run(move || {
            let mut done = work(&root);
            if done.status.is_none() {
                done.status = Some(read_status(&root));
            }
            match tab {
                Tab::Log if done.log.is_none() => done.log = Some(read_log(&root)),
                Tab::Branches if done.branches.is_none() => done.branches = Some(read_branches(&root)),
                _ => {}
            }
            done
        });
        true
    }

    pub fn key(&mut self, key: Key, unsaved: bool) -> Want {
        if self.status.is_none() {
            if matches!(key, Key::Esc | Key::Ctrl('c')) {
                return Want::Close;
            }
            self.early.push(key);
            return Want::Nothing;
        }
        if let Some(change) = self.confirm.take() {
            if key == Key::Char('y') {
                self.discard(change);
            } else {
                self.note = Some(info("kept as it was"));
            }
            return Want::Nothing;
        }
        if self.typing.is_some() {
            self.type_key(key);
            return Want::Nothing;
        }
        if let Some(o) = self.output.as_mut() {
            let max = o.lines.len().saturating_sub(1);
            match key {
                Key::Char('j') | Key::Down => o.scroll = (o.scroll + 1).min(max),
                Key::Char('k') | Key::Up => o.scroll = o.scroll.saturating_sub(1),
                Key::Ctrl('d') | Key::PageDown => o.scroll = (o.scroll + 10).min(max),
                Key::Ctrl('u') | Key::PageUp => o.scroll = o.scroll.saturating_sub(10),
                Key::Char('g') => o.scroll = 0,
                Key::Char('G') => o.scroll = max,
                Key::Esc | Key::Char('q') => self.output = None,
                _ => return self.menu_key(key, unsaved),
            }
            return Want::Nothing;
        }
        self.menu_key(key, unsaved)
    }

    fn menu_key(&mut self, key: Key, unsaved: bool) -> Want {
        let files = self.tab == Tab::Files;
        match key {
            Key::Esc | Key::Char('q') | Key::Ctrl('c') => return Want::Close,
            Key::Char('j') | Key::Down => self.select(self.selected + 1),
            Key::Char('k') | Key::Up => self.select(self.selected.saturating_sub(1)),
            Key::Char('g') | Key::Home => self.select(0),
            Key::Char('G') | Key::End => self.select(usize::MAX),
            Key::Char('J') | Key::Ctrl('d') | Key::PageDown => {
                let max = self.peek().map_or(0, |p| p.len().saturating_sub(1));
                self.peek_scroll = (self.peek_scroll + 10).min(max);
            }
            Key::Char('K') | Key::Ctrl('u') | Key::PageUp => self.peek_scroll = self.peek_scroll.saturating_sub(10),
            Key::Char('r') => {
                self.start("refreshing", |_| Done::default());
            }
            Key::Char(':' | '!') => {
                self.typing = Some((Typing::Command, String::new(), 0));
                self.recall = None;
            }
            Key::Char('l') => self.switch_tab(if self.tab == Tab::Log { Tab::Files } else { Tab::Log }),
            Key::Char('b') => self.switch_tab(if self.tab == Tab::Branches {
                Tab::Files
            } else {
                Tab::Branches
            }),
            Key::Char('p') => self.remote("pull"),
            Key::Char('P') => self.remote("push"),
            Key::Char('s') if files => self.toggle_stage(),
            Key::Char('a') if files => {
                self.start("staging", |root| {
                    changed(git(root, &["add", "-A"]), "staged everything")
                });
            }
            Key::Char('u') if files => {
                self.start("unstaging", |root| {
                    let mut out = git(root, &["reset", "-q"]);
                    if out.0 != 0 {
                        out = git(root, &["rm", "-r", "-q", "--cached", "."]);
                    }
                    changed(out, "unstaged everything")
                });
            }
            Key::Char('d') if files => {
                if let Some(c) = self.current().cloned() {
                    self.confirm = Some(c);
                }
            }
            Key::Char('c') if files => {
                if self.changes().iter().any(|c| c.group == Group::Staged) {
                    self.typing = Some((Typing::Commit, String::new(), 0));
                } else {
                    self.note = Some(error("nothing staged: s stages a file, a stages them all"));
                }
            }
            Key::Enter => match self.tab {
                Tab::Files => {
                    if let (Some(root), Some(c)) = (self.root(), self.current()) {
                        let path = root.join(&c.path);
                        if path.is_file() {
                            return Want::Open(path);
                        }
                        self.note = Some(error(format!("{} isn't there to open", c.path)));
                    }
                }
                Tab::Branches => self.switch_branch(unsaved),
                Tab::Log => {}
            },
            _ => {}
        }
        Want::Nothing
    }

    fn select(&mut self, at: usize) {
        let at = at.min(self.len().saturating_sub(1));
        if at != self.selected {
            self.selected = at;
            self.peek_scroll = 0;
        }
        self.want_peek();
    }

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.selected = 0;
        self.peek_scroll = 0;
        match tab {
            Tab::Files => self.want_peek(),
            Tab::Log => {
                self.start("reading the log", |root| Done {
                    log: Some(read_log(root)),
                    ..Done::default()
                });
            }
            Tab::Branches => {
                self.start("reading branches", |root| Done {
                    branches: Some(read_branches(root)),
                    ..Done::default()
                });
            }
        }
    }

    fn toggle_stage(&mut self) {
        let Some(c) = self.current().cloned() else { return };
        if c.group == Group::Staged {
            self.start("unstaging", move |root| {
                // a rename is unstaged from both ends
                if let Some(from) = &c.from {
                    let _ = git(root, &["restore", "--staged", "--", from]);
                }
                let mut out = git(root, &["restore", "--staged", "--", &c.path]);
                if out.0 != 0 {
                    out = git(root, &["rm", "-q", "--cached", "--", &c.path]);
                }
                changed(out, &format!("unstaged {}", c.path))
            });
        } else {
            self.start("staging", move |root| {
                changed(git(root, &["add", "-A", "--", &c.path]), &format!("staged {}", c.path))
            });
        }
    }

    fn discard(&mut self, c: Change) {
        self.start("discarding", move |root| {
            // a rename goes back to its old name
            if let Some(from) = &c.from {
                let _ = git(root, &["restore", "--source=HEAD", "--staged", "--worktree", "--", from]);
            }
            let out = match c.group {
                Group::Untracked => git(root, &["clean", "-f", "-q", "--", &c.path]),
                Group::Changes => git(root, &["restore", "--", &c.path]),
                Group::Staged if git(root, &["cat-file", "-e", &format!("HEAD:{}", c.path)]).0 == 0 => git(
                    root,
                    &["restore", "--source=HEAD", "--staged", "--worktree", "--", &c.path],
                ),
                Group::Staged => {
                    let out = git(root, &["rm", "-q", "-f", "--cached", "--", &c.path]);
                    let _ = std::fs::remove_file(root.join(&c.path));
                    out
                }
            };
            changed(out, &format!("discarded {}", c.path))
        });
    }

    fn remote(&mut self, verb: &'static str) {
        self.start(&format!("{verb}ing"), move |root| {
            let (code, out, err) = git(root, &[verb]);
            Done {
                output: Some(output(format!("git {verb}"), code, &out, &err)),
                changed: true,
                ..Done::default()
            }
        });
    }

    fn switch_branch(&mut self, unsaved: bool) {
        let Some(e) = self.branches.get(self.selected).cloned() else {
            return;
        };
        if e.current {
            self.note = Some(info(format!("already on {}", e.id)));
            return;
        }
        if unsaved {
            self.note = Some(error("save your changes before switching branches"));
            return;
        }
        self.start("switching", move |root| {
            let (code, _, err) = git(root, &["switch", "-q", &e.id]);
            if code == 0 {
                return Done {
                    note: Some(info(format!("on {} now", e.id))),
                    changed: true,
                    ..Done::default()
                };
            }
            let why = if err.contains("would be overwritten") || err.contains("commit your changes") {
                "can't switch: commit or stash your changes first".to_string()
            } else {
                first_line(&err)
            };
            Done {
                note: Some(error(why)),
                ..Done::default()
            }
        });
    }

    fn commit(&mut self, message: String) {
        self.start("committing", move |root| {
            let (code, out, err) = git(root, &["commit", "-q", "-m", &message]);
            if code != 0 {
                let why = if err.trim().is_empty() { out } else { err };
                return Done {
                    note: Some(error(first_line(&why))),
                    ..Done::default()
                };
            }
            let sha = git(root, &["rev-parse", "--short", "HEAD"]).1;
            Done {
                note: Some(info(format!("committed {} {}", sha.trim(), first_line(&message)))),
                changed: true,
                ..Done::default()
            }
        });
    }

    fn command(&mut self, line: &str) {
        let mut args = match split_args(line) {
            Ok(a) => a,
            Err(e) => {
                self.note = Some(error(e));
                return;
            }
        };
        if args.first().is_some_and(|a| a == "git") {
            args.remove(0);
        }
        if args.is_empty() {
            return;
        }
        let title = format!("git {}", line.trim().trim_start_matches("git ").trim());
        self.start(&format!("running {title}"), move |root| {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let (code, out, err) = git(root, &args);
            Done {
                output: Some(output(title, code, &out, &err)),
                changed: true,
                ..Done::default()
            }
        });
    }

    fn type_key(&mut self, key: Key) {
        let Some((kind, text, at)) = self.typing.as_mut() else {
            return;
        };
        let kind = *kind;
        let byte = |s: &str, i: usize| s.char_indices().nth(i).map_or(s.len(), |(b, _)| b);
        match key {
            Key::Esc | Key::Ctrl('c') => self.typing = None,
            Key::Enter => {
                let line = std::mem::take(text);
                self.typing = None;
                match kind {
                    Typing::Commit if line.trim().is_empty() => {
                        self.note = Some(error("an empty message, so no commit"))
                    }
                    Typing::Commit => self.commit(line.trim().to_string()),
                    Typing::Command => {
                        if !line.trim().is_empty() {
                            self.history.retain(|h| h != &line);
                            self.history.push(line.clone());
                            if self.history.len() > 100 {
                                self.history.remove(0);
                            }
                        }
                        self.command(&line);
                    }
                }
            }
            Key::Char(c) => {
                let b = byte(text, *at);
                text.insert(b, c);
                *at += 1;
            }
            Key::Backspace if *at > 0 => {
                *at -= 1;
                let b = byte(text, *at);
                text.remove(b);
            }
            Key::Delete if *at < text.chars().count() => {
                let b = byte(text, *at);
                text.remove(b);
            }
            Key::Left => *at = at.saturating_sub(1),
            Key::Right => *at = (*at + 1).min(text.chars().count()),
            Key::Home | Key::Ctrl('a') => *at = 0,
            Key::End | Key::Ctrl('e') => *at = text.chars().count(),
            Key::Ctrl('u') => {
                let b = byte(text, *at);
                text.replace_range(..b, "");
                *at = 0;
            }
            Key::Up | Key::Down if kind == Typing::Command && !self.history.is_empty() => {
                let last = self.history.len() - 1;
                let next = match (self.recall, key) {
                    (None, Key::Up) => Some(last),
                    (None, _) => None,
                    (Some(i), Key::Up) => Some(i.saturating_sub(1)),
                    (Some(i), _) if i < last => Some(i + 1),
                    (Some(_), _) => None,
                };
                self.recall = next;
                let line = next.map_or_else(String::new, |i| self.history[i].clone());
                if let Some((_, text, at)) = self.typing.as_mut() {
                    *at = line.chars().count();
                    *text = line;
                }
            }
            _ => {}
        }
    }

    pub fn paste(&mut self, s: &str) {
        let line = s.lines().next().unwrap_or("");
        for c in line.chars() {
            self.type_key(Key::Char(c));
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

fn changed((code, out, err): (i32, String, String), done: &str) -> Done {
    let note = if code == 0 {
        info(done)
    } else {
        error(first_line(if err.trim().is_empty() { &out } else { &err }))
    };
    Done {
        note: Some(note),
        changed: true,
        ..Done::default()
    }
}

fn output(title: String, code: i32, out: &str, err: &str) -> Output {
    let mut lines: Vec<String> = tinsel::text::strip_ansi(out)
        .lines()
        .chain(tinsel::text::strip_ansi(err).lines())
        .map(|l| l.replace('\t', "    "))
        .take(MAX_LINES)
        .collect();
    if lines.is_empty() {
        lines.push("(no output)".into());
    }
    Output {
        title,
        lines,
        code,
        scroll: 0,
    }
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

fn read_log(root: &Path) -> Vec<Entry> {
    let n = format!("-n{LOG_LENGTH}");
    let (_, out, _) = git(root, &["log", &n, "--format=%h%x1f%an%x1f%ar%x1f%s"]);
    out.lines()
        .filter_map(|l| {
            let mut f = l.split('\x1f');
            let (id, who, when, subject) = (f.next()?, f.next()?, f.next()?, f.next().unwrap_or(""));
            Some(Entry {
                id: id.to_string(),
                title: subject.to_string(),
                detail: format!("{who}, {when}"),
                current: false,
            })
        })
        .collect()
}

fn read_branches(root: &Path) -> Vec<Entry> {
    let (_, out, _) = git(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(HEAD)%1f%(refname:short)%1f%(committerdate:relative)%1f%(contents:subject)",
            "refs/heads",
        ],
    );
    out.lines()
        .filter_map(|l| {
            let mut f = l.split('\x1f');
            let (head, name, when, subject) = (f.next()?, f.next()?, f.next()?, f.next().unwrap_or(""));
            Some(Entry {
                id: name.to_string(),
                title: subject.to_string(),
                detail: when.to_string(),
                current: head == "*",
            })
        })
        .collect()
}

enum Peeking {
    File(Change),
    Commit(String),
    Branch(String),
}

// the first megabyte, and whether there was more
fn read_start(path: &Path) -> Option<(String, bool)> {
    use std::io::Read;
    const CAP: usize = 1 << 20;
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(CAP as u64 + 1).read_to_end(&mut bytes).ok()?;
    let cut = bytes.len() > CAP;
    bytes.truncate(CAP);
    match String::from_utf8(bytes) {
        Ok(text) => Some((text, cut)),
        Err(e) if cut && e.utf8_error().valid_up_to() > 0 => {
            let valid = e.utf8_error().valid_up_to();
            let mut bytes = e.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).ok().map(|t| (t, cut))
        }
        Err(_) => None,
    }
}

fn peek(root: &Path, what: Peeking) -> Vec<String> {
    let lines = |s: String| -> Vec<String> { s.lines().map(|l| l.replace('\t', "    ")).take(MAX_LINES).collect() };
    match what {
        Peeking::File(c) if c.group == Group::Untracked => match read_start(&root.join(&c.path)) {
            Some((text, cut)) => {
                let more = if cut { "+" } else { "" };
                let mut v = vec![format!("@@ new file, {}{more} lines @@", text.lines().count())];
                v.extend(lines(text).into_iter().map(|l| format!("+{l}")));
                v
            }
            None => vec!["(not text, or not readable)".into()],
        },
        Peeking::File(c) => {
            let mut args = vec!["diff", "--no-ext-diff"];
            if c.group == Group::Staged {
                args.push("--cached");
            }
            args.extend(["--", &c.path]);
            let mut out = lines(git(root, &args).1);
            out.retain(|l| !l.starts_with("diff --git") && !l.starts_with("index "));
            out
        }
        Peeking::Commit(id) => lines(git(root, &["show", "--stat", "--format=medium", &id]).1),
        Peeking::Branch(id) => lines(git(root, &["log", "--oneline", "-n20", &id]).1),
    }
}

fn split_args(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' => word.extend(chars.next()),
            Some(_) => word.push(c),
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    in_word = true;
                }
                '\\' => {
                    word.extend(chars.next());
                    in_word = true;
                }
                c if c.is_whitespace() => {
                    if in_word {
                        out.push(std::mem::take(&mut word));
                        in_word = false;
                    }
                }
                c => {
                    word.push(c);
                    in_word = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err("a quote isn't closed".into());
    }
    if in_word {
        out.push(word);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_split_on_spaces_and_respect_quotes() {
        let s = |l: &str| split_args(l).unwrap();
        assert_eq!(s("log --oneline -5"), ["log", "--oneline", "-5"]);
        assert_eq!(s("commit -m 'two words'"), ["commit", "-m", "two words"]);
        assert_eq!(s(r#"commit -m "say \"hi\"" "#), ["commit", "-m", "say \"hi\""]);
        assert_eq!(s("a ''  b"), ["a", "", "b"]);
        assert!(split_args("log 'oops").is_err());
    }

    #[test]
    fn status_is_grouped() {
        let out = "## main...origin/main [ahead 2, behind 1]\0M  a.rs\0 M b.rs\0MM c.rs\0R  new.rs\0old.rs\0?? d.rs\0";
        let (branch, up, changes) = parse_status(out);
        assert_eq!((branch.as_str(), up), ("main", Some((2, 1))));
        let shown: Vec<_> = changes.iter().map(|c| (c.group, c.letter, c.path.as_str())).collect();
        assert_eq!(
            shown,
            [
                (Group::Staged, 'M', "a.rs"),
                (Group::Staged, 'M', "c.rs"),
                (Group::Staged, 'R', "new.rs"),
                (Group::Changes, 'M', "b.rs"),
                (Group::Changes, 'M', "c.rs"),
                (Group::Untracked, 'U', "d.rs"),
            ]
        );
        assert_eq!(parse_status("## No commits yet on trunk\0").0, "trunk");
        assert_eq!(parse_status("## trunk\0").1, None);
    }
}
