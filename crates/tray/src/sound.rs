//! The drive's sounds, made in code: the motor pulling a disc in and the
//! click as it seats, the disc spinning up to be read, and the motor
//! pushing it back out. Stereo, interleaved, at whatever rate the sound is
//! played at.
use std::f32::consts::TAU;

/// A small, steady noise source, the same every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Noise smoothed toward its low end, `k` from 0 (still) to 1 (raw).
struct Rumble {
    noise: Noise,
    level: f32,
    k: f32,
}

impl Rumble {
    fn new(seed: u32, k: f32) -> Rumble {
        Rumble {
            noise: Noise(seed),
            level: 0.0,
            k,
        }
    }

    fn next(&mut self) -> f32 {
        self.level += (self.noise.next() - self.level) * self.k;
        self.level
    }
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rises over `attack` seconds and falls over the last `release` of `len`.
fn envelope(t: f32, len: f32, attack: f32, release: f32) -> f32 {
    smooth(0.0, attack, t) * (1.0 - smooth(len - release, len, t))
}

/// A motor's buzz at `freq`: a few harmonics, falling off, as a small DC
/// motor and its gears sound.
fn buzz(phase: f32) -> f32 {
    (1..=6)
        .map(|k| (phase * k as f32).sin() / k as f32)
        .sum::<f32>()
        * 0.45
}

/// A click: a sharp tick that rings briefly, over a soft thump, `t`
/// seconds after it.
fn click(t: f32, loud: f32) -> f32 {
    if !(0.0..0.08).contains(&t) {
        return 0.0;
    }
    let tick = (TAU * 2300.0 * t).sin() * (-t * 140.0).exp();
    let thump = (TAU * 70.0 * t).sin() * (-t * 45.0).exp();
    loud * (0.6 * tick + 0.8 * thump)
}

/// Turns a mono signal of `len` seconds into stereo 16-bit samples.
fn render(rate: u32, len: f32, mut at: impl FnMut(f32) -> f32) -> Vec<i16> {
    let n = (rate as f32 * len) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = (at(i as f32 / rate as f32).clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&[s, s]);
    }
    out
}

/// How long the motor takes to pull a disc in; it seats, with a click, at
/// `seat` seconds, when the picture shows it seated.
pub fn pull_in(rate: u32, seat: f32) -> Vec<i16> {
    let len = seat + 0.12;
    let mut rattle = Rumble::new(0x9e37_79b9, 0.35);
    let mut phase = 0.0f32;
    render(rate, len, |t| {
        // The motor winds up a little as it takes the disc.
        let freq = 95.0 + 55.0 * smooth(0.0, seat, t);
        phase += TAU * freq / rate as f32;
        let motor = envelope(t, seat + 0.04, 0.03, 0.05);
        let gears = rattle.next() * (0.6 + 0.4 * (phase * 0.5).sin());
        0.22 * motor * (buzz(phase) + 0.35 * gears) + click(t - seat, 0.55)
    })
}

/// The disc spinning up to be read: a whine rising in pitch under a hiss
/// of air, with the head seeking twice near the end.
pub fn spin_up(rate: u32) -> Vec<i16> {
    let len = 1.1;
    let mut air = Rumble::new(0x85eb_ca6b, 0.08);
    let mut phase = 0.0f32;
    render(rate, len, |t| {
        let u = t / len;
        let freq = 160.0 * (5.5f32).powf(smooth(0.0, 0.8, u));
        phase += TAU * freq / rate as f32;
        let level = envelope(t, len, 0.15, 0.45);
        let whine = phase.sin() + 0.3 * (phase * 2.0).sin();
        let hiss = air.next() * smooth(0.0, 0.6, u);
        let seeks = click(t - 0.62, 0.18) + click(t - 0.74, 0.14);
        level * (0.11 * whine + 0.18 * hiss) + seeks
    })
}

/// The motor pushing a disc out after it spins down for `down` seconds,
/// taking `out` seconds and ending in a light click as it lets go.
pub fn push_out(rate: u32, down: f32, out: f32) -> Vec<i16> {
    let len = down + out + 0.1;
    let mut rattle = Rumble::new(0xc2b2_ae35, 0.35);
    let (mut whine_phase, mut motor_phase) = (0.0f32, 0.0f32);
    render(rate, len, |t| {
        // The disc slowing from its spin.
        let slow = 1.0 - smooth(0.0, down, t);
        whine_phase += TAU * (180.0 + 520.0 * slow) / rate as f32;
        let whine = 0.07 * slow * smooth(0.0, 0.02, t) * whine_phase.sin();
        // Then the motor, winding down as the disc leaves.
        let m = t - down;
        let freq = 150.0 - 50.0 * smooth(0.0, out, m);
        motor_phase += TAU * freq / rate as f32;
        let motor = envelope(m, out, 0.03, 0.06) * (m >= 0.0) as u8 as f32;
        let gears = rattle.next() * (0.6 + 0.4 * (motor_phase * 0.5).sin());
        whine + 0.22 * motor * (buzz(motor_phase) + 0.35 * gears) + click(m - out, 0.3)
    })
}

/// A drive sound, as the disc animations ask for them.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Sound {
    PullIn,
    SpinUp,
    PushOut,
}

/// Seconds the disc animations give each part of a sound: how long a disc
/// takes to go in or come out, and how long a disc spins down before it
/// starts out.
pub const IN: f32 = crate::shelf::IN_S;
pub const OUT: f32 = crate::shelf::OUT_S;
pub const DOWN: f32 = 0.12;

impl Sound {
    /// The sound at `rate`, stereo.
    pub fn render(self, rate: u32) -> Vec<i16> {
        match self {
            Sound::PullIn => pull_in(rate, IN),
            Sound::SpinUp => spin_up(rate),
            Sound::PushOut => push_out(rate, DOWN, OUT),
        }
    }
}
