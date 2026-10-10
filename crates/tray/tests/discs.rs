//! Games on more than one disc, played with the real core on two copies of
//! the test card built as CD images.
mod common;

use slot::audio::StubSink;
use slot_gfx::{HeadlessSurface, PicturePass};
use tray::library::scan;
use tray::play::{card_path, last_disc};

const DISCS: [&str; 2] = ["testcard (Disc 1).iso", "testcard (Disc 2).iso"];

#[test]
fn discs_change_mid_game_and_the_game_starts_on_its_last_disc() {
    let Some(d) = common::card_with(&DISCS) else {
        return;
    };
    let root = d.path();
    let games = scan(&root.join("Games/PS"));
    assert_eq!(games.len(), 1, "the two discs are one game");
    let game = &games[0];
    let surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();
    let mut sink = StubSink::draining();

    let mut play = common::start(root, game, &mut sink);
    assert_eq!(play.disc_count(), 2);
    assert_eq!(play.disc(), 0);
    common::run(&mut play, &mut picture, 120);
    assert!(picture.size().0 >= 256, "the first disc never drew");

    play.set_lid_open(true);
    common::run(&mut play, &mut picture, 20);
    play.set_disc(1);
    common::run(&mut play, &mut picture, 20);
    play.set_lid_open(false);
    common::run(&mut play, &mut picture, 120);
    assert_eq!(play.disc(), 1);
    play.finish(&mut sink);
    assert_eq!(
        last_disc(root, &game.stem),
        1,
        "the disc in was not remembered"
    );
    assert!(
        card_path(root, &game.stem).is_file(),
        "the discs have no shared card"
    );

    let mut again = common::start(root, game, &mut sink);
    assert_eq!(
        again.disc(),
        1,
        "the game did not start on the disc it was left on"
    );
    common::run(&mut again, &mut picture, 120);
    again.finish(&mut sink);
}
