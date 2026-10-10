//! Save states for disc games, through the snapshot slot keeps them with,
//! on the real core.
mod common;

use slot::audio::StubSink;
use slot_gfx::{HeadlessSurface, PicturePass};
use tray::library::scan;
use tray::play::thumbnail;

#[test]
fn a_state_saved_mid_game_loads_back_and_resumes_a_new_start() {
    let Some(d) = common::card_with(&["testcard (Disc 1).iso"]) else {
        return;
    };
    // The host build of the core recompiles with Lightrec, which crashes
    // running on after loading a state here; the interpreter shows the
    // states themselves are sound. The device build, with its own
    // recompiler, saves and loads states cleanly (ps1_boot with PS1_STATE
    // under qemu-aarch64 checks that).
    std::env::set_var("TRAY_CORE_OPTIONS", "pcsx_rearmed_drc=disabled");
    let games = scan(&d.path().join("Games/PS"));
    let surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();
    let mut sink = StubSink::draining();

    let mut game = common::start(d.path(), &games[0], &mut sink);
    common::run(&mut game, &mut picture, 60);
    let snap = game.snapshot();
    let state = snap.state().expect("the core gave no state");
    assert!(
        state.len() > 1024 * 1024,
        "a PlayStation state holds its RAM"
    );
    let thumb = snap.thumb().expect("no thumbnail");
    assert_eq!(&thumb[1..4], b"PNG");
    common::run(&mut game, &mut picture, 60);
    assert_ne!(snap.state().unwrap(), state, "the game did not move on");
    snap.load(state.clone());
    common::run(&mut game, &mut picture, 30);
    let back = snap.state().unwrap();
    assert_eq!(back.len(), state.len());
    drop(snap);
    game.finish(&mut sink);
    let mut again = common::resume(d.path(), &games[0], &mut sink, Some(state));
    common::run(&mut again, &mut picture, 10);
    again.finish(&mut sink);
}

#[test]
fn a_thumbnail_fills_a_game_boy_advance_screen() {
    // A 4:3 frame cropped to 3:2: the middle row survives, the top does not.
    let (w, h) = (320u32, 240u32);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    for x in 0..w {
        let at = ((120 * w + x) * 4) as usize;
        frame[at..at + 4].copy_from_slice(&[255, 255, 255, 0]);
    }
    let png = thumbnail(&frame, w, h).expect("no png");
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (240, 160));
    let mid = (80 * 240 * 3) as usize;
    assert_eq!(&buf[mid..mid + 3], &[255, 255, 255]);
}
