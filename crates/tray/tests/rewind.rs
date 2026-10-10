//! Rewind for disc games on the real core: states kept every few frames
//! while playing, played back in reverse, and the history running out.
mod common;

use slot::audio::{AudioSink, StubSink};
use slot_gfx::{HeadlessSurface, PicturePass};
use tray::library::scan;

#[test]
fn rewinding_plays_kept_states_back_until_they_run_out() {
    let Some(d) = common::card_with(&["testcard.exe"]) else {
        return;
    };
    // The host core's Lightrec crashes running on after loading a state;
    // the interpreter stands in (see the states test).
    std::env::set_var("TRAY_CORE_OPTIONS", "pcsx_rearmed_drc=disabled");
    let games = scan(&d.path().join("Games/PS"));
    let surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();
    let mut sink = StubSink::draining();
    let mut game = common::start(d.path(), &games[0], &mut sink);

    game.set_recording(true);
    common::run(&mut game, &mut picture, 40);
    // The rewind thread takes the states in its own time.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let before = game.snapshot().state().unwrap();

    // Forty frames at a state every four is ten states, one played back
    // every second swap.
    let mut swaps = 0;
    while game.rewind_step(&mut picture) {
        swaps += 1;
        assert!(swaps < 100, "rewind never ran out");
    }
    assert!((18..=22).contains(&swaps), "rewound for {swaps} swaps");
    assert!(sink.ring().muted(), "rewinding was heard");
    assert_eq!(game.rewind_fill(), 0, "history left after it ran out");
    assert_ne!(
        game.snapshot().state().unwrap(),
        before,
        "nothing went back"
    );

    // Playing on picks the sound back up and keeps new history.
    common::run(&mut game, &mut picture, 8);
    assert!(!sink.ring().muted());
    game.finish(&mut sink);
}
