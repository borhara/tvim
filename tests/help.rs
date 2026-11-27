mod common;
use common::ed;
use tvim::keys::parse;

#[test]
fn help_opens_the_docs_above_at_a_topic() {
    let mut e = ed("my text");
    e.ex("help spelling");
    assert_eq!(e.window_count(), 2);
    assert!(e.text().starts_with("# tvim"), "the README");
    let line = e.text().lines().nth(e.cursor().row).unwrap().to_string();
    assert!(line.starts_with("**Spelling.**"), "{line}");
    e.keys(&parse("x"));
    assert_eq!(e.message().unwrap().text, "help.md can't be changed");
}

#[test]
fn help_finds_keys_and_commands_too_and_reuses_its_window() {
    let mut e = ed("my text");
    e.ex("help zg");
    let at = |e: &tvim::Editor| e.text().lines().nth(e.cursor().row).unwrap().to_string();
    assert!(at(&e).contains("`zg`"), "{}", at(&e));
    e.keys(&parse("<C-w>j"));
    assert_eq!(e.text(), "my text");
    e.ex("help :Gdiff");
    assert_eq!(e.window_count(), 2, "the same window again");
    assert!(at(&e).contains("`:Gdiff`"), "{}", at(&e));
    e.ex("help nothing like this at all");
    assert_eq!(
        e.message().unwrap().text,
        "E149: Sorry, no help for nothing like this at all"
    );
}

#[test]
fn help_topics_are_suggested_as_typed() {
    let mut e = ed("");
    e.keys(&parse(":help spel"));
    let labels: Vec<String> = e.cmd_suggestions().0.iter().map(|s| s.label.clone()).collect();
    assert_eq!(labels, ["Spelling"]);
}
