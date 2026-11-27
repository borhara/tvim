mod common;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::temp;
use tvim::Editor;
use tvim::clipboard::Memory;
use tvim::keys::parse;

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

// a repo with a.txt committed as `committed`, or None without git
fn repo(name: &str, committed: &str) -> Option<PathBuf> {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if !git(&dir, &["init", "-q"]) {
        return None;
    }
    git(&dir, &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.join("a.txt"), committed).unwrap();
    assert!(git(&dir, &["add", "a.txt"]));
    assert!(git(&dir, &["commit", "-q", "-m", "first"]));
    Some(dir)
}

fn open(path: &Path) -> Editor {
    let mut e = Editor::open(path).unwrap();
    e.set_clipboard(Box::new(Memory::default()));
    e.set_screen(80, 24);
    e.wait_for_git();
    e
}

// the background colour the text `word` is drawn on
fn background(e: &Editor, word: &str) -> String {
    let screen = tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
        let mut frame = tinsel::app::Frame::new(80, 20);
        tvim::ui::draw(e, &mut frame);
        frame.snapshot().to_string()
    });
    let seg = screen
        .split("\u{1b}[")
        .find(|s| s.split_once('m').is_some_and(|(_, t)| t.trim_start().starts_with(word)))
        .unwrap_or_else(|| panic!("{word} in {screen:?}"));
    let code = seg.split('m').next().unwrap();
    code.split(';')
        .skip_while(|p| *p != "48")
        .take(5)
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn gdiff_opens_head_beside_the_file_to_read_only() {
    let Some(dir) = repo("gdiff", "one\ntwo\nthree\n") else {
        return;
    };
    let file = dir.join("a.txt");
    std::fs::write(&file, "one\nTWO\nthree\nfour\n").unwrap();
    let mut e = open(&file);
    e.ex("Gdiff");
    assert_eq!(e.window_count(), 2);
    assert_eq!(e.text(), "one\nTWO\nthree\nfour", "the file keeps the focus");
    let names: Vec<String> = e.buffers().into_iter().map(|b| b.name).collect();
    assert!(names.contains(&"a.txt @ HEAD".to_string()), "{names:?}");
    e.keys(&parse("<C-w>h"));
    assert_eq!(e.text(), "one\ntwo\nthree");
    e.keys(&parse("x"));
    assert_eq!(e.text(), "one\ntwo\nthree");
    assert_eq!(e.message().unwrap().text, "a.txt @ HEAD can't be changed");
    e.ex("w");
    assert_eq!(e.message().unwrap().text, "a.txt @ HEAD isn't a file to write");
    assert!(!e.is_modified());
}

#[test]
fn compared_lines_are_tinted_until_diffoff() {
    let Some(dir) = repo("gdiff_tint", "one\ntwo\nthree\n") else {
        return;
    };
    let file = dir.join("a.txt");
    std::fs::write(&file, "one\nTWO\nthree\nfour\n").unwrap();
    let mut e = open(&file);
    e.ex("Gdiff");
    let plain = background(&e, "three");
    let (changed, added, gone) = (background(&e, "TWO"), background(&e, "four"), background(&e, "two"));
    assert_ne!(changed, plain);
    assert_ne!(added, plain);
    assert_ne!(added, changed);
    assert_eq!(gone, changed, "a changed line, as it was and is");
    e.ex("diffoff!");
    assert_eq!(background(&e, "four"), plain);
}

#[test]
fn diffthis_compares_two_windows_of_any_files() {
    let a = temp("diffthis_a.txt");
    let b = temp("diffthis_b.txt");
    std::fs::write(&a, "same\nleft\n").unwrap();
    std::fs::write(&b, "same\nright\n").unwrap();
    let mut e = open(&a);
    e.ex("diffthis");
    assert_eq!(
        e.message().unwrap().text,
        "comparing this window: :diffthis in another to compare it with"
    );
    e.ex(&format!("vsplit {}", b.display()));
    let (right, left) = (background(&e, "right"), background(&e, "left"));
    e.ex("diffthis");
    assert_ne!(background(&e, "right"), right);
    assert_ne!(background(&e, "left"), left);
    e.ex("diffoff");
    assert_eq!(background(&e, "right"), right);
}
