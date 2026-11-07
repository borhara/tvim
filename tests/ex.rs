mod common;
use common::{cursor_after as at, ed, temp, text_after as t};
use tvim::keys::parse;

#[test]
fn going_to_lines() {
    let src = "a\n  b\nc\nd";
    assert_eq!(at(src, ":2<CR>"), (1, 2));
    assert_eq!(at(src, ":$<CR>"), (3, 0));
    assert_eq!(at(src, ":99<CR>"), (3, 0));
    assert_eq!(at(src, ":$-1<CR>"), (2, 0));
    assert_eq!(at(src, "j:.+2<CR>"), (3, 0));
    assert_eq!(at(src, "jjma:1<CR>:'a<CR>"), (2, 0));
}

#[test]
fn substitute() {
    assert_eq!(t("a a\na a", ":s/a/b/<CR>"), "b a\na a");
    assert_eq!(t("a a\na a", ":s/a/b/g<CR>"), "b b\na a");
    assert_eq!(t("a a\na a", ":%s/a/b/g<CR>"), "b b\nb b");
    assert_eq!(t("a\na\na", ":2,3s/a/b/<CR>"), "a\nb\nb");
    assert_eq!(t("a/b", r":s#/#\/x#<CR>"), "a/xb");
    assert_eq!(t("a/b", r":s/\//-/<CR>"), "a-b");
    assert_eq!(t("cat", ":s/cat/[&]/<CR>"), "[cat]");
    assert_eq!(t("a,b", r":s/,/\n/<CR>"), "a\nb");
    assert_eq!(t("Aa", ":s/a/x/g<CR>"), "xx", "smart case: lowercase matches both");
    assert_eq!(t("Aa", ":s/a/x/gI<CR>"), "Ax");
    assert_eq!(
        t("foo bar", "/bar<CR>:s//baz/<CR>"),
        "foo baz",
        "an empty pattern is the last search"
    );
    assert_eq!(t("a a\na a", ":s/a/b/<CR>j&"), "b a\nb a", "& does it again");
}

#[test]
fn substitute_reports() {
    let mut e = ed("a a\na");
    e.ex("%s/a/b/g");
    assert_eq!(e.message().unwrap().text, "3 substitutions on 2 lines");
    e.ex("s/zzz/y/");
    assert_eq!(e.message().unwrap().text, "E486: Pattern not found: zzz");
    e.ex("s/zzz/y/e");
    assert!(e.message().is_none_or(|m| !m.error));
    e.ex("sxaxbx");
    assert!(e.message().unwrap().text.starts_with("E146"));
}

#[test]
fn substitute_is_one_change() {
    assert_eq!(t("a\na\na", ":%s/a/b/<CR>u"), "a\na\na");
}

#[test]
fn deleting_yanking_and_shifting_ranges() {
    assert_eq!(t("a\nb\nc\nd", ":2,3d<CR>"), "a\nd");
    assert_eq!(t("a\nb\nc", ":2,3y<CR>P"), "b\nc\na\nb\nc");
    assert_eq!(t("a\nb", ":%><CR>"), "    a\n    b");
    assert_eq!(t("a\nb", ":%>><CR>"), "        a\n        b");
    assert_eq!(t("    a", ":<<CR>"), "a");
    assert_eq!(t("a\nb\nc", ":1,2j<CR>"), "a b\nc");
    assert_eq!(t("a\nb\nc", ":%norm A;<CR>"), "a;\nb;\nc;");
    assert_eq!(t("a\nb", ":3,1d<CR>"), "a\nb", "a range past the end");
}

#[test]
fn unknown_commands() {
    let mut e = ed("a");
    e.ex("frobnicate");
    assert_eq!(e.message().unwrap().text, "E492: Not an editor command: frobnicate");
}

#[test]
fn quitting() {
    let mut e = ed("a");
    e.ex("q");
    assert!(e.should_quit());
    let mut e = ed("a");
    e.keys(&parse("x:q<CR>"));
    assert!(!e.should_quit());
    assert_eq!(
        e.message().unwrap().text,
        "E37: No write since last change (add ! to override)"
    );
    e.keys(&parse(":q!<CR>"));
    assert!(e.should_quit());
    let mut e = ed("a");
    e.keys(&parse("ZQ"));
    assert!(e.should_quit());
}

