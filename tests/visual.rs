mod common;
use common::{after, ed, text_after as t};
use tvim::{Mode, keys::parse};

#[test]
fn characters() {
    assert_eq!(after("hello world", "vlld"), ("lo world".into(), (0, 0)));
    assert_eq!(t("hello world", "wvex"), "hello ");
    assert_eq!(t("one\ntwo\nthree", "lvjd"), "oo\nthree");
    assert_eq!(t("hello", "$vhd"), "hel");
    assert_eq!(t("a\n\nb", "jvd"), "a\nb", "an empty line takes its line break");
    assert_eq!(t("hello world", "vecbye<Esc>"), "bye world");
}

#[test]
fn lines() {
    assert_eq!(t("a\nb\nc", "Vjd"), "c");
    assert_eq!(t("a\nb\nc", "jVd"), "a\nc");
    assert_eq!(t("a\nb\nc", "VjyGp"), "a\nb\nc\na\nb");
    assert_eq!(t("a\nb\nc", "Vj>"), "    a\n    b\nc");
    assert_eq!(t("a\nb", "V3>"), "            a\nb");
    assert_eq!(t("a\nb\nc", "VjcX<Esc>"), "X\nc");
    assert_eq!(t("ab", "vX"), "", "X deletes the lines");
}

#[test]
fn switching_and_leaving() {
    let mut e = ed("abc");
    e.keys(&parse("v"));
    assert_eq!(e.mode(), Mode::Visual);
    e.keys(&parse("V"));
    assert_eq!(e.mode(), Mode::VisualLine);
    e.keys(&parse("V"));
    assert_eq!(e.mode(), Mode::Normal);
    e.keys(&parse("vl<Esc>"));
    assert_eq!(e.mode(), Mode::Normal);
    assert_eq!(e.cursor().col, 1);
    e.keys(&parse("gvd"));
    assert_eq!(e.text(), "c", "gv selects it again");
}

#[test]
fn the_other_end() {
    assert_eq!(after("abcdef", "lvlloh").1, (0, 0));
    assert_eq!(t("abcdef", "lllvlohd"), "abf");
}
