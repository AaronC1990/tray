//! A card with the real PlayStation core on it, for tests that play. The
//! host build of the core lives in vendor/; without it these tests skip.
#![allow(dead_code)]
use std::path::{Path, PathBuf};

use slot::audio::StubSink;
use slot_gfx::PicturePass;
use tray::library::Game;
use tray::play::{DiscGame, CORE};

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn testcard(name: &str) -> PathBuf {
    repo().join("tools/ps1-testcard").join(name)
}

/// A card holding the core and `discs` from the test card folder in
/// Games/PS, or None when the host core has not been built.
pub fn card_with(discs: &[&str]) -> Option<tempfile::TempDir> {
    let core = repo().join("vendor/pcsx_rearmed_libretro.host.so");
    if !core.is_file() {
        eprintln!("skipped: no host core in vendor/");
        return None;
    }
    let d = tempfile::tempdir().unwrap();
    for dir in ["System", "BIOS", "Games/PS"] {
        std::fs::create_dir_all(d.path().join(dir)).unwrap();
    }
    std::os::unix::fs::symlink(&core, d.path().join(CORE)).unwrap();
    for name in discs {
        std::fs::copy(testcard(name), d.path().join("Games/PS").join(name)).unwrap();
    }
    Some(d)
}

pub fn run(game: &mut DiscGame, picture: &mut PicturePass, frames: u32) {
    for _ in 0..frames {
        game.frame(picture, 100, slot_retro::ButtonMask(0));
    }
}

pub fn start(root: &Path, game: &Game, sink: &mut StubSink) -> DiscGame {
    resume(root, game, sink, None)
}

pub fn resume(root: &Path, game: &Game, sink: &mut StubSink, state: Option<Vec<u8>>) -> DiscGame {
    DiscGame::start(root, game, sink, None, state).expect("the game starts")
}
