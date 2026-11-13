use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
pub enum Want {
    Nothing,
    Close,
    Open(PathBuf),
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
