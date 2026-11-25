//! Keeping tvim up to date: how it was installed, whether a newer release is available, and for an
//! install by install.sh, updating it in place: download, unpack and swap the binary, drawn as
//! a tinsel step list under a strip of stars.
//!
//! A check runs in the background at most once a day and keeps what it found in the state
//! folder, so the next launch can offer it without waiting on the network. Checks ask GitHub
//! with curl, which install.sh needs too, so tvim itself needs no HTTP of its own.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tinsel::{Block, Card, Live, Region, bold, mix, paint, palette};

const RELEASES: &str = "https://github.com/borhara/tvim/releases";
const DAY: u64 = 24 * 60 * 60;

/// How this tvim was installed, which decides how it's updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// By install.sh, into this folder, where it can update itself.
    Script(PathBuf),
    Homebrew,
    Cargo,
    /// Some way tvim can't tell, in a folder it can't write to, like a package manager's.
    Elsewhere,
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
            Ok(exe) => match Install::of(&real(exe), cargo_bin.as_deref()) {
                Install::Script(dir) if !writable(&dir) => Install::Elsewhere,
                install => install,
            },
            Err(_) => Install::Dev,
        }
    }

    /// The news of a newer version, for an install tvim can't update itself.
    fn news(&self, latest: &str) -> String {
        match self {
            Install::Dev => {
                format!("tvim v{latest} is available; this one was built from a checkout, so pull and build again")
            }
            Install::Elsewhere => format!("tvim v{latest} is available: update it the way you installed it"),
            install => format!(
                "tvim v{latest} is available: run {}",
                install.command().unwrap_or_default()
            ),
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

/// Whether tvim could put a new binary in `dir`.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".tvim-update-{}", std::process::id()));
    let wrote = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    wrote
}

/// The release archive for this machine, named as install.sh and the release script name them.
pub fn archive(arch: &str, os: &str) -> Option<String> {
    let arch = match arch {
        "aarch64" | "x86_64" => arch,
        _ => return None,
    };
    let os = match os {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-musl",
        _ => return None,
    };
    Some(format!("tvim-{arch}-{os}.tar.gz"))
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

/// A row like the skyline along the bottom of tvim, at `t` seconds: dim dots, stars fading up
/// through ⋆ ✧ ✦ and back. Busier and quicker than the editor's, for a moment of news.
fn skyline(width: usize, t: f64) -> String {
    let rest = mix(mix(palette::LINE, palette::SOFT, 0.35), palette::MUTED, 0.3);
    let row: String = (0..width)
        .map(|i| match crate::ui::twinkle_among(i, width, t * 2.0, 5) {
            tw if tw >= 0.08 => paint(
                tinsel::style::sample(&[palette::BERRY, palette::ROSE, palette::SHINE], tw),
                ["⋆", "✧", "✦"][((tw * 3.0) as usize).min(2)],
            ),
            _ => paint(rest, "·"),
        })
        .collect();
    format!("  {row}")
}

/// What's out, after a mark.
fn news(mark: &str, latest: &str, current: &str) -> String {
    format!(
        "  {mark} {}  {}",
        bold(&format!("tvim v{latest} is available")),
        paint(palette::MUTED, &format!("you have v{current}"))
    )
}

fn sky_width() -> usize {
    tinsel::terminal_width().saturating_sub(4).clamp(20, 60)
}

/// The news, with the skyline still, for when nothing's asked.
fn announce(latest: &str, current: &str) {
    println!();
    println!("{}", skyline(sky_width(), 4.0));
    println!("{}", news(&paint(palette::ROSE, "✦"), latest, current));
    println!();
}

/// Asks whether to update, with the skyline twinkling above the question the whole time it
/// waits. One key answers: y or Enter yes, n no, and
/// Esc or Ctrl-C not now (None), as does there being no terminal to ask on.
fn ask(latest: &str, current: &str) -> Option<bool> {
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let Some(keys) = terminal.then(Keys::raw).flatten() else {
        announce(latest, current);
        return tinsel::prompt::confirm("Update now?", true).ok();
    };
    let answer = std::sync::Arc::new(std::sync::Mutex::new(None::<&'static str>));
    let (width, began, shown) = (sky_width(), Instant::now(), answer.clone());
    let (latest, current) = (latest.to_string(), current.to_string());
    println!();
    let region = Region::new(move || {
        // a few seconds in, so some stars are already out
        let t = began.elapsed().as_secs_f64() + 3.0;
        let reply = match *shown.lock().unwrap() {
            Some(said) => paint(palette::SOFT, said),
            None => paint(palette::MUTED, "Y/n"),
        };
        let mut frame = Block::new();
        frame.push(skyline(width, t));
        frame.push(news(&paint(palette::ROSE, "✦"), &latest, &current));
        frame.push(String::new());
        frame.push(format!(
            "  {}  {}  {reply}",
            paint(palette::ROSE, "?"),
            bold("Update now?")
        ));
        frame
    })
    .keep();
    let choice = keys.answer();
    drop(keys);
    *answer.lock().unwrap() = Some(match choice {
        Some(true) => "yes",
        Some(false) => "no",
        None => "not now",
    });
    // a beat for the answer to be drawn before the last frame stays
    std::thread::sleep(Duration::from_millis(120));
    region.finish();
    println!();
    choice
}

/// The terminal taken out of line mode to read single keys, and put back when dropped.
struct Keys {
    saved: String,
}

impl Keys {
    /// Keys one at a time, unechoed, with Ctrl-C read as a key rather than ending tvim; None
    /// when stty can't do it.
    fn raw() -> Option<Keys> {
        let stty = |args: &[&str]| Command::new("stty").args(args).stdin(Stdio::inherit()).output().ok();
        let saved = stty(&["-g"]).filter(|o| o.status.success())?;
        let saved = String::from_utf8_lossy(&saved.stdout).trim().to_string();
        stty(&["-icanon", "-echo", "-isig", "min", "1"])
            .filter(|o| o.status.success())
            .map(|_| Keys { saved })
    }

    fn answer(&self) -> Option<bool> {
        use std::io::Read;
        let mut byte = [0u8];
        while std::io::stdin().read_exact(&mut byte).is_ok() {
            match byte[0] {
                b'y' | b'Y' | b'\r' | b'\n' => return Some(true),
                b'n' | b'N' => return Some(false),
                // Esc (and anything starting with it, like an arrow key), Ctrl-C, Ctrl-D, q
                0x1b | 0x03 | 0x04 | b'q' => return None,
                _ => {}
            }
        }
        None
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        let _ = Command::new("stty").arg(&self.saved).stdin(Stdio::inherit()).status();
    }
}

/// Downloads `version` for this machine, unpacks it, and swaps it in for the tvim in `dir`,
/// as a step list ending in a card. The new binary goes beside the old one and is renamed over
/// it, so a failure at any point leaves the old one as it was.
pub fn update_in(dir: &Path, version: &str) -> Option<PathBuf> {
    update_from(RELEASES, dir, version)
}

/// As update_in, downloading from `releases`, which tests point at a folder of their own.
fn update_from(releases: &str, dir: &Path, version: &str) -> Option<PathBuf> {
    let Some(name) = archive(std::env::consts::ARCH, std::env::consts::OS) else {
        tinsel::error("there's no prebuilt tvim for this machine: use cargo install tvim");
        return None;
    };
    let work = std::env::temp_dir().join(format!("tvim-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    if let Err(e) = std::fs::create_dir_all(&work) {
        tinsel::error(&format!("couldn't make a folder to update in: {e}"));
        return None;
    }
    let live = Live::new("tvim", format!("v{version}")).no_history().no_notify();
    let (download, unpack, install) = (live.step("Download"), live.step("Unpack"), live.step("Install"));
    let tvim = dir.join("tvim");
    let ok = download
        .run(
            Command::new("curl")
                .args(["-fsSL", "--max-time", "120", "-o"])
                .arg(work.join(&name))
                .arg(format!("{releases}/download/v{version}/{name}")),
        )
        .is_ok()
        && unpack
            .run(
                Command::new("tar")
                    .arg("xzf")
                    .arg(work.join(&name))
                    .arg("-C")
                    .arg(&work),
            )
            .is_ok()
        && {
            install.start();
            install.detail(dir.display().to_string());
            // checked before it replaces anything
            let new = work.join("tvim");
            let swapped = if is_ok_version(&new, version) {
                swap(&new, &tvim).map_err(|e| e.to_string())
            } else {
                Err(format!("the download isn't tvim v{version}"))
            };
            match &swapped {
                Ok(()) => install.done(),
                Err(why) => install.fail(why.clone()),
            }
            swapped.is_ok()
        };
    let _ = std::fs::remove_dir_all(&work);
    if !ok {
        live.finish();
        return None;
    }
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty());
    let shown = match home.as_deref().and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    };
    live.finish_with_card(Card::success(&format!("updated to tvim v{version}")).kv("in", &shown));
    Some(tvim)
}

/// Puts `new` where `old` is: copied in beside it first, then renamed over it in one go.
fn swap(new: &Path, old: &Path) -> std::io::Result<()> {
    let beside = old.with_file_name(format!(".tvim-new-{}", std::process::id()));
    std::fs::copy(new, &beside)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&beside, old).inspect_err(|_| {
        let _ = std::fs::remove_file(&beside);
    })
}

/// Whether the binary at `tvim` says it's `version`.
fn is_ok_version(tvim: &Path, version: &str) -> bool {
    Command::new(tvim)
        .arg("--version")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == format!("tvim {version}"))
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

/// At launch: asks whether to update an install.sh install, or for the others says what to
/// run, once for each new version.
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
            match ask(&latest, current) {
                Some(true) => {
                    return match update_in(bin, &latest) {
                        Some(tvim) => {
                            // a moment to see the card before the editor takes the screen
                            std::thread::sleep(Duration::from_millis(900));
                            Offer::Restart(tvim)
                        }
                        None => Offer::Nothing,
                    };
                }
                Some(false) => Offer::Nothing,
                // Esc, or no terminal to ask on: ask again next time
                None => return Offer::Nothing,
            }
        }
        _ => Offer::Tell(install.news(&latest)),
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
        tinsel::success(&format!("tvim v{current} is the latest"));
        return true;
    }
    match Install::current() {
        Install::Script(dir) => {
            announce(&latest, current);
            update_in(&dir, &latest).is_some()
        }
        install => {
            tinsel::info(&install.news(&latest));
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
        assert_eq!(Install::Elsewhere.command(), None);
        assert!(Install::Elsewhere.news("1.4.0").contains("the way you installed it"));
    }

    #[test]
    fn the_news_says_how_each_install_is_updated() {
        assert_eq!(
            Install::Homebrew.news("1.4.1"),
            "tvim v1.4.1 is available: run brew upgrade tvim"
        );
        assert_eq!(
            Install::Cargo.news("1.4.1"),
            "tvim v1.4.1 is available: run cargo install tvim"
        );
        assert!(Install::Dev.news("1.4.1").contains("pull and build again"));
        assert_eq!(Install::Dev.command(), None);
        assert_eq!(Install::Script("/home/me/.local/bin".into()).command(), None);
    }

    #[test]
    fn the_new_binary_is_swapped_in_whole_and_runnable() {
        let dir = std::env::temp_dir().join(format!("tvim-swap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (new, old) = (dir.join("new"), dir.join("tvim"));
        std::fs::write(&new, "new").unwrap();
        std::fs::write(&old, "old").unwrap();
        swap(&new, &old).unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&old).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left.len(), 2, "nothing left beside it: {left:?}");
        assert!(swap(&dir.join("missing"), &old).is_err());
        assert_eq!(
            std::fs::read_to_string(&old).unwrap(),
            "new",
            "a failed swap leaves it be"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_download_is_only_trusted_when_it_says_its_the_version_asked_for() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tvim-version-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tvim = dir.join("tvim");
        std::fs::write(&tvim, "#!/bin/sh\necho 'tvim 1.4.1'\n").unwrap();
        std::fs::set_permissions(&tvim, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_ok_version(&tvim, "1.4.1"));
        assert!(!is_ok_version(&tvim, "1.4.0"));
        assert!(!is_ok_version(&dir.join("missing"), "1.4.1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_release_is_downloaded_unpacked_and_swapped_in() {
        use std::os::unix::fs::PermissionsExt;
        let Some(name) = archive(std::env::consts::ARCH, std::env::consts::OS) else {
            return;
        };
        let root = std::env::temp_dir().join(format!("tvim-release-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (releases, bin, build) = (root.join("releases"), root.join("bin"), root.join("build"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&build).unwrap();
        let tvim = bin.join("tvim");
        std::fs::write(&tvim, "old").unwrap();
        // a release on disk as GitHub lays one out, holding a tvim that says it's `says`
        let release = |version: &str, says: &str| {
            let new = build.join("tvim");
            std::fs::write(&new, format!("#!/bin/sh\necho 'tvim {says}'\n")).unwrap();
            std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)).unwrap();
            let at = releases.join(format!("download/v{version}"));
            std::fs::create_dir_all(&at).unwrap();
            let tar = Command::new("tar")
                .arg("czf")
                .arg(at.join(&name))
                .arg("-C")
                .arg(&build)
                .arg("tvim")
                .status()
                .unwrap();
            assert!(tar.success());
        };
        let url = format!("file://{}", releases.display());

        release("9.0.1", "9.0.0");
        assert_eq!(update_from(&url, &bin, "9.0.1"), None, "not the version it says");
        assert_eq!(std::fs::read_to_string(&tvim).unwrap(), "old");
        assert_eq!(update_from(&url, &bin, "9.0.2"), None, "no such release");
        assert_eq!(std::fs::read_to_string(&tvim).unwrap(), "old");

        release("9.0.3", "9.0.3");
        assert_eq!(update_from(&url, &bin, "9.0.3"), Some(tvim.clone()));
        assert!(is_ok_version(&tvim, "9.0.3"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_archive_is_named_for_the_machine() {
        assert_eq!(
            archive("aarch64", "macos").as_deref(),
            Some("tvim-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(
            archive("x86_64", "linux").as_deref(),
            Some("tvim-x86_64-unknown-linux-musl.tar.gz")
        );
        assert_eq!(archive("x86_64", "windows"), None);
        assert_eq!(archive("riscv64", "linux"), None);
    }

    #[test]
    fn a_folder_that_cant_be_written_to_isnt_updated_in() {
        let dir = std::env::temp_dir().join(format!("tvim-writable-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(writable(&dir));
        assert!(!writable(&dir.join("missing")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_skyline_is_dots_with_a_few_stars_coming_and_going() {
        let plain = |t| tinsel::with_color_mode(tinsel::ColorMode::None, || skyline(40, t));
        assert_eq!(
            plain(0.0),
            format!("  {}", "·".repeat(40)),
            "nothing out before it starts"
        );
        let rows: Vec<String> = (1..40).map(|t| plain(t as f64 * 0.5)).collect();
        assert!(rows.iter().all(|row| row.chars().count() == 42), "{rows:?}");
        assert!(rows.iter().any(|row| row.contains('✦')), "stars come out: {rows:?}");
        assert!(rows.iter().any(|row| row != &rows[0]), "and move: {rows:?}");
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
