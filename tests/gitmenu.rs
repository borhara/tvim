mod common;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::temp;
use tvim::Editor;
use tvim::clipboard::Memory;
use tvim::gitmenu::Group;
use tvim::keys::parse;

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// a repo on trunk with a.txt committed by t@t; its own config, so tvim's commits work
// without reading or touching anyone's global one
fn repo(name: &str, committed: &str) -> Option<PathBuf> {
    let dir = temp(&format!("menu-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if git(&dir, &["init", "-q"]).is_none() {
        eprintln!("no git, skipping");
        return None;
    }
    for (k, v) in [
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(&dir, &["config", k, v]).unwrap();
    }
    git(&dir, &["symbolic-ref", "HEAD", "refs/heads/trunk"]).unwrap();
    std::fs::write(dir.join("a.txt"), committed).unwrap();
    git(&dir, &["add", "a.txt"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "first"]).unwrap();
    Some(dir)
}

fn open(path: &Path) -> Editor {
    let mut e = Editor::open(path).unwrap();
    e.set_clipboard(Box::new(Memory::default()));
    e.set_screen(100, 30);
    e.wait_for_git();
    e
}

fn menu(e: &mut Editor, keys: &str) {
    e.keys(&parse(keys));
    e.wait_for_git_menu();
}

fn shown(e: Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&tvim::ui::Tvim::new(e), width, height).plain()
    })
}

fn groups(e: &Editor) -> Vec<(Group, String)> {
    let m = e.git_menu().unwrap();
    m.changes().iter().map(|c| (c.group, c.path.clone())).collect()
}

#[test]
fn the_lens_names_who_changed_the_line() {
    let Some(dir) = repo("lens", "a\nb\nc\n") else { return };
    let mut e = open(&dir.join("a.txt"));
    e.wait_for_blame();
    assert_eq!(e.lens().as_deref(), Some("you, just now · first"));
    git(&dir, &["config", "user.email", "someone@else"]).unwrap();
    let e2 = open(&dir.join("a.txt"));
    e2.wait_for_blame();
    assert_eq!(
        e2.lens().as_deref(),
        Some("t, just now · first"),
        "a new blame reads the email again"
    );

    e.keys(&parse("jOnew<Esc>"));
    assert_eq!(e.lens().as_deref(), Some("not committed yet"));
    e.keys(&parse("jj"));
    assert_eq!(e.cursor().row, 3);
    assert_eq!(
        e.lens().as_deref(),
        Some("you, just now · first"),
        "rows past an edit map back to HEAD"
    );
}

#[test]
fn the_lens_hides_while_typing_and_when_off() {
    let Some(dir) = repo("lens-rules", "a\n") else { return };
    let mut e = open(&dir.join("a.txt"));
    e.wait_for_blame();
    assert!(e.lens().is_some());
    e.keys(&parse("A"));
    assert_eq!(e.lens(), None, "not in insert mode");
    e.keys(&parse("<Esc>d"));
    assert_eq!(e.lens(), None, "not halfway through a command");
    e.keys(&parse("<Esc>"));
    assert!(e.lens().is_some());
    e.ex("set nogl");
    assert_eq!(e.lens(), None);
    e.ex("set gitlens");
    assert!(e.lens().is_some());

    let outside = temp("lens-outside.txt");
    std::fs::write(&outside, "x\n").unwrap();
    let e = open(&outside);
    e.wait_for_blame();
    if git(outside.parent().unwrap(), &["rev-parse"]).is_none() {
        assert_eq!(e.lens(), None, "no lens outside a repo");
    }
}

#[test]
fn the_lens_is_drawn_after_the_line_only_when_it_fits() {
    let Some(dir) = repo("lens-draw", "short\n") else {
        return;
    };
    let e = open(&dir.join("a.txt"));
    e.wait_for_blame();
    let wide = shown(e, 100, 10);
    assert!(wide.contains("short    you, just now · first"), "{wide}");
    let e = open(&dir.join("a.txt"));
    e.wait_for_blame();
    let narrow = shown(e, 34, 10);
    assert!(
        narrow.contains("short    you, just now") && !narrow.contains("first"),
        "{narrow}"
    );
    let e = open(&dir.join("a.txt"));
    e.wait_for_blame();
    let tiny = shown(e, 22, 10);
    assert!(!tiny.contains("you"), "{tiny}");
}

#[test]
fn space_capital_g_opens_the_menu_with_grouped_changes() {
    let Some(dir) = repo("open", "a\n") else { return };
    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
    std::fs::write(dir.join("b.txt"), "staged\n").unwrap();
    std::fs::write(dir.join("c.txt"), "new\n").unwrap();
    git(&dir, &["add", "b.txt"]).unwrap();
    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>G");
    let m = e.git_menu().expect("open");
    assert!(m.ready());
    assert_eq!(m.status().unwrap().branch, "trunk");
    assert_eq!(
        groups(&e),
        [
            (Group::Staged, "b.txt".into()),
            (Group::Changes, "a.txt".into()),
            (Group::Untracked, "c.txt".into()),
        ]
    );
    assert_eq!(e.lens(), None, "no lens under the menu");
    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>G");
    let screen = shown(e, 110, 30);
    for want in [
        "⎇  trunk",
        "Staged 1",
        "Changes 1",
        "Untracked 1",
        "+staged",
        "s stage",
        "c commit",
    ] {
        assert!(screen.contains(want), "{want}\n{screen}");
    }
}

#[test]
fn discard_asks_first() {
    let Some(dir) = repo("discard", "a\n") else { return };
    let file = dir.join("a.txt");
    std::fs::write(&file, "oops\n").unwrap();
    let mut e = open(&file);
    assert_eq!(e.text(), "oops");
    menu(&mut e, "<Space>Gd");
    assert!(e.git_menu().unwrap().confirming().is_some());
    menu(&mut e, "n");
    assert!(e.git_menu().unwrap().confirming().is_none());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "oops\n", "n keeps it");
    menu(&mut e, "dy");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a\n");
    assert_eq!(groups(&e), []);
    assert_eq!(e.text(), "a", "the clean buffer is read again");

    std::fs::write(dir.join("junk.txt"), "x\n").unwrap();
    menu(&mut e, "r");
    menu(&mut e, "dy");
    assert!(!dir.join("junk.txt").exists(), "untracked files are removed");
}
