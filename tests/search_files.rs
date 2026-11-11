mod common;
use tvim::Editor;
use tvim::keys::parse;

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
