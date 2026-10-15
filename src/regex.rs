const MAX_INSTS: usize = 20_000;
const MAX_REPEAT: u32 = 1000;

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
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, String> {
        let mut p = Parser {
            chars: pattern.chars().collect(),
            i: 0,
            groups: 0,
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
        })
    }

}

struct Parser {
    chars: Vec<char>,
    i: usize,
    groups: usize,
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
