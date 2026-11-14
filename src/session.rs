use std::path::PathBuf;

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
