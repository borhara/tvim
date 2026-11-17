use std::cell::{Cell, RefCell};

use crate::editor::Editor;

const FRAME: f32 = 1.0 / 60.0;
const PULSE: f32 = 0.4;
const SLIDE: f32 = 0.18;
pub(crate) const FADE: f32 = 0.15;
// scrolls this small just snap, so holding j or k never lags
const SNAP: f32 = 2.0;
const LENS_WAIT: f32 = 0.35;
const LENS_FADE: f32 = 0.25;
const OPEN: f32 = 0.2;

// a frame at time 0 (tests, snapshots) always draws settled
#[derive(Default)]
pub(crate) struct Anim {
    scroll: Glider,
    shown: Cell<usize>,
    landed: Cell<bool>,
    pulse: Cell<Option<(usize, f32)>>,
    zen: Cell<Option<(bool, f32)>>,
    slide: Cell<Option<Glide>>,
    text_x: Cell<usize>,
    busy_until: Cell<f32>,
    pub(crate) star: Glider,
    // the moment the terminal lost focus, while it's away
    paused: Cell<Option<f32>>,
    lens: RefCell<Option<(String, f32)>>,
}

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

impl Anim {
    pub(crate) fn reset(&self) {
        self.scroll.reset();
        self.pulse.set(None);
    }

    pub(crate) fn land(&self) {
        self.landed.set(true);
    }

    pub(crate) fn follow(&self, e: &Editor, t: f32) -> usize {
        let (top, rows) = (e.top(), e.text_rows());
        let on = e.options.smooth && t > 0.0;
        let shown = self.scroll.step(top, rows, on, t);
        let end = self.scroll.end();
        if self.landed.take() && on && !e.mode().is_visual() {
            let start = end.unwrap_or(t);
            self.pulse.set(Some((e.cursor().row, start)));
        }
        if !on {
            self.pulse.set(None);
        }
        self.shown.set(shown);
        let until = end.unwrap_or(0.0).max(self.pulse.get().map_or(0.0, |(_, s)| s + PULSE));
        self.pace(t, until);
        shown
    }

    pub(crate) fn shown(&self) -> usize {
        self.shown.get()
    }

    pub(crate) fn glow(&self, row: usize, t: f32) -> f32 {
        match self.pulse.get() {
            Some((r, start)) if r == row => {
                let left = 1.0 - ((t - start) / PULSE).clamp(0.0, 1.0);
                left * left
            }
            _ => 0.0,
        }
    }

    pub(crate) fn zen(&self, zen: bool, t: f32) -> f32 {
        let since = match self.zen.get() {
            Some((was, at)) if was == zen => at,
            Some(_) if t > 0.0 => {
                self.slide.set(zen.then(|| Glide {
                    from: self.text_x.get() as f32,
                    to: 0.0,
                    start: t - FRAME,
                    length: SLIDE,
                }));
                self.pace(t, t + SLIDE.max(FADE));
                t
            }
            _ => f32::NEG_INFINITY,
        };
        self.zen.set(Some((zen, since)));
        if t > 0.0 { t - since } else { f32::INFINITY }
    }

    pub(crate) fn column_x(&self, x: usize, t: f32) -> usize {
        match self.slide.get().filter(|g| t > 0.0 && t < g.end()) {
            Some(g) => Glide { to: x as f32, ..g }.at(t).round().max(0.0) as usize,
            None => x,
        }
    }

    pub(crate) fn lens(&self, key: &str, t: f32) -> f32 {
        if t <= 0.0 {
            return 1.0;
        }
        let mut seen = self.lens.borrow_mut();
        let since = match seen.as_ref() {
            Some((k, at)) if k == key => *at,
            _ => {
                *seen = Some((key.to_string(), t));
                t
            }
        };
        self.pace(t, since + LENS_WAIT + LENS_FADE);
        ((t - since - LENS_WAIT) / LENS_FADE).clamp(0.0, 1.0)
    }

    pub(crate) fn lens_gone(&self) {
        self.lens.replace(None);
    }

    pub(crate) fn opening(&self, opened: Option<f32>, t: f32) -> f32 {
        let Some(at) = opened.filter(|_| t > 0.0) else {
            return 1.0;
        };
        self.pace(t, at + OPEN);
        let p = ((t - at) / OPEN).clamp(0.0, 1.0);
        1.0 - (1.0 - p).powi(3)
    }

    pub(crate) fn text_at(&self, x: usize) {
        self.text_x.set(x);
    }

    pub(crate) fn pace(&self, t: f32, until: f32) {
        if until > t {
            self.busy_until.set(self.busy_until.get().max(until + FRAME));
        }
    }

    pub(crate) fn moving(&self, now: f32) -> bool {
        now < self.busy_until.get()
    }
}
