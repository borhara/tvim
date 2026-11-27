mod common;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::temp;
use tvim::Editor;
use tvim::clipboard::Memory;
use tvim::diff::Mark::*;
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

// a repo on branch trunk with one committed file, or None without git
fn repo(name: &str, committed: &str) -> Option<PathBuf> {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if !git(&dir, &["init", "-q"]) {
        eprintln!("no git, skipping");
        return None;
    }
    // in the repo's own config, so tvim's git commands see it too: files come back from git
    // as they went in, not with Windows line endings
    assert!(git(&dir, &["config", "core.autocrlf", "false"]));
    git(&dir, &["symbolic-ref", "HEAD", "refs/heads/trunk"]);
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

#[test]
fn marks_compare_against_head() {
    let Some(dir) = repo("head", "a\nb\nc\n") else { return };
    let file = dir.join("a.txt");
    std::fs::write(&file, "a\nB\nc\nd\n").unwrap();
    let mut e = open(&file);
    assert!(!e.is_modified());
    assert_eq!(e.change_marks().to_vec(), [None, Some(Changed), None, Some(Added)]);
    e.keys(&parse("jdd"));
    assert_eq!(e.change_marks().to_vec(), [Some(Removed), None, Some(Added)]);
    assert_eq!(e.branch().as_deref(), Some("trunk"));
}

#[test]
fn untracked_files_are_all_new() {
    let Some(dir) = repo("untracked", "a\n") else { return };
    let file = dir.join("new.txt");
    std::fs::write(&file, "x\ny\n").unwrap();
    let e = open(&file);
    assert_eq!(
        e.change_marks().to_vec(),
        [None, None],
        "no gutter bars; the sidebar's ? says it's new"
    );
    std::fs::write(dir.join(".gitignore"), "skip.txt\n").unwrap();
    let skipped = dir.join("skip.txt");
    std::fs::write(&skipped, "x\n").unwrap();
    assert_eq!(
        open(&skipped).change_marks().to_vec(),
        [None],
        "ignored files fall back to the save"
    );
}

#[test]
fn outside_a_repo_the_save_is_the_base() {
    let dir = temp("outside");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("plain.txt");
    std::fs::write(&file, "a\nb\n").unwrap();
    let mut e = open(&file);
    assert_eq!(e.change_marks().to_vec(), [None, None]);
    assert_eq!(e.branch(), None);
    e.keys(&parse("ox<Esc>"));
    assert_eq!(e.change_marks().to_vec(), [None, Some(Added), None]);
    e.ex("w");
    assert_eq!(e.change_marks().to_vec(), [None, None, None]);
}

#[test]
fn saving_picks_up_a_new_commit() {
    let Some(dir) = repo("commit", "a\nb\n") else { return };
    let file = dir.join("a.txt");
    let mut e = open(&file);
    e.keys(&parse("Gox<Esc>"));
    e.ex("w");
    e.wait_for_git();
    assert_eq!(
        e.change_marks().to_vec(),
        [None, None, Some(Added)],
        "still differs from HEAD"
    );
    assert!(git(&dir, &["commit", "-q", "-am", "second"]));
    e.ex("w");
    e.wait_for_git();
    assert_eq!(e.change_marks().to_vec(), [None, None, None]);
}

#[test]
fn detached_head_shows_a_short_sha() {
    let Some(dir) = repo("detached", "a\n") else { return };
    assert!(git(&dir, &["checkout", "-q", "--detach"]));
    let e = open(&dir.join("a.txt"));
    let b = e.branch().unwrap();
    assert!(b.len() >= 4 && b.chars().all(|c| c.is_ascii_hexdigit()), "{b}");
}

#[test]
fn bracket_c_walks_the_hunks_without_wrapping() {
    let Some(dir) = repo("walk", "1\n2\n3\n4\n5\n6\n7\n8\n9\n") else {
        return;
    };
    let file = dir.join("a.txt");
    std::fs::write(&file, "1\nX\n3\n4\n5\n7\n8\n9\nY\n").unwrap();
    let mut e = open(&file);
    let row = |e: &mut Editor, keys: &str| {
        e.keys(&parse(keys));
        e.cursor().row
    };
    assert_eq!(row(&mut e, "]c"), 1);
    assert_eq!(row(&mut e, "]c"), 4, "a removal sits on the line above it");
    assert_eq!(row(&mut e, "]c"), 8);
    assert_eq!(row(&mut e, "]c"), 8, "no wrapping at the end");
    assert_eq!(row(&mut e, "[c"), 4);
    assert_eq!(row(&mut e, "''"), 8, "hunk jumps are jumps");
    assert_eq!(row(&mut e, "gg2]c"), 4);
    assert_eq!(row(&mut e, "9[c"), 1, "a big count stops at the first");
    assert_eq!(row(&mut e, "[c"), 1);
    assert_eq!(row(&mut e, "]c<C-o>"), 1, "and go in the jump list");
    assert_eq!(row(&mut e, "]h]h"), 8, "]h and [h as LazyVim has them");
    assert_eq!(row(&mut e, "[h"), 4);
}

#[test]
fn space_h_r_resets_the_hunk_under_the_cursor() {
    let Some(dir) = repo("reset", "a\nb\nc\nd\n") else {
        return;
    };
    let file = dir.join("a.txt");
    std::fs::write(&file, "a\nB\nB2\nc\n").unwrap();
    let mut e = open(&file);
    e.keys(&parse("j<Space>hr"));
    assert_eq!(e.text(), "a\nb\nc");
    assert_eq!(e.cursor().row, 1);
    e.keys(&parse("j<Space>hr"));
    assert_eq!(e.text(), "a\nb\nc\nd", "the line above a removal brings it back");
    e.keys(&parse("u"));
    assert_eq!(e.text(), "a\nb\nc");
    e.keys(&parse("u"));
    assert_eq!(e.text(), "a\nB\nB2\nc");
    e.keys(&parse("gg<Space>hr"));
    assert_eq!(e.text(), "a\nB\nB2\nc", "nothing to reset off a hunk");
}

#[test]
fn space_h_p_shows_the_hunk_as_it_was_and_is() {
    let Some(dir) = repo("preview", "a\nb\nc\nd\n") else {
        return;
    };
    let file = dir.join("a.txt");
    std::fs::write(&file, "a\nB\nB2\nc\n").unwrap();
    let mut e = open(&file);
    e.keys(&parse("j<Space>hp"));
    assert_eq!(e.hover(), Some("```diff\n-b\n+B\n+B2\n```"));
    let screen = shown_ref(&e, 40, 12);
    assert!(screen.contains("-b") && screen.contains("+B2"), "{screen}");
    e.keys(&parse("j"));
    assert_eq!(e.hover(), None, "gone with the next key");
    e.keys(&parse("gg<Space>hp"));
    assert_eq!(e.hover(), None, "nothing off a hunk");
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn space_h_s_stages_only_the_hunk_under_the_cursor() {
    let Some(dir) = repo("stage", "1\n2\n3\n4\n5\n6\n") else {
        return;
    };
    let file = dir.join("a.txt");
    std::fs::write(&file, "1\nTWO\n3\n4\n5\nSIX\n").unwrap();
    let mut e = open(&file);
    e.keys(&parse("j<Space>hs"));
    assert_eq!(e.message().unwrap().text, "staged");
    assert_eq!(git_out(&dir, &["show", ":a.txt"]), "1\nTWO\n3\n4\n5\n6\n");
    // unsaved, as the buffer has it
    e.keys(&parse("Gciwsix<Esc><Space>hs"));
    assert_eq!(git_out(&dir, &["show", ":a.txt"]), "1\nTWO\n3\n4\n5\nsix\n");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "1\nTWO\n3\n4\n5\nSIX\n");
    e.keys(&parse("gg<Space>hs"));
    assert_eq!(e.message().unwrap().text, "nothing here to stage");
}

#[test]
fn space_h_s_stages_in_a_folder_and_a_new_file() {
    let Some(dir) = repo("stage_deep", "a\n") else {
        return;
    };
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let file = dir.join("sub/new.txt");
    std::fs::write(&file, "x\ny\n").unwrap();
    let mut e = open(&file);
    e.keys(&parse("<Space>hs"));
    assert_eq!(e.message().map(|m| m.text.clone()), Some("staged".into()));
    assert_eq!(git_out(&dir, &["show", ":sub/new.txt"]), "x\ny\n");
    assert_eq!(git_out(&dir, &["status", "--porcelain"]), "A  sub/new.txt\n");
}

#[test]
fn the_branch_shows_at_the_top_right() {
    let Some(dir) = repo("looks", "a\n") else { return };
    let mut e = open(&dir.join("a.txt"));
    e.clear_message();
    let shown = tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&tvim::ui::Tvim::new(e), 140, 12).plain()
    });
    let top = shown.lines().next().unwrap();
    assert!(top.ends_with("trunk"), "{top}");
}

