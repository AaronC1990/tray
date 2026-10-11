//! The drive's sounds: the right length, within range, and loud where
//! the picture says something happens.
use tray::sound::{pull_in, push_out, spin_up, Sound};

const RATE: u32 = 44_100;

fn peak(s: &[i16]) -> i16 {
    s.iter().map(|v| v.saturating_abs()).max().unwrap_or(0)
}

/// The loudest sample within `ms` either side of `at` seconds.
fn peak_near(s: &[i16], at: f32, ms: f32) -> i16 {
    let frame = |t: f32| ((t.max(0.0) * RATE as f32) as usize * 2).min(s.len());
    peak(&s[frame(at - ms / 1000.0)..frame(at + ms / 1000.0)])
}

#[test]
fn the_pull_in_clicks_as_the_disc_seats() {
    let s = pull_in(RATE, 0.45);
    assert_eq!(s.len(), ((0.45 + 0.12) * RATE as f32) as usize * 2);
    assert!(peak(&s) < 30_000, "the sound clips");
    let click = peak_near(&s, 0.452, 6.0);
    let motor = peak_near(&s, 0.2, 6.0);
    assert!(
        click > motor,
        "no click as the disc seats: {click} against {motor}"
    );
    assert_eq!(s[0], 0, "the sound starts with a jump");
}

#[test]
fn the_spin_up_rises_and_fades() {
    let s = spin_up(RATE);
    assert!(peak(&s) < 30_000);
    assert!(peak_near(&s, 0.4, 20.0) > peak_near(&s, 0.02, 20.0));
    assert!(peak_near(&s, 1.09, 5.0) < 500, "the spin-up stops short");
}

#[test]
fn the_push_out_ends_in_a_click_as_the_disc_lets_go() {
    let s = push_out(RATE, 0.12, 0.45);
    assert!(peak(&s) < 30_000);
    assert!(peak_near(&s, 0.12 + 0.45 + 0.002, 6.0) > 2_000);
}

#[test]
fn every_sound_renders_at_any_rate_and_none_at_no_rate() {
    for sound in [Sound::PullIn, Sound::SpinUp, Sound::PushOut] {
        assert!(!sound.render(32_768).is_empty());
        assert!(
            sound.render(0).is_empty(),
            "a sound with no device to play on"
        );
    }
}

#[test]
fn no_sound_starts_with_a_jump() {
    for sound in [Sound::PullIn, Sound::SpinUp, Sound::PushOut] {
        let s = sound.render(RATE);
        assert!(peak(&s[..20]) < 300, "{sound:?} starts with a jump");
    }
}
