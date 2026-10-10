//! The shelf's state: which disc is picked, where the row of discs is on
//! its way to, and what the buttons ask for.
use slot_input::{Btn, RawEvent};

/// How long a held direction waits before it repeats, and how often it
/// repeats after that.
const REPEAT_AFTER_MS: u64 = 320;
const REPEAT_EVERY_MS: u64 = 85;
/// The row never trails the pick by more than this many discs, so a long
/// held scroll keeps up.
const MAX_LAG: f32 = 1.5;
/// Below this many discs the row stops at its ends, since wrapping would
/// show the same disc on both sides.
const WRAP_FROM: usize = 4;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Play(usize),
    /// Move to the next system's shelf, -1 for left and 1 for right.
    Switch(i32),
    /// The pick jumped to another letter, which slot shows in its slot.
    Letter(char),
}

pub struct Carousel {
    len: usize,
    /// The pick, counted without wrapping, so the row slides the short way
    /// round when it wraps.
    target: i64,
    /// Where the row is, in discs; eases toward `target`.
    pos: f32,
    held: Option<(Btn, u64)>,
    /// Each disc's first letter, for Up and Down to jump between.
    initials: Vec<char>,
}

impl Carousel {
    /// A row of discs whose titles start with `initials`, in shelf order.
    pub fn new(initials: Vec<char>, selected: usize) -> Self {
        let len = initials.len();
        let target = selected.min(len.saturating_sub(1)) as i64;
        Carousel {
            len,
            target,
            pos: target as f32,
            held: None,
            initials,
        }
    }

    /// Moves to the first disc of the next letter, or of this letter's
    /// previous one going back, as slot's shelf does with Up and Down.
    fn jump_letter(&mut self, dir: i32) -> Option<Pick> {
        if self.len == 0 {
            return None;
        }
        let at = self.selected();
        let letter = |i: usize| self.initials[i];
        let start_of = |mut i: usize| {
            while i > 0 && letter(i - 1) == letter(i) {
                i -= 1;
            }
            i
        };
        let to = match dir > 0 {
            true => (at + 1..self.len).find(|&i| letter(i) != letter(at)),
            false => match start_of(at) {
                0 => None,
                g => Some(start_of(g - 1)),
            },
        };
        let to = match (to, self.wraps(), dir > 0) {
            (Some(to), _, _) => to,
            (None, false, _) => return None,
            (None, true, true) => 0,
            (None, true, false) => start_of(self.len - 1),
        };
        if to == at {
            return None;
        }
        // Slide the way the jump goes, even when it wraps round.
        let n = self.len as i64;
        let ahead = (to as i64 - at as i64).rem_euclid(n);
        self.step(if dir > 0 { ahead } else { ahead - n });
        Some(Pick::Letter(self.initials[to]))
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
            RawEvent::Down(Btn::A) if self.len > 0 => return Some(Pick::Play(self.selected())),
            RawEvent::Down(b @ (Btn::Left | Btn::Right)) => {
                self.step(if b == Btn::Left { -1 } else { 1 });
                self.held = Some((b, now_ms + REPEAT_AFTER_MS));
            }
            RawEvent::Down(Btn::L1) => return Some(Pick::Switch(-1)),
            RawEvent::Down(Btn::R1) => return Some(Pick::Switch(1)),
            RawEvent::Down(Btn::Up) => return self.jump_letter(-1),
            RawEvent::Down(Btn::Down) => return self.jump_letter(1),
            RawEvent::Up(b) if self.held.is_some_and(|(h, _)| h == b) => self.held = None,
            _ => {}
        }
        None
    }

    /// Lets go of anything held, as when a game takes over the buttons.
    pub fn release_all(&mut self) {
        self.held = None;
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
