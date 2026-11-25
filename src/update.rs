//! Keeping tvim up to date: how it was installed, whether a newer release is out, and for an
//! install by install.sh, updating it in place.
//!
//! A check runs in the background at most once a day and keeps what it found in the state
//! folder, so the next launch can offer it without waiting on the network. Checks ask GitHub
//! with curl, which install.sh needs too, so tvim itself needs no HTTP of its own.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const RELEASES: &str = "https://github.com/borhara/tvim/releases";
const DAY: u64 = 24 * 60 * 60;

/// How this tvim was installed, which decides how it's updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// By install.sh, into this folder, where it can update itself.
    Script(PathBuf),
    Homebrew,
    Cargo,
    /// Built from a checkout, which updates with the code.
    Dev,
}

impl Install {
    /// How the binary at `exe` was installed, given the folder cargo installs binaries into.
    pub fn of(exe: &Path, cargo_bin: Option<&Path>) -> Install {
        let has = |name: &str| exe.components().any(|c| c.as_os_str() == name);
        if has("Cellar") {
            Install::Homebrew
        } else if cargo_bin.is_some_and(|bin| exe.starts_with(bin)) {
            Install::Cargo
        } else if has("target") {
            Install::Dev
        } else if !cfg!(unix) {
            // install.sh doesn't run there, so cargo is the only other way in
            Install::Cargo
        } else {
            exe.parent()
                .map_or(Install::Dev, |dir| Install::Script(dir.to_path_buf()))
        }
    }

    /// This tvim's, from where its binary really is: Homebrew's `bin` links into its Cellar.
    pub fn current() -> Install {
        let real = |p: PathBuf| p.canonicalize().unwrap_or(p);
        let cargo_bin = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
            .map(|home| real(home.join("bin")));
        match std::env::current_exe() {
            Ok(exe) => Install::of(&real(exe), cargo_bin.as_deref()),
            Err(_) => Install::Dev,
        }
    }

    /// What to run to update, when tvim can't update itself.
    pub fn command(&self) -> Option<&'static str> {
        match self {
            Install::Homebrew => Some("brew upgrade tvim"),
            Install::Cargo => Some("cargo install tvim"),
            _ => None,
        }
    }
}

/// Whether `latest` is a later version than `current`, both as in 1.4.0.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |v: &str| v.split('.').map(|n| n.parse::<u64>().ok()).collect::<Option<Vec<_>>>();
    match (parts(latest), parts(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// The version a link to the latest release lands on, as in `…/releases/tag/v1.4.0`.
pub fn version_in(url: &str) -> Option<String> {
    let version = url.trim().rsplit_once("/releases/tag/v")?.1;
    let numbers = version
        .split('.')
        .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    numbers.then(|| version.to_string())
}

/// The latest release's version, from GitHub, or None offline or without curl.
pub fn latest() -> Option<String> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "10", "-o", null, "-w", "%{url_effective}"])
        .arg(format!("{RELEASES}/latest"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| version_in(&String::from_utf8_lossy(&out.stdout)))?
}

/// What the checks have found, kept in the state folder between launches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// When GitHub was last asked, in seconds since 1970.
    pub checked: u64,
    pub latest: Option<String>,
    /// A version already offered and turned down, or told about, so it isn't brought up again.
    pub skipped: Option<String>,
}

impl State {
    fn path(dir: &Path) -> PathBuf {
        dir.join("update")
    }

    pub fn parse(text: &str) -> State {
        let mut state = State::default();
        for line in text.lines() {
            match line.split_once(' ') {
                Some(("checked", v)) => state.checked = v.parse().unwrap_or(0),
                Some(("latest", v)) => state.latest = Some(v.to_string()),
                Some(("skipped", v)) => state.skipped = Some(v.to_string()),
                _ => {}
            }
        }
        state
    }

    pub fn text(&self) -> String {
        let mut text = format!("checked {}\n", self.checked);
        for (key, value) in [("latest", &self.latest), ("skipped", &self.skipped)] {
            if let Some(v) = value {
                text.push_str(&format!("{key} {v}\n"));
            }
        }
        text
    }

