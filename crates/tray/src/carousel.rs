//! The shelf's state: which disc is picked, where the row of discs is on
//! its way to, and what the buttons ask for.
use slot_input::{Btn, RawEvent};

/// How long a held direction waits before it repeats, and how often it
/// repeats after that.
const REPEAT_AFTER_MS: u64 = 320;
const REPEAT_EVERY_MS: u64 = 85;
/// How many discs L1 and R1 skip.
const JUMP: i64 = 5;
/// The row never trails the pick by more than this many discs, so a long
/// held scroll keeps up.
const MAX_LAG: f32 = 1.5;
/// Below this many discs the row stops at its ends, since wrapping would
/// show the same disc on both sides.
const WRAP_FROM: usize = 4;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Play(usize),
    Quit,
}

pub struct Carousel {
    len: usize,
    /// The pick, counted without wrapping, so the row slides the short way
    /// round when it wraps.
    target: i64,
    /// Where the row is, in discs; eases toward `target`.
    pos: f32,
    held: Option<(Btn, u64)>,
    menu: bool,
}

impl Carousel {
    pub fn new(len: usize, selected: usize) -> Self {
        let target = selected.min(len.saturating_sub(1)) as i64;
        Carousel {
            len,
            target,
            pos: target as f32,
            held: None,
            menu: false,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn wraps(&self) -> bool {
        self.len >= WRAP_FROM
    }

    pub fn selected(&self) -> usize {
        match self.len {
            0 => 0,
            n => self.target.rem_euclid(n as i64) as usize,
        }
    }

    pub fn step(&mut self, by: i64) {
        if self.len == 0 {
            return;
        }
        self.target = match self.wraps() {
            true => self.target + by,
            false => (self.target + by).clamp(0, self.len as i64 - 1),
        };
    }

    /// Takes one button event; returns what it asks for, if anything.
    pub fn handle(&mut self, ev: RawEvent, now_ms: u64) -> Option<Pick> {
        match ev {
            RawEvent::Down(Btn::Menu) => self.menu = true,
            RawEvent::Up(Btn::Menu) => self.menu = false,
            RawEvent::Down(Btn::Start) if self.menu => return Some(Pick::Quit),
            RawEvent::Down(Btn::A) if self.len > 0 => return Some(Pick::Play(self.selected())),
            RawEvent::Down(b @ (Btn::Left | Btn::Right)) => {
                self.step(if b == Btn::Left { -1 } else { 1 });
                self.held = Some((b, now_ms + REPEAT_AFTER_MS));
            }
            RawEvent::Down(Btn::L1) => self.step(-JUMP),
            RawEvent::Down(Btn::R1) => self.step(JUMP),
            RawEvent::Up(b) if self.held.is_some_and(|(h, _)| h == b) => self.held = None,
            _ => {}
        }
        None
    }

    /// Lets go of anything held, as when a game takes over the buttons.
    pub fn release_all(&mut self) {
        self.held = None;
        self.menu = false;
    }

    /// Repeats a held direction and moves the row `dt` seconds on.
    pub fn tick(&mut self, now_ms: u64, dt: f32) {
        while let Some((b, at)) = self.held {
            if now_ms < at {
                break;
            }
            self.step(if b == Btn::Left { -1 } else { 1 });
            self.held = Some((b, at + REPEAT_EVERY_MS));
        }
        let goal = self.target as f32;
        self.pos = self.pos.clamp(goal - MAX_LAG, goal + MAX_LAG);
        self.pos += (goal - self.pos) * (1.0 - (-dt * 14.0).exp());
        if (goal - self.pos).abs() < 0.002 {
            self.pos = goal;
        }
    }

    pub fn settled(&self) -> bool {
        self.pos == self.target as f32
    }

    /// The discs to draw: each game's index and how far it sits from the
    /// middle in discs, farthest first so nearer discs draw over them.
    pub fn placed(&self, reach: f32) -> Vec<(usize, f32)> {
        if self.len == 0 {
            return Vec::new();
        }
        let lo = (self.pos - reach).floor() as i64;
        let hi = (self.pos + reach).ceil() as i64;
        let mut out: Vec<(usize, f32)> = (lo..=hi)
            .filter(|i| self.wraps() || (0..self.len as i64).contains(i))
            .map(|i| (i.rem_euclid(self.len as i64) as usize, i as f32 - self.pos))
            .filter(|(_, d)| d.abs() <= reach)
            .collect();
        out.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        out
    }
}
