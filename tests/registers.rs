mod common;
use common::ed;
use tvim::keys::parse;

#[test]
fn the_clipboard() {
    let mut e = ed("hello\nworld");
    e.keys(&parse("\"+yyj\"+p"));
    assert_eq!(e.text(), "hello\nworld\nhello");
    e.keys(&parse("gg\"*yiwG$\"*p"));
    assert_eq!(e.text(), "hello\nworld\nhellohello");
}