fn shown_ref(e: &Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        let mut frame = tinsel::app::Frame::new(width, height);
        tvim::ui::draw(e, &mut frame);
        frame.snapshot().plain()
    })
}

fn shown(e: Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&tvim::ui::Tvim::new(e), width, height).plain()
    })
}

#[test]
fn diff_totals_sit_by_the_branch() {
    let Some(dir) = repo("totals", "1\n2\n3\n4\n5\n") else {
        return;
    };
    let file = dir.join("a.txt");
    let mut e = open(&file);
    e.clear_message();
    let clean = shown(e, 100, 10);
    let tabs = clean.lines().next().unwrap();
    assert!(
        tabs.contains("trunk") && !tabs.contains("+0") && !tabs.contains('~'),
        "{tabs}"
    );
    std::fs::write(&file, "1\nX\n3\n5\nnew\n").unwrap();
    let mut e = open(&file);
    e.clear_message();
    let screen = shown(e, 100, 10);
    assert!(screen.contains("trunk  +1 ~1 -1"), "{screen}");
    let mut e = open(&file);
    e.keys(&parse("ggjdd"));
    e.clear_message();
    let tabs = shown(e, 100, 10).lines().next().unwrap().to_string();
    assert!(
        tabs.contains("trunk  +1 -2"),
        "zeros are left out, on the tab row: {tabs}"
    );
}