    pub fn read(dir: &Path) -> State {
        std::fs::read_to_string(State::path(dir)).map_or_else(|_| State::default(), |t| State::parse(&t))
    }

    /// Written whole and then moved into place, so a tvim quitting midway leaves the old one.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let partial = dir.join(format!("update.{}", std::process::id()));
        std::fs::write(&partial, self.text())?;
        std::fs::rename(&partial, State::path(dir))
    }

    /// A version to bring up: later than `current`, and not turned down already.
    pub fn pending(&self, current: &str) -> Option<&str> {
        let latest = self.latest.as_deref()?;
        (is_newer(latest, current) && self.skipped.as_deref() != Some(latest)).then_some(latest)
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Asks GitHub for the latest version on another thread, unless it was asked in the last day,
/// and keeps the answer in `dir` for the next launch.
pub fn check_in_background(dir: &Path) {
    if now().saturating_sub(State::read(dir).checked) < DAY {
        return;
    }
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let Some(latest) = latest() else { return };
        let mut state = State::read(&dir);
        state.checked = now();
        state.latest = Some(latest);
        let _ = state.write(&dir);
    });
}

/// Runs the latest release's install.sh into `dir`, replacing the tvim there. Downloaded first
/// rather than piped, so a failed download isn't mistaken for a finished install.
pub fn install_into(dir: &Path) -> Result<(), String> {
    let script = std::env::temp_dir().join(format!("tvim-install-{}.sh", std::process::id()));
    let downloaded = Command::new("curl")
        .args(["-fsSL", "--max-time", "60", "-o"])
        .arg(&script)
        .arg(format!("{RELEASES}/latest/download/install.sh"))
        .status();
    let ran = match downloaded {
        Ok(s) if s.success() => Command::new("sh").arg(&script).env("TVIM_INSTALL_DIR", dir).status(),
        Ok(_) => return Err("couldn't download the installer".into()),
        Err(e) => return Err(format!("couldn't run curl: {e}")),
    };
    let _ = std::fs::remove_file(&script);
    match ran {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err("the installer failed".into()),
        Err(e) => Err(format!("couldn't run the installer: {e}")),
    }
}

/// What a launch should do about a newer version an earlier check found.
#[derive(Debug, PartialEq, Eq)]
pub enum Offer {
    Nothing,
    /// Updated in place: start this binary instead.
    Restart(PathBuf),
    /// Say this in the editor.
    Tell(String),
}

/// At launch: asks whether to update an install.sh install, or for Homebrew and cargo says
/// what to run, once for each new version.
pub fn offer(dir: &Path) -> Offer {
    let current = env!("CARGO_PKG_VERSION");
    let mut state = State::read(dir);
    let Some(latest) = state.pending(current).map(str::to_string) else {
        return Offer::Nothing;
    };
    let install = Install::current();
    let offer = match &install {
        Install::Dev => return Offer::Nothing,
        Install::Script(bin) => {
            let question = format!("tvim {latest} is out (you have {current}). Update now?");
            match tinsel::prompt::confirm(&question, true) {
                Ok(true) => match install_into(bin) {
                    Ok(()) => return Offer::Restart(bin.join("tvim")),
                    Err(why) => {
                        tinsel::error(&format!("couldn't update tvim: {why}"));
                        return Offer::Nothing;
                    }
                },
                Ok(false) => Offer::Nothing,
                // Esc, or no terminal to ask on: ask again next time
                Err(_) => return Offer::Nothing,
            }
        }
        _ => Offer::Tell(format!(
            "tvim {latest} is out: run {}",
            install.command().unwrap_or_default()
        )),
    };
    state.skipped = Some(latest);
    let _ = state.write(dir);
    offer
}

