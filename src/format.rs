//! Rewrapping text to a width, as gq does: paragraphs joined up and broken again between
//! words, keeping their indent, the comment marker they start with, and a list item's bullet.

use crate::buffer::Pos;
use crate::text::vcol;

/// How a file's lines start: the marker its comments begin with, if they run to the end of
/// the line, and whether it's markdown, where headings, tables and code are left alone.
#[derive(Clone, Copy, Debug, Default)]
pub struct Kind<'a> {
    pub comment: Option<&'a str>,
    pub markdown: bool,
}

/// `lines` rewrapped so none is wider than `width` columns, unless a word is.
pub fn wrap(lines: &[String], width: usize, tabstop: usize, kind: Kind) -> Vec<String> {
    let mut out = Vec::new();
    let mut para: Option<Para> = None;
    let mut fenced = false;
    for line in lines {
        let fence = kind.markdown && line.trim_start().starts_with("```");
        if fenced || fence || kind.markdown && is_markdown_block(line) {
            flush(&mut para, &mut out, width, tabstop);
            fenced ^= fence;
            out.push(line.clone());
            continue;
        }
        let (leader, rest) = split_leader(line, kind);
        if rest.trim().is_empty() {
            flush(&mut para, &mut out, width, tabstop);
            out.push(line.trim_end().to_string());
            continue;
        }
        let bullet = bullet_len(rest);
        match &mut para {
            Some(p) if bullet == 0 && p.rest == leader => p.words.push(rest.to_string()),
            _ => {
                flush(&mut para, &mut out, width, tabstop);
                let pad = " ".repeat(rest[..bullet].chars().count());
                para = Some(Para {
                    first: format!("{leader}{}", &rest[..bullet]),
                    rest: format!("{leader}{pad}"),
                    words: vec![rest[bullet..].to_string()],
                });
            }
        }
    }
    flush(&mut para, &mut out, width, tabstop);
    out
}

/// Which word of `lines` holds `p`, and how far into it, or the next word if `p` is on a
/// blank or a line's leader, so `place` can find it again once they're rewrapped.
pub fn word_at(lines: &[String], kind: Kind, p: Pos) -> Option<(usize, usize)> {
    let words = words(lines, kind);
    if let Some(i) = words
        .iter()
        .position(|&(row, a, b)| row == p.row && (a..b).contains(&p.col))
    {
        return Some((i, p.col - words[i].1));
    }
    match words.iter().position(|&(row, a, _)| (row, a) > (p.row, p.col)) {
        Some(i) => Some((i, 0)),
        None => words.len().checked_sub(1).map(|i| (i, words[i].2 - words[i].1 - 1)),
    }
}

/// Where the `offset`th character of word `word` of `lines` is.
pub fn place(lines: &[String], kind: Kind, (word, offset): (usize, usize)) -> Option<Pos> {
    let (row, a, b) = *words(lines, kind).get(word)?;
    Some(Pos::new(row, (a + offset).min(b - 1)))
}

// each word as its row and columns, past the indent, comment marker and bullet before it
fn words(lines: &[String], kind: Kind) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let (leader, rest) = split_leader(line, kind);
        let chars: Vec<char> = line.chars().collect();
        let mut col = leader.chars().count() + rest[..bullet_len(rest)].chars().count();
        while col < chars.len() {
            if chars[col].is_whitespace() {
                col += 1;
                continue;
            }
            let start = col;
            while col < chars.len() && !chars[col].is_whitespace() {
                col += 1;
            }
            out.push((row, start, col));
        }
    }
    out
}

struct Para {
    // what the first line starts with, and the rest
    first: String,
    rest: String,
    words: Vec<String>,
}

