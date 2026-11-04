mod common;
use common::ed;
use tvim::keys::parse;

#[test]
fn splitting_by_key_and_command() {
    let mut e = ed("hello");
    e.keys(&parse("<C-w>s"));
    assert_eq!(e.window_count(), 2);
    e.keys(&parse("<C-w>v"));
    assert_eq!(e.window_count(), 3);
    e.ex("sp");
    e.ex("vs");
    assert_eq!(e.window_count(), 5);
    assert_eq!(e.window_index(), 0, "the new window comes first and has the focus");
}

#[test]
fn quit_closes_the_window_until_the_last() {
    let mut e = ed("hello");
    e.ex("vsplit");
    e.keys(&parse("x"));
    e.ex("q");
    assert!(!e.should_quit(), "a window closes even with unsaved changes");
    assert_eq!(e.window_count(), 1);
    e.ex("q");
    assert!(!e.should_quit(), "the last one still guards them");
    assert!(e.message().unwrap().text.starts_with("E37"));
    e.ex("q!");
    assert!(e.should_quit());
}

#[test]
fn ctrl_w_q_and_close() {
    let mut e = ed("hello");
    e.keys(&parse("<C-w>s<C-w>q"));
    assert_eq!(e.window_count(), 1);
    e.ex("close");
    assert!(e.message().unwrap().text.starts_with("E444"));
    assert!(!e.should_quit());
    e.keys(&parse("<C-w>q"));
    assert!(e.should_quit());
}

#[test]
fn only_keeps_the_focused_window() {
    let mut e = ed("hello");
    e.keys(&parse("<C-w>s<C-w>v<C-w>v"));
    e.keys(&parse("<C-w>o"));
    assert_eq!(e.window_count(), 1);
    e.ex("sp");
    e.ex("only");
    assert_eq!(e.window_count(), 1);
}

#[test]
fn moving_between_windows() {
    let mut e = ed("hello");
    e.ex("vsplit");
    e.ex("split");
    // top left, bottom left, right
    assert_eq!(e.window_index(), 0);
    e.keys(&parse("<C-w>j"));
    assert_eq!(e.window_index(), 1);
    e.keys(&parse("<C-w>l"));
    assert_eq!(e.window_index(), 2);
    e.keys(&parse("<C-w>h"));
    assert_eq!(e.window_index(), 0, "the one level with the cursor");
    e.keys(&parse("<C-w><Down>"));
    assert_eq!(e.window_index(), 1);
    e.keys(&parse("<C-w>k"));
    assert_eq!(e.window_index(), 0);
    e.keys(&parse("<C-w>k"));
    assert_eq!(e.window_index(), 0, "nothing above stays put");
    e.keys(&parse("<C-w>w<C-w>w<C-w>w"));
    assert_eq!(e.window_index(), 0, "w goes round");
    e.keys(&parse("<C-w>W"));
    assert_eq!(e.window_index(), 2);
    e.keys(&parse("2<C-w>w"));
    assert_eq!(e.window_index(), 1);
}
