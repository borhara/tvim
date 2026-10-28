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
