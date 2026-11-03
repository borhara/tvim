mod common;
use common::{cursor_after as at, ed, text_after as t};
use tvim::{Mode, Pos, keys::parse};

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
fn case_is_ignored_unless_there_are_capitals() {
    let src = "Foo foo FOO";
    assert_eq!(at(src, "/foo<CR>"), (0, 4));
    assert_eq!(at(src, "/foo<CR>n"), (0, 8));
    assert_eq!(at(src, "/FOO<CR>"), (0, 8));
    let mut e = ed(src);
    e.ex("set noic");
    e.keys(&parse("/foo<CR>n"));
    assert_eq!(e.cursor(), Pos::new(0, 4));
}

#[test]
fn whole_words_under_the_cursor() {
    let src = "cat concat cat\ncat";
    assert_eq!(at(src, "*"), (0, 11), "not inside concat");
    assert_eq!(at(src, "**"), (1, 0));
    assert_eq!(at(src, "#"), (1, 0));
    assert_eq!(at(src, "*n"), (1, 0));
    assert_eq!(at("x y", "*"), (0, 0));
}

#[test]
fn as_a_motion() {
    assert_eq!(t("one two three two", "/two<CR>0dn"), "two three two");
    assert_eq!(t("a x b x", "/x<CR>cnY<Esc>"), "a Yx");
}

#[test]
fn typing_shows_matches_and_escape_goes_back() {
    let mut e = ed("abc\nxyz\nabc");
    e.keys(&parse("/xy"));
    assert_eq!(e.mode(), Mode::Search);
    assert_eq!(e.cursor(), Pos::new(1, 0), "jumps there as it's typed");
    assert_eq!(e.search_pattern(), Some("xy"));
    e.keys(&parse("<Esc>"));
    assert_eq!(e.cursor(), Pos::new(0, 0));
    assert_eq!(e.mode(), Mode::Normal);
    e.keys(&parse("/q<BS><BS>"));
    assert_eq!(e.mode(), Mode::Normal, "backspace on nothing leaves");
}

#[test]
fn an_empty_search_repeats_the_last() {
    assert_eq!(at("a\na\na", "/a<CR>/<CR>"), (2, 0));
    let mut e = ed("a");
    e.keys(&parse("/<CR>"));
    assert!(e.message().unwrap().text.starts_with("E35"));
}

#[test]
fn highlighting_stops_with_noh_and_starts_again_with_n() {
    let mut e = ed("a b a");
    e.keys(&parse("/a<CR>"));
    assert_eq!(e.search_pattern(), Some("a"));
    e.keys(&parse(":noh<CR>"));
    assert_eq!(e.search_pattern(), None);
    e.keys(&parse("n"));
    assert_eq!(e.search_pattern(), Some("a"));
    e.keys(&parse(":set nohls<CR>"));
    assert_eq!(e.search_pattern(), None);
}

#[test]
fn history() {
    let mut e = ed("a b c");
    e.keys(&parse("/b<CR>/c<CR>/<Up><Up>"));
    assert_eq!(e.cmdline().text, "b");
    e.keys(&parse("<Down>"));
    assert_eq!(e.cmdline().text, "c");
    e.keys(&parse("<Esc>:set nu<CR>:<Up>"));
    assert_eq!(e.cmdline().text, "set nu");
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

#[test]
fn a_bad_pattern_says_so() {
    let mut e = ed("a(b");
    e.keys(&parse("/a(<CR>"));
    let m = e.message().unwrap();
    assert!(m.error && m.text.starts_with("E383"), "{}", m.text);
    assert_eq!(e.cursor(), Pos::new(0, 0));
    e.keys(&parse("/a\\(<CR>"));
    assert!(e.message().is_none_or(|m| !m.error));
    let mut e = ed("abc");
    e.keys(&parse("/[a"));
    assert_eq!(e.search_count(), None, "while it's typed nothing breaks");
}

#[test]
fn matches_cover_what_the_pattern_matched() {
    let e = ed("");
    assert_eq!(e.matches_in("ab 123 4", r"\d+"), [(3, 6), (7, 8)]);
    let mut e = ed("x1 x22 x333");
    e.keys(&parse(r"/x\d+<CR>n"));
    assert_eq!(e.search_count(), Some((3, 3)));
}
