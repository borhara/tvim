mod common;
use tvim::Editor;
use tvim::keys::parse;

#[test]
fn space_is_the_leader() {
    let mut e = Editor::with_text("x");
    e.keys(&parse(" g"));
    assert!(e.grep_picker().is_some());
    e.keys(&parse("<Esc> /"));
    assert!(e.grep_picker().is_some());
    e.keys(&parse("<Esc> f"));
    assert!(e.finder().is_some());
    e.keys(&parse("<Esc> b"));
    assert_eq!(e.finder().unwrap().title(), "open files");
    e.keys(&parse("<Esc> z"));
    assert!(
        e.finder().is_none() && e.grep_picker().is_none(),
        "anything else after space does nothing"
    );
    assert_eq!(e.cursor().col, 0, "and space no longer moves right");
}

#[test]
fn the_open_files_picker() {
    let root = project("buffers");
    let mut e = Editor::new();
    e.ex(&format!("e {}", root.join("src/one.rs").display()));
    e.ex(&format!("e {}", root.join("src/two.rs").display()));
    e.pick_buffer_list();
    let found: Vec<String> = e.finder().unwrap().matches().map(|(p, _)| p.to_string()).collect();
    assert_eq!(found.len(), 2);
    e.keys(&parse("one<CR>"));
    assert!(e.file().path.as_ref().unwrap().ends_with("src/one.rs"));
}
