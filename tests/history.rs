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

#[test]
fn each_state_says_what_changed_from_the_one_it_was_made_to() {
    let mut e = ed("one");
    e.keys(&parse("o1<CR>2<CR>3<CR>4<CR>5<Esc>"));
    e.keys(&parse("u"));
    e.keys(&parse("oonly<Esc>"));
    e.keys(&parse("<Space>u"));
    let hints: Vec<String> = e.menu().unwrap().entries.iter().map(|x| x.hint.clone()).collect();
    assert_eq!(hints, ["+1", "+5", ""], "from one, not from the five lines undone");
}

#[test]
fn a_files_kept_history_is_listed_before_any_change() {
    let dir = common::temp("history_kept");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (file, undo) = (dir.join("a.txt"), dir.join("undo"));
    std::fs::write(&file, "a\n").unwrap();
    let open = |file: &std::path::Path| {
        let mut e = tvim::Editor::open(file).unwrap();
        e.set_undo_dir(Some(undo.clone()));
        e
    };
    let mut e = open(&file);
    e.keys(&parse("Ab<Esc>"));
    e.ex("w");
    let mut e = open(&file);
    e.keys(&parse("<Space>u"));
    let m = e.menu().expect("the kept history");
    assert_eq!(m.entries.len(), 2);
    assert!(m.entries[0].label.ends_with("· now"));
}
