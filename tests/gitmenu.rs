mod common;

use common::temp;
use tvim::Editor;
use tvim::gitmenu::Group;
use tvim::keys::parse;

fn menu(e: &mut Editor, keys: &str) {
    e.keys(&parse(keys));
    e.wait_for_git_menu();
}

fn groups(e: &Editor) -> Vec<(Group, String)> {
    let m = e.git_menu().unwrap();
    m.changes().iter().map(|c| (c.group, c.path.clone())).collect()
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
