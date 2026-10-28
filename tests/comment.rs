mod common;

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