fn side(screen: &str, row: &str) -> String {
    let line = screen.lines().find(|l| l.contains(row)).unwrap_or("");
    // the sidebar is 26 wide, the last its dotted rule
    line.chars().take(25).collect::<String>().trim_end().to_string()
}

#[test]
fn the_sidebar_shows_each_files_git_status() {
    let Some(dir) = repo("sidebar", "a\n") else { return };
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/in.txt"), "x\n").unwrap();
    std::fs::write(dir.join("kept.txt"), "k\n").unwrap();
    assert!(git(&dir, &["add", "."]));
    assert!(git(&dir, &["commit", "-q", "-m", "more"]));
    std::fs::write(dir.join("a.txt"), "changed\n").unwrap();
    std::fs::write(dir.join("sub/in.txt"), "y\n").unwrap();
    std::fs::write(dir.join("staged.txt"), "s\n").unwrap();
    assert!(git(&dir, &["add", "staged.txt"]));
    std::fs::write(dir.join("loose.txt"), "l\n").unwrap();
    let mut e = Editor::new();
    e.explore(&dir);
    e.wait_for_git();
    // the sidebar row's glyph and the colour it's painted in
    let ink = |e: &Editor, name: &str| -> (char, String) {
        let (plain, line) = tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
            let mut frame = tinsel::app::Frame::new(80, 12);
            tvim::ui::draw(e, &mut frame);
            let y = frame
                .snapshot()
                .plain()
                .lines()
                .position(|l| l.ends_with(&format!(" {name}")) || l.contains(&format!(" {name} ")))
                .unwrap();
            (frame.snapshot().plain(), frame.snapshot().lines()[y].clone())
        });
        let seg = line
            .split("\u{1b}[")
            .find(|s| s.contains(['⋆', '·', '◇', '✶']))
            .expect(&plain);
        let (code, text) = seg.split_once('m').unwrap();
        let fg = code
            .split(';')
            .skip_while(|p| *p != "38")
            .take(5)
            .collect::<Vec<_>>()
            .join(";");
        (text.chars().find(|c| "⋆·◇✶".contains(*c)).unwrap(), fg)
    };
    let rgb = |c: tinsel::Rgb| format!("38;2;{};{};{}", c.0, c.1, c.2);
    let screen = shown_ref(&e, 80, 12);
    assert!(screen.contains("─ sidebar ─"), "{screen}");
    assert_eq!(ink(&e, "a.txt"), ('⋆', rgb(tinsel::palette::AMBER)), "{screen}");
    assert_eq!(ink(&e, "staged.txt"), ('⋆', rgb(tinsel::palette::SAGE)));
    assert_eq!(ink(&e, "loose.txt"), ('⋆', rgb(tinsel::palette::MUTED)));
    assert_eq!(
        ink(&e, "sub"),
        ('◇', rgb(tinsel::palette::AMBER)),
        "folders with changes in them"
    );
    assert_eq!(ink(&e, "kept.txt").0, '·', "clean files are plain dots");
    assert!(
        !side(&screen, "a.txt").ends_with(" M"),
        "no letters at the edge: {screen}"
    );

    let outside = temp("sidebar-outside");
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("plain.txt"), "p\n").unwrap();
    let mut e = Editor::new();
    e.explore(&outside);
    e.wait_for_git();
    assert!(e.tree().with(|t| t.is_empty()), "nothing outside a repo");
}

#[test]
fn the_sidebar_status_follows_saves() {
    let Some(dir) = repo("sidebar-save", "a\n") else { return };
    let mut e = open(&dir.join("a.txt"));
    e.ex("Explore");
    e.wait_for_git();
    let path = tvim::paths::canonical(&dir).unwrap().join("a.txt");
    assert_eq!(e.tree().with(|t| t.file(&path)), None);
    e.keys(&parse("<Tab>ox<Esc>"));
    e.ex("w");
    e.wait_for_git();
    assert_eq!(e.tree().with(|t| t.file(&path)), Some(tvim::git::FileState::Modified));
}

#[test]
fn files_in_an_untracked_folder_count_as_untracked() {
    let Some(dir) = repo("untracked-dir", "1\n") else {
        return;
    };
    std::fs::create_dir_all(dir.join("new/deeper")).unwrap();
    std::fs::write(dir.join("new/deeper/x.rs"), "fn x() {}\n").unwrap();
    let tree = tvim::git::Tree::default();
    tree.refresh(&dir);
    tree.wait();
    let file = tvim::paths::canonical(&dir.join("new/deeper/x.rs")).unwrap();
    let root = tvim::paths::canonical(&dir).unwrap();
    tree.with(|s| {
        let found = s.file(&file).or_else(|| s.file(&root.join("new/deeper/x.rs")));
        assert_eq!(found, Some(tvim::git::FileState::Untracked));
        assert!(s.dir_changed(&root.join("new/deeper")) || s.dir_changed(&dir.join("new/deeper")));
    });
}
