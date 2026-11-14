use std::path::PathBuf;
use std::process::ExitCode;

use tinsel::app;
use tvim::editor::Editor;
use tvim::session;
use tvim::ui::Tvim;

const USAGE: &str = "\
tvim: a vim-like editor for the terminal

usage: tvim [options] [file or folder]

  file          the file to edit; a folder opens a file browser;
                none reopens the files open when tvim last quit
  +N            start on line N
  +             start on the last line
  +/text        start on the first line holding text
  -h, --help    show this and exit
  -V, --version show the version and exit

settings are read from ~/.config/tvim/config, one :set line each";

fn main() -> ExitCode {
    let mut file: Option<PathBuf> = None;
    let mut start: Option<String> = None;
    let mut options = true;
    for arg in std::env::args().skip(1) {
        let flag = options && (arg.starts_with('+') || arg.starts_with('-') && arg.len() > 1);
        match arg.as_str() {
            _ if file.is_some() && !flag => {
                tinsel::error("tvim opens one file or folder at a time");
                return ExitCode::from(2);
            }
            _ if !options => file = Some(PathBuf::from(arg)),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("tvim {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--" => options = false,
            a if a.starts_with('+') => start = Some(a[1..].to_string()),
            a if a.starts_with('-') && a.len() > 1 => {
                tinsel::error(&format!("unknown option {a}"));
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    let state = session::state_dir();
    let recent = state.as_deref().map(session::read_recent).unwrap_or_default();
    let mut editor = match &file {
        Some(path) if path.is_dir() => {
            let mut e = Editor::new();
            e.set_recent(recent);
            e.explore(path);
            if e.explorer().is_none() {
                let why = e.message().map_or(String::new(), |m| m.text.clone());
                tinsel::error(&format!("can't open {}: {why}", path.display()));
                return ExitCode::FAILURE;
            }
            e
        }
        Some(path) => match Editor::open(path) {
            Ok(mut e) => {
                e.set_recent(recent);
                e.restore_place();
                e
            }
            Err(e) => {
                tinsel::error(&format!("can't open {}: {e}", path.display()));
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut e = Editor::new();
            e.set_recent(recent);
            if let Some(dir) = &state {
                e.restore_session(dir);
            }
            e
        }
    };
    let config = session::config_path().and_then(|p| std::fs::read_to_string(p).ok());
    if let Some(start) = start {
        go_to(&mut editor, &start);
    }
    if let Some(problem) = problem {
    }

    match app::run(Tvim::new(editor)) {
        Ok(mut tvim) => {
            if let Some(dir) = &state {
                if let Err(e) = tvim.editor.save_state(dir) {
                    tinsel::error(&format!("couldn't save the session in {}: {e}", dir.display()));
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            tinsel::error(&format!("tvim stopped: {e}"));
            ExitCode::FAILURE
        }
    }
}

fn go_to(editor: &mut Editor, start: &str) {
    let last = editor.buffer().last_row();
    if let Some(text) = start.strip_prefix('/') {
        let mut keys = tvim::keys::parse("G$/");
        keys.extend(text.chars().map(tinsel::app::Key::Char));
        keys.push(tinsel::app::Key::Enter);
        editor.keys(&keys);
        return;
    }
    let row = if start.is_empty() {
        last
    } else {
        start.parse::<usize>().map_or(0, |n| n.saturating_sub(1)).min(last)
    };
    editor.keys(&tvim::keys::parse(&format!("{}G", row + 1)));
}
