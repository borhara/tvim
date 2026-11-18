mod common;
use common::temp;
use std::path::{Path, PathBuf};
use tvim::keys::parse;
use tvim::session::{self, Place};
use tvim::ui::Tvim;
use tvim::{Editor, Pos};

fn fresh(name: &str) -> PathBuf {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn numbered(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    let text: String = (1..=lines).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&path, text).unwrap();
    tvim::paths::canonical(&path).unwrap()
}

fn screen(e: Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), width, height).plain()
    })
}

fn place(path: &Path, row: usize, col: usize) -> Place {
    Place {
        path: path.to_path_buf(),
        pos: Pos::new(row, col),
    }
}

#[test]
fn open_files_come_back_where_they_were() {
    let dir = fresh("session-back");
    let state = dir.join("state");
    let a = numbered(&dir, "a.txt", 50);
    let b = numbered(&dir, "b.txt", 50);
    let mut e = Editor::new();
    e.ex(&format!("e {}", a.display()));
    e.keys(&parse("30G3l"));
    e.ex(&format!("e {}", b.display()));
    e.keys(&parse("12G"));
    e.ex("bfirst");
    e.save_state(&state).unwrap();

    let saved = std::fs::read_to_string(state.join("session")).unwrap();
    assert_eq!(
        saved,
        format!("current 0\n30 4 {}\n12 1 {}\n", a.display(), b.display())
    );

    let mut e = Editor::new();
    assert!(e.restore_session(&state));
    assert_eq!(e.buffers().len(), 2);
    assert_eq!(e.buffer_index(), 0);
    assert_eq!(e.cursor(), Pos::new(29, 3));
    assert_eq!(e.message().unwrap().text, "reopened 2 files");
    e.ex("bnext");
    assert_eq!(e.cursor(), Pos::new(11, 0));
}

#[test]
fn files_that_are_gone_are_skipped() {
    let dir = fresh("session-gone");
    let state = dir.join("state");
    let a = numbered(&dir, "a.txt", 3);
    let b = numbered(&dir, "b.txt", 3);
    let mut e = Editor::new();
    e.ex(&format!("e {}", a.display()));
    e.ex(&format!("e {}", b.display()));
    e.save_state(&state).unwrap();
    std::fs::remove_file(&b).unwrap();
    let mut e = Editor::new();
    assert!(e.restore_session(&state));
    assert_eq!(e.buffers().len(), 1);
    assert_eq!(e.file().path.as_ref().unwrap().file_name().unwrap(), "a.txt");

    assert!(!Editor::new().restore_session(&dir.join("nothing-here")));
}

#[test]
fn recent_files_are_most_recent_first_and_remember_the_line() {
    let dir = fresh("session-recent");
    let state = dir.join("state");
    let a = numbered(&dir, "a.txt", 20);
    let b = numbered(&dir, "b.txt", 20);
    let c = numbered(&dir, "c.txt", 20);
    let mut e = Editor::new();
    e.ex(&format!("e {}", a.display()));
    e.keys(&parse("5G"));
    e.ex(&format!("e {}", b.display()));
    e.ex("bd");
    e.ex(&format!("e {}", c.display()));
    e.keys(&parse("9G"));
    e.save_state(&state).unwrap();
    let recent = session::read_recent(&state);
    assert_eq!(recent, [place(&c, 8, 0), place(&a, 4, 0), place(&b, 0, 0)]);
    assert_eq!(
        std::fs::read_to_string(state.join("recent"))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
        format!("9 1 {}", c.display())
    );

    let mut e = Editor::open(&a).unwrap();
    e.set_recent(recent.clone());
    e.restore_place();
    assert_eq!(e.cursor(), Pos::new(4, 0), "an argument opens at its last line too");

    let mut e = Editor::new();
    e.set_recent(recent);
    e.ex(&format!("e {}", c.display()));
    assert_eq!(e.cursor().row, 8, "and so does :e");
}

#[test]
fn the_recent_list_is_capped_and_merges_with_whats_on_disk() {
    let dir = fresh("session-cap");
    let state = dir.join("state");
    let files: Vec<PathBuf> = (0..30).map(|i| numbered(&dir, &format!("f{i}.txt"), 1)).collect();
    let mut e = Editor::new();
    e.set_recent(files[..25].iter().map(|f| place(f, 0, 0)).collect());
    assert_eq!(e.recent().len(), session::RECENT_SHOWN);
    e.save_state(&state).unwrap();

    let mut other = Editor::new();
    other.ex(&format!("e {}", files[29].display()));
    other.save_state(&state).unwrap();
    let recent = session::read_recent(&state);
    assert_eq!(recent[0].path, files[29]);
    assert_eq!(recent.len(), 26, "the other tvim's files are kept");
}

#[test]
fn the_welcome_screen_points_to_recent_files() {
    let dir = fresh("session-welcome");
    let a = numbered(&dir, "alpha.rs", 40);
    let mut e = Editor::new();
    e.set_recent(vec![place(&a, 11, 0)]);
    let shown = screen(e, 100, 30);
    let line = shown.lines().find(|l| l.contains("recent files")).unwrap();
    assert!(line.contains("space r"), "{line}");
    assert!(
        !shown.contains("alpha.rs"),
        "the list itself is in the space r window: {shown}"
    );
    assert!(shown.contains("start writing"), "{shown}");
}

