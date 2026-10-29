mod common;
use common::{after, ed, text_after as t};
use tvim::keys::parse;

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

#[test]
fn enter_between_brackets_opens_a_middle_line() {
    assert_eq!(t("fn f() ", "A{<CR>x<Esc>"), "fn f() {\n    x\n}");
    assert_eq!(t("f", "A(<CR>a<Esc>"), "f(\n    a\n)");
    assert_eq!(t("", "i[<CR><Esc>"), "[\n\n]");
}

#[test]
fn it_can_be_turned_off() {
    let mut e = ed("");
    e.ex("set noautopairs");
    e.keys(&parse("i(\"<Esc>"));
    assert_eq!(e.text(), "(\"");
    e.ex("set ap?");
    assert_eq!(e.message().unwrap().text, "noautopairs");
}

#[test]
fn replace_mode_doesnt_pair() {
    assert_eq!(t("abc", "R(<Esc>"), "(bc");
}

#[test]
fn undo_and_repeat() {
    assert_eq!(t("a", "A(x<Esc>u"), "a");
    assert_eq!(t("a\nb", "A(x<Esc>j."), "a(x)\nb(x)");
}
