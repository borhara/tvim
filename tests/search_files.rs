mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

fn project(name: &str) -> std::path::PathBuf {
    let root = temp(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/one.rs"), "fn first() {}\nfn needle() {}\n").unwrap();
    std::fs::write(root.join("src/two.rs"), "// no match\n\n    let x = needle();\n").unwrap();
    root
}

#[test]
fn typing_searches_every_file_and_enter_jumps_to_the_line() {
    let root = project("jump");
    let mut e = Editor::new();
    e.grep_in(&root, "");
    e.wait_for_grep();
    e.keys(&parse("needle"));
    let g = e.grep_picker().unwrap();
    assert_eq!(g.hits().len(), 2);
    e.keys(&parse("<Down><CR>"));
    assert!(e.grep_picker().is_none());
    assert!(e.file().path.as_ref().unwrap().ends_with("src/two.rs"));
    assert_eq!((e.cursor().row, e.cursor().col), (2, 12));
    assert_eq!(
        e.search_pattern(),
        Some("needle"),
        "the match stays lit, and n goes to the next"
    );
}

#[test]
fn escape_closes_it_and_nothing_changes() {
    let root = project("esc");
    let mut e = Editor::with_text("mine");
    e.grep_in(&root, "needle");
    e.keys(&parse("<Esc>"));
    assert!(e.grep_picker().is_none());
    assert_eq!(e.text(), "mine");
}

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
fn grep_command_starts_with_a_query() {
    let mut e = Editor::with_text("x");
    e.ex("grep fn scroll");
    e.wait_for_grep();
    assert_eq!(e.grep_picker().unwrap().query(), "fn scroll");
    assert!(
        !e.grep_picker().unwrap().hits().is_empty(),
        "run from the crate, so it searches tvim's own files"
    );
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
