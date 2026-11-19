use std::path::{Path, PathBuf};

use crate::buffer::Pos;
use crate::editor::{Editor, Mode, listed, relative_to_cwd};
use crate::finder::Finder;

pub const RECENT_SHOWN: usize = 20;
const RECENT_KEPT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub path: PathBuf,
    pub pos: Pos,
}

pub fn state_dir() -> Option<PathBuf> {
    state_dir_in(|k| std::env::var(k).ok())
}

pub fn config_path() -> Option<PathBuf> {
    config_path_in(|k| std::env::var(k).ok())
}

pub fn state_dir_in(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    var("TVIM_STATE_DIR")
        .or_else(|| var("XDG_STATE_HOME").map(|d| d.join("tvim")))
        .or_else(|| var("HOME").map(|h| h.join(".local/state/tvim")))
}

pub fn config_path_in(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    var("TVIM_CONFIG")
        .or_else(|| var("XDG_CONFIG_HOME").map(|d| d.join("tvim/config")))
        .or_else(|| var("HOME").map(|h| h.join(".config/tvim/config")))
}

// one place per line: "LINE COL PATH", 1-based
fn parse_places(text: &str) -> Vec<Place> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let row: usize = parts.next()?.parse().ok()?;
            let col: usize = parts.next()?.parse().ok()?;
            let path = parts.next().filter(|p| !p.is_empty())?;
            Some(Place {
                path: PathBuf::from(path),
                pos: Pos::new(row.saturating_sub(1), col.saturating_sub(1)),
            })
        })
        .collect()
}

fn format_places(places: &[Place]) -> String {
    places
        .iter()
        .map(|p| format!("{} {} {}\n", p.pos.row + 1, p.pos.col + 1, p.path.display()))
        .collect()
}

pub fn read_recent(dir: &Path) -> Vec<Place> {
    let text = std::fs::read_to_string(dir.join("recent")).unwrap_or_default();
    parse_places(&text).into_iter().filter(|p| p.path.is_file()).collect()
}

fn absolute(path: &Path) -> PathBuf {
    crate::paths::canonical(path).unwrap_or_else(|_| path.to_path_buf())
}

fn push_front(list: &mut Vec<Place>, place: Place) {
    list.retain(|p| p.path != place.path);
    list.insert(0, place);
    list.truncate(RECENT_KEPT);
}

impl Editor {
    pub fn set_recent(&mut self, places: Vec<Place>) {
        self.recent = places;
    }

    pub fn recent(&self) -> &[Place] {
        &self.recent[..self.recent.len().min(RECENT_SHOWN)]
    }

    pub(crate) fn remember_place(&mut self) {
        if let Some(path) = self.file.path.as_deref().filter(|p| p.is_file()) {
            let place = Place {
                path: absolute(path),
                pos: self.cursor,
            };
            push_front(&mut self.recent, place);
        }
    }

    pub fn restore_place(&mut self) {
        let Some(path) = self.file.path.as_deref().map(absolute) else {
            return;
        };
        if let Some(p) = self.recent.iter().find(|p| p.path == path) {
            let pos = p.pos;
            self.go_to_place(pos);
        }
    }

    fn go_to_place(&mut self, pos: Pos) {
        let row = pos.row.min(self.buf.last_row());
        let col = pos.col.min(self.buf.line_len(row).saturating_sub(1));
        self.set_cursor(Pos::new(row, col));
        self.top.set(row.saturating_sub(self.text_rows() / 3));
    }

    pub fn restore_session(&mut self, dir: &Path) -> bool {
        let Ok(text) = std::fs::read_to_string(dir.join("session")) else {
            return false;
        };
        let (head, rest) = text.split_once('\n').unwrap_or((&text, ""));
        let current: usize = head.strip_prefix("current ").and_then(|n| n.parse().ok()).unwrap_or(0);
        let mut opened = 0;
        let mut back = None;
        for (i, place) in parse_places(rest).into_iter().enumerate() {
            if !place.path.is_file() || self.open_path(&relative_to_cwd(&place.path)).is_err() {
                continue;
            }
            self.go_to_place(place.pos);
            if i == current {
                back = Some(self.buffer_index());
            }
            opened += 1;
        }
        if let Some(i) = back {
            self.switch_to(i);
        }
        if opened > 1 {
            self.info(format!("reopened {opened} files"));
        }
        opened > 0
    }

    pub fn save_state(&mut self, dir: &Path) -> std::io::Result<()> {
        let open: Vec<Place> = self
            .places()
            .into_iter()
            .map(|(path, pos)| Place {
                path: absolute(&path),
                pos,
            })
            .filter(|p| p.path.is_file())
            .collect();
        let current_path = self.file.path.as_deref().map(absolute);
        let mut recent = self.recent.clone();
        for p in open.iter().rev().filter(|p| Some(&p.path) != current_path.as_ref()) {
            push_front(&mut recent, p.clone());
        }
        if let Some(p) = open.iter().find(|p| Some(&p.path) == current_path.as_ref()) {
            push_front(&mut recent, p.clone());
        }
        // keep what another tvim saved meanwhile
        for p in read_recent(dir) {
            if recent.len() < RECENT_KEPT && !recent.iter().any(|r| r.path == p.path) {
                recent.push(p);
            }
        }
        let current_at = self
            .buffers()
            .iter()
            .filter(|b| b.path.as_deref().is_some_and(Path::is_file))
            .position(|b| b.current)
            .unwrap_or(0);
        std::fs::create_dir_all(dir)?;
        write_whole(&dir.join("recent"), &format_places(&recent))?;
        write_whole(
            &dir.join("session"),
            &format!("current {current_at}\n{}", format_places(&open)),
        )?;
        self.recent = recent;
        Ok(())
    }

    pub fn apply_config(&mut self, text: &str) -> Option<String> {
        let mut problems = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches(':').trim_start();
            if line.is_empty() || line.starts_with(['"', '#']) {
                continue;
            }
            let name: String = line.chars().take_while(char::is_ascii_alphabetic).collect();
            let bang = line[name.len()..].starts_with('!');
            if !["set", "se", "lsp"].contains(&name.as_str()) && crate::ex::map_command(&name, bang).is_none() {
                problems.push(format!(
                    "config line {}: only set, map and lsp lines are allowed: {line}",
                    n + 1
                ));
                continue;
            }
            self.ex(line);
            if let Some(m) = self.message.take().filter(|m| m.error) {
                problems.push(format!("config line {}: {}", n + 1, m.text));
            }
        }
        let first = problems.first()?.clone();
        let more = problems.len() - 1;
        let shown = if more > 0 {
            format!("{first} (and {more} more)")
        } else {
            first
        };
        self.error(shown.clone());
        Some(shown)
    }

    pub fn show_error(&mut self, text: &str) {
        self.error(text);
    }

    pub fn pick_recent(&mut self) {
        self.remember_place();
        let here = self.file.path.as_deref().map(absolute);
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let (names, lines) = self
            .recent
            .iter()
            .filter(|p| Some(&p.path) != here.as_ref() && p.path.is_file())
            .take(RECENT_SHOWN)
            .map(|p| (listed(&p.path), p.pos.row))
            .unzip();
        self.pending.clear();
        self.message = None;
        self.leave_visual_if_any();
        self.mode = Mode::Normal;
        self.finder = Some(Finder::recent(&root, names, lines));
    }
}

// so two tvims quitting at once never leave half a file
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}
