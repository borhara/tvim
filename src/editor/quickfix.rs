use super::Editor;
use crate::quickfix::{Quickfix, Spot};

impl Editor {
    // keeps `spots` as the quickfix list, to go through with ]q and [q
    pub(crate) fn keep_quickfix(&mut self, title: &str, spots: Vec<Spot>) {
        if spots.is_empty() {
            return self.error("nothing to keep");
        }
        let n = spots.len();
        self.quickfix = Some(Quickfix::new(title, spots));
        let places = if n == 1 { "1 place" } else { &format!("{n} places") };
        self.info(format!("{places} in the quickfix list, ]q for the next"));
    }

    pub fn quickfix(&self) -> Option<&Quickfix> {
        self.quickfix.as_ref()
    }

    // `by` places on in the quickfix list, or back for less than none
    pub(crate) fn step_quickfix(&mut self, by: isize) {
        let Some(q) = self.quickfix.as_mut() else {
            return self.error("E42: No Errors");
        };
        match q.step(by) {
            Some(i) => self.go_to_quickfix(i),
            None => self.error("E553: No more items"),
        }
    }

    // to the nth place in the quickfix list, counting from 0, or the last for None
    pub(crate) fn quickfix_to(&mut self, nth: Option<usize>) {
        let Some(q) = self.quickfix.as_mut() else {
            return self.error("E42: No Errors");
        };
        let last = q.spots.len() - 1;
        let at = nth.unwrap_or(last).min(last);
        q.at = Some(at);
        self.go_to_quickfix(at);
    }

    fn go_to_quickfix(&mut self, i: usize) {
        let Some(q) = &self.quickfix else { return };
        let (n, spot) = (q.spots.len(), q.spots[i].clone());
        if self.go_to(&spot.path, spot.pos) {
            self.info(format!("({} of {n}) {}", i + 1, spot.note));
        }
    }
}