fn flush(para: &mut Option<Para>, out: &mut Vec<String>, width: usize, tabstop: usize) {
    let Some(p) = para.take() else { return };
    let mut line = p.first.clone();
    let mut empty = true;
    for word in p.words.iter().flat_map(|w| w.split_whitespace()) {
        let wide = vcol(&line, line.chars().count(), tabstop) + 1 + tinsel::text::width(word);
        if !empty && wide > width {
            out.push(line);
            line = p.rest.clone();
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(word);
        empty = false;
    }
    out.push(line);
}

/// What a line broken from `line` starts with: its indent and comment marker, and spaces as
/// wide as its bullet if it's a list item.
pub fn continuation(line: &str, kind: Kind) -> String {
    let (leader, rest) = split_leader(line, kind);
    format!("{leader}{}", " ".repeat(bullet_len(rest)))
}

/// A line's indent, with the comment marker after it and the spaces after that, and the rest.
pub fn split_leader<'a>(line: &'a str, kind: Kind) -> (&'a str, &'a str) {
    let text = line.trim_start_matches([' ', '\t']);
    let mut at = line.len() - text.len();
    let marker = kind.comment.filter(|m| text.starts_with(m)).or_else(|| {
        // a markdown quote, as > text
        (kind.markdown && text.starts_with('>')).then_some(">")
    });
    if let Some(m) = marker {
        let after = &text[m.len()..];
        at += m.len() + after.len() - after.trim_start_matches([' ', '\t']).len();
    }
    line.split_at(at)
}

/// How long a list item's bullet is, as in "- " or "12. ", or 0 if the text isn't one.
pub fn bullet_len(text: &str) -> usize {
    if text.starts_with(['-', '*', '+']) && text[1..].starts_with(' ') {
        return 2;
    }
    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
    let after = &text[digits..];
    if digits > 0 && after.starts_with(['.', ')']) && after[1..].starts_with(' ') {
        digits + 2
    } else {
        0
    }
}

// a heading, table row or rule, kept on a line of its own
fn is_markdown_block(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#') || t.starts_with('|') || t.starts_with("---") || t.starts_with("===")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, width: usize, kind: Kind) -> String {
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        wrap(&lines, width, 4, kind).join("\n")
    }

    const PLAIN: Kind = Kind {
        comment: None,
        markdown: false,
    };

    #[test]
    fn joins_and_breaks_between_words() {
        assert_eq!(w("aa bb cc dd", 5, PLAIN), "aa bb\ncc dd");
        assert_eq!(w("aa\nbb\ncc", 80, PLAIN), "aa bb cc");
        assert_eq!(w("a\n\nb\nc", 80, PLAIN), "a\n\nb c", "blank lines part paragraphs");
        assert_eq!(w("  aa bb cc", 7, PLAIN), "  aa bb\n  cc", "the indent stays");
        assert_eq!(w("abcdefgh ij", 4, PLAIN), "abcdefgh\nij", "a long word gets a line");
        assert_eq!(w("a  b   c", 80, PLAIN), "a b c");
    }

    #[test]
    fn comments_keep_their_marker() {
        let rust = Kind {
            comment: Some("//"),
            markdown: false,
        };
        assert_eq!(w("    // aa bb cc", 12, rust), "    // aa bb\n    // cc");
        assert_eq!(w("// aa\n// bb", 80, rust), "// aa bb");
        assert_eq!(w("// aa\ncode", 80, rust), "// aa\ncode", "code isn't joined on");
        assert_eq!(w("// aa\n//\n// bb", 80, rust), "// aa\n//\n// bb");
    }

    #[test]
    fn words_are_found_again_after_rewrapping() {
        let rust = Kind {
            comment: Some("//"),
            markdown: false,
        };
        let before: Vec<String> = vec!["// aa bbb".into(), "// cc".into()];
        let after = wrap(&before, 80, 4, rust);
        let at = word_at(&before, rust, Pos::new(1, 4)).unwrap();
        assert_eq!(at, (2, 1));
        assert_eq!(place(&after, rust, at), Some(Pos::new(0, 11)));
        assert_eq!(
            word_at(&before, rust, Pos::new(0, 1)),
            Some((0, 0)),
            "on the marker, the next word"
        );
        assert_eq!(
            word_at(&before, rust, Pos::new(1, 9)),
            Some((2, 1)),
            "past them all, the last"
        );
    }

    #[test]
    fn list_items_hang() {
        let md = Kind {
            comment: None,
            markdown: true,
        };
        assert_eq!(w("- aa bb cc", 7, md), "- aa bb\n  cc");
        assert_eq!(w("- aa\n- bb", 80, md), "- aa\n- bb", "each item is its own");
        assert_eq!(w("1. aa\n   bb", 80, md), "1. aa bb");
        assert_eq!(w("# Title\ntext\nmore", 80, md), "# Title\ntext more");
        assert_eq!(w("a\n```\nx\ny\n```\nb", 80, md), "a\n```\nx\ny\n```\nb");
        assert_eq!(w("> aa bb", 5, md), "> aa\n> bb");
    }
}
