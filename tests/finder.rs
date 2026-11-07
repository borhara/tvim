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
fn nothing_matching() {
    let root = project("none");
    let mut e = Editor::new();
    e.find_files_in(&root, "qqqq");
    assert!(found(&e).is_empty());
    e.keys(&parse("<CR>"));
    assert!(e.finder().is_some(), "enter on nothing does nothing");
}
