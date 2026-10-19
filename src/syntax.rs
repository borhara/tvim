#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Function,
    Type,
    Str,
    Escape,
    Number,
    Constant,
    Comment,
    Todo,
    Macro,
    Attribute,
    Punct,
    Operator,
    Label,
    Heading,
    Strong,
    Emphasis,
    Code,
    Link,
    Url,
    ListMark,
    Quote,
}

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

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Syntax::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn",
                "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
                "static", "struct", "super", "trait", "type", "unsafe", "use", "where", "while", "yield",
            ],
            Syntax::Python => &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else",
                "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not",
                "or", "pass", "raise", "return", "try", "while", "with", "yield", "match", "case",
            ],
            Syntax::JavaScript => &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "finally",
                "for",
                "from",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "interface",
                "let",
                "new",
                "of",
                "return",
                "static",
                "switch",
                "throw",
                "try",
                "type",
                "typeof",
                "var",
                "void",
                "while",
                "yield",
                "enum",
                "implements",
                "private",
                "public",
                "protected",
                "readonly",
                "as",
            ],
            Syntax::Go => &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
            ],
            Syntax::Java => &[
                "abstract",
                "break",
                "case",
                "catch",
                "class",
                "continue",
                "default",
                "do",
                "else",
                "enum",
                "extends",
                "final",
                "finally",
                "for",
                "if",
                "implements",
                "import",
                "instanceof",
                "interface",
                "new",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "static",
                "super",
                "switch",
                "synchronized",
                "throw",
                "throws",
                "try",
                "var",
                "void",
                "while",
                "val",
                "def",
                "object",
            ],
            Syntax::Shell => &[
                "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "until", "case", "esac",
                "function", "return", "export", "local", "readonly", "set", "unset", "source", "exit",
            ],
            Syntax::Lua => &[
                "and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local", "not",
                "or", "repeat", "return", "then", "until", "while",
            ],
            _ => &[],
        }
    }

    fn constants(self) -> &'static [&'static str] {
        match self {
            Syntax::Rust => &["true", "false", "self", "Self", "Some", "None", "Ok", "Err"],
            Syntax::Python => &["True", "False", "None", "self", "cls"],
            Syntax::JavaScript => &["true", "false", "null", "undefined", "this", "NaN", "Infinity"],
            Syntax::Go => &["true", "false", "nil", "iota"],
            Syntax::C | Syntax::Java => &["true", "false", "NULL", "nullptr", "null", "this", "self", "nil"],
            Syntax::Lua => &["true", "false", "nil", "self"],
            Syntax::Toml | Syntax::Yaml | Syntax::Json => &["true", "false", "null", "yes", "no", "on", "off"],
            _ => &[],
        }
    }

    fn quotes(self) -> &'static [char] {
        match self {
            Syntax::Rust | Syntax::C | Syntax::Java | Syntax::Json | Syntax::Go => &['"', '`'],
            Syntax::JavaScript => &['"', '\'', '`'],
            Syntax::Python | Syntax::Shell | Syntax::Lua | Syntax::Toml | Syntax::Yaml => &['"', '\''],
            Syntax::Markdown => &[],
        }
    }

    fn multiline_quote(self, q: char) -> bool {
        matches!(
            (self, q),
            (Syntax::JavaScript | Syntax::Go, '`') | (Syntax::Rust, '"') | (Syntax::Shell, _)
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Normal,
    Comment(u8),
    Str(char),
    Triple(char),
    Fence,
}

pub fn highlight(line: &str, syntax: Syntax, state: State) -> (Vec<Kind>, State) {
    let chars: Vec<char> = line.chars().collect();
    match syntax {
        Syntax::Markdown => markdown(&chars, state),
    }
}
