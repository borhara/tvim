use common::{cursor_after as at, text_after as t};

#[test]
fn patterns_are_regular_expressions() {
    let src = "ab 12 cd 345\nba";
    assert_eq!(at(src, r"/\d+<CR>"), (0, 3));
    assert_eq!(at(src, r"/\d+<CR>n"), (0, 9));
    assert_eq!(at(src, "/^b<CR>"), (1, 0));
    assert_eq!(at(src, "/cd|ba<CR>n"), (1, 0));
    assert_eq!(at(src, "/(cd|ba)$<CR>"), (1, 0));
    assert_eq!(at(src, r"/\<\w{2}\><CR>n"), (0, 6));
    assert_eq!(at(src, "/[3-5]{3}<CR>"), (0, 9));
    assert_eq!(at("a.b x|a.b", r"/x\|a\.b<CR>"), (0, 4), "\\| is a literal bar");
}

#[test]
fn case_flags_in_the_pattern() {
    let src = "Foo foo FOO";
    assert_eq!(at(src, r"/\Cfoo<CR>n"), (0, 4));
    assert_eq!(at(src, r"/FOO\c<CR>"), (0, 4));
    assert_eq!(
        at("Foo FOO", r"/\w\Wfoo<CR>"),
        (0, 2),
        "\\W isn't a capital, so case is still ignored"
    );
}