#[test]
fn space_r_lists_recent_files() {
    let dir = fresh("session-space-r");
    let a = numbered(&dir, "alpha.rs", 40);
    let b = numbered(&dir, "beta.md", 40);
    let mut e = Editor::new();
    e.set_recent(vec![place(&a, 11, 0), place(&b, 2, 0)]);
    e.keys(&parse("<Space>r"));
    let f = e.finder().expect("open");
    assert_eq!(f.title(), "recent files");
    assert!(f.is_recent());
    let shown = screen(e, 100, 30);
    let alpha = shown
        .lines()
        .find(|l| l.contains("alpha.rs"))
        .unwrap_or_else(|| panic!("{shown}"));
    assert!(alpha.contains("▎ ⊛ alpha.rs") && alpha.contains(":12"), "{alpha}");
    let beta = shown
        .lines()
        .find(|l| l.contains("beta.md"))
        .unwrap_or_else(|| panic!("{shown}"));
    assert!(!beta.contains('▎') && beta.contains(":3"), "{beta}");
    assert!(
        shown.find("alpha.rs").unwrap() < shown.find("beta.md").unwrap(),
        "most recent first"
    );
}

#[test]
fn moving_filtering_and_opening_recent_files() {
    let dir = fresh("session-recent-pick");
    let a = numbered(&dir, "alpha.rs", 40);
    let b = numbered(&dir, "beta.md", 40);
    let c = numbered(&dir, "gamma.txt", 40);
    let recent = vec![place(&a, 11, 0), place(&b, 20, 2), place(&c, 5, 0)];

    let mut e = Editor::new();
    e.set_recent(recent.clone());
    e.keys(&parse("<Space>rjjk"));
    assert_eq!(e.finder().unwrap().selected(), 1, "j and k move while nothing is typed");
    e.keys(&parse("<C-n><Up><Down>"));
    assert_eq!(e.finder().unwrap().selected(), 2);
    e.keys(&parse("<Esc>"));
    assert!(e.finder().is_none());

    let mut e = Editor::new();
    e.set_recent(recent.clone());
    e.keys(&parse("<Space>rgk"));
    let f = e.finder().unwrap();
    assert_eq!(f.query(), "gk", "once typing, j and k are letters");
    e.keys(&parse("<BS><BS>bt"));
    assert_eq!(e.finder().unwrap().match_count(), 1);
    e.keys(&parse("<CR>"));
    assert!(e.finder().is_none());
    assert_eq!(tvim::paths::canonical(e.file().path.as_ref().unwrap()).unwrap(), b);
    assert_eq!(e.cursor(), Pos::new(20, 2), "at the remembered place");

    e.ex("recent");
    let names: Vec<String> = e.finder().unwrap().matches().map(|(p, _)| p.to_string()).collect();
    assert_eq!(names.len(), 2, "the file in front isn't listed: {names:?}");
    assert!(names[0].ends_with("alpha.rs"), "{names:?}");
}

#[test]
fn no_recent_files_yet() {
    let mut e = Editor::new();
    e.keys(&parse("<Space>r"));
    assert!(screen(e, 100, 30).contains("nothing opened yet"));
}

#[test]
fn where_state_and_config_live() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    };
    assert_eq!(
        session::state_dir_in(env(&[("HOME", "/h")])),
        Some(PathBuf::from("/h/.local/state/tvim"))
    );
    assert_eq!(
        session::state_dir_in(env(&[("HOME", "/h"), ("XDG_STATE_HOME", "/x")])),
        Some(PathBuf::from("/x/tvim"))
    );
    assert_eq!(
        session::state_dir_in(env(&[("XDG_STATE_HOME", "/x"), ("TVIM_STATE_DIR", "/t")])),
        Some(PathBuf::from("/t"))
    );
    assert_eq!(
        session::config_path_in(env(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "")])),
        Some(PathBuf::from("/h/.config/tvim/config"))
    );
    assert_eq!(
        session::config_path_in(env(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "/x")])),
        Some(PathBuf::from("/x/tvim/config"))
    );
    assert_eq!(
        session::config_path_in(env(&[("TVIM_CONFIG", "/c/tvimrc")])),
        Some(PathBuf::from("/c/tvimrc"))
    );
    assert_eq!(session::state_dir_in(env(&[])), None);
}

#[test]
fn the_config_file_sets_options() {
    let mut e = Editor::new();
    let config = "\
\" my settings
# also a comment

set tabstop=2 noexpandtab
  :set noroomy
se rnu
";
    assert_eq!(e.apply_config(config), None);
    assert_eq!(e.options.tab_stop, 2);
    assert!(!e.options.expand_tab);
    assert!(e.options.relative_number);
    assert!(!e.options.roomy);
    assert!(e.message().is_none());
}

#[test]
fn config_lines_can_map_keys() {
    let mut e = common::ed("one two");
    let config = "nnoremap <leader>d dw\ninoremap jk <Esc>\nmap! QQ hi\n";
    assert_eq!(e.apply_config(config), None);
    e.keys(&parse(" dAxjk"));
    assert_eq!(e.text(), "twox");
    assert_eq!(e.mode(), tvim::Mode::Normal);
}

#[test]
fn bad_config_lines_say_so() {
    let mut e = Editor::new();
    let problem = e.apply_config("set ts=3\nset bogus\nnormal dd\nset sw=0\n");
    assert_eq!(
        problem.as_deref(),
        Some("config line 2: E518: Unknown option: bogus (and 2 more)")
    );
    assert!(e.message().unwrap().error);
    assert_eq!(e.options.tab_stop, 3, "the good lines still count");
    let mut e = Editor::new();
    assert_eq!(
        e.apply_config("dd").as_deref(),
        Some("config line 1: only set and map lines are allowed: dd")
    );
}
