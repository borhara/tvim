use crate::text::is_word;

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
    Key,
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
            Syntax::Rust => "rust",
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

    pub(crate) fn line_comment(self) -> &'static [&'static str] {
        match self {
            Syntax::Rust | Syntax::JavaScript | Syntax::Go | Syntax::C | Syntax::Java | Syntax::Json => &["//"],
            Syntax::Python | Syntax::Shell | Syntax::Toml | Syntax::Yaml => &["#"],
            Syntax::Lua => &["--"],
            Syntax::Markdown => &[],
        }
    }

    pub(crate) fn block_comment(self) -> Option<(&'static str, &'static str)> {
        match self {
            Syntax::Rust | Syntax::JavaScript | Syntax::Go | Syntax::C | Syntax::Java | Syntax::Json => {
                Some(("/*", "*/"))
            }
            Syntax::Lua => Some(("--[[", "]]")),
            _ => None,
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
            Syntax::C => &[
                "auto",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "constexpr",
                "continue",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "extern",
                "for",
                "friend",
                "goto",
                "if",
                "inline",
                "namespace",
                "new",
                "operator",
                "private",
                "protected",
                "public",
                "return",
                "sizeof",
                "static",
                "struct",
                "switch",
                "template",
                "throw",
                "try",
                "typedef",
                "typename",
                "union",
                "using",
                "virtual",
                "volatile",
                "while",
                "#include",
                "#define",
                "#if",
                "#ifdef",
                "#ifndef",
                "#endif",
                "#else",
                "#pragma",
                "func",
                "let",
                "var",
                "fn",
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
        _ => code(&chars, syntax, state),
    }
}

const TODO: [&str; 6] = ["TODO", "FIXME", "XXX", "HACK", "NOTE", "SAFETY"];

fn starts(chars: &[char], i: usize, s: &str) -> bool {
    let mut j = i;
    for c in s.chars() {
        if chars.get(j) != Some(&c) {
            return false;
        }
        j += 1;
    }
    true
}

fn fill(kinds: &mut [Kind], from: usize, to: usize, kind: Kind) {
    let to = to.min(kinds.len());
    for k in &mut kinds[from.min(to)..to] {
        *k = kind;
    }
}

fn comment(chars: &[char], kinds: &mut [Kind], from: usize, to: usize) {
    fill(kinds, from, to, Kind::Comment);
    let mut i = from;
    while i < to {
        if let Some(word) = TODO.iter().find(|w| starts(chars, i, w)) {
            let end = i + word.len();
            let bounded =
                (i == 0 || !chars[i - 1].is_alphanumeric()) && chars.get(end).is_none_or(|c| !c.is_alphanumeric());
            if bounded {
                fill(kinds, i, end, Kind::Todo);
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn closing(chars: &[char], from: usize, quote: char) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            c if c == quote => return Some(j),
            _ => j += 1,
        }
    }
    None
}

fn code(chars: &[char], syntax: Syntax, mut state: State) -> (Vec<Kind>, State) {
    let n = chars.len();
    let mut kinds = vec![Kind::Plain; n];
    let mut i = 0;
    let first_word = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    let mut prev_word = String::new();
    let mut word = String::new();
    let config = matches!(syntax, Syntax::Toml | Syntax::Yaml | Syntax::Json);

    if config && state == State::Normal {
        if syntax == Syntax::Toml && chars.get(first_word) == Some(&'[') {
            let end = chars.iter().rposition(|&c| c == ']').map_or(n, |e| e + 1);
            fill(&mut kinds, first_word, end, Kind::Type);
            fill(&mut kinds, first_word, first_word + 1, Kind::Punct);
            if end > 0 {
                fill(&mut kinds, end - 1, end, Kind::Punct);
            }
            i = end;
        } else if syntax != Syntax::Json {
            let mut start = first_word;
            if syntax == Syntax::Yaml && starts(chars, start, "- ") {
                fill(&mut kinds, start, start + 1, Kind::ListMark);
                start += 2;
            }
            let sep = if syntax == Syntax::Toml { '=' } else { ':' };
            let quoted = chars.get(start).is_some_and(|c| *c == '"' || *c == '\'');
            if let Some(at) = (start..n)
                .find(|&j| chars[j] == sep && (sep == '=' || chars.get(j + 1).is_none_or(|c| c.is_whitespace())))
            {
                let key_ok = !quoted && chars[start..at].iter().all(|c| is_word(*c) || "-. ".contains(*c));
                if key_ok && at > start && !(syntax == Syntax::Yaml && chars[start] == '#') {
                    fill(&mut kinds, start, at, Kind::Key);
                    fill(&mut kinds, at, at + 1, Kind::Punct);
                    i = at + 1;
                }
            }
        }
    }

    while i < n {
        let c = chars[i];
        match state {
            State::Comment(depth) => {
                let (open, close) = syntax.block_comment().unwrap_or(("/*", "*/"));
                let start = i;
                let mut depth = depth;
                while i < n {
                    if syntax == Syntax::Rust && starts(chars, i, open) {
                        depth += 1;
                        i += open.chars().count();
                    } else if starts(chars, i, close) {
                        i += close.chars().count();
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                comment(chars, &mut kinds, start, i);
                state = if depth == 0 {
                    State::Normal
                } else {
                    State::Comment(depth)
                };
                continue;
            }
            State::Str(q) => {
                let start = i;
                while i < n {
                    if chars[i] == '\\' && (syntax != Syntax::Toml || q == '"') {
                        fill(&mut kinds, i, i + 2, Kind::Escape);
                        i += 2;
                        continue;
                    }
                    if chars[i] == q {
                        i += 1;
                        state = State::Normal;
                        break;
                    }
                    if syntax == Syntax::JavaScript && q == '`' && starts(chars, i, "${") {
                        let end = (i..n).find(|&j| chars[j] == '}').map_or(n, |e| e + 1);
                        fill(&mut kinds, i, end, Kind::Operator);
                        i = end;
                        continue;
                    }
                    if kinds[i] == Kind::Plain {
                        kinds[i] = Kind::Str;
                    }
                    i += 1;
                }
                fill_str(&mut kinds, start, i);
                if state == State::Str(q) && !syntax.multiline_quote(q) {
                    state = State::Normal;
                }
                continue;
            }
            State::Triple(q) => {
                let start = i;
                while i < n && !chars[i..].starts_with(&[q; 3]) {
                    i += 1;
                }
                if i < n {
                    i += 3;
                    state = State::Normal;
                }
                fill(&mut kinds, start, i, Kind::Str);
                continue;
            }
            State::Fence => unreachable!("only Markdown has fences"),
            State::Normal => {}
        }

        if let Some(open) = syntax.block_comment().map(|b| b.0) {
            if starts(chars, i, open) {
                state = State::Comment(1);
                comment(chars, &mut kinds, i, i + open.chars().count());
                i += open.chars().count();
                continue;
            }
        }
        if syntax.line_comment().iter().any(|lc| starts(chars, i, lc)) {
        }
        if syntax == Syntax::Python && (starts(chars, i, "\"\"\"") || starts(chars, i, "'''")) {
            state = State::Triple(c);
            fill(&mut kinds, i, i + 3, Kind::Str);
            i += 3;
            continue;
        }
        // 'a is a lifetime, 'a' is a char
        if syntax == Syntax::Rust && c == '\'' {
            let is_char = chars.get(i + 2) == Some(&'\'') || chars.get(i + 1) == Some(&'\\');
            if !is_char {
                let end = (i + 1..n).find(|&j| !is_word(chars[j])).unwrap_or(n);
                fill(&mut kinds, i, end, Kind::Label);
                i = end.max(i + 1);
                continue;
            }
            let end = closing(chars, i + 1, '\'').map_or(n, |e| e + 1);
            fill(&mut kinds, i, end, Kind::Str);
            if chars.get(i + 1) == Some(&'\\') {
                fill(&mut kinds, i + 1, (i + 3).min(end), Kind::Escape);
            }
            i = end;
            continue;
        }
        if syntax.quotes().contains(&c) || (syntax == Syntax::Rust && c == '\'') {
            kinds[i] = Kind::Str;
            state = State::Str(c);
            i += 1;
            if syntax == Syntax::Json {
                let end = closing(chars, i, '"');
                if let Some(end) = end {
                    let after = (end + 1..n).find(|&j| !chars[j].is_whitespace());
                    if after.is_some_and(|a| chars[a] == ':') {
                        fill(&mut kinds, i - 1, end + 1, Kind::Key);
                        i = end + 1;
                        state = State::Normal;
                    }
                }
            }
            continue;
        }
        if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit) && (i == 0 || !is_word(chars[i - 1])))
        {
            if i > 0 && is_word(chars[i - 1]) {
                i += 1;
                continue;
            }
            let end = (i..n)
                .find(|&j| {
                    !(is_word(chars[j]) || chars[j] == '.' && chars.get(j + 1).is_some_and(char::is_ascii_digit))
                })
                .unwrap_or(n);
            fill(&mut kinds, i, end, Kind::Number);
            i = end;
            continue;
        }
        if is_ident_start(c) || (c == '#' && syntax == Syntax::C && i == first_word) {
            let end = (i + 1..n).find(|&j| !is_word(chars[j])).unwrap_or(n);
            word.clear();
            word.extend(&chars[i..end]);
            let next = (end..n).find(|&j| !chars[j].is_whitespace()).map(|j| chars[j]);
            let kind = if syntax.keywords().contains(&word.as_str()) {
                Kind::Keyword
            } else if syntax.constants().contains(&word.as_str()) {
                Kind::Constant
            } else if syntax == Syntax::Rust && chars.get(end) == Some(&'!') && chars.get(end + 1) != Some(&'=') {
                fill(&mut kinds, i, end + 1, Kind::Macro);
                std::mem::swap(&mut prev_word, &mut word);
                i = end + 1;
                continue;
            } else if word.len() > 1
                && word.chars().all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
                && word.chars().any(char::is_alphabetic)
            {
                Kind::Constant
            } else if is_function(&prev_word, next, chars, end, syntax) || syntax == Syntax::Shell && i == first_word {
                Kind::Function
            } else if word.starts_with(char::is_uppercase) && !config
                || matches!(
                    prev_word.as_str(),
                    "struct" | "enum" | "trait" | "type" | "class" | "interface" | "impl"
                )
            {
                Kind::Type
            } else {
                Kind::Plain
            };
            fill(&mut kinds, i, end, kind);
            std::mem::swap(&mut prev_word, &mut word);
            i = end;
            continue;
        }
        if c == '$' && syntax == Syntax::Shell {
            let end = if chars.get(i + 1) == Some(&'{') {
                (i..n).find(|&j| chars[j] == '}').map_or(n, |e| e + 1)
            } else {
                (i + 1..n).find(|&j| !is_word(chars[j])).unwrap_or(n).max(i + 2).min(n)
            };
            fill(&mut kinds, i, end, Kind::Constant);
            i = end;
            continue;
        }
        if c == '#' && syntax == Syntax::Rust && matches!(chars.get(i + 1), Some('[') | Some('!')) {
            let mut depth = 0usize;
            let end = (i..n)
                .find(|&j| match chars[j] {
                    '[' => {
                        depth += 1;
                        false
                    }
                    ']' => {
                        depth = depth.saturating_sub(1);
                        depth == 0
                    }
                    _ => false,
                })
                .map_or(n, |e| e + 1);
            fill(&mut kinds, i, end, Kind::Attribute);
            i = end;
            continue;
        }
        if c == '@' && matches!(syntax, Syntax::Python | Syntax::Java | Syntax::JavaScript) {
            let end = (i + 1..n)
                .find(|&j| !(is_word(chars[j]) || chars[j] == '.'))
                .unwrap_or(n);
            fill(&mut kinds, i, end, Kind::Attribute);
            i = end;
            continue;
        }
        if syntax == Syntax::Yaml && (c == '&' || c == '*') && chars.get(i + 1).is_some_and(|c| is_ident_start(*c)) {
            let end = (i + 1..n).find(|&j| !is_word(chars[j])).unwrap_or(n);
            fill(&mut kinds, i, end, Kind::Label);
            i = end;
            continue;
        }
        if "()[]{},;.".contains(c) {
            kinds[i] = Kind::Punct;
        } else if "=+-*/<>!&|%^?:~".contains(c) {
            kinds[i] = Kind::Operator;
        }
        if !c.is_whitespace() {
            prev_word.clear();
        }
        i += 1;
    }
    if let State::Str(q) = state {
        if !syntax.multiline_quote(q) {
            state = State::Normal;
        }
    }
    (kinds, state)
}

fn is_function(prev_word: &str, next: Option<char>, chars: &[char], end: usize, syntax: Syntax) -> bool {
    matches!(prev_word, "fn" | "def" | "func" | "function")
        || next == Some('(')
        || syntax == Syntax::Rust
            && next == Some(':')
            && chars.get(end + 1) == Some(&':')
            && chars.get(end + 2) == Some(&'<')
}

fn fill_str(kinds: &mut [Kind], from: usize, to: usize) {
    let to = to.min(kinds.len());
    for k in &mut kinds[from.min(to)..to] {
        if *k != Kind::Escape && *k != Kind::Operator {
            *k = Kind::Str;
        }
    }
}

fn markdown(chars: &[char], state: State) -> (Vec<Kind>, State) {
    let n = chars.len();
    let mut kinds = vec![Kind::Plain; n];
    let lead = chars.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    let fence = starts(chars, lead, "```") || starts(chars, lead, "~~~");
    if state == State::Fence {
        if fence {
            fill(&mut kinds, 0, n, Kind::Punct);
            return (kinds, State::Normal);
        }
        fill(&mut kinds, 0, n, Kind::Code);
        return (kinds, State::Fence);
    }
    if fence {
        fill(&mut kinds, 0, n, Kind::Punct);
        fill(&mut kinds, lead + 3, n, Kind::Label);
        return (kinds, State::Fence);
    }
    if chars.get(lead) == Some(&'#') {
        let hashes = chars[lead..].iter().take_while(|c| **c == '#').count();
        if hashes <= 6 && chars.get(lead + hashes).is_none_or(|c| *c == ' ') {
            fill(&mut kinds, lead, lead + hashes, Kind::Punct);
            fill(&mut kinds, lead + hashes, n, Kind::Heading);
            return (kinds, State::Normal);
        }
    }
    let first = chars.get(lead).copied();
    if matches!(first, Some('-' | '*' | '='))
        && chars.iter().filter(|c| !c.is_whitespace()).count() >= 3
        && chars.iter().all(|&c| c.is_whitespace() || Some(c) == first)
    {
        fill(&mut kinds, 0, n, Kind::Punct);
        return (kinds, State::Normal);
    }
    let mut i = lead;
    if chars.get(lead) == Some(&'>') {
        fill(&mut kinds, lead, n, Kind::Quote);
        return (kinds, State::Normal);
    }
    if matches!(chars.get(lead), Some('-' | '*' | '+')) && chars.get(lead + 1) == Some(&' ') {
        kinds[lead] = Kind::ListMark;
        i = lead + 2;
        if starts(chars, i, "[ ]") || starts(chars, i, "[x]") || starts(chars, i, "[X]") {
            fill(&mut kinds, i, i + 3, Kind::ListMark);
            i += 3;
        }
    } else {
        let digits = chars[lead..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0
            && matches!(chars.get(lead + digits), Some('.' | ')'))
            && chars.get(lead + digits + 1) == Some(&' ')
        {
            fill(&mut kinds, lead, lead + digits + 1, Kind::ListMark);
            i = lead + digits + 2;
        }
    }
    inline(chars, &mut kinds, i);
    (kinds, State::Normal)
}

fn inline(chars: &[char], kinds: &mut [Kind], from: usize) {
    let n = chars.len();
    let mut i = from;
    while i < n {
        let c = chars[i];
        if c == '`' {
            let ticks = chars[i..].iter().take_while(|c| **c == '`').count();
            if let Some(end) = (i + ticks..n).find(|&j| chars[j..].starts_with(&chars[i..i + ticks])) {
                fill(kinds, i, end + ticks, Kind::Code);
                i = end + ticks;
                continue;
            }
        }
        if (c == '*' || c == '_') && chars.get(i + 1) == Some(&c) {
            let pair = [c, c];
            if let Some(end) = (i + 2..n.saturating_sub(1)).find(|&j| chars[j..j + 2] == pair) {
                fill(kinds, i, end + 2, Kind::Strong);
                i = end + 2;
                continue;
            }
        }
        if (c == '*' || c == '_')
            && chars.get(i + 1).is_some_and(|n| !n.is_whitespace())
            && (i == 0 || !is_word(chars[i - 1]))
        {
            if let Some(end) = (i + 1..n).find(|&j| chars[j] == c && !chars[j - 1].is_whitespace()) {
                fill(kinds, i, end + 1, Kind::Emphasis);
                i = end + 1;
                continue;
            }
        }
        if c == '[' {
            if let Some(close) = (i + 1..n).find(|&j| chars[j] == ']') {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = (close + 2..n).find(|&j| chars[j] == ')') {
                        fill(kinds, i, close + 1, Kind::Link);
                        fill(kinds, close + 1, end + 1, Kind::Url);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if starts(chars, i, "http://") || starts(chars, i, "https://") {
            let end = (i..n)
                .find(|&j| chars[j].is_whitespace() || chars[j] == ')')
                .unwrap_or(n);
            fill(kinds, i, end, Kind::Url);
            i = end;
            continue;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind::*;

    fn kinds_of(line: &str, syntax: Syntax) -> Vec<(String, Kind)> {
        let (kinds, _) = highlight(line, syntax, State::Normal);
        let chars: Vec<char> = line.chars().collect();
        let mut out: Vec<(String, Kind)> = Vec::new();
        for (c, k) in chars.iter().zip(kinds) {
            match out.last_mut() {
                Some((s, last)) if *last == k => s.push(*c),
                _ => out.push((c.to_string(), k)),
            }
        }
        out.retain(|(s, _)| !s.trim().is_empty());
        out
    }

    fn kind_of(line: &str, syntax: Syntax, word: &str) -> Kind {
        kinds_of(line, syntax)
            .into_iter()
            .find(|(s, _)| s.trim() == word)
            .map(|(_, k)| k)
            .unwrap_or_else(|| panic!("{word} in {:?}", kinds_of(line, syntax)))
    }

    #[test]
    fn an_escaped_backslash_closes_its_quote() {
        let r = Syntax::Rust;
        assert_eq!(kind_of("let c = '\\\\'; foo()", r, "foo"), Function);
    }

    #[test]
    fn an_attribute_ends_at_its_own_bracket() {
        assert_eq!(kind_of("#[inline] fn f() -> [u8; 4]", Syntax::Rust, "fn"), Keyword);
    }

    #[test]
    fn rust() {
        let r = Syntax::Rust;
        let line = "pub fn draw(e: &Editor) -> Option<u8> { println!(\"hi\\n\"); MAX + 1.5 } // TODO: tidy";
        assert_eq!(kind_of(line, r, "pub"), Keyword);
        assert_eq!(kind_of(line, r, "draw"), Function);
        assert_eq!(kind_of(line, r, "Editor"), Type);
        assert_eq!(kind_of(line, r, "println!"), Macro);
        assert_eq!(kind_of(line, r, "\\n"), Escape);
        assert_eq!(kind_of(line, r, "MAX"), Constant);
        assert_eq!(kind_of(line, r, "1.5"), Number);
        assert_eq!(kind_of(line, r, "TODO"), Todo);
        assert_eq!(kind_of("fn f<'a>(x: &'a str) -> char { 'x' }", r, "'a"), Label);
        assert_eq!(kind_of("let c = 'x';", r, "'x'"), Str);
        assert_eq!(kind_of("#[derive(Debug)]", r, "#[derive(Debug)]"), Attribute);
        assert_eq!(kind_of("x.len()", r, "len"), Function);
        assert_eq!(kind_of("Vec::<u8>::new()", r, "new"), Function);
    }

    #[test]
    fn comments_and_strings_over_lines() {
        let (k, s) = highlight("a /* open", Syntax::C, State::Normal);
        assert_eq!((k[0], k[2], s), (Plain, Comment, State::Comment(1)));
        let (k, s) = highlight("still */ b", Syntax::C, s);
        assert_eq!((k[0], k[9], s), (Comment, Plain, State::Normal));
        let (_, s) = highlight("/* /* nested */", Syntax::Rust, State::Normal);
        assert_eq!(s, State::Comment(1));
        let (_, s) = highlight("x = \"\"\"doc", Syntax::Python, State::Normal);
        assert_eq!(s, State::Triple('"'));
        let (k, s) = highlight("end\"\"\" + y", Syntax::Python, s);
        assert_eq!((k[0], k[9], s), (Str, Plain, State::Normal));
        let (_, s) = highlight("let s = `multi", Syntax::JavaScript, State::Normal);
        assert_eq!(s, State::Str('`'));
        let (_, s) = highlight("x = \"unclosed", Syntax::Python, State::Normal);
        assert_eq!(s, State::Normal, "a Python string doesn't run on");
    }

}
