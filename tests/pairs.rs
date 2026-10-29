mod common;
use common::{after, text_after as t};

#[test]
fn openers_bring_their_closers() {
    assert_eq!(after("", "i(<Esc>"), ("()".into(), (0, 0)));
    assert_eq!(t("", "if(x<Esc>"), "f(x)");
    assert_eq!(t("", "i[{<Esc>"), "[{}]");
    assert_eq!(t("", "i\"hi<Esc>"), "\"hi\"");
    assert_eq!(t("", "i'a<Esc>"), "'a'");
    assert_eq!(t("", "i`a<Esc>"), "`a`");
}

#[test]
fn typing_the_closer_steps_over_it() {
    assert_eq!(t("", "if(x)<Esc>"), "f(x)");
    assert_eq!(t("", "i[()]<Esc>"), "[()]");
    assert_eq!(t("", "i\"a\"b<Esc>"), "\"a\"b");
    assert_eq!(after("", "i()x<Esc>"), ("()x".into(), (0, 2)));
}

#[test]
fn quotes_after_a_word_and_openers_before_one_stay_single() {
    assert_eq!(t("", "idon't<Esc>"), "don't");
    assert_eq!(t("", "iit's work<Esc>"), "it's work");
    assert_eq!(t("x", "i(<Esc>"), "(x");
    assert_eq!(t("x", "i\"<Esc>"), "\"x");
}

#[test]
fn backspace_takes_an_empty_pair() {
    assert_eq!(t("", "i(<BS><Esc>"), "");
    assert_eq!(t("", "i\"<BS><Esc>"), "");
    assert_eq!(t("", "i(x<BS><BS><Esc>"), "");
    assert_eq!(t("a)", "A<Left><BS><Esc>"), ")");
}
