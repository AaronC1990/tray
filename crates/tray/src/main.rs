//! tray's shelf: the PlayStation discs on the card in a row. Pick one with
//! A to play it; MENU and START together quit a game back to the shelf, and
//! quit the shelf back to slot.
use std::path::PathBuf;
use std::time::Instant;

use slot::input::DeviceInput;
use slot_gfx::{FbdevSurface, Overlay, PicturePass, Surface};
use slot_input::InputSource;
use tray::carousel::{Carousel, Pick};
use tray::library::scan;
use tray::play::{play, Console};
use tray::shelf::ShelfView;

const CARD: &str = "/mnt/sdcard";
const GAMES: &str = "Games/PS";
const LABELS: &str = "Labels/PS";

fn main() {
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let games = scan(&root.join(GAMES));
    for g in &games {
        eprintln!("tray: found {}", g.path.display());
    }

    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => return eprintln!("tray: display: {e}"),
    };
    let panel = surface.window_size();
    eprintln!("tray: panel {}x{}, {} games", panel.0, panel.1, games.len());
    let mut picture = match PicturePass::new(&surface) {
        Ok(p) => p,
        Err(e) => return eprintln!("tray: picture: {e}"),
    };
    let mut overlay = match Overlay::new(&surface, panel) {
        Ok(o) => o,
        Err(e) => return eprintln!("tray: overlay: {e}"),
    };
    let mut view = ShelfView::new(&mut overlay, panel, &root.join(LABELS));
    let mut carousel = Carousel::new(games.len(), 0);
    let mut input = DeviceInput::open(&root);
    let started = Instant::now();
    let mut period = None;
    let mut last = Instant::now();
    loop {
        let now_ms = started.elapsed().as_millis() as u64;
        let mut pick = None;
        for ev in input.poll(now_ms) {
            pick = pick.or(carousel.handle(ev, now_ms));
        }
        match pick {
            Some(Pick::Quit) => return eprintln!("tray: back to slot"),
            Some(Pick::Play(i)) => {
                let mut console = Console {
                    surface: &mut surface,
                    picture: &mut picture,
                    input: &mut input,
                    started,
                    panel,
                    period,
                };
                if let Err(e) = play(&mut console, &root, &games[i].path) {
                    eprintln!("tray: {}: {e}", games[i].path.display());
                }
                period = console.period;
                carousel.release_all();
                last = Instant::now();
                continue;
            }
            None => {}
        }
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        carousel.tick(started.elapsed().as_millis() as u64, dt);
        view.tick(dt, (!games.is_empty()).then(|| carousel.selected()));
        view.draw(&mut overlay, &games, &carousel);
        if let Err(e) = surface.swap() {
            eprintln!("tray: swap: {e}");
        }
    }
}
