//! A game's memory card outlives the game. PCSX ReARMed hands card 1 to the
//! frontend to keep, so if tray never wrote it, every save would be lost on
//! quitting. Runs the real core on the test card when the host build of it
//! is in vendor/.
use std::path::{Path, PathBuf};

use slot::audio::StubSink;
use slot_gfx::{HeadlessSurface, PicturePass};
use tray::play::{card_path, DiscGame, CORE};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn play_a_little(root: &Path, disc: &Path, picture: &mut PicturePass) {
    let mut sink = StubSink::draining();
    let mut game = DiscGame::start(root, disc, &mut sink, None).expect("the game starts");
    for _ in 0..30 {
        game.frame(picture, 100);
    }
    game.finish(&mut sink);
}

#[test]
fn the_memory_card_is_written_on_quitting_and_loaded_next_time() {
    let core = repo().join("vendor/pcsx_rearmed_libretro.host.so");
    let disc = repo().join("tools/ps1-testcard/testcard.exe");
    if !core.is_file() || !disc.is_file() {
        eprintln!("skipped: no host core in vendor/");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("System")).unwrap();
    std::fs::create_dir_all(d.path().join("BIOS")).unwrap();
    std::os::unix::fs::symlink(&core, d.path().join(CORE)).unwrap();
    let surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();

    play_a_little(d.path(), &disc, &mut picture);
    let card = card_path(d.path(), &disc);
    let fresh = std::fs::read(&card).expect("quitting left no memory card");
    assert_eq!(fresh.len(), 128 * 1024, "a PlayStation card is 128 KiB");

    // Mark the card as a save would, and check the mark survives a game.
    let mut marked = fresh.clone();
    marked[0x2000..0x2008].copy_from_slice(b"traysave");
    std::fs::write(&card, &marked).unwrap();
    play_a_little(d.path(), &disc, &mut picture);
    let after = std::fs::read(&card).unwrap();
    assert_eq!(
        &after[0x2000..0x2008],
        b"traysave",
        "the card was not loaded back"
    );
}
