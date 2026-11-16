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

fn chart_file(name: &str) -> std::path::PathBuf {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let body = |head: &str| format!("{head} {{\n{}}}\n", "    x();\n".repeat(16));
    let text = format!(
        "use a;\nuse b;\n\n{}\n{}\nstruct Gamma {{\n{}}}\n",
        body("fn alpha()"),
        body("fn beta()"),
        "    a: u8,\n".repeat(6)
    );
    let path = dir.join("chart.rs");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_last_row_names_the_stretches_of_the_star_line() {
    let mut e = Editor::open(&chart_file("chart")).unwrap();
    e.keys(&parse("31G"));
    let shown = rows(&screen(e, 90, 10));
    // 49 lines on a 49-dot map from column 20, so line n is column 20 + n
    assert_eq!(shown[8].chars().position(|c| c == '✶'), Some(50), "{}", shown[8]);
    let expected = format!(
        "{}╰╯ ╰─── ƒ alpha ────╯ ╰──── ƒ beta ────╯ ╰──────╯{}31:5",
        " ".repeat(20),
        " ".repeat(15)
    );
    assert_eq!(
        shown[9], expected,
        "a bracket under each item's dots; every label centred, \
         too-short brackets bare, and the place at the end where there's room"
    );
}

#[test]
fn the_lit_label_stays_put_while_the_star_moves() {
    let path = chart_file("chart-slide");
    let label_at = |keys: &str| {
        let mut e = Editor::open(&path).unwrap();
        e.keys(&parse(keys));
        let shown = rows(&screen(e, 90, 10));
        let chars: Vec<char> = shown[9].chars().collect();
        chars
            .windows(4)
            .position(|w| w.iter().collect::<String>() == "beta")
            .unwrap()
    };
    assert_eq!(label_at("24G"), 50, "centred in its bracket");
    assert_eq!(label_at("31G"), 50);
    assert_eq!(label_at("40G"), 50);
}

#[test]
fn every_kind_of_file_has_a_chart() {
    let dir = temp("chart-kinds");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: String| {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        let mut e = Editor::open(&p).unwrap();
        e.clear_message();
        let shown = rows(&screen(e, 90, 10));
        shown[9].clone()
    };
    let filler = |n: usize| "text\n".repeat(n);
    let md = write(
        "notes.md",
        format!("# Notes\n\n## Intro\n{}## Usage\n{}", filler(20), filler(20)),
    );
    assert!(
        md.contains(" Intro ") && md.contains(" Usage ") && !md.contains("Notes"),
        "{md}"
    );
    let toml = write("conf.toml", format!("[package]\n{}[deps]\n{}", filler(20), filler(20)));
    assert!(toml.contains(" [package] ") && toml.contains(" [deps] "), "{toml}");
    let plain = write("story.txt", filler(30));
    assert!(
        plain.contains(" story.txt ") && plain.matches('╰').count() == 1,
        "one bracket over the lot, named for the file: {plain}"
    );
}

#[test]
fn messages_and_typing_take_the_row_back() {
    let mut e = Editor::open(&chart_file("chart-busy")).unwrap();
    e.keys(&parse("31G:nope<CR>"));
    let last = rows(&screen(e, 90, 10)).pop().unwrap();
    assert!(
        last.starts_with("  ✕ E492") && last.ends_with("31:5") && !last.contains('╰'),
        "{last}"
    );
    let mut e = Editor::open(&chart_file("chart-pending")).unwrap();
    e.keys(&parse("31G\"a"));
    let last = rows(&screen(e, 70, 10)).pop().unwrap();
    assert!(last.contains("ƒ beta") && last.ends_with("\"a    31:5"), "{last}");
    assert!(!last.contains("╰──────╯"), "labels in the way give way: {last}");
}
