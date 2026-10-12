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
