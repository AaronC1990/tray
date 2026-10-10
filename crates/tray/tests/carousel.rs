use slot_input::{Btn, RawEvent};
use tray::carousel::{Carousel, Pick};

fn tap(c: &mut Carousel, b: Btn, at: u64) -> Option<Pick> {
    let got = c.handle(RawEvent::Down(b), at);
    c.handle(RawEvent::Up(b), at + 10);
    got
}

#[test]
fn a_plays_the_picked_disc() {
    let mut c = Carousel::new(6, 0);
    tap(&mut c, Btn::Right, 0);
    tap(&mut c, Btn::Right, 20);
    assert_eq!(tap(&mut c, Btn::A, 40), Some(Pick::Play(2)));
}

#[test]
fn a_long_shelf_wraps_both_ways() {
    let mut c = Carousel::new(6, 0);
    tap(&mut c, Btn::Left, 0);
    assert_eq!(c.selected(), 5);
    tap(&mut c, Btn::Right, 20);
    tap(&mut c, Btn::Right, 40);
    assert_eq!(c.selected(), 1);
}

#[test]
fn a_short_shelf_stops_at_its_ends() {
    let mut c = Carousel::new(2, 0);
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
fn menu_and_start_quit_but_start_alone_does_not() {
    let mut c = Carousel::new(3, 0);
    assert_eq!(tap(&mut c, Btn::Start, 0), None);
    c.handle(RawEvent::Down(Btn::Menu), 10);
    assert_eq!(c.handle(RawEvent::Down(Btn::Start), 20), Some(Pick::Quit));
}

#[test]
fn an_empty_shelf_plays_nothing() {
    let mut c = Carousel::new(0, 0);
    assert_eq!(tap(&mut c, Btn::A, 0), None);
    tap(&mut c, Btn::Right, 20);
    assert!(c.placed(2.2).is_empty());
}

#[test]
fn a_held_direction_repeats_after_a_pause() {
    let mut c = Carousel::new(20, 0);
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
    let mut c = Carousel::new(8, 0);
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
    let mut c = Carousel::new(8, 0);
    tap(&mut c, Btn::Left, 0);
    c.tick(20, 1.0 / 60.0);
    let placed = c.placed(2.2);
    let (_, d) = placed.iter().find(|p| p.0 == 7).unwrap();
    assert!(
        *d < 0.0 && *d > -1.0,
        "disc 7 should come in from the left: {d}"
    );
}
