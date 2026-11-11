mod common;
use common::{cursor_after as at, ed};
use tvim::keys::parse;

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

#[test]
fn a_new_jump_drops_the_newer_ones() {
    let mut e = ed(LINES);
    e.keys(&parse("3G5G<C-o><C-o>"));
    assert_eq!(e.cursor().row, 0);
    e.keys(&parse("2G<C-i>"));
    assert_eq!(e.cursor().row, 1, "nothing newer now");
    e.keys(&parse("<C-o>"));
    assert_eq!(e.cursor().row, 0);
}

#[test]
fn a_line_is_kept_once() {
    assert_eq!(at("a\nb\nc", "GggGgg<C-o><C-o><C-o>"), (2, 0));
}

#[test]
fn the_list_is_capped() {
    let text = vec!["x"; 200].join("\n");
    let mut e = ed(&text);
    for n in 2..=151 {
        e.keys(&parse(&format!("{n}G")));
    }
    for _ in 0..200 {
        e.keys(&parse("<C-o>"));
    }
    assert_eq!(
        e.cursor().row,
        51,
        "only the last hundred are kept, counting where it left from"
    );
}

#[test]
fn back_to_the_last_jump_with_quotes() {
    assert_eq!(at(LINES, "G''"), (0, 0));
    assert_eq!(at(LINES, "G''''"), (6, 0));
    assert_eq!(at(LINES, "jlllG``"), (1, 3));
    assert_eq!(at(LINES, "G<C-o>''"), (6, 0), "not where it already is");
}
