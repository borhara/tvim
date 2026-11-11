use std::path::PathBuf;

use crate::buffer::Pos;

const MAX_JUMPS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    pub buffer: usize,
    pub path: Option<PathBuf>,
    pub pos: Pos,
}

#[derive(Clone, Debug, Default)]
pub struct JumpList {
    spots: Vec<Spot>,
    // spots.len() when not stepping through the list
    at: usize,
}

impl JumpList {
    pub fn push(&mut self, spot: Spot) {
        self.spots.truncate(self.at);
        self.add(spot);
    }

    fn add(&mut self, spot: Spot) {
        self.spots
            .retain(|s| !(s.buffer == spot.buffer && s.pos.row == spot.pos.row));
        self.spots.push(spot);
        if self.spots.len() > MAX_JUMPS {
            self.spots.remove(0);
        }
        self.at = self.spots.len();
    }

    pub fn back(&mut self, here: Spot, n: usize) -> Option<Spot> {
        if self.at >= self.spots.len() {
            self.add(here);
            self.at = self.spots.len() - 1;
        }
        self.at = self.at.checked_sub(n)?;
        Some(self.spots[self.at].clone())
    }

    pub fn forward(&mut self, n: usize) -> Option<Spot> {
        let to = self.at + n;
        if to >= self.spots.len() {
            return None;
        }
        self.at = to;
        Some(self.spots[to].clone())
    }

    pub fn has_newer(&self) -> bool {
        self.at + 1 < self.spots.len()
    }

    pub fn latest_elsewhere(&self, buffer: usize, row: usize) -> Option<Pos> {
        self.spots
            .iter()
            .rev()
            .find(|s| s.buffer == buffer && s.pos.row != row)
            .map(|s| s.pos)
    }

    pub fn len(&self) -> usize {
        self.spots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(row: usize) -> Spot {
        Spot {
            buffer: 1,
            path: None,
            pos: Pos::new(row, 0),
        }
    }

    fn rows(j: &JumpList) -> Vec<usize> {
        j.spots.iter().map(|s| s.pos.row).collect()
    }

    #[test]
    fn back_and_forward() {
        let mut j = JumpList::default();
        j.push(spot(1));
        j.push(spot(5));
        assert_eq!(j.back(spot(9), 1), Some(spot(5)), "remembers where it left from");
        assert_eq!(rows(&j), [1, 5, 9]);
        assert_eq!(j.back(spot(5), 1), Some(spot(1)));
        assert_eq!(j.back(spot(1), 1), None);
        assert_eq!(j.forward(2), Some(spot(9)));
        assert_eq!(j.forward(1), None);
        assert!(!j.has_newer());
    }

}
