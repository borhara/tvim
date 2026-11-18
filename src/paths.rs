//! Paths as tvim compares and shows them.

use std::io;
use std::path::{Path, PathBuf};

/// The path with links and `..` resolved, as `canonicalize` gives it, but on Windows without
/// the `\\?\` it puts in front whenever the plain path means the same thing: that prefix isn't
/// for people, and a path with it never equals one without.
pub(crate) fn canonical(path: &Path) -> io::Result<PathBuf> {
    let path = path.canonicalize()?;
    if cfg!(windows) {
        if let Some(plain) = path.to_str().and_then(plain) {
            return Ok(PathBuf::from(plain));
        }
    }
    Ok(path)
}

/// `\\?\C:\a` as `C:\a` and `\\?\UNC\server\share` as `\\server\share`, or `None` when the
/// path has no such prefix or needs it: too long for the plain form, or with a part Windows
/// would read differently without it.
pub(crate) fn plain(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    let plain = match rest.strip_prefix(r"UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => {
            let drive = rest.as_bytes();
            if drive.len() < 3 || !drive[0].is_ascii_alphabetic() || &drive[1..3] != br":\" {
                return None;
            }
            rest.to_string()
        }
    };
    let reserved = |part: &str| {
        let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
        matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
    };
    let fine = plain.len() < 260
        && plain
            .split('\\')
            .skip(1)
            .all(|part| !part.ends_with(['.', ' ']) && !reserved(part));
    fine.then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn the_prefix_goes_when_the_plain_path_means_the_same() {
        assert_eq!(plain(r"\\?\C:\Users\me\a.rs").as_deref(), Some(r"C:\Users\me\a.rs"));
        assert_eq!(
            plain(r"\\?\UNC\server\share\a.rs").as_deref(),
            Some(r"\\server\share\a.rs")
        );
        assert_eq!(plain(r"C:\Users\me\a.rs"), None, "nothing to take off");
        assert_eq!(plain(r"\\?\C:\files\con.txt"), None, "a device name needs the prefix");
        assert_eq!(plain(r"\\?\C:\files\trailing."), None, "so does a trailing dot");
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(plain(&long), None, "too long without it");
        assert_eq!(plain(r"\\?\Volume{1234}\a"), None, "not a drive");
    }
}
