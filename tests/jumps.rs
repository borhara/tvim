mod common;
use common::cursor_after as at;

const LINES: &str = "one\ntwo foo\n\nthree\nfoo four\n\nfive";

#[test]
fn back_and_forward_through_jumps() {
    assert_eq!(at(LINES, "G<C-o>"), (0, 0));
    assert_eq!(at(LINES, "G<C-o><C-i>"), (6, 0));
    assert_eq!(at(LINES, "G<C-o><Tab>"), (6, 0), "Tab is Ctrl-i to a terminal");
    assert_eq!(at(LINES, "jlG<C-o>"), (1, 1), "the column comes back too");
    assert_eq!(at(LINES, "Ggg<C-o>"), (6, 0), "where it came from, not this line again");
    assert_eq!(at(LINES, "Ggg<C-o><C-i>"), (0, 0));
    assert_eq!(at(LINES, "G<C-o><C-o>"), (0, 0), "nothing older stays put");
    assert_eq!(at(LINES, "3G5G7G2<C-o>"), (2, 0), "a count goes further");
}

#[test]
fn what_counts_as_a_jump() {
    assert_eq!(at(LINES, "/foo<CR><C-o>"), (0, 0));
    assert_eq!(at(LINES, "/foo<CR>n<C-o>"), (1, 4));
    assert_eq!(at(LINES, "?foo<CR>N<C-o>"), (4, 0));
    assert_eq!(at(LINES, "jw*<C-o>"), (1, 4));
    assert_eq!(at(LINES, "5G#<C-o>"), (4, 0));
    assert_eq!(at(LINES, "}<C-o>"), (0, 0));
    assert_eq!(at(LINES, "G{<C-o>"), (6, 0));
    assert_eq!(at("f(a,\n  b)", "j$%<C-o>"), (1, 3));
    assert_eq!(at(LINES, ":4<CR><C-o>"), (0, 0));
    assert_eq!(at(LINES, "jmaG'a<C-o>"), (6, 0));
    assert_eq!(at(LINES, "jj<C-o>"), (2, 0), "plain moves aren't");
}
