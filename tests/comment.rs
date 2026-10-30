mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

fn after(name: &str, text: &str, spec: &str) -> String {
    let path = temp(name);
    std::fs::write(&path, text).unwrap();
    let mut e = Editor::open(&path).unwrap();
    e.set_clipboard(Box::new(tvim::clipboard::Memory::default()));
    e.keys(&parse(spec));
    e.text()
}

#[test]
fn gcc_toggles_the_line() {
    assert_eq!(after("a.rs", "let x = 1;", "gcc"), "// let x = 1;");
    assert_eq!(after("b.rs", "// let x = 1;", "gcc"), "let x = 1;");
    assert_eq!(
        after("c.rs", "    //let x;", "gcc"),
        "    let x;",
        "the space after is optional"
    );
    assert_eq!(after("d.rs", "a\nb\nc", "2gcc"), "// a\n// b\nc");
    assert_eq!(
        after("e.rs", "a\nb", "j5gcc"),
        "a\n// b",
        "a count past the end stops there"
    );
}

#[test]
fn the_comment_follows_the_language() {
    assert_eq!(after("a.py", "x = 1", "gcc"), "# x = 1");
    assert_eq!(after("a.lua", "x = 1", "gcc"), "-- x = 1");
    assert_eq!(after("a.sql", "select 1", "gcc"), "-- select 1");
    assert_eq!(after("a.lisp", "(x)", "gcc"), "; (x)");
    assert_eq!(after("a.html", "<p>", "gcc"), "<!-- <p> -->");
    assert_eq!(after("b.html", "  <!-- <p> -->", "gcc"), "  <p>");
    assert_eq!(after("a.css", "a { }", "gcc"), "/* a { } */");
    assert_eq!(after("a.md", "hi", "gcc"), "<!-- hi -->");
    assert_eq!(after("noext", "x", "gcc"), "# x");
}

#[test]
fn a_block_is_commented_at_its_least_indent() {
    let text = "fn f() {\n    a;\n\n        b;\n}";
    assert_eq!(
        after("block.rs", text, "jgcj"),
        "fn f() {\n    // a;\n\n        b;\n}",
        "blank lines are skipped"
    );
    assert_eq!(
        after("block2.rs", text, "jgc2j"),
        "fn f() {\n    // a;\n\n    //     b;\n}"
    );
    assert_eq!(
        after("block3.rs", text, "gcip"),
        "// fn f() {\n//     a;\n\n        b;\n}"
    );
}

#[test]
fn mixed_lines_get_commented_and_all_commented_get_uncommented() {
    assert_eq!(after("mix.py", "# a\nb", "gcj"), "# # a\n# b");
    assert_eq!(after("all.py", "# a\n\n  # b", "gc2j"), "a\n\n  b");
}

#[test]
fn visual_mode() {
    assert_eq!(after("v.rs", "a\nb\nc", "Vjgc"), "// a\n// b\nc");
    assert_eq!(after("v2.rs", "a\nb\nc", "vjgc"), "// a\n// b\nc");
    assert_eq!(
        after("v3.rs", "a\nb\nc\nd", "Vjgcjj."),
        "// a\n// b\n// c\n// d",
        "dot does as many lines"
    );
}