/// `tvim --update`: checks now, and updates or says how.
pub fn now_or_say() -> bool {
    let current = env!("CARGO_PKG_VERSION");
    let Some(latest) = tinsel::spin("Asking GitHub for the latest tvim", latest) else {
        tinsel::error("couldn't reach GitHub to check for a newer tvim");
        return false;
    };
    if let Some(dir) = crate::session::state_dir() {
        let mut state = State::read(&dir);
        state.checked = now();
        state.latest = Some(latest.clone());
        let _ = state.write(&dir);
    }
    if !is_newer(&latest, current) {
        tinsel::success(&format!("tvim {current} is the latest"));
        return true;
    }
    match Install::current() {
        Install::Script(dir) => match install_into(&dir) {
            Ok(()) => true,
            Err(why) => {
                tinsel::error(&format!("couldn't update tvim: {why}"));
                false
            }
        },
        Install::Dev => {
            tinsel::info(&format!(
                "tvim {latest} is out; this one was built from a checkout, so pull and build again"
            ));
            true
        }
        install => {
            tinsel::info(&format!(
                "tvim {latest} is out: run {}",
                install.command().unwrap_or_default()
            ));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_is_told_by_where_the_binary_is() {
        let cargo = Path::new("/home/me/.cargo/bin");
        let of = |exe: &str| Install::of(Path::new(exe), Some(cargo));
        assert_eq!(of("/opt/homebrew/Cellar/tvim/1.3.0/bin/tvim"), Install::Homebrew);
        assert_eq!(
            of("/home/linuxbrew/.linuxbrew/Cellar/tvim/1.3.0/bin/tvim"),
            Install::Homebrew
        );
        assert_eq!(of("/home/me/.cargo/bin/tvim"), Install::Cargo);
        assert_eq!(of("/home/me/code/tvim/target/release/tvim"), Install::Dev);
        if cfg!(unix) {
            assert_eq!(
                of("/home/me/.local/bin/tvim"),
                Install::Script("/home/me/.local/bin".into())
            );
        }
        assert_eq!(Install::Homebrew.command(), Some("brew upgrade tvim"));
        assert_eq!(Install::Cargo.command(), Some("cargo install tvim"));
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("1.4.0", "1.3.0"));
        assert!(is_newer("1.10.0", "1.9.2"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.3.0", "1.3.0"));
        assert!(!is_newer("1.2.9", "1.3.0"));
        assert!(!is_newer("nonsense", "1.3.0"));
    }

    #[test]
    fn the_version_is_read_from_where_the_latest_release_link_lands() {
        let url = "https://github.com/borhara/tvim/releases/tag/v1.4.0";
        assert_eq!(version_in(url).as_deref(), Some("1.4.0"));
        assert_eq!(version_in(&format!("{url}\n")).as_deref(), Some("1.4.0"));
        // no releases yet: the link stays on the releases page
        assert_eq!(version_in("https://github.com/borhara/tvim/releases"), None);
        assert_eq!(version_in("https://github.com/borhara/tvim/releases/tag/vnext"), None);
    }

    #[test]
    fn what_was_found_is_kept_between_launches() {
        let state = State {
            checked: 1_764_100_000,
            latest: Some("1.4.0".into()),
            skipped: None,
        };
        assert_eq!(state.text(), "checked 1764100000\nlatest 1.4.0\n");
        assert_eq!(State::parse(&state.text()), state);
        assert_eq!(State::parse("nonsense\n"), State::default());

        let dir = std::env::temp_dir().join(format!("tvim-update-test-{}", std::process::id()));
        state.write(&dir).unwrap();
        assert_eq!(State::read(&dir), state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_version_is_brought_up_until_turned_down() {
        let mut state = State {
            latest: Some("1.4.0".into()),
            ..State::default()
        };
        assert_eq!(state.pending("1.3.0"), Some("1.4.0"));
        assert_eq!(state.pending("1.4.0"), None, "already on it");
        state.skipped = Some("1.4.0".into());
        assert_eq!(state.pending("1.3.0"), None, "turned down");
        state.latest = Some("1.5.0".into());
        assert_eq!(state.pending("1.3.0"), Some("1.5.0"), "a later one is brought up again");
        assert_eq!(State::default().pending("1.3.0"), None, "never checked");
    }
}
