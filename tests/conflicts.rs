mod common;
use common::ed;
use tvim::keys::parse;

const TEXT: &str = "top\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> topic\nmiddle\n<<<<<<< HEAD\nmine\n||||||| base\nold\n=======\nyours\n>>>>>>> other\nend";

#[test]
fn brackets_x_go_from_one_conflict_to_the_next() {
    let mut e = ed(TEXT);
    let row = |e: &mut tvim::Editor, keys: &str| {
        e.keys(&parse(keys));
        e.cursor().row
    };
    assert_eq!(row(&mut e, "]x"), 1);
    assert_eq!(row(&mut e, "]x"), 7);
    assert_eq!(row(&mut e, "]x"), 7, "no wrapping at the end");
    assert_eq!(row(&mut e, "[x"), 1);
    assert_eq!(row(&mut e, "G2[x"), 1);
    assert_eq!(row(&mut e, "gg2]x"), 7);
}
