mod common;
use common::temp;
use tvim::Editor;

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
