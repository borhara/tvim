use std::rc::Rc;

use super::{Editor, remember};
use crate::buffer::Pos;
use crate::regex::Regex;
use crate::text::is_word;

impl Editor {
    pub(crate) fn search_hits(&self) -> Option<Rc<[Pos]>> {
        let pat = self.search_pattern()?;
        if self.buf.len() > 50_000 {
            return None;
        }
        let re = self.regex(pat).ok()?;
        let mut cache = self.hits_cache.borrow_mut();
        let fresh = cache
            .as_ref()
            .is_some_and(|(id, v, r, _)| *id == self.buffer_id && *v == self.version && Rc::ptr_eq(r, &re));
        if !fresh {
            let hits = self
                .buf
                .lines()
                .iter()
                .enumerate()
                .flat_map(|(row, line)| re.spans(line).into_iter().map(move |(col, _)| Pos::new(row, col)))
                .collect();
            *cache = Some((self.buffer_id, self.version, re, hits));
        }
        cache.as_ref().map(|c| c.3.clone())
    }

    pub fn search_count(&self) -> Option<(usize, usize)> {
        let hits = self.search_hits()?;
        let current = hits.partition_point(|p| *p <= self.cursor);
        (!hits.is_empty()).then_some((current.max(1), hits.len()))
    }

    pub(crate) fn regex(&self, pat: &str) -> Result<Rc<Regex>, String> {
        let case = (self.options.ignore_case, self.options.smart_case);
        let mut cache = self.regex_cache.borrow_mut();
        if let Some((p, c, re)) = cache.as_ref() {
            if p == pat && *c == case {
                return Ok(re.clone());
            }
        }
        let mut re = Regex::new(pat)?;
        re.set_case(case.0, case.1);
        let re = Rc::new(re);
        *cache = Some((pat.to_string(), case, re.clone()));
        Ok(re)
    }

    pub(crate) fn bad_pattern(&mut self, pat: &str, e: &str) {
        self.error(format!("E383: Invalid search string: {pat} ({e})"));
    }

    pub fn matches_in(&self, line: &str, pat: &str) -> Vec<(usize, usize)> {
        self.regex(pat).map(|re| re.spans(line)).unwrap_or_default()
    }

    pub(crate) fn find(&mut self, pat: &str, from: Pos, forward: bool, skip_here: bool) -> Option<Pos> {
        let re = match self.regex(pat) {
            Ok(re) => re,
            Err(e) => {
                self.bad_pattern(pat, &e);
                return None;
            }
        };
        let rows = self.buf.len();
        let hits =
            |e: &Self, row: usize| -> Vec<usize> { re.spans(e.buf.line(row)).into_iter().map(|m| m.0).collect() };
        let mut wrapped = false;
        if forward {
            for i in 0..=rows {
                let row = (from.row + i) % rows;
                if from.row + i >= rows {
                    wrapped = true;
                }
                let found = hits(self, row).into_iter().find(|&c| {
                    i > 0 && i < rows
                        || i == 0 && (c > from.col || !skip_here && c == from.col)
                        || i == rows && c <= from.col
                });
                if let Some(c) = found {
                    if wrapped {
                        self.info("search hit BOTTOM, continuing at TOP");
                    }
                    return Some(Pos::new(row, c));
                }
            }
        } else {
            for i in 0..=rows {
                let row = (from.row + rows * 2 - i) % rows;
                if i > from.row {
                    wrapped = true;
                }
                let found = hits(self, row).into_iter().rev().find(|&c| {
                    i > 0 && i < rows
                        || i == 0 && (c < from.col || !skip_here && c == from.col)
                        || i == rows && c >= from.col
                });
                if let Some(c) = found {
                    if wrapped {
                        self.info("search hit TOP, continuing at BOTTOM");
                    }
                    return Some(Pos::new(row, c));
                }
            }
        }
        self.error(format!("E486: Pattern not found: {pat}"));
        None
    }

    pub(super) fn word_under_cursor(&self) -> Option<(usize, String)> {
        let chars: Vec<char> = self.buf.line(self.cursor.row).chars().collect();
        let mut col = self.cursor.col;
        while col < chars.len() && !is_word(chars[col]) {
            col += 1;
        }
        if col >= chars.len() {
            return None;
        }
        let mut start = col;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = col;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        Some((start, chars[start..end].iter().collect()))
    }

    pub(super) fn search_word(&mut self, forward: bool) {
        let Some((col, word)) = self.word_under_cursor() else {
            self.error("E348: No string under cursor");
            return;
        };
        let pat = format!("\\<{}\\>", crate::regex::escape(&word));
        self.last_search = Some(pat.clone());
        self.search_backward = !forward;
        self.search_shown = true;
        remember(&mut self.search_history, &pat);
        let start = self.cursor;
        self.message = None;
        // from the start of the word, so # doesn't land on it
        let Some(at) = self.find(&pat, Pos::new(start.row, col), forward, true) else {
            return;
        };
        self.jump_from(start);
        self.cursor = at;
        self.set_want();
        self.land(true);
        if self.message.is_none() {
            let prompt = if forward { '/' } else { '?' };
            self.info(format!("{prompt}{pat}"));
        }
    }
}
