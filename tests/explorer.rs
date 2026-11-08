mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

fn folder(name: &str) -> std::path::PathBuf {
    let root = temp(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("b.txt"), "bee\n").unwrap();
    std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(root.join("Zed.md"), "# zed\n").unwrap();
    std::fs::write(root.join(".hidden"), "").unwrap();
    std::fs::write(root.join("sub/inner.txt"), "inside\n").unwrap();
    root.canonicalize().unwrap()
}

fn names(e: &Editor) -> Vec<String> {
    e.explorer()
        .unwrap()
        .visible()
        .iter()
        .map(|(en, _)| en.name.clone())
        .collect()
}

fn selected(e: &Editor) -> usize {
    e.explorer().unwrap().selected()
}

fn browse(name: &str) -> (Editor, std::path::PathBuf) {
    let root = folder(name);
    let mut e = Editor::new();
    e.explore(&root);
    (e, root)
}

#[test]
fn folders_first_then_files_hidden_ones_left_out() {
    let (mut e, _) = browse("list");
    assert_eq!(names(&e), ["sub", "a.rs", "b.txt", "Zed.md"]);
    assert_eq!(selected(&e), 0, "it starts on the first thing in the folder");
    e.keys(&parse("jj."));
    assert_eq!(names(&e), ["sub", ".hidden", "a.rs", "b.txt", "Zed.md"]);
    assert_eq!(selected(&e), 3, "still on b.txt");
}

#[test]
fn enter_opens_a_file_and_the_browser_stays() {
    let (mut e, root) = browse("open");
    assert!(e.explorer_focused());
    e.keys(&parse("j<CR>"));
    assert_eq!(e.text(), "fn a() {}");
    assert_eq!(e.file().path.as_deref(), Some(root.join("a.rs").as_path()));
    assert!(e.explorer().is_some(), "still beside the file");
    assert!(!e.explorer_focused(), "and typing goes to the file");
    e.keys(&parse("x"));
    assert_eq!(e.text(), "n a() {}");
}

#[test]
fn tab_moves_between_the_file_and_the_browser() {
    let (mut e, _) = browse("tab");
    e.keys(&parse("j<CR>"));
    e.keys(&parse("<Tab>"));
    assert!(e.explorer_focused());
    e.keys(&parse("j"));
    assert_eq!(selected(&e), 2, "keys move the browser, not the text");
    e.keys(&parse("<Tab>"));
    assert!(!e.explorer_focused());
    e.keys(&parse("<Tab><Esc>"));
    assert!(!e.explorer_focused(), "Esc goes back to the file too");
    let mut e = Editor::with_text("x");
    e.keys(&parse("<Tab>"));
    assert!(e.explorer_focused(), "Tab opens it when it's hidden");
}
