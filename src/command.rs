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
    With(char, char),
    G(char),
    Select { kind: char, inner: bool },
    Leader(char),
    Hunk(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cmd {
    pub register: Option<char>,
    pub count: Option<usize>,
    pub act: Act,
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

pub enum MotionParse {
    More,
    Invalid,
    Done(Motion, usize),
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

}
