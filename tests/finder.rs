mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

fn project(name: &str) -> std::path::PathBuf {
    let root = temp(name);
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["src/ui", "target/debug", ".git", "node_modules/x", "logs"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    for (file, text) in [
        ("src/main.rs", "fn main() {}\n"),
        ("src/ui/panel.rs", "// panel\n"),
        ("README.md", "# hi\n"),
        ("target/debug/main.rs", "built\n"),
        (".git/config", "x\n"),
        ("node_modules/x/index.js", "x\n"),
        ("logs/run.log", "x\n"),
        (".gitignore", "logs/\n*.tmp\n"),
        ("scratch.tmp", "x\n"),
    ] {
        std::fs::write(root.join(file), text).unwrap();
    }
    root
}

fn found(e: &Editor) -> Vec<String> {
    e.finder().unwrap().matches().map(|(p, _)| p.to_string()).collect()
}

#[test]
fn lists_every_file_but_the_ignored_ones() {
    let root = project("all");
    let mut e = Editor::new();
    e.find_files_in(&root, "");
    let mut files = found(&e);
    files.sort();
    assert_eq!(files, [".gitignore", "README.md", "src/main.rs", "src/ui/panel.rs"]);
}

#[test]
fn typing_narrows_and_enter_opens() {
    let root = project("open");
    let mut e = Editor::new();
    e.find_files_in(&root, "");
    e.keys(&parse("pnl"));
    assert_eq!(found(&e), ["src/ui/panel.rs"]);
    e.keys(&parse("<CR>"));
    assert!(e.finder().is_none());
    assert_eq!(e.text(), "// panel");
}

#[test]
fn moving_and_closing() {
    let root = project("moving");
    let mut e = Editor::new();
    e.find_files_in(&root, "rs");
    assert_eq!(e.finder().unwrap().selected(), 0);
    e.keys(&parse("<Down>"));
    assert_eq!(e.finder().unwrap().selected(), 1);
    e.keys(&parse("<C-p>"));
    assert_eq!(e.finder().unwrap().selected(), 0);
    e.keys(&parse("<Esc>"));
    assert!(e.finder().is_none());
    assert!(e.text().is_empty());
    e.find_files_in(&root, "a");
    e.keys(&parse("<BS><BS>"));
    assert!(e.finder().is_none(), "backspace on nothing closes it");
}

#[test]
fn nothing_matching() {
    let root = project("none");
    let mut e = Editor::new();
    e.find_files_in(&root, "qqqq");
    assert!(found(&e).is_empty());
    e.keys(&parse("<CR>"));
    assert!(e.finder().is_some(), "enter on nothing does nothing");
}

#[test]
fn opening_keeps_the_changed_file_open() {
    let root = project("dirty");
    let mut e = Editor::with_text("mine");
    e.keys(&parse("x"));
    e.find_files_in(&root, "main");
    e.keys(&parse("<CR>"));
    assert_eq!(e.text(), "fn main() {}");
    assert_eq!(e.buffers().len(), 2);
    assert!(e.finder().is_none());
}

#[test]
fn ctrl_p_and_find_open_it() {
    let mut e = Editor::with_text("x");
    e.keys(&parse("<C-p>"));
    assert!(e.finder().is_some());
    e.keys(&parse("<Esc>"));
    e.keys(&parse(":find edit<CR>"));
    assert_eq!(e.finder().unwrap().query(), "edit");
    assert_eq!(
        found(&e)[0],
        "src/editor.rs",
        "run from the crate, so it finds tvim's own files"
    );
}

#[test]
fn it_floats_over_the_editor() {
    let mut e = Editor::with_text("x");
    e.keys(&parse(":find ui/mo<CR>"));
    let shown = tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&tvim::ui::Tvim::new(e), 110, 24).plain()
    });
    assert!(shown.contains(&format!("find in {}", common::folder())), "{shown}");
    assert!(shown.contains("› ui/mo"), "{shown}");
    assert!(shown.contains("› ⊛ src/ui/mod.rs"), "with its icon: {shown}");
    assert!(
        shown.contains("src/ui/mod.rs  rust"),
        "and a preview beside it: {shown}"
    );
    assert!(shown.contains(" mod gitmenu;"), "{shown}");
    assert!(shown.contains("esc close"), "{shown}");
}
