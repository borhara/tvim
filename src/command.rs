use tinsel::app::Key;

use crate::motion::Find;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    LineStart,
    FirstNonBlank,
    LineEnd,
    LastNonBlank,
    WordForward(bool),
    WordBack(bool),
    WordEnd(bool),
    WordEndBack(bool),
    FirstLine,
    LastLine,
    Find(Find),
    RepeatFind(bool),
    Bracket,
    ParagraphForward,
    ParagraphBack,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    NextLine,
    PrevLine,
    CountLine,
    Column,
    SearchNext(bool),
    Mark(char, bool),
    Hunk(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Delete,
    Change,
    Yank,
    Indent,
    Dedent,
    Lower,
    Upper,
    ToggleCase,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Motion(Motion),
    Lines,
    Object { kind: char, inner: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Move(Motion),
    Operate(Op, Target),
    Key(Key),
    With(char, char),
    G(char),
    Z(Key),
    Select { kind: char, inner: bool },
    Leader(char),
    Hunk(char),
    Window(Key),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cmd {
    pub register: Option<char>,
    pub count: Option<usize>,
    pub act: Act,
    pub keys: Vec<Key>,
}

impl Cmd {
    pub fn n(&self) -> usize {
        self.count.unwrap_or(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parse {
    More,
    Invalid,
    Done(Cmd),
}

pub fn is_register(c: char) -> bool {
    c.is_ascii_alphanumeric() || "\"-_+*/.".contains(c)
}

fn count(keys: &[Key]) -> (Option<usize>, usize) {
    let mut n: Option<usize> = None;
    let mut used = 0;
    for k in keys {
        match k {
            Key::Char(c @ '0'..='9') if n.is_some() || *c != '0' => {
                let d = *c as usize - '0' as usize;
                n = Some(n.unwrap_or(0).saturating_mul(10).saturating_add(d).min(999_999));
                used += 1;
            }
            _ => break,
        }
    }
    (n, used)
}

pub enum MotionParse {
    More,
    Invalid,
    Done(Motion, usize),
}

pub fn motion(keys: &[Key]) -> MotionParse {
    use Motion::*;
    let Some(&key) = keys.first() else {
        return MotionParse::More;
    };
    let one = |m| MotionParse::Done(m, 1);
    let with_char = |make: &dyn Fn(char) -> Motion| match keys.get(1) {
        None => MotionParse::More,
        Some(Key::Char(c)) => MotionParse::Done(make(*c), 2),
        Some(Key::Tab) => MotionParse::Done(make('\t'), 2),
        _ => MotionParse::Invalid,
    };
    match key {
        Key::Char('h') | Key::Left | Key::Backspace | Key::Ctrl('h') => one(Left),
        Key::Char('l') | Key::Right | Key::Char(' ') => one(Right),
        Key::Char('j') | Key::Down | Key::Ctrl('j') | Key::Ctrl('n') => one(Down),
        Key::Char('k') | Key::Up => one(Up),
        Key::Char('0') | Key::Home => one(LineStart),
        Key::Char('^') => one(FirstNonBlank),
        Key::Char('$') | Key::End => one(LineEnd),
        Key::Char('w') => one(WordForward(false)),
        Key::Char('W') => one(WordForward(true)),
        Key::Char('b') => one(WordBack(false)),
        Key::Char('B') => one(WordBack(true)),
        Key::Char('e') => one(WordEnd(false)),
        Key::Char('E') => one(WordEnd(true)),
        Key::Char('G') => one(LastLine),
        Key::Char('%') => one(Bracket),
        Key::Char('}') => one(ParagraphForward),
        Key::Char('{') => one(ParagraphBack),
        Key::Char('H') => one(ScreenTop),
        Key::Char('M') => one(ScreenMiddle),
        Key::Char('L') => one(ScreenBottom),
        Key::Enter | Key::Char('+') | Key::Ctrl('m') => one(NextLine),
        Key::Char('-') => one(PrevLine),
        Key::Char('_') => one(CountLine),
        Key::Char('|') => one(Column),
        Key::Char('n') => one(SearchNext(false)),
        Key::Char('N') => one(SearchNext(true)),
        Key::Char(';') => one(RepeatFind(false)),
        Key::Char(',') => one(RepeatFind(true)),
        Key::Char(c @ ('f' | 'F' | 't' | 'T')) => with_char(&|ch| {
            Find(crate::motion::Find {
                forward: c.is_ascii_lowercase(),
                till: c.eq_ignore_ascii_case(&'t'),
                ch,
            })
        }),
        Key::Char(c @ (']' | '[')) => match keys.get(1) {
            None => MotionParse::More,
            Some(Key::Char('c')) => MotionParse::Done(Hunk(c == ']'), 2),
            _ => MotionParse::Invalid,
        },
        Key::Char('\'') => with_char(&|c| Mark(c, false)),
        Key::Char('`') => with_char(&|c| Mark(c, true)),
        Key::Char('g') => match keys.get(1) {
            None => MotionParse::More,
            Some(Key::Char('g')) => MotionParse::Done(FirstLine, 2),
            Some(Key::Char('e')) => MotionParse::Done(WordEndBack(false), 2),
            Some(Key::Char('E')) => MotionParse::Done(WordEndBack(true), 2),
            Some(Key::Char('_')) => MotionParse::Done(LastNonBlank, 2),
            Some(Key::Char('0') | Key::Home) => MotionParse::Done(LineStart, 2),
            Some(Key::Char('$') | Key::End) => MotionParse::Done(LineEnd, 2),
            _ => MotionParse::Invalid,
        },
        _ => MotionParse::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::parse as k;

    fn cmd(spec: &str) -> Cmd {
        match parse(&k(spec), false) {
            Parse::Done(c) => c,
            other => panic!("{spec}: {other:?}"),
        }
    }

    fn state(spec: &str) -> Parse {
        parse(&k(spec), false)
    }

    #[test]
    fn motions_with_counts() {
        let c = cmd("12w");
        assert_eq!((c.count, c.act), (Some(12), Act::Move(Motion::WordForward(false))));
        assert_eq!(cmd("0").act, Act::Move(Motion::LineStart));
        assert_eq!(cmd("10j").count, Some(10));
        assert_eq!(cmd("gg").act, Act::Move(Motion::FirstLine));
        assert_eq!(state("g"), Parse::More);
        assert_eq!(state("f"), Parse::More);
        assert!(matches!(cmd("fx").act, Act::Move(Motion::Find(f)) if f.ch == 'x' && f.forward && !f.till));
    }

    #[test]
    fn operators_and_their_targets() {
        let c = cmd("2d3w");
        assert_eq!(c.count, Some(6));
        assert_eq!(
            c.act,
            Act::Operate(Op::Delete, Target::Motion(Motion::WordForward(false)))
        );
        assert_eq!(c.keys, k("dw"));
        assert_eq!(cmd("dd").act, Act::Operate(Op::Delete, Target::Lines));
        assert_eq!(cmd("gUU").act, Act::Operate(Op::Upper, Target::Lines));
        assert_eq!(cmd("gUgU").act, Act::Operate(Op::Upper, Target::Lines));
        assert_eq!(
            cmd("gUgg").act,
            Act::Operate(Op::Upper, Target::Motion(Motion::FirstLine))
        );
        assert_eq!(
            cmd("ci\"").act,
            Act::Operate(Op::Change, Target::Object { kind: '"', inner: true })
        );
        assert_eq!(cmd(">>").act, Act::Operate(Op::Indent, Target::Lines));
        assert_eq!(state("d"), Parse::More);
        assert_eq!(state("di"), Parse::More);
        assert_eq!(state("diz"), Parse::Invalid);
        assert_eq!(state("dz"), Parse::Invalid);
        assert_eq!(state("dg"), Parse::More);
    }

}
