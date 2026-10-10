use std::time::{Duration, Instant};

use slot::audio::Metronome;

const SPAN: Duration = Duration::from_micros(11_610); // 512 frames at 44.1 kHz

/// Runs `periods` periods where every sleep overshoots by `late`, and
/// returns the average time each took.
fn average_period(periods: u32, late: Duration) -> Duration {
    let start = Instant::now();
    let mut now = start;
    let mut m = Metronome::new(SPAN);
    for _ in 0..periods {
        let wait = m.wait(now);
        if !wait.is_zero() {
            now += wait + late;
        }
    }
    (now - start) / periods
}

#[test]
fn sleeps_that_run_long_do_not_slow_the_drain() {
    // A sleep a whole millisecond late would make a span-per-sleep drain
    // take 12.6 ms a period, nine percent slow.
    let avg = average_period(1000, Duration::from_millis(1));
    let off = avg.as_secs_f64() / SPAN.as_secs_f64() - 1.0;
    assert!(
        off.abs() < 0.002,
        "periods averaged {avg:?} against {SPAN:?}"
    );
}

#[test]
fn a_long_stall_restarts_the_schedule_instead_of_racing_to_catch_up() {
    let start = Instant::now();
    let mut m = Metronome::new(SPAN);
    m.wait(start);
    let after_stall = start + Duration::from_secs(2);
    let wait = m.wait(after_stall);
    assert_eq!(wait, SPAN, "it ran periods back to back to catch up");
}

#[test]
fn a_reset_starts_the_next_period_from_now() {
    let start = Instant::now();
    let mut m = Metronome::new(SPAN);
    m.wait(start);
    m.reset();
    let later = start + Duration::from_millis(500);
    assert_eq!(m.wait(later), SPAN);
}
