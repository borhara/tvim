use crate::syntax::{Syntax, extension};
use crate::text::{byte_at, char_count, indent};

pub fn markers(name: &str) -> (&'static str, &'static str) {
    match extension(name).as_str() {
        "html" | "htm" | "xml" | "xhtml" | "svg" | "vue" | "svelte" | "md" | "markdown" | "mdx" => {
            return ("<!--", "-->");
        }
        "css" | "less" => return ("/*", "*/"),
        "scss" | "sass" | "php" => return ("//", ""),
        "sql" | "hs" | "elm" | "ada" => return ("--", ""),
        "lisp" | "el" | "clj" | "cljs" | "scm" | "rkt" | "asm" | "s" | "ini" => return (";", ""),
        "vim" => return ("\"", ""),
        "tex" | "erl" | "m4" => return ("%", ""),
        _ => {}
    }
    match Syntax::detect(name) {
        Some(s) => match (s.line_comment().first(), s.block_comment()) {
            (Some(line), _) => (line, ""),
            (None, Some(block)) => block,
            (None, None) => ("#", ""),
        },
        None => ("#", ""),
    }
}

fn is_commented(line: &str, open: &str, close: &str) -> bool {
    let t = line.trim();
    t.starts_with(open) && t.ends_with(close) && t.len() >= open.len() + close.len()
}

fn uncomment(line: &str, open: &str, close: &str) -> String {
    let ind = indent(line);
    let rest = &line[ind.len()..];
    let rest = rest.strip_prefix(open).unwrap_or(rest);
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    let mut rest = rest;
    if !close.is_empty() {
        rest = rest.trim_end_matches([' ', '\t']);
        rest = rest.strip_suffix(close).unwrap_or(rest);
        rest = rest.strip_suffix(' ').unwrap_or(rest);
    }
    format!("{ind}{rest}")
}

pub fn toggle(lines: &[String], open: &str, close: &str) -> Vec<String> {
    let blank = |l: &String| l.trim().is_empty();
    let filled: Vec<&String> = lines.iter().filter(|l| !blank(l)).collect();
    if filled.iter().all(|l| is_commented(l, open, close)) {
        return lines
            .iter()
            .map(|l| if blank(l) { l.clone() } else { uncomment(l, open, close) })
            .collect();
    }
    let at = filled.iter().map(|l| char_count(indent(l))).min().unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if blank(l) {
                return l.clone();
            }
            let split = byte_at(l, at);
            let tail = if close.is_empty() {
                String::new()
            } else {
                format!(" {close}")
            };
            format!("{}{open} {}{tail}", &l[..split], &l[split..])
        })
        .collect()
}
