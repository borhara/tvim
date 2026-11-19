mod common;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::temp;
use tvim::Editor;
use tvim::clipboard::Memory;
use tvim::gitmenu::{Group, Tab};
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
        // files come back from git as they went in, not with Windows line endings
        ("core.autocrlf", "false"),
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
fn stage_unstage_and_commit() {
    let Some(dir) = repo("commit", "a\n") else { return };
    std::fs::write(dir.join("a.txt"), "a\nb\n").unwrap();
    let mut e = open(&dir.join("a.txt"));
    e.ex("git");
    e.wait_for_git_menu();
    assert_eq!(groups(&e), [(Group::Changes, "a.txt".into())]);
    menu(&mut e, "c");
    assert!(e.git_menu().unwrap().note().unwrap().error, "nothing staged yet");
    menu(&mut e, "s");
    assert_eq!(groups(&e), [(Group::Staged, "a.txt".into())]);
    menu(&mut e, "s");
    assert_eq!(groups(&e), [(Group::Changes, "a.txt".into())]);
    menu(&mut e, "a");
    assert_eq!(groups(&e), [(Group::Staged, "a.txt".into())]);
    menu(&mut e, "u");
    assert_eq!(groups(&e), [(Group::Changes, "a.txt".into())]);
    menu(&mut e, "a");
    e.keys(&parse("c"));
    assert!(e.git_menu().unwrap().typing().is_some());
    e.keys(&parse("add b<Esc>"));
    assert!(e.git_menu().unwrap().typing().is_none(), "esc cancels");
    assert_eq!(groups(&e).len(), 1);
    menu(&mut e, "cadd b<CR>");
    assert_eq!(groups(&e), []);
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).unwrap().trim(), "add b");
    assert!(e.git_menu().unwrap().note().unwrap().text.contains("add b"));
    menu(&mut e, "q");
    assert!(e.git_menu().is_none());
    e.keys(&parse("j"));
    e.wait_for_blame();
    assert_eq!(
        e.lens().as_deref(),
        Some("you, just now · add b"),
        "the lens follows the commit"
    );
    assert_eq!(e.change_marks().to_vec(), [None, None], "and so does the gutter");
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

#[test]
fn the_command_box_runs_git_and_shows_its_output() {
    let Some(dir) = repo("command", "a\n") else { return };
    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>G:log --format='%s by %an'<CR>");
    let m = e.git_menu().unwrap();
    let o = m.output().expect("output");
    assert_eq!(o.code, 0);
    assert_eq!(o.lines, ["first by t"]);
    menu(&mut e, ":nosuchthing<CR>");
    let o = e.git_menu().unwrap().output().unwrap();
    assert_ne!(o.code, 0);
    assert!(o.lines.iter().any(|l| l.contains("nosuchthing")), "{:?}", o.lines);
    let screen = shown(e, 110, 30);
    assert!(
        screen.contains("$ git nosuchthing") && screen.contains("✕ exit 1"),
        "{screen}"
    );

    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>G:status --short<CR>");
    e.keys(&parse(":<Up>"));
    assert_eq!(e.git_menu().unwrap().typing().unwrap().1, "status --short", "history");
    menu(&mut e, "<Esc>");
    assert!(e.git_menu().unwrap().typing().is_none());
    assert!(e.git_menu().unwrap().output().is_some());
    menu(&mut e, "<Esc>");
    assert!(e.git_menu().unwrap().output().is_none());

    let mut e = open(&dir.join("a.txt"));
    e.ex("git rev-parse --abbrev-ref HEAD");
    e.wait_for_git_menu();
    e.wait_for_git_menu();
    assert_eq!(
        e.git_menu().unwrap().output().unwrap().lines,
        ["trunk"],
        ":git takes arguments"
    );
}

