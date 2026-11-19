use tinsel::text::char_width_at;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Blank,
    Word,
    Punct,
}

pub fn class(c: char, big: bool) -> Class {
    if c.is_whitespace() {
        Class::Blank
    } else if big || is_word(c) {
        Class::Word
    } else {
        Class::Punct
    }
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

pub fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
}

pub fn indent(s: &str) -> &str {
    &s[..s.len() - s.trim_start_matches([' ', '\t']).len()]
}

pub fn first_non_blank(s: &str) -> usize {
    char_count(indent(s))
}

pub fn cell_width(c: char, vcol: usize, tabstop: usize) -> usize {
    match c {
        // shown as ^X
        c if c.is_control() && c != '\t' => 2,
        c => char_width_at(c, vcol, tabstop),
    }
}

pub fn vcol(line: &str, col: usize, tabstop: usize) -> usize {
    line.chars().take(col).fold(0, |v, c| v + cell_width(c, v, tabstop))
}

pub fn col_at(line: &str, target: usize, tabstop: usize) -> usize {
    let mut v = 0;
    for (i, c) in line.chars().enumerate() {
        let w = cell_width(c, v, tabstop);
        if v + w > target {
            return i;
        }
        v += w;
    }
    char_count(line)
}

pub fn expand(line: &str, tabstop: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut v = 0;
    for c in line.chars() {
        let w = cell_width(c, v, tabstop);
        match c {
            '\t' => out.extend(std::iter::repeat_n(' ', w)),
            c if c.is_control() => {
                out.push('^');
                out.push(caret(c));
            }
            c => out.push(c),
        }
        v += w;
    }
    out
}

/// Where `line` breaks into screen rows `width` wide, as each row's first character and the
/// screen column it starts at; the first row is always (0, 0). A character never splits, so
/// a row ends early rather than cut a tab or a wide character in two. With `linebreak` a row
/// ends after its last blank, as long as that leaves something on it.
pub fn wrap_rows(line: &str, width: usize, tabstop: usize, linebreak: bool) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut rows = vec![(0, 0)];
    let (mut v, mut row_v) = (0, 0);
    let mut blank: Option<(usize, usize)> = None;
    for (i, c) in line.chars().enumerate() {
        let w = cell_width(c, v, tabstop);
        while v + w > row_v + width && v > row_v {
            let at = match blank.filter(|b| linebreak && b.1 > row_v) {
                Some(b) => b,
                None => (i, v),
            };
            rows.push(at);
            row_v = at.1;
            blank = None;
        }
        if c == ' ' || c == '\t' {
            blank = Some((i + 1, v + w));
        }
        v += w;
    }
    rows
}

/// Which of `rows`, from [`wrap_rows`], holds the character at `col`.
pub fn row_holding(rows: &[(usize, usize)], col: usize) -> usize {
    rows.partition_point(|r| r.0 <= col).saturating_sub(1)
}

pub(crate) fn caret(c: char) -> char {
    match c as u32 {
        n @ 0..=0x1f => char::from_u32(n + 0x40).unwrap_or('?'),
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(class('a', false), Class::Word);
        assert_eq!(class('_', false), Class::Word);
        assert_eq!(class('.', false), Class::Punct);
        assert_eq!(class('.', true), Class::Word);
        assert_eq!(class('\t', true), Class::Blank);
    }

    #[test]
    fn columns_follow_tabs_and_wide_characters() {
        assert_eq!(vcol("\tx", 1, 4), 4);
        assert_eq!(vcol("ab\tx", 3, 4), 4);
        assert_eq!(vcol("界x", 1, 4), 2);
        assert_eq!(col_at("\tx", 2, 4), 0);
        assert_eq!(col_at("\tx", 4, 4), 1);
        assert_eq!(col_at("ab", 9, 4), 2);
        assert_eq!(expand("a\tb\r", 4), "a   b^M");
    }

    #[test]
    fn long_lines_wrap_into_rows() {
        assert_eq!(wrap_rows("", 4, 4, false), [(0, 0)]);
        assert_eq!(wrap_rows("abcd", 4, 4, false), [(0, 0)]);
        assert_eq!(wrap_rows("abcdefghij", 4, 4, false), [(0, 0), (4, 4), (8, 8)]);
        assert_eq!(
            wrap_rows("abc界d", 4, 4, false),
            [(0, 0), (3, 3)],
            "wide characters don't split"
        );
        assert_eq!(
            wrap_rows("ab\tcd", 3, 4, false),
            [(0, 0), (2, 2), (4, 5)],
            "nor do tabs"
        );
        assert_eq!(
            wrap_rows("界", 1, 4, false),
            [(0, 0)],
            "a character wider than a row has it to itself"
        );
        let rows = wrap_rows("abcdefghij", 4, 4, false);
        assert_eq!(row_holding(&rows, 0), 0);
        assert_eq!(row_holding(&rows, 5), 1);
        assert_eq!(row_holding(&rows, 10), 2);
    }

    #[test]
    fn linebreak_wraps_at_blanks() {
        assert_eq!(wrap_rows("one two three", 9, 4, true), [(0, 0), (8, 8)]);
        assert_eq!(wrap_rows("one two three", 9, 4, false), [(0, 0), (9, 9)]);
        assert_eq!(
            wrap_rows("abcdefgh ij", 4, 4, true),
            [(0, 0), (4, 4), (8, 8)],
            "or mid-word, with no blank"
        );
    }

    #[test]
    fn indents() {
        assert_eq!(indent("  \tx y"), "  \t");
        assert_eq!(first_non_blank("    "), 4);
        assert_eq!(byte_at("aé b", 2), 3);
        assert_eq!(byte_at("ab", 5), 2);
    }
}
