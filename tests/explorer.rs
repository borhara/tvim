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

fn depths(e: &Editor) -> Vec<usize> {
    e.explorer().unwrap().visible().iter().map(|(en, _)| en.depth).collect()
}

#[test]
fn folders_open_in_place() {
    let (mut e, root) = browse("folders");
    e.keys(&parse("<CR>"));
    assert_eq!(e.explorer().unwrap().root(), root, "the tree keeps its root");
    assert_eq!(names(&e), ["sub", "inner.txt", "a.rs", "b.txt", "Zed.md"]);
    assert_eq!(depths(&e), [0, 1, 0, 0, 0]);
    e.keys(&parse("<CR>"));
    assert_eq!(names(&e).len(), 4, "Enter folds it again");
    e.keys(&parse("l"));
    assert_eq!((names(&e).len(), selected(&e)), (5, 0), "l opens it");
    e.keys(&parse("l"));
    assert_eq!(selected(&e), 1, "and on an open folder steps in");
    e.keys(&parse("h"));
    assert_eq!(selected(&e), 0, "h goes to the folder above");
    e.keys(&parse("h"));
    assert_eq!(names(&e).len(), 4, "and on an open folder folds it");
    e.keys(&parse("<Right>j<Left>"));
    assert_eq!(selected(&e), 0, "the arrows do the same");
}

#[test]
fn dash_makes_the_parent_the_root() {
    let (mut e, root) = browse("dash");
    e.keys(&parse("l"));
    e.keys(&parse("-"));
    let x = e.explorer().unwrap();
    assert_eq!(x.root(), root.parent().unwrap());
    let (picked, _) = &x.visible()[x.selected()];
    assert_eq!(picked.path, root, "on the folder it came out of");
    assert!(
        x.is_expanded(&root) && x.is_expanded(&root.join("sub")),
        "still open, as it was"
    );
}

#[test]
fn the_open_files_folders_open_by_themselves() {
    let root = folder("reveal");
    std::fs::create_dir_all(root.join("sub/deep")).unwrap();
    std::fs::write(root.join("sub/deep/far.txt"), "far\n").unwrap();
    let mut e = Editor::open(&root.join("sub/deep/far.txt")).unwrap();
    e.explore(&root);
    assert_eq!(
        names(&e),
        ["sub", "deep", "far.txt", "inner.txt", "a.rs", "b.txt", "Zed.md"]
    );
    assert_eq!(selected(&e), 2, "with the file picked");

    e.keys(&parse("<Tab>"));
    e.ex(&format!("e {}", root.join("a.rs").display()));
    e.keys(&parse("gT"));
    assert_eq!(names(&e)[selected(&e)], "far.txt", "switching files follows along");
    e.keys(&parse("gt"));
    assert_eq!(names(&e)[selected(&e)], "a.rs");

    e.keys(&parse(":Ex<CR>"));
    assert_eq!(e.explorer().unwrap().root(), root, ":Ex opens the file's own folder");
    e.ex(&format!("e {}", root.join("sub/inner.txt").display()));
    e.keys(&parse(":Ex<CR>"));
    assert_eq!(e.explorer().unwrap().root(), root.join("sub"));
    assert_eq!(names(&e)[selected(&e)], "inner.txt", "with the file picked");
}

#[test]
fn moving_around() {
    let (mut e, _) = browse("moving");
    e.keys(&parse("G"));
    assert_eq!(selected(&e), 3);
    e.keys(&parse("j"));
    assert_eq!(selected(&e), 3);
    e.keys(&parse("gk"));
    assert_eq!(selected(&e), 0);
}

#[test]
fn filtering_by_letters_in_order() {
    let (mut e, _) = browse("filter");
    e.keys(&parse("/zd"));
    assert!(e.explorer().unwrap().is_filtering());
    assert_eq!(names(&e), ["Zed.md"]);
    e.keys(&parse("<CR>"));
    assert_eq!(e.text(), "# zed");
    assert_eq!(names(&e).len(), 4, "the whole folder again once a file's open");
    assert_eq!(names(&e)[selected(&e)], "Zed.md");
    let (mut e, _) = browse("filter-esc");
    e.keys(&parse("/xyz"));
    assert!(names(&e).is_empty());
    e.keys(&parse("<Esc>"));
    assert_eq!(names(&e).len(), 4);
    e.keys(&parse("/.h"));
    assert_eq!(
        names(&e),
        [".hidden"],
        "a filter starting with a dot finds hidden files"
    );
}

#[test]
fn filtering_looks_inside_open_folders() {
    let (mut e, _) = browse("filter-tree");
    e.keys(&parse("/inn"));
    assert!(names(&e).is_empty(), "a folded folder isn't searched");
    e.keys(&parse("<Esc>l/inn"));
    assert_eq!(names(&e), ["sub", "inner.txt"], "the match under its folder");
    assert_eq!(selected(&e), 1, "on the match, not the folder");
    e.keys(&parse("<CR>"));
    assert_eq!(e.text(), "inside");
}

#[test]
fn q_hides_it_and_lex_toggles_it() {
    let (mut e, _) = browse("close");
    e.keys(&parse("q"));
    assert!(e.explorer().is_none() && !e.should_quit());
    e.keys(&parse(":Lex<CR>"));
    assert!(e.explorer_focused());
    e.keys(&parse(":Lex<CR>"));
    assert!(e.explorer().is_none());
}

#[test]
fn unsaved_changes_stay_open_in_the_background() {
    let (mut e, _) = browse("dirty");
    e.keys(&parse("j<CR>x<Tab>j<CR>"));
    assert_eq!(e.text(), "bee", "the other file opens");
    assert_eq!(e.buffers().len(), 2);
    assert!(e.buffers()[0].modified, "and the changed one is kept, still changed");
    e.keys(&parse("gT"));
    assert_eq!(e.text(), "n a() {}");
}

#[test]
fn edit_with_a_folder_browses_it() {
    let root = folder("edit-dir");
    let mut e = Editor::with_text("");
    e.ex(&format!("e {}", root.display()));
    assert_eq!(e.explorer().unwrap().root(), root);
    let e = Editor::open(&root).unwrap();
    assert!(e.explorer().is_some());
}

#[test]
fn commands_work_from_the_browser() {
    let (mut e, _) = browse("commands");
    e.keys(&parse(":q<CR>"));
    assert!(e.should_quit());
}
