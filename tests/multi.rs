mod common;
use common::{ed, text_after as t};
use tvim::keys::parse;

#[test]
fn ctrl_n_selects_each_next_word_and_c_changes_them_all() {
    assert_eq!(
        t("let foo = foo + 1;\nfoo();\nfood", "w<C-n><C-n><C-n>cbar<Esc>"),
        "let bar = bar + 1;\nbar();\nfood",
        "whole words only, typed at each"
    );
}

#[test]
fn ctrl_x_skips_one_and_ctrl_p_lets_the_last_go() {
    assert_eq!(t("a x\nb x\nc x", "w<C-n><C-n><C-x>cy<Esc>"), "a y\nb x\nc y");
    assert_eq!(t("a x\nb x\nc x", "w<C-n><C-n><C-p>cy<Esc>"), "a y\nb x\nc x");
}

#[test]
fn i_and_a_type_before_and_after_each() {
    assert_eq!(t("x x", "<C-n><C-n>i(<Esc>"), "(x (x");
    assert_eq!(t("x x", "<C-n><C-n>c(<Esc>"), "() ()", "with brackets closing at each");
    assert_eq!(t("x x", "<C-n><C-n>a!<Esc>"), "x! x!");
}

#[test]
fn d_deletes_them_all_and_u_brings_them_back_at_once() {
    let mut e = ed("one two one two one");
    e.keys(&parse("<C-n><C-n><C-n>d"));
    assert_eq!(e.text(), " two  two ");
    e.keys(&parse("u"));
    assert_eq!(e.text(), "one two one two one");
}

#[test]
fn a_selection_on_one_line_is_looked_for_as_it_is() {
    assert_eq!(t("a.b a.bc", "vll<C-n><C-n>cX<Esc>"), "X Xc");
}

#[test]
fn backspace_and_more_typing_follow_at_each() {
    assert_eq!(t("ab ab", "<C-n><C-n>cxyz<BS><BS>q<Esc>"), "xq xq");
}

#[test]
fn any_other_key_leaves_the_cursors() {
    let mut e = ed("x x");
    e.keys(&parse("<C-n><C-n>"));
    assert_eq!(e.multi_cursors().len(), 2);
    e.keys(&parse("0"));
    assert!(e.multi_cursors().is_empty());
    assert_eq!(e.cursor().col, 0, "and the key does as usual");
    e.keys(&parse("<C-n><C-n>cy<CR>z"));
    assert_eq!(e.text(), "y y\nz", "a new line leaves them too");
}
