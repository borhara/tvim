mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

fn files(name: &str) -> Vec<std::path::PathBuf> {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ["one.txt", "two.txt", "three.txt"]
        .iter()
        .map(|f| {
            let p = dir.join(f);
            std::fs::write(&p, format!("{}\n", f.trim_end_matches(".txt"))).unwrap();
            p
        })
        .collect()
}

fn open_all(e: &mut Editor, paths: &[std::path::PathBuf]) {
    for p in paths {
        e.ex(&format!("e {}", p.display()));
    }
}

#[test]
fn opening_files_adds_buffers() {
    let paths = files("adds");
    let mut e = Editor::new();
    open_all(&mut e, &paths);
    assert_eq!(e.buffers().len(), 3, "the empty buffer it started with is reused");
    assert_eq!(e.text(), "three");
    assert_eq!(e.buffer_index(), 2);
    e.ex(&format!("e {}", paths[0].display()));
    assert_eq!(
        e.buffers().len(),
        3,
        "an open file is brought forward, not opened twice"
    );
    assert_eq!(e.text(), "one");
}

#[test]
fn moving_between_them() {
    let paths = files("moving");
    let mut e = Editor::new();
    open_all(&mut e, &paths);
    e.keys(&parse("gt"));
    assert_eq!(e.text(), "one", "round from the last to the first");
    e.keys(&parse("gT"));
    assert_eq!(e.text(), "three");
    e.keys(&parse("2gt"));
    assert_eq!(e.text(), "two", "a count picks the tab");
    e.ex("bn");
    assert_eq!(e.text(), "three");
    e.ex("bp");
    assert_eq!(e.text(), "two");
    e.ex("bfirst");
    assert_eq!(e.text(), "one");
    e.ex("blast");
    assert_eq!(e.text(), "three");
    e.ex("b two");
    assert_eq!(e.text(), "two");
    e.ex("b 1");
    assert_eq!(e.text(), "one");
    e.ex("b nope");
    assert!(e.message().unwrap().text.starts_with("E94"));
}

#[test]
fn each_keeps_its_own_cursor_undo_and_changes() {
    let paths = files("own");
    let mut e = Editor::new();
    open_all(&mut e, &paths[..2]);
    e.keys(&parse("Aaa<Esc>"));
    e.keys(&parse("gt"));
    e.keys(&parse("x"));
    assert_eq!(e.text(), "ne");
    e.keys(&parse("gt"));
    assert_eq!((e.text().as_str(), e.cursor().col), ("twoaa", 4));
    e.keys(&parse("u"));
    assert_eq!(e.text(), "two", "undo is per file");
    e.keys(&parse("gtu"));
    assert_eq!(e.text(), "one");
}

#[test]
fn quitting_minds_every_file() {
    let paths = files("quit");
    let mut e = Editor::new();
    open_all(&mut e, &paths[..2]);
    e.keys(&parse("xgt"));
    e.ex("q");
    assert!(!e.should_quit());
    assert!(e.message().unwrap().text.starts_with("E162"), "{:?}", e.message());
    e.ex("q!");
    assert!(e.should_quit());
}

#[test]
fn write_all_and_quit_all() {
    let paths = files("wall");
    let mut e = Editor::new();
    open_all(&mut e, &paths[..2]);
    e.keys(&parse("A!<Esc>gtA?<Esc>"));
    e.ex("wa");
    assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "one?\n");
    assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "two!\n");
    assert!(e.buffers().iter().all(|b| !b.modified));
    e.keys(&parse("x"));
    e.ex("wqa");
    assert!(e.should_quit());
    assert_eq!(
        std::fs::read_to_string(&paths[0]).unwrap(),
        "one\n",
        "x took the ? the cursor was on"
    );
}

#[test]
fn closing_a_buffer() {
    let paths = files("close");
    let mut e = Editor::new();
    open_all(&mut e, &paths);
    e.keys(&parse("2gt"));
    e.ex("bd");
    assert_eq!(e.buffers().len(), 2);
    assert_eq!(e.text(), "three", "the next one comes forward");
    e.keys(&parse("x"));
    e.ex("bd");
    assert!(e.message().unwrap().text.starts_with("E89"));
    e.ex("bd!");
    e.ex("bd");
    assert_eq!(e.buffers().len(), 1);
    assert!(
        e.text().is_empty() && e.file().path.is_none(),
        "closing the last leaves an empty one"
    );
}

#[test]
fn listing_them() {
    let paths = files("ls");
    let mut e = Editor::new();
    open_all(&mut e, &paths[..2]);
    e.keys(&parse("x"));
    e.ex("ls");
    let text = e.message().unwrap().text.clone();
    assert!(text.contains("1  ") && text.contains("one.txt"), "{text}");
    assert!(text.contains("2% ") && text.contains("two.txt +"), "{text}");
}
