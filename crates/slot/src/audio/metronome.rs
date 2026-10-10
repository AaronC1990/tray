use std::time::{Duration, Instant};

/// How far behind the schedule may fall before it is given up and restarted
/// from now, rather than run fast to catch up.
const MAX_BEHIND: u32 = 8;

/// Keeps a run of periods to the wall clock. Each period's deadline is the
/// last one's plus a span, so time a sleep overshoots by is made up on the
/// next period instead of adding up, as sleeping a span each time would.
pub struct Metronome {
    span: Duration,
    next: Option<Instant>,
}

impl Metronome {
    pub fn new(span: Duration) -> Self {
        Metronome { span, next: None }
    }

    /// Forgets the schedule; the next period starts from whenever it is asked.
    pub fn reset(&mut self) {
        self.next = None;
    }

    /// How long to wait at `now` before the next period is due.
    pub fn wait(&mut self, now: Instant) -> Duration {
        let mut next = self.next.unwrap_or(now) + self.span;
        if now.saturating_duration_since(next) > self.span * MAX_BEHIND {
            next = now + self.span;
        }
        self.next = Some(next);
        next.saturating_duration_since(now)
    }
}
