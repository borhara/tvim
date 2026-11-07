mod common;
use common::text_after as t;

#[test]
fn ctrl_n_completes_from_the_nearest_word() {
    assert_eq!(t("foobar\nfoo", "jA<C-n><C-y><Esc>"), "foobar\nfoobar");
    assert_eq!(t("format\nfoobar\n", "Gifo<C-n><C-y><Esc>"), "format\nfoobar\nfoobar");
    assert_eq!(
        t("format\nfoobar\n", "Gifo<C-n><C-n><C-y><Esc>"),
        "format\nfoobar\nformat",
        "again goes further"
    );
}

#[test]
fn ctrl_p_goes_backwards() {
    assert_eq!(t("format\nfoobar\n", "Gifo<C-p><C-y><Esc>"), "format\nfoobar\nformat");
    assert_eq!(
        t("format\nfoobar\n", "Gifo<C-n><C-p><C-p><C-y><Esc>"),
        "format\nfoobar\nformat"
    );
}

#[test]
fn cycling_past_the_end_comes_back_to_what_was_typed() {
    assert_eq!(
        t("format\nfoobar\n", "Gifo<C-n><C-n><C-n><C-y><Esc><Esc>"),
        "format\nfoobar\nfo"
    );
}
