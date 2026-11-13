mod common;

use tvim::Editor;
use tvim::diff::Mark::*;
use tvim::keys::parse;

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
