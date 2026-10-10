//! Fast forward on the real core: several frames a swap, at the speed
//! asked for, with the sound sped up or kept quiet as slot's settings say.
mod common;

use slot::audio::{AudioSink, StubSink};
use slot_gfx::{HeadlessSurface, PicturePass};
use slot_retro::ButtonMask;
use tray::library::scan;
use tray::play::Fast;

#[test]
fn fast_forward_runs_several_frames_a_swap_and_quiet_when_asked() {
    let Some(d) = common::card_with(&["testcard.exe"]) else {
        return;
    };
    let games = scan(&d.path().join("Games/PS"));
    let surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();
    let mut sink = StubSink::draining();
    let mut game = common::start(d.path(), &games[0], &mut sink);
    common::run(&mut game, &mut picture, 10);
    assert_eq!(game.fast_frames(), 0, "fast without being asked");

    let quiet = Fast {
        speed: 4,
        sound: false,
    };
    for _ in 0..20 {
        game.frame(&mut picture, 100, ButtonMask(0), Some(quiet));
    }
    // As many as fit before the next swap, never more than four a swap.
    let extra = game.fast_frames();
    assert!(extra > 0, "fast forward ran no extra frames");
    assert!(
        extra <= 20 * 3,
        "ran {extra} extra frames, past four a swap"
    );
    assert!(sink.ring().muted(), "quiet fast forward was heard");

    common::run(&mut game, &mut picture, 1);
    assert!(
        !sink.ring().muted(),
        "the sound stayed off after fast forward"
    );
    game.finish(&mut sink);
}

#[test]
fn frames_are_planned_to_fill_whole_refreshes() {
    use tray::play::plan_frames;
    let period = 1.0 / 59.5;
    // A light game runs all the speed asks for in one refresh.
    assert_eq!(plan_frames(4, 0.003, 0.002, period), 4);
    // A heavy one, at 8.7 ms a frame: two frames need two refreshes, the
    // same as three, so three; five fit in three refreshes for more again.
    assert_eq!(plan_frames(3, 0.0087, 0.002, period), 3);
    assert_eq!(plan_frames(6, 0.0087, 0.002, period), 5);
    // A frame that already spills into a second refresh leaves room for
    // another in the third.
    assert_eq!(plan_frames(2, 0.016, 0.002, period), 2);
    // Nothing known yet: one frame.
    assert_eq!(plan_frames(4, 0.0, 0.002, period), 1);
}
