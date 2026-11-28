mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

// a project with an .editorconfig, and a file in it
fn project(name: &str, config: &str, file: &str, text: &str) -> std::path::PathBuf {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".editorconfig"), config).unwrap();
    let path = dir.join(file);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn a_files_editorconfig_sets_how_it_indents() {
    let path = project(
        "ec_go",
        "root = true\n[*.go]\nindent_style = tab\nindent_size = 8\n",
        "a.go",
        "x\n",
    );
    let mut e = Editor::open(&path).unwrap();
    let o = &e.options;
    assert_eq!((o.expand_tab, o.tab_stop, o.shift_width), (false, 8, 8));
    e.keys(&parse(">>"));
    assert_eq!(e.text(), "\tx");
}

#[test]
fn your_own_settings_come_back_in_a_file_without_one() {
    let path = project("ec_back", "root = true\n[*.md]\nindent_size = 2\n", "a.md", "x\n");
    let other = project("ec_none", "root = true\n", "b.txt", "y\n");
    let mut e = Editor::open(&path).unwrap();
    assert_eq!(e.apply_config("set sw=3 ts=3"), None);
    assert_eq!(e.options.shift_width, 2, "the project's over your config");
    e.ex(&format!("e {}", other.display()));
    assert_eq!((e.options.shift_width, e.options.tab_stop), (3, 3));
    e.ex("b1");
    assert_eq!((e.options.shift_width, e.options.tab_stop), (2, 2));
    e.ex("b2");
    assert_eq!(e.options.shift_width, 3);
}

#[test]
fn writing_trims_and_ends_the_file_as_it_says() {
    let config = "root = true\n[*]\ntrim_trailing_whitespace = true\ninsert_final_newline = false\n";
    let path = project("ec_write", config, "a.txt", "one  \ntwo\t\n");
    let mut e = Editor::open(&path).unwrap();
    e.ex("w");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo");
    e.keys(&parse("u"));
    assert_eq!(e.text(), "one  \ntwo\t", "taken off as a change of its own");
    let path = project(
        "ec_newline",
        "root = true\n[*]\ninsert_final_newline = true\n",
        "b.txt",
        "no end",
    );
    let mut e = Editor::open(&path).unwrap();
    e.ex("w");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "no end\n");
}

#[test]
fn a_set_made_in_a_file_with_one_carries_to_files_without() {
    let path = project("ec_set", "root = true\n[*.md]\nindent_size = 2\n", "a.md", "x\n");
    let other = project("ec_set_none", "root = true\n", "b.txt", "y\n");
    let mut e = Editor::open(&path).unwrap();
    e.ex("set sw=5 noet");
    assert_eq!(e.options.shift_width, 5);
    e.ex(&format!("e {}", other.display()));
    assert_eq!((e.options.shift_width, e.options.expand_tab), (5, false), "not lost");
    assert_eq!(e.options.tab_stop, 4, "what wasn't set is as it was");
}
