//! Which language server looks after a file, and where the file's project starts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::syntax::extension;
use crate::text::split_args;

struct Known {
    // what :lsp calls it
    name: &'static str,
    // the language each extension is sent as
    languages: &'static [(&'static str, &'static [&'static str])],
    command: &'static str,
    // files whose folder is the project's top
    roots: &'static [&'static str],
}

const KNOWN: &[Known] = &[
    Known {
        name: "rust",
        languages: &[("rust", &["rs"])],
        command: "rust-analyzer",
        roots: &["Cargo.toml"],
    },
    Known {
        name: "python",
        languages: &[("python", &["py", "pyi"])],
        command: "pyright-langserver --stdio",
        roots: &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt"],
    },
    Known {
        name: "go",
        languages: &[("go", &["go"])],
        command: "gopls",
        roots: &["go.work", "go.mod"],
    },
    Known {
        name: "typescript",
        languages: &[
            ("typescript", &["ts", "mts", "cts"]),
            ("typescriptreact", &["tsx"]),
            ("javascript", &["js", "mjs", "cjs"]),
            ("javascriptreact", &["jsx"]),
        ],
        command: "typescript-language-server --stdio",
        roots: &["tsconfig.json", "jsconfig.json", "package.json"],
    },
    Known {
        name: "c",
        languages: &[("c", &["c", "h"]), ("cpp", &["cc", "cpp", "cxx", "hh", "hpp", "hxx"])],
        command: "clangd",
        roots: &["compile_commands.json", "compile_flags.txt", ".clangd"],
    },
    Known {
        name: "lua",
        languages: &[("lua", &["lua"])],
        command: "lua-language-server",
        roots: &[".luarc.json", ".luarc.jsonc"],
    },
    Known {
        name: "zig",
        languages: &[("zig", &["zig"])],
        command: "zls",
        roots: &["build.zig"],
    },
    Known {
        name: "shell",
        languages: &[("shellscript", &["sh", "bash"])],
        command: "bash-language-server start",
        roots: &[],
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub name: &'static str,
    pub language: &'static str,
    pub command: Vec<String>,
    pub root: PathBuf,
}

#[derive(Debug, Default)]
pub struct Servers {
    // commands given with :lsp, None for a server turned off
    chosen: HashMap<&'static str, Option<Vec<String>>>,
    // counts the changes made, so a choice worked out before can tell it's out of date
    changes: u64,
}

impl Servers {
    pub fn for_file(&self, path: &Path) -> Option<Choice> {
        let ext = extension(path.file_name()?.to_str()?);
        let (known, language) = KNOWN.iter().find_map(|k| {
            let (language, _) = k.languages.iter().find(|(_, exts)| exts.contains(&ext.as_str()))?;
            Some((k, *language))
        })?;
        let command = match self.chosen.get(known.name) {
            Some(chosen) => chosen.clone()?,
            None => split_args(known.command).ok()?,
        };
        Some(Choice {
            name: known.name,
            language,
            command,
            root: project_root(path, known.roots),
        })
    }

    // `off` turns a server off
    pub fn set(&mut self, name: &str, command: &str) -> Result<(), String> {
        let known = find(name)?;
        let command = match command {
            "off" => None,
            line => Some(split_args(line)?),
        };
        self.chosen.insert(known.name, command);
        self.changes += 1;
        Ok(())
    }

    pub fn changes(&self) -> u64 {
        self.changes
    }

    pub fn describe(&self, name: &str) -> Result<String, String> {
        let known = find(name)?;
        let command = match self.chosen.get(known.name) {
            Some(Some(words)) => words.join(" "),
            Some(None) => "off".to_string(),
            None => known.command.to_string(),
        };
        Ok(format!("{}: {command}", known.name))
    }
}

fn find(name: &str) -> Result<&'static Known, String> {
    KNOWN.iter().find(|k| k.name == name).ok_or_else(|| {
        let names: Vec<&str> = KNOWN.iter().map(|k| k.name).collect();
        format!("no language server called {name}, only {}", names.join(", "))
    })
}

// the nearest folder up from the file holding one of the markers, or failing that the
// repository it's in, or failing that its own folder
fn project_root(path: &Path, markers: &[&str]) -> PathBuf {
    let path = crate::paths::absolute(path);
    let dir = path.parent().unwrap_or(&path);
    let nearest = |names: &[&str]| {
        dir.ancestors()
            .find(|d| names.iter().any(|name| d.join(name).exists()))
            .map(Path::to_path_buf)
    };
    nearest(markers)
        .or_else(|| nearest(&[".git"]))
        .unwrap_or_else(|| dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tvim-servers-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for f in files {
            let path = dir.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        crate::paths::canonical(&dir).unwrap()
    }

    #[test]
    fn the_extension_picks_the_server_and_the_language() {
        let servers = Servers::default();
        let rust = servers.for_file(Path::new("/x/main.RS")).unwrap();
        assert_eq!((rust.name, rust.language), ("rust", "rust"));
        assert_eq!(rust.command, ["rust-analyzer"]);
        let tsx = servers.for_file(Path::new("/x/App.tsx")).unwrap();
        assert_eq!((tsx.name, tsx.language), ("typescript", "typescriptreact"));
        assert_eq!(tsx.command, ["typescript-language-server", "--stdio"]);
        assert_eq!(servers.for_file(Path::new("/x/notes.txt")), None);
        assert_eq!(servers.for_file(Path::new("/x/Makefile")), None);
    }

    #[test]
    fn a_server_can_be_swapped_or_turned_off() {
        let mut servers = Servers::default();
        servers.set("rust", "my-server --log 'my log'").unwrap();
        let c = servers.for_file(Path::new("/x/a.rs")).unwrap();
        assert_eq!(c.command, ["my-server", "--log", "my log"]);
        assert_eq!(servers.describe("rust").unwrap(), "rust: my-server --log my log");
        servers.set("rust", "off").unwrap();
        assert_eq!(servers.for_file(Path::new("/x/a.rs")), None);
        assert_eq!(servers.describe("rust").unwrap(), "rust: off");
        assert_eq!(
            servers.describe("go").unwrap(),
            "go: gopls",
            "the others are left alone"
        );
    }

    #[test]
    fn mistakes_are_explained() {
        let mut servers = Servers::default();
        let err = servers.set("cobol", "x").unwrap_err();
        assert!(
            err.starts_with("no language server called cobol, only rust, python"),
            "{err}"
        );
        assert!(servers.set("rust", "ra 'oops").is_err());
        assert!(servers.describe("cobol").is_err());
    }

    #[test]
    fn the_project_is_the_nearest_folder_with_a_marker() {
        let dir = tree(
            "marker",
            &[
                "ws/Cargo.toml",
                "ws/.git/HEAD",
                "ws/crate/Cargo.toml",
                "ws/crate/src/deep/a.rs",
            ],
        );
        let root = |p: &str| Servers::default().for_file(&dir.join(p)).unwrap().root;
        assert_eq!(root("ws/crate/src/deep/a.rs"), dir.join("ws/crate"));
        assert_eq!(
            root("ws/crate/src/new.rs"),
            dir.join("ws/crate"),
            "a file not saved yet"
        );
    }

    #[test]
    fn without_a_marker_the_repository_or_the_folder_will_do() {
        let dir = tree("fallback", &["repo/.git/HEAD", "repo/scripts/a.py", "loose/b.py"]);
        let root = |p: &str| Servers::default().for_file(&dir.join(p)).unwrap().root;
        assert_eq!(root("repo/scripts/a.py"), dir.join("repo"));
        assert_eq!(root("loose/b.py"), dir.join("loose"));
    }
}
