mod common;

use common::temp;
use tvim::keys::parse;

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
