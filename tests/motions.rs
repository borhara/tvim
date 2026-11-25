mod common;
use common::cursor_after as at;

#[test]
fn hjkl_and_counts() {
    let t = "hello\nworld\nthere";
    assert_eq!(at(t, "l"), (0, 1));
    assert_eq!(at(t, "3l"), (0, 3));
    assert_eq!(at(t, "9l"), (0, 4));
    assert_eq!(at(t, "j"), (1, 0));
    assert_eq!(at(t, "5j"), (2, 0));
    assert_eq!(at(t, "jjk"), (1, 0));
    assert_eq!(at(t, "$h"), (0, 3));
    assert_eq!(at(t, "<Right><Down>"), (1, 1));
}

#[test]
fn the_column_is_remembered_through_short_lines() {
    let t = "long line\nab\nlonger line";
    assert_eq!(at(t, "6lj"), (1, 1));
    assert_eq!(at(t, "6ljj"), (2, 6));
    assert_eq!(at(t, "$jj"), (2, 10));
}

#[test]
fn line_starts_and_ends() {
    let t = "   indented line\nnext";
    assert_eq!(at(t, "$"), (0, 15));
    assert_eq!(at(t, "$0"), (0, 0));
    assert_eq!(at(t, "$^"), (0, 3));
    assert_eq!(at(t, "2$"), (1, 3));
    assert_eq!(at("a b  \n", "g_"), (0, 2));
    assert_eq!(at(t, "j_"), (1, 0));
    assert_eq!(at(t, "5|"), (0, 4));
}

#[test]
fn words() {
    let t = "one two.three\n  four";
    assert_eq!(at(t, "w"), (0, 4));
    assert_eq!(at(t, "ww"), (0, 7));
    assert_eq!(at(t, "W"), (0, 4));
    assert_eq!(at(t, "WW"), (1, 2));
    assert_eq!(at(t, "e"), (0, 2));
    assert_eq!(at(t, "3e"), (0, 7));
    assert_eq!(at(t, "jb"), (0, 8));
    assert_eq!(at(t, "jB"), (0, 4));
    assert_eq!(at(t, "Gwge"), (0, 12));
    assert_eq!(at(t, "10w"), (1, 5), "past the last word: the end of the buffer");
}

#[test]
fn lines_by_number() {
    let t = "a\n  b\nc\nd";
    assert_eq!(at(t, "G"), (3, 0));
    assert_eq!(at(t, "Ggg"), (0, 0));
    assert_eq!(at(t, "2G"), (1, 2));
    assert_eq!(at(t, "3gg"), (2, 0));
    assert_eq!(at(t, "99G"), (3, 0));
    assert_eq!(at(t, "<CR>"), (1, 2));
    assert_eq!(at(t, "G-"), (2, 0));
    assert_eq!(at(t, "50%"), (1, 2));
}

#[test]
fn finding_characters() {
    let t = "a,b,c,d";
    assert_eq!(at(t, "f,"), (0, 1));
    assert_eq!(at(t, "2f,"), (0, 3));
    assert_eq!(at(t, "f,;"), (0, 3));
    assert_eq!(at(t, "f,;,"), (0, 1));
    assert_eq!(at(t, "t,"), (0, 0));
    assert_eq!(at(t, "t,;"), (0, 2), "a repeated t goes on to the next");
    assert_eq!(at(t, "$F,"), (0, 5));
    assert_eq!(at(t, "$T,"), (0, 6));
    assert_eq!(at(t, "fz"), (0, 0));
}

#[test]
fn brackets_and_paragraphs() {
    assert_eq!(at("x (a [b] c)", "%"), (0, 10));
    assert_eq!(at("x (a [b] c)", "%%"), (0, 2));
    let t = "a\nb\n\nc\nd\n\ne";
    assert_eq!(at(t, "}"), (2, 0));
    assert_eq!(at(t, "}}"), (5, 0));
    assert_eq!(at(t, "}}}"), (6, 0));
    assert_eq!(at(t, "G{"), (5, 0));
    assert_eq!(at(t, "G2{"), (2, 0));
}

