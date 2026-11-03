mod common;
use common::{cursor_after as at, ed, text_after as t};
use tvim::{Pos, keys::parse};

#[test]
fn forward_and_back() {
    let src = "one\ntwo one\nthree one";
    assert_eq!(at(src, "/one<CR>"), (1, 4));
    assert_eq!(at(src, "/one<CR>n"), (2, 6));
    assert_eq!(at(src, "/one<CR>nn"), (0, 0), "round the end");
    assert_eq!(at(src, "/one<CR>N"), (0, 0));
    assert_eq!(at(src, "?one<CR>"), (2, 6), "round the start");
    assert_eq!(at(src, "?one<CR>n"), (1, 4));
    assert_eq!(at(src, "?one<CR>N"), (0, 0));
    assert_eq!(at(src, "2/one<CR>"), (1, 4), "a count before / is ignored here");
    assert_eq!(at(src, "/one<CR>2n"), (0, 0));
}

#[test]
fn says_when_it_goes_round_or_finds_nothing() {
    let mut e = ed("a\nb\na");
    e.keys(&parse("G/a<CR>"));
    assert_eq!(e.message().unwrap().text, "search hit BOTTOM, continuing at TOP");
    e.keys(&parse("/zzz<CR>"));
    let m = e.message().unwrap();
    assert!(m.error && m.text == "E486: Pattern not found: zzz");
    assert_eq!(e.cursor(), Pos::new(0, 0), "stays where it was");
}

#[test]
fn patterns_are_regular_expressions() {
    let src = "ab 12 cd 345\nba";
    assert_eq!(at(src, r"/\d+<CR>"), (0, 3));
    assert_eq!(at(src, r"/\d+<CR>n"), (0, 9));
    assert_eq!(at(src, "/^b<CR>"), (1, 0));
    assert_eq!(at(src, "/cd|ba<CR>n"), (1, 0));
    assert_eq!(at(src, "/(cd|ba)$<CR>"), (1, 0));
    assert_eq!(at(src, r"/\<\w{2}\><CR>n"), (0, 6));
    assert_eq!(at(src, "/[3-5]{3}<CR>"), (0, 9));
    assert_eq!(at("a.b x|a.b", r"/x\|a\.b<CR>"), (0, 4), "\\| is a literal bar");
}

#[test]
fn case_flags_in_the_pattern() {
    let src = "Foo foo FOO";
    assert_eq!(at(src, r"/\Cfoo<CR>n"), (0, 4));
    assert_eq!(at(src, r"/FOO\c<CR>"), (0, 4));
    assert_eq!(
        at("Foo FOO", r"/\w\Wfoo<CR>"),
        (0, 2),
        "\\W isn't a capital, so case is still ignored"
    );
}
