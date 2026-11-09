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
