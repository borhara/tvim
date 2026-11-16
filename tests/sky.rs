mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::Tvim;

fn screen(e: Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), width, height).plain()
    })
}

fn rows(s: &str) -> Vec<String> {
    s.lines().map(str::to_string).collect()
}

fn project(name: &str) -> Vec<std::path::PathBuf> {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    let files = [
        (
            "main.rs",
            "impl App {\n    fn run(&self) {\n        let x = 1;\n    }\n}\n",
        ),
        ("a/mod.rs", "one\n"),
        ("b/mod.rs", "two\n"),
    ];
    files
        .iter()
        .map(|(f, text)| {
            let p = dir.join(f);
            std::fs::write(&p, text).unwrap();
            p
        })
        .collect()
}

#[test]
fn the_open_files_are_listed_across_the_top() {
    let paths = project("tabs");
    let mut e = Editor::new();
    for p in &paths {
        e.ex(&format!("e {}", p.display()));
    }
    e.keys(&parse("x"));
    let shown = screen(e, 90, 10);
    let top = &rows(&shown)[0];
    assert!(top.starts_with("    ⋆ main.rs ─── "), "over the line numbers: {top}");
    assert!(
        top.contains("main.rs ─── ⋆ a/mod.rs ─── ✶ b/mod.rs ●"),
        "folders tell apart two of the same name, and the changed one is marked: {top}"
    );
}
