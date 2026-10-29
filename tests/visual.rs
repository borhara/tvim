mod common;
use common::{after, text_after as t};

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
