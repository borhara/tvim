//! A list of places to go through one after another, as vim's quickfix list is: what a
//! search found, say, or the problems a language server sees.

use std::path::PathBuf;

use crate::buffer::Pos;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    pub path: PathBuf,
    pub pos: Pos,
    // what to say about it: the line found, or what's wrong there
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quickfix {
    pub title: String,
    pub spots: Vec<Spot>,
    // the one last gone to, None until one is
    pub at: Option<usize>,
}

impl Quickfix {
    pub fn new(title: &str, spots: Vec<Spot>) -> Quickfix {
        Quickfix {
            title: title.to_string(),
            spots,
            at: None,
        }
    }

    // `by` places on from the last one gone to, stopping at the ends, or None if already
    // there; going on from none gone to yet starts at the first
    pub fn step(&mut self, by: isize) -> Option<usize> {
        let last = self.spots.len().checked_sub(1)?;
        let to = match self.at {
            None if by > 0 => (by as usize - 1).min(last),
            None => return None,
            Some(at) => at.checked_add_signed(by)?.min(last),
        };
        if Some(to) == self.at {
            return None;
        }
        self.at = Some(to);
        Some(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize) -> Quickfix {
        let spot = |i| Spot {
            path: PathBuf::from("a"),
            pos: Pos::new(i, 0),
            note: String::new(),
        };
        Quickfix::new("t", (0..n).map(spot).collect())
    }

    #[test]
    fn stepping_stops_at_the_ends() {
        let mut q = list(3);
        assert_eq!(q.step(-1), None);
        assert_eq!(q.step(1), Some(0), "the first to begin with");
        assert_eq!(q.step(1), Some(1));
        assert_eq!(q.step(5), Some(2), "as far as there is");
        assert_eq!(q.step(1), None);
        assert_eq!(q.step(-2), Some(0));
        assert_eq!(list(0).step(1), None);
    }
}
