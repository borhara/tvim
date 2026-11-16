mod common;
use common::temp;
use tinsel::app::Frame;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::{Tvim, draw};

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

#[test]
fn the_star_line_and_the_last_row() {
    let mut e = common::ed("first\n\tsecond");
    e.keys(&parse("j"));
    let shown = rows(&screen(e, 50, 8));
    assert_eq!(
        shown[6], "  ·······················✶······················",
        "the star line runs the full width, with the file's two dots in the middle and the cursor's line as the star"
    );
    assert_eq!(
        shown[7],
        format!("{}╰ untitled ╯{}2:1", " ".repeat(24), " ".repeat(9)),
        "the file's bracket, widened to name it, and the place"
    );
    let mut e = common::ed("a b a b a");
    e.keys(&parse("/a"));
    let last = rows(&screen(e, 70, 8)).pop().unwrap();
    assert!(last.contains("/ a") && last.contains("2 of 3"), "{last}");
    let mut e = common::ed("abc");
    e.ex("nope");
    assert!(screen(e, 90, 8).contains("✕ E492: Not an editor command: nope"));
}

#[test]
fn a_short_file_sits_in_the_middle_of_the_sky() {
    let mut e = common::ed("a\nb\nc\nd\ne");
    e.keys(&parse("G"));
    let line = rows(&screen(e, 40, 8))[6].clone();
    let colour = tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
        let e = common::ed("a\nb\nc\nd\ne");
        let mut frame = Frame::new(40, 8);
        draw(&e, &mut frame);
        frame.snapshot().lines()[6].clone()
    });
    let cells: Vec<char> = line.chars().collect();
    assert_eq!(cells.len(), 38, "two cells in from each edge: {line}");
    assert_eq!(cells.iter().position(|c| *c == '✶'), Some(2 + 15 + 4), "{line}");
    let inks: Vec<&str> = colour.split("\u{1b}[").filter(|s| s.contains('·')).collect();
    assert_eq!(inks.len(), 3, "the sky, the file's dots, the sky again: {colour:?}");
    assert_eq!(inks[0].split('m').next(), inks[2].split('m').next());
    assert_ne!(
        inks[0].split('m').next(),
        inks[1].split('m').next(),
        "the file is brighter"
    );
}

#[test]
fn stars_drift_only_past_the_end() {
    let text: String = (1..=4).map(|_| format!("{}\n", "x".repeat(70))).collect();
    let shown = rows(&screen(common::ed(&text), 80, 24));
    for row in &shown[2..6] {
        assert!(!row.contains('·') && !row.contains('⋆'), "never over the text: {row}");
    }
    let sky: usize = shown[7..21]
        .iter()
        .map(|r| r.chars().filter(|c| "⋆✧✦".contains(*c)).count())
        .sum();
    assert!((1..40).contains(&sky), "a few, not a field: {sky}");
    assert_eq!(
        rows(&screen(common::ed(&text), 80, 24)),
        shown,
        "the same moment draws the same sky"
    );
}

#[test]
fn everything_sits_on_the_darkest_colour() {
    tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
        let mut e = common::ed(" abc\n def");
        e.keys(&parse("j"));
        // the cursor's line has its own faint band
        e.options.cursor_line = false;
        let mut frame = Frame::new(60, 12);
        draw(&e, &mut frame);
        for line in frame.snapshot().lines() {
            for part in line.split("48;2;").skip(1) {
                assert!(part.starts_with("23;18;27m"), "no bands or tints: {line:?}");
            }
        }
    });
}

#[test]
fn there_is_one_look() {
    let mut e = common::ed("abc");
    e.ex("set look=frames");
    assert!(e.message().unwrap().error, "{:?}", e.message());
    e.ex("set look?");
    assert!(e.message().unwrap().text.starts_with("E518"), "{:?}", e.message());
}

