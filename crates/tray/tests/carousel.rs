use slot_input::{Btn, RawEvent};
use tray::carousel::{Carousel, Pick};

/// A row of `n` discs, each starting with its own letter.
fn row(n: usize) -> Vec<char> {
    ('A'..).take(n).collect()
}

fn tap(c: &mut Carousel, b: Btn, at: u64) -> Option<Pick> {
    let got = c.handle(RawEvent::Down(b), at);
    c.handle(RawEvent::Up(b), at + 10);
    got
}

#[test]
fn a_plays_the_picked_disc() {
    let mut c = Carousel::new(row(6), 0);
    tap(&mut c, Btn::Right, 0);
    tap(&mut c, Btn::Right, 20);
    c.handle(RawEvent::Down(Btn::A), 40);
    assert_eq!(
        c.handle(RawEvent::Up(Btn::A), 60),
        Some(Pick::Play(2, false))
    );
}

#[test]
fn a_long_shelf_wraps_both_ways() {
    let mut c = Carousel::new(row(6), 0);
    tap(&mut c, Btn::Left, 0);
    assert_eq!(c.selected(), 5);
    tap(&mut c, Btn::Right, 20);
    tap(&mut c, Btn::Right, 40);
    assert_eq!(c.selected(), 1);
}

#[test]
fn a_short_shelf_stops_at_its_ends() {
    let mut c = Carousel::new(row(2), 0);
    tap(&mut c, Btn::Left, 0);
    assert_eq!(c.selected(), 0);
    tap(&mut c, Btn::Right, 20);
    tap(&mut c, Btn::Right, 40);
    assert_eq!(c.selected(), 1);
    let shown: Vec<usize> = c.placed(2.2).iter().map(|p| p.0).collect();
    assert_eq!(
        shown.len(),
        2,
        "a short shelf shows each disc once: {shown:?}"
    );
}

#[test]
fn the_shoulders_ask_for_the_next_system() {
    let mut c = Carousel::new(row(3), 1);
    assert_eq!(tap(&mut c, Btn::L1, 0), Some(Pick::Switch(-1)));
    assert_eq!(tap(&mut c, Btn::R1, 20), Some(Pick::Switch(1)));
    assert_eq!(c.selected(), 1, "a shoulder moved the row");
}

#[test]
fn up_and_down_jump_between_letters() {
    let mut c = Carousel::new("AABBBCD".chars().collect(), 3);
    tap(&mut c, Btn::Down, 0);
    assert_eq!(c.selected(), 5, "down goes to the first C");
    tap(&mut c, Btn::Up, 20);
    assert_eq!(c.selected(), 2, "up goes to the first of the letter before");
    tap(&mut c, Btn::Up, 40);
    assert_eq!(c.selected(), 0);
    tap(&mut c, Btn::Up, 60);
    assert_eq!(
        c.selected(),
        6,
        "up from the first letter wraps to the last"
    );
    tap(&mut c, Btn::Down, 80);
    assert_eq!(
        c.selected(),
        0,
        "down from the last letter wraps to the first"
    );
}

#[test]
fn an_empty_shelf_plays_nothing() {
    let mut c = Carousel::new(row(0), 0);
    assert_eq!(tap(&mut c, Btn::A, 0), None);
    assert_eq!(c.held_play(5000), None);
    tap(&mut c, Btn::Right, 20);
    assert!(c.placed(2.2).is_empty());
}

#[test]
fn a_held_direction_repeats_after_a_pause() {
    let mut c = Carousel::new(row(20), 0);
    c.handle(RawEvent::Down(Btn::Right), 0);
    c.tick(300, 0.016);
    assert_eq!(c.selected(), 1, "repeated before the pause");
    c.tick(320 + 85 * 3, 0.016);
    assert_eq!(c.selected(), 5);
    c.handle(RawEvent::Up(Btn::Right), 700);
    c.tick(2000, 0.016);
    assert_eq!(c.selected(), 5, "kept going after release");
}

#[test]
fn the_row_slides_to_the_pick_and_settles_there() {
    let mut c = Carousel::new(row(8), 0);
    tap(&mut c, Btn::Right, 0);
    assert!(!c.settled());
    let mid = c.placed(2.2);
    assert!(
        mid.iter().any(|&(g, d)| g == 1 && d > 0.9),
        "jumped before sliding: {mid:?}"
    );
    for i in 0..60 {
        c.tick(100 + i * 16, 1.0 / 60.0);
    }
    assert!(c.settled());
    let near = c.placed(2.2);
    assert_eq!(
        near.last(),
        Some(&(1, 0.0)),
        "the pick is drawn last, in the middle"
    );
}

#[test]
fn wrapping_slides_the_short_way_round() {
    let mut c = Carousel::new(row(8), 0);
    tap(&mut c, Btn::Left, 0);
    c.tick(20, 1.0 / 60.0);
    let placed = c.placed(2.2);
    let (_, d) = placed.iter().find(|p| p.0 == 7).unwrap();
    assert!(
        *d < 0.0 && *d > -1.0,
        "disc 7 should come in from the left: {d}"
    );
}

#[test]
fn holding_a_starts_the_disc_fresh() {
    let mut c = Carousel::new(row(4), 1);
    c.handle(RawEvent::Down(Btn::A), 1000);
    assert_eq!(
        c.held_play(1200),
        None,
        "fresh before the hold was long enough"
    );
    assert_eq!(c.held_play(1500), Some(Pick::Play(1, true)));
    assert_eq!(c.handle(RawEvent::Up(Btn::A), 1600), None, "played twice");
}
