mod common;
use common::ed;
use tvim::keys::parse;

#[test]
fn space_u_lists_every_state_the_text_has_been_in() {
    let mut e = ed("one");
    e.keys(&parse("A two<Esc>"));
    e.keys(&parse("A three<Esc>"));
    e.keys(&parse("<Space>u"));
    let m = e.menu().expect("listed");
    assert_eq!(m.title, "undo history");
    let labels: Vec<&str> = m.entries.iter().map(|x| x.label.as_str()).collect();
    assert_eq!(labels.len(), 3);
    assert!(labels[0].ends_with("· now"), "{labels:?}");
    assert_eq!(m.selected, 0, "now is chosen to start");
    assert_eq!(m.entries[0].hint, "+1 −1");
}

#[test]
fn a_state_undone_and_lost_to_a_new_change_can_be_gone_back_to() {
    let mut e = ed("one");
    e.keys(&parse("A two<Esc>"));
    e.keys(&parse("u"));
    // undo can't reach "one two" again after this
    e.keys(&parse("A ONE<Esc>"));
    e.keys(&parse("<C-r>"));
    assert_eq!(e.text(), "one ONE");
    e.keys(&parse("<Space>u"));
    let labels: Vec<String> = e.menu().unwrap().entries.iter().map(|x| x.label.clone()).collect();
    assert_eq!(labels.len(), 3, "{labels:?}");
    // newest first: one ONE (now), one two, one
    e.keys(&parse("2"));
    assert_eq!(e.text(), "one two");
    e.keys(&parse("u"));
    assert_eq!(e.text(), "one ONE", "going back is a change u takes back");
}

#[test]
fn nothing_to_list_before_a_change() {
    let mut e = ed("one");
    e.ex("undolist");
    assert!(e.menu().is_none());
    assert_eq!(e.message().unwrap().text, "no changes yet");
}