#[test]
fn the_cursor_takes_the_mode_colour_and_the_app_its_own_background() {
    tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
        let mut e = common::ed("abc");
        let mut frame = Frame::new(40, 8);
        draw(&e, &mut frame);
        assert!(frame.background().is_some());
        e.keys(&parse("i"));
        let mut typing = Frame::new(40, 8);
        draw(&e, &mut typing);
        assert!(typing.cursor().is_some());
    });
}

#[test]
fn tiny_screens_dont_break() {
    for (w, h) in [(1, 1), (2, 2), (5, 3), (80, 1), (0, 0), (12, 4), (3, 5), (30, 6)] {
        for keys in ["jl", ":abc", "/x", "d", "Vj", "<Tab>", "<Tab>/x", ":set re"] {
            let mut e = common::ed("hello\nworld");
            e.keys(&parse(keys));
            let _ = screen(e, w, h);
        }
        let _ = screen(Editor::new(), w, h);
    }
}

#[test]
fn the_lit_label_is_in_the_modes_colour() {
    let last_row = |keys: &str| {
        let mut e = common::ed(&"abc\n".repeat(20));
        e.keys(&parse(keys));
        tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
            let mut frame = Frame::new(60, 8);
            draw(&e, &mut frame);
            frame.snapshot().lines().last().unwrap().clone()
        })
    };
    let normal = last_row("");
    let insert = last_row("i");
    let ink = |row: &str, word: &str| {
        let at = row.find(word).unwrap();
        let start = row[..at].rfind('\x1b').unwrap();
        row[start..at].split("48;").next().unwrap().to_string()
    };
    assert!(ink(&normal, "untitled").contains("38;"), "{normal:?}");
    assert_ne!(
        ink(&normal, "untitled"),
        ink(&insert, "untitled"),
        "each mode its own colour"
    );
    assert_ne!(ink(&normal, "╰"), ink(&insert, "╰"), "the bracket too");
}

#[test]
fn typing_on_the_status_row_puts_the_cursor_after_it() {
    let mut e = common::ed("abc");
    e.keys(&parse(":ab"));
    let (shown, cursor) = tinsel::with_color_mode(tinsel::ColorMode::None, || {
        let mut frame = Frame::new(60, 5);
        draw(&e, &mut frame);
        (frame.snapshot().plain(), frame.cursor())
    });
    let last = rows(&shown).pop().unwrap();
    assert!(last.starts_with("  : ab"), "{last}");
    let (x, y, _) = cursor.unwrap();
    assert_eq!((x, y), (last.chars().position(|c| c == 'b').unwrap() + 1, 4));
}

#[test]
fn only_windows_line_endings_are_mentioned() {
    let dir = temp("endings");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("dos.txt"), "a\r\nb\r\n").unwrap();
    let mut e = Editor::new();
    e.ex(&format!("e {}", dir.join("dos.txt").display()));
    let top = rows(&screen(e, 80, 10))[0].clone();
    assert!(top.ends_with("2 lines · crlf"), "{top}");
}

#[test]
fn the_open_files_are_a_constellation() {
    let dir = temp("tab-words");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut e = Editor::new();
    for f in ["notes.md", "Cargo.toml", "main.rs"] {
        std::fs::write(dir.join(f), "x\n").unwrap();
        e.ex(&format!("e {}", dir.join(f).display()));
    }
    let top = rows(&screen(e, 100, 10))[0].clone();
    assert!(top.starts_with("    ⋆ notes.md ─── ⋆ Cargo.toml ─── ✶ main.rs  "), "{top}");
    assert!(top.ends_with("rust · 1 line"), "the current file's facts in the corner: {top}");
    assert_eq!(rows(&screen(common::ed("x"), 60, 10))[0].trim_start().split("  ").next(), Some("✶ untitled"));
    assert_eq!(
        rows(&screen(Editor::new(), 60, 10))[0],
        "  tvim",
        "the name when nothing's open"
    );
}
