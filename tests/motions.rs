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
