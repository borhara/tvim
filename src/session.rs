use std::path::PathBuf;

use crate::buffer::Pos;

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
