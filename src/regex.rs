use crate::text::is_word;

const MAX_INSTS: usize = 20_000;
const MAX_REPEAT: u32 = 1000;
const MAX_STEPS: usize = 2_000_000;
const MAX_MEMO: usize = 1 << 24;

pub type Span = (usize, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Start,
    End,
    Boundary,
    NotBoundary,
    WordStart,
    WordEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Perl {
    Digit,
    Word,
    Space,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Range(char, char),
    Perl(Perl, bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Set {
    negated: bool,
    items: Vec<Item>,
}

enum Node {
    Empty,
    Char(char),
    Any,
    Set(Set),
    Look(Look),
    Group(Box<Node>, Option<usize>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
}

#[derive(Clone, Debug)]
enum Inst {
    Char(char),
    Any,
    Set(Set),
    Look(Look),
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Match,
}

#[derive(Clone, Debug)]
pub struct Regex {
    insts: Vec<Inst>,
    slots: usize,
    pub ignore_case: bool,
    forced: Option<bool>,
    upper: bool,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, String> {
        let mut p = Parser {
            chars: pattern.chars().collect(),
            i: 0,
            groups: 0,
            upper: false,
            forced: None,
        };
        let root = p.alt()?;
        if p.i < p.chars.len() {
            return Err("unmatched )".into());
        }
        let mut c = Compiler { insts: Vec::new() };
        c.push(Inst::Save(0))?;
        c.emit(&root)?;
        c.push(Inst::Save(1))?;
        c.push(Inst::Match)?;
        Ok(Regex {
            insts: c.insts,
            slots: 2 * (p.groups + 1),
            ignore_case: false,
            forced: p.forced,
            upper: p.upper,
        })
    }

    // \c and \C win over the options
    pub fn set_case(&mut self, ignore_case: bool, smart_case: bool) {
        self.ignore_case = self.forced.unwrap_or(ignore_case && !(smart_case && self.upper));
    }

    pub fn find_at(&self, hay: &[char], from: usize) -> Option<Vec<Option<Span>>> {
        if from > hay.len() {
            return None;
        }
        let len = hay.len() - from + 1;
        // plain backtracking first, then remember dead states if it gets slow
        let budget = 1000 + 4 * self.insts.len() * len;
        let slots = match self.scan(hay, from, None, budget) {
            Ok(found) => found,
            Err(()) if self.insts.len() * (hay.len() + 1) <= MAX_MEMO => {
                let mut memo = vec![0u64; (self.insts.len() * (hay.len() + 1)).div_ceil(64)];
                self.scan(hay, from, Some(&mut memo), usize::MAX).ok().flatten()
            }
            Err(()) => self.scan(hay, from, None, MAX_STEPS).ok().flatten(),
        }?;
        Some(
            slots
                .chunks(2)
                .map(|p| match (p[0], p[1]) {
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => None,
                })
                .collect(),
        )
    }

    pub fn captures_all(&self, hay: &[char]) -> Vec<Vec<Option<Span>>> {
        let mut out = Vec::new();
        let mut from = 0;
        let mut last_end = None;
        while let Some(caps) = self.find_at(hay, from) {
            let Some((s, e)) = caps[0] else { break };
            from = if e > s { e } else { e + 1 };
            // like vim, no empty match right where the last one ended
            if s == e && last_end == Some(s) {
                continue;
            }
            last_end = Some(e);
            out.push(caps);
        }
        out
    }

    pub fn spans(&self, line: &str) -> Vec<Span> {
        let hay: Vec<char> = line.chars().collect();
        self.captures_all(&hay).into_iter().filter_map(|c| c[0]).collect()
    }

    pub fn is_match(&self, line: &str) -> bool {
        let hay: Vec<char> = line.chars().collect();
        self.find_at(&hay, 0).is_some()
    }

    fn scan(
        &self,
        hay: &[char],
        from: usize,
        mut memo: Option<&mut Vec<u64>>,
        limit: usize,
    ) -> Result<Option<Vec<Option<usize>>>, ()> {
        let first = match self.insts.get(1) {
            Some(Inst::Char(c)) => Some(*c),
            _ => None,
        };
        let mut steps = 0;
        for start in from..=hay.len() {
            if let Some(c) = first {
                if !hay.get(start).is_some_and(|&h| self.same(h, c)) {
                    continue;
                }
            }
            if let Some(found) = self.run(hay, start, memo.as_deref_mut(), &mut steps, limit)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    fn run(
        &self,
        hay: &[char],
        start: usize,
        mut memo: Option<&mut Vec<u64>>,
        steps: &mut usize,
        limit: usize,
    ) -> Result<Option<Vec<Option<usize>>>, ()> {
        enum Job {
            Try(usize, usize),
            Restore(usize, Option<usize>),
        }
        let width = hay.len() + 1;
        let mut slots = vec![None; self.slots];
        let mut stack = vec![Job::Try(0, start)];
        while let Some(job) = stack.pop() {
            let (mut pc, mut pos) = match job {
                Job::Restore(s, v) => {
                    slots[s] = v;
                    continue;
                }
                Job::Try(pc, pos) => (pc, pos),
            };
            loop {
                if let Some(m) = memo.as_deref_mut() {
                    let k = pc * width + pos;
                    if m[k / 64] & (1 << (k % 64)) != 0 {
                        break;
                    }
                    m[k / 64] |= 1 << (k % 64);
                }
                *steps += 1;
                if *steps > limit {
                    return Err(());
                }
                match &self.insts[pc] {
                    Inst::Char(c) => {
                        if !hay.get(pos).is_some_and(|&h| self.same(h, *c)) {
                            break;
                        }
                        pc += 1;
                        pos += 1;
                    }
                    Inst::Any => {
                        if pos >= hay.len() {
                            break;
                        }
                        pc += 1;
                        pos += 1;
                    }
                    Inst::Set(set) => {
                        if !hay.get(pos).is_some_and(|&h| self.in_set(set, h)) {
                            break;
                        }
                        pc += 1;
                        pos += 1;
                    }
                    Inst::Look(l) => {
                        if !looks(*l, hay, pos) {
                            break;
                        }
                        pc += 1;
                    }
                    Inst::Split(a, b) => {
                        stack.push(Job::Try(*b, pos));
                        pc = *a;
                    }
                    Inst::Jmp(a) => pc = *a,
                    Inst::Save(s) => {
                        stack.push(Job::Restore(*s, slots[*s]));
                        slots[*s] = Some(pos);
                        pc += 1;
                    }
                    Inst::Match => return Ok(Some(slots)),
                }
            }
        }
        Ok(None)
    }

    fn same(&self, a: char, b: char) -> bool {
        a == b || self.ignore_case && fold(a) == fold(b)
    }

    fn in_set(&self, set: &Set, c: char) -> bool {
        let has = |c: char| set.items.iter().any(|it| item_has(it, c));
        let hit = has(c) || self.ignore_case && (has(fold(c)) || has(upper(c)));
        hit != set.negated
    }
}

pub fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "\\.^$|()[]{}*+?".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn fold(c: char) -> char {
    convert(c, char::to_ascii_lowercase, char::to_lowercase)
}

fn upper(c: char) -> char {
    convert(c, char::to_ascii_uppercase, char::to_uppercase)
}

fn convert<I: Iterator<Item = char>>(c: char, ascii: fn(&char) -> char, full: fn(char) -> I) -> char {
    if c.is_ascii() {
        return ascii(&c);
    }
    let mut m = full(c);
    match (m.next(), m.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

fn item_has(item: &Item, c: char) -> bool {
    match *item {
        Item::Range(a, b) => a <= c && c <= b,
        Item::Perl(p, negated) => {
            let hit = match p {
                Perl::Digit => c.is_ascii_digit(),
                Perl::Word => is_word(c),
                Perl::Space => c.is_whitespace(),
            };
            hit != negated
        }
    }
}

fn looks(l: Look, hay: &[char], pos: usize) -> bool {
    let before = pos.checked_sub(1).is_some_and(|i| is_word(hay[i]));
    let after = hay.get(pos).is_some_and(|&c| is_word(c));
    match l {
        Look::Start => pos == 0,
        Look::End => pos == hay.len(),
        Look::Boundary => before != after,
        Look::NotBoundary => before == after,
        Look::WordStart => !before && after,
        Look::WordEnd => before && !after,
    }
}

struct Parser {
    chars: Vec<char>,
    i: usize,
    groups: usize,
    upper: bool,
    forced: Option<bool>,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }

    fn alt(&mut self) -> Result<Node, String> {
        let mut alts = vec![self.concat()?];
        while self.peek() == Some('|') {
            self.i += 1;
            alts.push(self.concat()?);
        }
        Ok(if alts.len() == 1 {
            alts.pop().unwrap()
        } else {
            Node::Alt(alts)
        })
    }

    fn concat(&mut self) -> Result<Node, String> {
        let mut nodes = Vec::new();
        while !matches!(self.peek(), None | Some('|' | ')')) {
            let atom = self.atom()?;
            nodes.push(self.quantified(atom)?);
        }
        Ok(Node::Concat(nodes))
    }

    fn atom(&mut self) -> Result<Node, String> {
        let c = self.next().unwrap_or_default();
        Ok(match c {
            '(' => {
                let cap = if self.chars[self.i..].starts_with(&['?', ':']) {
                    self.i += 2;
                    None
                } else {
                    self.groups += 1;
                    Some(self.groups)
                };
                let inner = self.alt()?;
                if self.next() != Some(')') {
                    return Err("unmatched (".into());
                }
                Node::Group(Box::new(inner), cap)
            }
            '.' => Node::Any,
            '^' => Node::Look(Look::Start),
            '$' => Node::Look(Look::End),
            '[' => Node::Set(self.set()?),
            '*' | '+' | '?' => return Err(format!("nothing to repeat before {c}")),
            '{' if self.braces(self.i)?.is_some() => return Err("nothing to repeat before {".into()),
            '\\' => {
                let Some(e) = self.next() else {
                    return Err("trailing \\".into());
                };
                match e {
                    'd' | 'w' | 's' | 'D' | 'W' | 'S' => Node::Set(Set {
                        negated: false,
                        items: vec![perl(e)],
                    }),
                    'b' => Node::Look(Look::Boundary),
                    'B' => Node::Look(Look::NotBoundary),
                    '<' => Node::Look(Look::WordStart),
                    '>' => Node::Look(Look::WordEnd),
                    'c' | 'C' => {
                        self.forced = Some(e == 'c');
                        Node::Empty
                    }
                    _ => Node::Char(unescape(e)),
                }
            }
            c => {
                self.upper |= c.is_uppercase();
                Node::Char(c)
            }
        })
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, String> {
        let (min, max) = match self.peek() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            Some('{') => match self.braces(self.i + 1)? {
                Some(r) => r,
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        if matches!(atom, Node::Empty) {
            return Err("nothing to repeat".into());
        }
        if self.peek() != Some('{') {
            self.i += 1;
        } else {
            self.skip_braces();
        }
        let greedy = self.peek() != Some('?');
        if !greedy {
            self.i += 1;
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        })
    }

    // n}, n,}, ,m} or n,m} from just after a {
    fn braces(&self, at: usize) -> Result<Option<(u32, Option<u32>)>, String> {
        let rest: String = self.chars[at..].iter().collect();
        let Some(end) = rest.find('}') else { return Ok(None) };
        let body = &rest[..end];
        let num = |s: &str| -> Result<Option<u32>, ()> {
            if s.is_empty() {
                return Ok(None);
            }
            if !s.chars().all(|c| c.is_ascii_digit()) {
                return Err(());
            }
            Ok(Some(s.parse().unwrap_or(u32::MAX)))
        };
        let (min, max) = match body.split_once(',') {
            None => match num(body) {
                Ok(Some(n)) => (n, Some(n)),
                _ => return Ok(None),
            },
            Some((a, b)) => match (num(a), num(b)) {
                (Ok(None), Ok(None)) | (Err(()), _) | (_, Err(())) => return Ok(None),
                (Ok(a), Ok(b)) => (a.unwrap_or(0), b),
            },
        };
        if max.is_some_and(|m| m < min) {
            return Err(format!("bad repeat {{{body}}}"));
        }
        if min > MAX_REPEAT || max.is_some_and(|m| m > MAX_REPEAT) {
            return Err(format!("repeat count over {MAX_REPEAT}"));
        }
        Ok(Some((min, max)))
    }

    fn skip_braces(&mut self) {
        while self.next().is_some_and(|c| c != '}') {}
    }

    fn set(&mut self) -> Result<Set, String> {
        let negated = self.peek() == Some('^');
        if negated {
            self.i += 1;
        }
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.next() else {
                return Err("unmatched [".into());
            };
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let e = self.next().ok_or("unmatched [")?;
                if "dwsDWS".contains(e) {
                    items.push(perl(e));
                    continue;
                }
                unescape(e)
            } else {
                c
            };
            let hi = if self.peek() == Some('-') && !matches!(self.chars.get(self.i + 1), None | Some(']')) {
                self.i += 1;
                match self.next() {
                    Some('\\') => unescape(self.next().ok_or("unmatched [")?),
                    Some(h) => h,
                    None => return Err("unmatched [".into()),
                }
            } else {
                lo
            };
            if hi < lo {
                return Err(format!("bad range {lo}-{hi}"));
            }
            self.upper |= lo.is_uppercase() || hi.is_uppercase();
            items.push(Item::Range(lo, hi));
        }
        Ok(Set { negated, items })
    }
}

fn perl(e: char) -> Item {
    let kind = match e.to_ascii_lowercase() {
        'd' => Perl::Digit,
        'w' => Perl::Word,
        _ => Perl::Space,
    };
    Item::Perl(kind, e.is_ascii_uppercase())
}

fn unescape(e: char) -> char {
    match e {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        e => e,
    }
}

struct Compiler {
    insts: Vec<Inst>,
}

impl Compiler {
    fn push(&mut self, i: Inst) -> Result<usize, String> {
        if self.insts.len() >= MAX_INSTS {
            return Err("pattern too big".into());
        }
        self.insts.push(i);
        Ok(self.insts.len() - 1)
    }

    fn emit(&mut self, node: &Node) -> Result<(), String> {
        match node {
            Node::Empty => {}
            Node::Char(c) => _ = self.push(Inst::Char(*c))?,
            Node::Any => _ = self.push(Inst::Any)?,
            Node::Set(s) => _ = self.push(Inst::Set(s.clone()))?,
            Node::Look(l) => _ = self.push(Inst::Look(*l))?,
            Node::Group(inner, cap) => {
                if let Some(k) = cap {
                    self.push(Inst::Save(2 * k))?;
                }
                self.emit(inner)?;
                if let Some(k) = cap {
                    self.push(Inst::Save(2 * k + 1))?;
                }
            }
            Node::Concat(nodes) => {
                for n in nodes {
                    self.emit(n)?;
                }
            }
            Node::Alt(alts) => {
                let mut ends = Vec::new();
                for (i, n) in alts.iter().enumerate() {
                    if i + 1 == alts.len() {
                        self.emit(n)?;
                        break;
                    }
                    let split = self.push(Inst::Split(0, 0))?;
                    self.emit(n)?;
                    ends.push(self.push(Inst::Jmp(0))?);
                    self.insts[split] = Inst::Split(split + 1, self.insts.len());
                }
                let end = self.insts.len();
                for j in ends {
                    self.insts[j] = Inst::Jmp(end);
                }
            }
            Node::Repeat { node, min, max, greedy } => {
                for _ in 0..*min {
                    self.emit(node)?;
                }
                let split = |a, b| if *greedy { Inst::Split(a, b) } else { Inst::Split(b, a) };
                match max {
                    None => {
                        let at = self.push(Inst::Split(0, 0))?;
                        self.emit(node)?;
                        self.push(Inst::Jmp(at))?;
                        self.insts[at] = split(at + 1, self.insts.len());
                    }
                    Some(m) => {
                        let mut holes = Vec::new();
                        for _ in *min..*m {
                            holes.push(self.push(Inst::Split(0, 0))?);
                            self.emit(node)?;
                        }
                        let end = self.insts.len();
                        for h in holes {
                            self.insts[h] = split(h + 1, end);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(pat: &str, text: &str) -> Vec<Span> {
        Regex::new(pat).unwrap().spans(text)
    }

    fn first(pat: &str, text: &str) -> Option<Span> {
        all(pat, text).first().copied()
    }

    fn groups(pat: &str, text: &str) -> Vec<Option<String>> {
        let hay: Vec<char> = text.chars().collect();
        let caps = Regex::new(pat).unwrap().find_at(&hay, 0).unwrap();
        caps.iter()
            .map(|c| c.map(|(a, b)| hay[a..b].iter().collect()))
            .collect()
    }

    #[test]
    fn literals_and_dots() {
        assert_eq!(all("ab", "xabyab"), [(1, 3), (4, 6)]);
        assert_eq!(first("a.c", "abc a-c"), Some((0, 3)));
        assert_eq!(first("a.c", "ac"), None);
        assert_eq!(all("é", "café é"), [(3, 4), (5, 6)], "columns are chars");
    }

    #[test]
    fn classes() {
        assert_eq!(first("[a-c]+", "xxbcaz"), Some((2, 5)));
        assert_eq!(first("[^a-c ]+", "ab xyz"), Some((3, 6)));
        assert_eq!(first("[]x]+", "a]x]"), Some((1, 4)), "a leading ] is literal");
        assert_eq!(first("[a-]+", "b-a"), Some((1, 3)), "so is a trailing -");
        assert_eq!(first(r"[\d.]+", "v1.25"), Some((1, 5)));
        assert_eq!(first(r"\d+", "ab 123"), Some((3, 6)));
        assert_eq!(first(r"\w+", "  foo_1 "), Some((2, 7)));
        assert_eq!(first(r"\s+", "a \tb"), Some((1, 3)));
        assert_eq!(first(r"\D+", "12ab3"), Some((2, 4)));
        assert_eq!(first(r"\W+", "ab, c"), Some((2, 4)));
        assert_eq!(first(r"\S+", "  ab "), Some((2, 4)));
    }

    #[test]
    fn anchors_and_word_edges() {
        assert_eq!(all("^a", "aaa"), [(0, 1)]);
        assert_eq!(all("a$", "aaa"), [(2, 3)]);
        assert_eq!(all("^$", ""), [(0, 0)]);
        assert_eq!(all(r"\bcat\b", "cat concat cat"), [(0, 3), (11, 14)]);
        assert_eq!(all(r"\<cat\>", "cat concat cats"), [(0, 3)]);
        assert_eq!(all(r"\Bcat", "cat concat"), [(7, 10)]);
        assert_eq!(all(r"\<", "ab cd"), [(0, 0), (3, 3)]);
    }

    #[test]
    fn groups_and_alternation() {
        assert_eq!(first("cat|dog", "hotdog"), Some((3, 6)));
        assert_eq!(first("(ab)+", "xababa"), Some((1, 5)));
        assert_eq!(first("a(b|c)d", "acd"), Some((0, 3)));
        assert_eq!(first("(?:ab)+", "abab"), Some((0, 4)));
        assert_eq!(first("x(|y)z", "xz"), Some((0, 2)));
        assert_eq!(
            groups(r"(\w+)@(\w+)(x)?", "me@home"),
            [Some("me@home".into()), Some("me".into()), Some("home".into()), None]
        );
        assert_eq!(first("a|ab", "ab"), Some((0, 1)), "first alternative wins");
    }

    #[test]
    fn quantifiers() {
        assert_eq!(first("ab*", "abbbc"), Some((0, 4)));
        assert_eq!(first("ab+", "ac ab"), Some((3, 5)));
        assert_eq!(first("colou?r", "color"), Some((0, 5)));
        assert_eq!(first("a{2}", "aaaa"), Some((0, 2)));
        assert_eq!(first("a{2,}", "aaaa"), Some((0, 4)));
        assert_eq!(first("a{,2}", "aaaa"), Some((0, 2)));
        assert_eq!(first("a{1,3}", "aaaa"), Some((0, 3)));
        assert_eq!(first("<.*>", "<a><b>"), Some((0, 6)));
        assert_eq!(first("<.*?>", "<a><b>"), Some((0, 3)));
        assert_eq!(first("a+?", "aaa"), Some((0, 1)));
        assert_eq!(first("a??b", "ab"), Some((0, 2)));
        assert_eq!(first("a{2,3}?", "aaaa"), Some((0, 2)));
        assert_eq!(
            first("a{x}", "a{x}"),
            Some((0, 4)),
            "a brace that isn't a count is literal"
        );
        assert_eq!(all("x*", "ab"), [(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn escapes() {
        assert_eq!(first(r"a\.b", "axb a.b"), Some((4, 7)));
        assert_eq!(first(r"\(\)", "f()"), Some((1, 3)));
        assert_eq!(first(r"\t", "a\tb"), Some((1, 2)));
        assert_eq!(first(r"\\", r"a\b"), Some((1, 2)));
        let tricky = "a.b*(c)[d]{e}|f+?^$\\";
        assert_eq!(first(&escape(tricky), tricky), Some((0, tricky.chars().count())));
    }

    #[test]
    fn case() {
        let mut re = Regex::new("foo").unwrap();
        assert!(!re.is_match("FOO"));
        re.set_case(true, true);
        assert!(re.is_match("FOO"));
        let mut re = Regex::new("Foo").unwrap();
        re.set_case(true, true);
        assert!(!re.is_match("foo"), "smart case");
        re.set_case(true, false);
        assert!(re.is_match("foo"));
        let mut re = Regex::new(r"\W\S\D").unwrap();
        re.set_case(true, true);
        assert!(re.ignore_case, "escapes aren't capitals");
        let mut re = Regex::new(r"Foo\c").unwrap();
        re.set_case(false, false);
        assert!(re.is_match("fOO"));
        let mut re = Regex::new(r"\Cfoo").unwrap();
        re.set_case(true, false);
        assert!(!re.is_match("FOO"));
        let mut re = Regex::new("[a-c]+").unwrap();
        re.set_case(true, true);
        assert_eq!(re.spans("xABC"), [(1, 4)]);
        let mut re = Regex::new("straße").unwrap();
        re.set_case(true, true);
        assert!(re.is_match("STRAßE"));
    }

    #[test]
    fn bad_patterns_are_errors() {
        for bad in [
            "(a", "a)", "[ab", "*a", "a**", "+", "a{3,1}", "\\", "a{5000}", "[z-a]", "(?:x",
        ] {
            assert!(Regex::new(bad).is_err(), "{bad}");
        }
        assert!(Regex::new("(a{1000}){1000}").is_err(), "too big");
    }

    #[test]
    fn catastrophic_patterns_finish() {
        let hay = "a".repeat(30);
        assert!(!Regex::new("(a*)*b").unwrap().is_match(&hay));
        assert!(!Regex::new("(a|aa)+c").unwrap().is_match(&hay));
        assert!(!Regex::new("(x+x+)+y").unwrap().is_match(&"x".repeat(5000)));
        assert!(Regex::new("(a*)*").unwrap().is_match(&hay));
        assert_eq!(first("(a|a)*$", &hay), Some((0, 30)));
    }
}