#[test]
fn the_screen_top_middle_and_bottom() {
    let t: String = (1..=100).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
    let mut e = common::ed(&t);
    e.set_screen(80, 12);
    e.keys(&tvim::keys::parse("L"));
    assert_eq!(e.cursor().row, 6, "short of the bottom by the scroll offset");
    e.keys(&tvim::keys::parse("M"));
    assert_eq!(e.cursor().row, 4);
    e.keys(&tvim::keys::parse("50Gzt"));
    e.scroll();
    assert_eq!(e.top(), 46);
    e.keys(&tvim::keys::parse("H"));
    assert_eq!(e.cursor().row, 49);
}

#[test]
fn marks_and_jumps() {
    let t = "one\ntwo\nthree\nfour";
    assert_eq!(at(t, "jlmaG'a"), (1, 0));
    assert_eq!(at(t, "jlmaG`a"), (1, 1));
    assert_eq!(at(t, "jG''"), (1, 0));
    assert_eq!(at(t, "'z"), (0, 0));
}

#[test]
fn marks_keep_to_their_lines() {
    let t = "one\ntwo\nthree\nfour";
    assert_eq!(at(t, "jjlmaggdd`a"), (1, 1), "a line taken out above");
    assert_eq!(at(t, "jjlmaggOnew<Esc>`a"), (3, 1), "a line put in above");
    assert_eq!(at(t, "jjlmaggyyP`a"), (3, 1));
    assert_eq!(at(t, "jjlmaGdd`a"), (2, 1), "nothing moves for a change below");
    assert_eq!(at(t, "jjmajJ'a"), (2, 0), "a line joined to it");
    assert_eq!(at(t, "jmakJ'a"), (0, 0), "a line joined into the one above");
    assert_eq!(at(t, "ggmaG:1,2d<CR>'a"), (0, 0), "gone with its line");
    assert_eq!(at(t, "jjlmaggdd:'a<CR>"), (1, 0), "ranges use them too");
    assert_eq!(at(t, "jjlmaggddu`a"), (2, 1), "undo puts it back");
    assert_eq!(at(t, "jjlmaggddu<C-r>`a"), (1, 1), "and redo moves it again");
    assert_eq!(
        at(t, "jjmadduG'a"),
        (2, 0),
        "a mark taken with its line comes back with it"
    );

    assert_eq!(at(t, "jOxy<Esc>gg`^"), (1, 1), "nor does one set in the change itself");

    let mut e = common::ed(t);
    e.keys(&tvim::keys::parse("jjmadd'a"));
    assert_eq!(e.message().map(|m| m.text.as_str()), Some("E20: Mark not set"));
}

#[test]
fn the_last_selection_keeps_to_its_lines() {
    let mut e = common::ed("one\ntwo\nthree\nfour");
    e.keys(&tvim::keys::parse("jjVj<Esc>ggdd"));
    e.ex("'<,'>d");
    assert_eq!(e.text(), "two");
    let mut e = common::ed("one\ntwo\nthree\nfour");
    e.keys(&tvim::keys::parse("jjvl<Esc>ggOnew<Esc>gvd"));
    assert_eq!(e.text(), "new\none\ntwo\nree\nfour");
    let mut e = common::ed("one\ntwo\nthree");
    e.keys(&tvim::keys::parse("jVj<Esc>Gdd"));
    e.ex("'<,'>d");
    assert_eq!(e.text(), "one", "the end of one whose last line went moves up");
}

#[test]
fn brackets_on_a_very_long_line() {
    let line = format!("({})", "ab ".repeat(66_000));
    let mut e = common::ed(&line);
    e.keys(&tvim::keys::parse("%"));
    assert_eq!(e.cursor().col, line.chars().count() - 1);
}
