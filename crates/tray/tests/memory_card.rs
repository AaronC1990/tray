//! A game's memory card outlives the game. PCSX ReARMed hands card 1 to the
//! frontend to keep, so if tray never wrote it, every save would be lost on
//! quitting.
mod common;

use slot::audio::StubSink;
use slot_gfx::{HeadlessSurface, PicturePass};
use tray::library::scan;
use tray::play::card_path;

#[test]
fn the_memory_card_is_written_on_quitting_and_loaded_next_time() {
    let Some(d) = common::card_with(&["testcard.exe"]) else {
        return;
    };
    let games = scan(&d.path().join("Games/PS"));
    let surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();
    let mut sink = StubSink::draining();
    let play = |picture: &mut PicturePass, sink: &mut StubSink| {
        let mut game = common::start(d.path(), &games[0], sink);
        common::run(&mut game, picture, 30);
        game.finish(sink);
    };

    play(&mut picture, &mut sink);
    let card = card_path(d.path(), &games[0].stem);
    let fresh = std::fs::read(&card).expect("quitting left no memory card");
    assert_eq!(fresh.len(), 128 * 1024, "a PlayStation card is 128 KiB");

    // Mark the card as a save would, and check the mark survives a game.
    let mut marked = fresh.clone();
    marked[0x2000..0x2008].copy_from_slice(b"traysave");
    std::fs::write(&card, &marked).unwrap();
    play(&mut picture, &mut sink);
    let after = std::fs::read(&card).unwrap();
    assert_eq!(
        &after[0x2000..0x2008],
        b"traysave",
        "the card was not loaded back"
    );
}
