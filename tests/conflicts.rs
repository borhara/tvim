mod common;
use common::{ed, text_after};
use tvim::keys::parse;

const TEXT: &str = "top\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> topic\nmiddle\n<<<<<<< HEAD\nmine\n||||||| base\nold\n=======\nyours\n>>>>>>> other\nend";

#[test]
fn brackets_x_go_from_one_conflict_to_the_next() {
    let mut e = ed(TEXT);
    let row = |e: &mut tvim::Editor, keys: &str| {
        e.keys(&parse(keys));
        e.cursor().row
    };
    assert_eq!(row(&mut e, "]x"), 1);
    assert_eq!(row(&mut e, "]x"), 7);
    assert_eq!(row(&mut e, "]x"), 7, "no wrapping at the end");
    assert_eq!(row(&mut e, "[x"), 1);
    assert_eq!(row(&mut e, "G2[x"), 1);
    assert_eq!(row(&mut e, "gg2]x"), 7);
}

#[test]
fn space_h_keeps_ours_theirs_both_or_neither() {
    let rest = "\nmiddle\n<<<<<<< HEAD\nmine\n||||||| base\nold\n=======\nyours\n>>>>>>> other\nend";
    assert_eq!(text_after(TEXT, "jj<Space>ho"), format!("top\nours{rest}"));
    assert_eq!(text_after(TEXT, "]x<Space>ht"), format!("top\ntheirs{rest}"));
    assert_eq!(text_after(TEXT, "]x<Space>hb"), format!("top\nours\ntheirs{rest}"));
    assert_eq!(text_after(TEXT, "]x<Space>hn"), format!("top{rest}"));
    assert_eq!(
        text_after(TEXT, "G[x<Space>hb"),
        "top\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> topic\nmiddle\nmine\nyours\nend",
        "the base is left out"
    );
}

#[test]
fn resolving_is_one_change_and_dot_repeats_it() {
    let mut e = ed(TEXT);
    e.keys(&parse("]x<Space>ht"));
    e.keys(&parse("]x."));
    assert_eq!(e.text(), "top\ntheirs\nmiddle\nyours\nend");
    e.keys(&parse("u"));
    e.keys(&parse("u"));
    assert_eq!(e.text(), TEXT);
    e.keys(&parse("gg<Space>ho"));
    assert_eq!(e.text(), TEXT, "nothing off a conflict");
    assert_eq!(e.message().unwrap().text, "no conflict here");
}
