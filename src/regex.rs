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
        }
        Ok(Node::Concat(nodes))
    }

    fn atom(&mut self) -> Result<Node, String> {
        let c = self.next().unwrap_or_default();
        Ok(match c {
        })
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