#[test]
fn writing_and_quitting() {
    let path = temp("wq.txt");
    let _ = std::fs::remove_file(&path);
    let mut e = ed("hi");
    e.ex(&format!("w {}", path.display()));
    assert!(
        e.message().unwrap().text.ends_with("[New] 1L, 3B written"),
        "{:?}",
        e.message()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hi\n");
    assert_eq!(
        e.file().path.as_deref(),
        Some(path.as_path()),
        "an unnamed buffer takes the name"
    );
    e.keys(&parse("Ax<Esc>:x<CR>"));
    assert!(e.should_quit());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hix\n");
}

#[test]
fn shell_commands_are_handed_to_the_screen() {
    let mut e = ed("a");
    e.ex("!echo hi");
    assert_eq!(e.take_effects(), [tvim::Effect::Shell("echo hi".into())]);
    e.ex("!");
    assert!(e.message().unwrap().text.starts_with("E471"));
    e.keys(&parse("<C-z>"));
    assert_eq!(e.take_effects(), [tvim::Effect::Suspend]);
}

#[test]
fn completing_names() {
    let mut e = ed("a");
    e.keys(&parse(":se<Tab>"));
    assert_eq!(e.cmdline().text, "set");
    e.keys(&parse(" relat<Tab>"));
    assert_eq!(e.cmdline().text, "set relativenumber");
    let dir = temp("complete");
    std::fs::create_dir_all(dir.join("subdir")).unwrap();
    std::fs::write(dir.join("notes.txt"), "").unwrap();
    let mut e = ed("a");
    e.keys(&parse(&format!(":e {}/no<Tab>", dir.display())));
    assert_eq!(e.cmdline().text, format!("e {}/notes.txt", dir.display()));
    e.keys(&parse("<C-u>"));
    e.keys(&parse(&format!("e {}/s<Tab>", dir.display())));
    assert_eq!(e.cmdline().text, format!("e {}/subdir/", dir.display()));
}

#[test]
fn editing_the_command_line() {
    let mut e = ed("a");
    e.keys(&parse(":set nu<Left><Left><BS>x<Home>y<End>z"));
    assert_eq!(e.cmdline().text, "ysetxnuz");
    let mut e = ed("a");
    e.keys(&parse(":one two<C-w>"));
    assert_eq!(e.cmdline().text, "one ");
    e.paste("pasted\nlines");
    assert_eq!(e.cmdline().text, "one pasted");
}

#[test]
fn substitute_with_regular_expressions() {
    assert_eq!(t("hello world", r":s/(\w+) (\w+)/\2 \1/<CR>"), "world hello");
    assert_eq!(t("a\nb", ":%s/^/# /<CR>"), "# a\n# b");
    assert_eq!(t("a\nb", ":%s/$/;/<CR>"), "a;\nb;");
    assert_eq!(t("aa b aaa", ":s/a+/X/g<CR>"), "X b X");
    assert_eq!(t("a1b2", r":s/\d/<&>/g<CR>"), "a<1>b<2>");
    assert_eq!(t("a1b2", r":s/\d/<\0>/<CR>"), "a<1>b2");
    assert_eq!(t("abc", ":s/x*/-/g<CR>"), "-a-b-c-");
    assert_eq!(t("k=v", r":s/=/\n/<CR>"), "k\nv");
    assert_eq!(t("Aa", r":s/a\C/x/gi<CR>"), "Ax", "\\C beats the i flag");
    assert_eq!(t("one.two", r":s/\./ /<CR>"), "one two");
    assert_eq!(t("(x)", r":s/\((\w)\)/[\1]/<CR>"), "[x]");
}

#[test]
fn substitute_with_a_bad_pattern() {
    let mut e = ed("a[b");
    e.ex("s/[b/x/");
    assert_eq!(e.text(), "a[b");
    assert!(e.message().unwrap().text.starts_with("E383"));
}

#[test]
fn substitute_splitting_lines_reaches_the_whole_range() {
    let mut e = ed("a,b\nc,d");
    e.ex("%s/,/\\r/g");
    assert_eq!(e.text(), "a\nb\nc\nd");
}

#[test]
fn substitute_skips_an_empty_match_where_the_last_ended() {
    let mut e = ed("baaac");
    e.ex("s/a*/-/g");
    assert_eq!(e.text(), "-b-c-");
}

#[test]
fn read_at_line_zero_goes_above_the_first() {
    let p = temp("read-zero.txt");
    std::fs::write(&p, "x\n").unwrap();
    let mut e = ed("a\nb");
    e.ex(&format!("0r {}", p.display()));
    assert_eq!(e.text(), "x\na\nb");
}
