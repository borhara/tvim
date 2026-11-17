use std::cell::Cell;

const FRAME: f32 = 1.0 / 60.0;
// scrolls this small just snap, so holding j or k never lags
const SNAP: f32 = 2.0;

#[derive(Default)]
pub(crate) struct Glider {
    seen: Cell<Option<(usize, usize)>>,
    glide: Cell<Option<Glide>>,
}

impl Glider {
    pub(crate) fn reset(&self) {
        self.seen.set(None);
        self.glide.set(None);
    }

    pub(crate) fn step(&self, top: usize, rows: usize, on: bool, t: f32) -> usize {
        let before = self.seen.replace(Some((top, rows)));
        let running = self.glide.get().filter(|g| t < g.end());
        match before {
            Some((was, was_rows)) if on && was_rows == rows && was != top => {
                let from = running.map_or(was as f32, |g| g.at(t));
                let distance = (top as f32 - from).abs();
                self.glide.set((distance > SNAP).then(|| Glide {
                    from,
                    to: top as f32,
                    start: t - FRAME,
                    length: 0.12 + 0.06 * (distance / rows.max(1) as f32).min(1.0),
                }));
            }
            Some(_) if on => self.glide.set(running),
            _ => self.glide.set(None),
        }
        self.glide.get().map_or(top, |g| g.at(t).round().max(0.0) as usize)
    }

    pub(crate) fn end(&self) -> Option<f32> {
        self.glide.get().map(|g| g.end())
    }
}

#[derive(Clone, Copy, Debug)]
struct Glide {
    from: f32,
    to: f32,
    start: f32,
    length: f32,
}

impl Glide {
    fn end(&self) -> f32 {
        self.start + self.length
    }

    fn at(&self, t: f32) -> f32 {
        let p = ((t - self.start) / self.length).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - p).powi(3);
        self.from + (self.to - self.from) * eased
    }
}