#[test]
fn log_and_branches() {
    let Some(dir) = repo("log", "a\n") else { return };
    git(&dir, &["branch", "side"]).unwrap();
    std::fs::write(dir.join("a.txt"), "a\nb\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "second"]).unwrap();
    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>Gl");
    let m = e.git_menu().unwrap();
    assert_eq!(m.tab(), Tab::Log);
    let titles: Vec<_> = m.entries().iter().map(|en| en.title.as_str()).collect();
    assert_eq!(titles, ["second", "first"]);
    assert!(m.peek().unwrap().iter().any(|l| l.contains("a.txt |")), "a diffstat");

    menu(&mut e, "b");
    let m = e.git_menu().unwrap();
    assert_eq!(m.tab(), Tab::Branches);
    let names: Vec<_> = m.entries().iter().map(|en| (en.id.as_str(), en.current)).collect();
    assert!(
        names.contains(&("trunk", true)) && names.contains(&("side", false)),
        "{names:?}"
    );
    let side = names.iter().position(|n| n.0 == "side").unwrap();

    std::fs::write(dir.join("a.txt"), "local\n").unwrap();
    for _ in 0..side {
        e.keys(&parse("j"));
    }
    menu(&mut e, "<CR>");
    let note = e.git_menu().unwrap().note().unwrap().clone();
    assert!(note.error && note.text.contains("commit or stash"), "{note:?}");
    assert_eq!(git(&dir, &["branch", "--show-current"]).unwrap().trim(), "trunk");

    git(&dir, &["checkout", "--", "a.txt"]).unwrap();
    menu(&mut e, "<CR>");
    assert_eq!(git(&dir, &["branch", "--show-current"]).unwrap().trim(), "side");
    assert_eq!(e.text(), "a", "the buffer follows the switch");
    assert_eq!(e.branch().as_deref(), Some("side"));
}

#[test]
fn graph_tab() {
    let Some(dir) = repo("graph", "a\n") else { return };
    git(&dir, &["checkout", "-b", "side"]).unwrap();
    std::fs::write(dir.join("b.txt"), "branch\n").unwrap();
    git(&dir, &["add", "b.txt"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "side commit"]).unwrap();
    git(&dir, &["checkout", "trunk"]).unwrap();
    std::fs::write(dir.join("a.txt"), "main commit\n").unwrap();
    git(&dir, &["commit", "-q", "-am", "main commit"]).unwrap();
    git(&dir, &["merge", "--no-ff", "-m", "merge side", "side"]).unwrap();

    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>Gt");
    let m = e.git_menu().unwrap();
    assert_eq!(m.tab(), Tab::Graph);
    assert_eq!(m.entries().len(), 4); // exactly 4 commits, 0 blank spacer lines!
    let screen = shown(e, 110, 30);
    assert!(screen.contains("●─╮"), "{screen}");
    assert!(screen.contains("●─╯"), "{screen}");
    assert!(screen.contains("merge side"), "{screen}");
    assert!(screen.contains("✦"), "{screen}");
    assert!(screen.contains("⎇"), "{screen}");
    assert!(screen.contains("trunk"), "{screen}");
    assert!(screen.contains("side"), "{screen}");
}

#[test]
fn outside_a_repo_there_is_only_a_message() {
    let dir = temp("menu-outside");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("plain.txt");
    std::fs::write(&file, "a\n").unwrap();
    if git(&dir, &["rev-parse"]).is_some() {
        return;
    }
    let mut e = open(&file);
    menu(&mut e, "<Space>G");
    assert!(e.git_menu().is_none());
    assert_eq!(e.message().unwrap().text, "not in a git repository");
}

#[test]
fn enter_opens_the_file() {
    let Some(dir) = repo("enter", "a\n") else { return };
    std::fs::write(dir.join("b.txt"), "bee\n").unwrap();
    let mut e = open(&dir.join("a.txt"));
    menu(&mut e, "<Space>G<CR>");
    assert!(e.git_menu().is_none());
    assert_eq!(e.text(), "bee");
}

#[test]
fn a_staged_rename_unstages_and_discards_as_a_whole() {
    let Some(dir) = repo("rename", "a\n") else { return };
    git(&dir, &["mv", "a.txt", "b.txt"]).unwrap();
    let mut e = open(&dir.join("b.txt"));
    menu(&mut e, "<Space>G");
    assert_eq!(groups(&e), [(Group::Staged, "b.txt".to_string())]);
    menu(&mut e, "s");
    assert_eq!(
        git(&dir, &["diff", "--cached", "--name-only"]).unwrap(),
        "",
        "nothing left staged"
    );
    git(&dir, &["add", "-A"]).unwrap();
    menu(&mut e, "r");
    menu(&mut e, "dy");
    assert_eq!(
        std::fs::read_to_string(dir.join("a.txt")).unwrap(),
        "a\n",
        "back under its old name"
    );
    assert!(!dir.join("b.txt").exists());
    assert_eq!(git(&dir, &["status", "--porcelain"]).unwrap(), "");
}

#[test]
fn opening_git_menu_from_directory_explorer() {
    let Some(dir) = repo("gitmenu-explorer", "initial\n") else {
        return;
    };
    let mut e = Editor::new();
    e.explore(&dir);
    e.keys(&parse("<Esc><Space>G"));
    e.wait_for_git_menu();
    assert!(e.git_menu().is_some());
    let m = e.git_menu().unwrap();
    assert_eq!(m.repo_name(), dir.file_name().unwrap().to_str().unwrap());
}
