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
