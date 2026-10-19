#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    Rust,
    Python,
    JavaScript,
    Go,
    C,
    Java,
    Shell,
    Lua,
    Toml,
    Yaml,
    Json,
    Markdown,
}

pub(crate) fn extension(name: &str) -> String {
    name.rsplit_once('.').map_or(String::new(), |(_, e)| e.to_ascii_lowercase())
}

impl Syntax {
    pub fn detect(name: &str) -> Option<Syntax> {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "makefile" | ".bashrc" | ".zshrc" | ".profile" | ".bash_profile" | ".zprofile" => {
                return Some(Syntax::Shell);
            }
            "cargo.lock" | "pipfile" => return Some(Syntax::Toml),
            _ => {}
        }
        Some(match extension(name).as_str() {
            "rs" => Syntax::Rust,
            "py" | "pyi" => Syntax::Python,
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" => Syntax::JavaScript,
            "go" => Syntax::Go,
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "m" | "cs" | "swift" | "kt" | "zig" => Syntax::C,
            "java" | "scala" | "groovy" | "dart" => Syntax::Java,
            "sh" | "bash" | "zsh" | "fish" => Syntax::Shell,
            "lua" => Syntax::Lua,
            "toml" | "ini" | "cfg" | "conf" => Syntax::Toml,
            "yaml" | "yml" => Syntax::Yaml,
            "json" | "jsonc" | "json5" => Syntax::Json,
            "md" | "markdown" | "mdx" => Syntax::Markdown,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Syntax::Python => "python",
            Syntax::JavaScript => "javascript",
            Syntax::Go => "go",
            Syntax::C => "c",
            Syntax::Java => "java",
            Syntax::Lua => "lua",
            Syntax::Toml => "toml",
            Syntax::Yaml => "yaml",
            Syntax::Json => "json",
            Syntax::Markdown => "markdown",
        }
    }

}
