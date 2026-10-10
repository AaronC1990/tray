//! tray: slot's Game Boy shelves and a PlayStation disc shelf in one
//! frontend. L and R move between systems; slot runs its own shelves and
//! games exactly as it does alone, and tray runs the discs.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot::app::{set_outside_shelf, App, Phase};
use slot::frontend::Frontend;
use slot::input::DeviceInput;
use slot::pacer::{Pacer, MIN_FRAME, STEP_TIMEOUT};
use slot_gfx::{Compositor, Draw, FbdevSurface, Overlay, PicturePass, Surface};
use slot_input::{Btn, InputSource, RawEvent};
use slot_power::{trace_first_frame, DevicePlatform};
use tray::carousel::{Carousel, Pick};
use tray::library::{scan, Game};
use tray::play::{DiscGame, SAVES};
use tray::shelf::ShelfView;

const CARD: &str = "/mnt/sdcard";
const GAMES: &str = "Games/PS";
const LABELS: &str = "Labels/PS";
/// Which system was last on screen, so tray starts where it was left.
const LAST: &str = ".tray-last";
/// How long one screen takes to slide off and uncover the next.
const SLIDE: Duration = Duration::from_millis(280);

enum Mode {
    /// slot's shelves and games.
    Carts,
    Discs,
    Playing(Box<DiscGame>),
}

struct Slide {
    began: Instant,
    /// Which way the old screen leaves: -1 to the left, 1 to the right.
    dir: f32,
}

/// Buttons slot keeps whichever screen is up: power and the lid for doze
/// and the power menu, and volume, which slot sets for every game.
fn slot_keeps(ev: &RawEvent) -> bool {
    let (RawEvent::Down(b) | RawEvent::Up(b)) = ev;
    matches!(b, Btn::Power | Btn::Lid | Btn::VolUp | Btn::VolDown)
}

/// Whether slot has something of its own to show over any system: the
/// screen asleep, or its power menu.
fn slot_over(app: &App) -> bool {
    matches!(app.phase(), Phase::Doze { .. }) || app.power_menu().is_some()
}

fn last_path(root: &Path) -> PathBuf {
    root.join(SAVES).join(LAST)
}

/// The disc tray was last on, if it was last on the discs.
fn read_last(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(last_path(root)).ok()?;
    text.strip_prefix("discs\n")
        .map(|s| s.trim_end().to_string())
}

fn write_last(root: &Path, disc: Option<&Game>) {
    let text = match disc {
        Some(g) => format!("discs\n{}\n", g.stem),
        None => "carts\n".to_string(),
    };
    let _ = std::fs::create_dir_all(root.join(SAVES));
    let _ = std::fs::write(last_path(root), text);
}

/// Gives up with a failing status, which the launcher takes as its cue to
/// start slot on its own instead.
fn fail(why: String) -> ! {
    eprintln!("{why}");
    std::process::exit(1)
}

fn main() {
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let games = scan(&root.join(GAMES));
    eprintln!("tray: {} discs", games.len());
    set_outside_shelf(!games.is_empty());

    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => fail(format!("tray: display: {e}")),
    };
    let panel = surface.window_size();
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => fail(format!("tray: {e}")),
    };
    let platform = DevicePlatform::new(root.clone());
    eprintln!("tray: {}", platform.report());
    platform.trace_boot();
    let mut frontend = Frontend::boot(Box::new(platform));
    compositor.fit(panel);
    frontend.upload_faces(&mut compositor);
    frontend.upload_bezel(&mut compositor, panel);
    let mut picture = match PicturePass::new(&surface) {
        Ok(p) => p,
        Err(e) => fail(format!("tray: picture: {e}")),
    };
    let mut overlay = match Overlay::new(&surface, panel) {
        Ok(o) => o,
        Err(e) => fail(format!("tray: overlay: {e}")),
    };
    let mut view = ShelfView::new(&mut overlay, panel, &root.join(LABELS));
    let initials = games
        .iter()
        .map(|g| slot_store::initial(&g.title))
        .collect();
    let last = read_last(&root);
    let picked = last
        .as_deref()
        .and_then(|stem| games.iter().position(|g| g.stem == stem))
        .unwrap_or(0);
    let mut carousel = Carousel::new(initials, picked);
    let mut input = DeviceInput::open(&root);
    let card = root.clone();
    let _ = std::thread::Builder::new()
        .name("slot-bootlogo".into())
        .spawn(move || slot::bootlogo::refresh(&card));
    frontend.drive_emulator();

    // Start on the discs if that is where tray was left, unless slot is
    // resuming a cart or asking for the clock; and always if there are no
    // carts to show.
    let on_shelf = matches!(frontend.app().phase(), Phase::Shelf);
    let mut mode = match (games.is_empty(), frontend.app().has_carts()) {
        (true, _) => Mode::Carts,
        (false, false) => Mode::Discs,
        (false, true) if on_shelf && last.is_some() => Mode::Discs,
        _ => Mode::Carts,
    };
    let mut period: Option<Duration> = None;
    let mut slide: Option<Slide> = None;
    let mut pacer = Pacer::new();
    let mut last_tick = Instant::now();
    let mut drawn = false;
    loop {
        let over = slot_over(frontend.app());
        let carts = matches!(mode, Mode::Carts) || over;
        if !matches!(mode, Mode::Playing(_)) {
            pacer.wait();
        }
        let began = Instant::now();

        let events = input.poll(frontend.now());
        let (to_slot, mine): (Vec<RawEvent>, Vec<RawEvent>) = match carts {
            true => (events, Vec::new()),
            false => events.into_iter().partition(slot_keeps),
        };
        frontend.advance_events(to_slot);
        if frontend.restarting() || frontend.powering_off() {
            // Finishing a game writes its memory card.
            if let Mode::Playing(game) = std::mem::replace(&mut mode, Mode::Discs) {
                game.finish(frontend.audio_sink());
            }
            if frontend.restarting() {
                frontend.restart();
            } else {
                frontend.poweroff();
                return;
            }
        }

        let now = Instant::now();
        let dt = (now - last_tick).as_secs_f32().min(0.1);
        last_tick = now;
        let mut switch: Option<i32> = None;
        let mut next: Option<Mode> = None;
        match &mut mode {
            Mode::Carts => switch = frontend.app_mut().take_leave(),
            Mode::Discs => {
                let ms = frontend.now();
                for ev in mine {
                    match carousel.handle(ev, ms) {
                        Some(Pick::Play(i)) => {
                            write_last(&root, Some(&games[i]));
                            match DiscGame::start(
                                &root,
                                &games[i].path,
                                frontend.audio_sink(),
                                period,
                            ) {
                                Ok(game) => next = Some(Mode::Playing(Box::new(game))),
                                Err(e) => eprintln!("tray: {}: {e}", games[i].path.display()),
                            }
                            break;
                        }
                        Some(Pick::Switch(by)) if frontend.app().has_carts() => {
                            switch = Some(by);
                            break;
                        }
                        _ => {}
                    }
                }
                carousel.tick(frontend.now(), dt);
                view.tick(dt, (!games.is_empty()).then(|| carousel.selected()));
            }
            Mode::Playing(game) => {
                let quit = mine.into_iter().any(|ev| game.handle(ev));
                if quit {
                    next = Some(Mode::Discs);
                } else if over {
                    // The game waits, silent, while slot's screen is up,
                    // with its card written in case the power goes next.
                    game.release_all();
                    game.save_card();
                } else {
                    game.frame(&mut picture, frontend.app().output_volume());
                }
            }
        }
        if let Some(to) = next {
            if let Mode::Playing(game) = std::mem::replace(&mut mode, to) {
                period = game.finish(frontend.audio_sink()).or(period);
                carousel.release_all();
            }
            last_tick = Instant::now();
        }
        if matches!(mode, Mode::Carts) {
            frontend.step_emulator(pacer.period, STEP_TIMEOUT);
        }

        let draw = |mode: &Mode,
                    frontend: &mut Frontend,
                    compositor: &mut Compositor,
                    overlay: &mut Overlay,
                    view: &mut ShelfView,
                    picture: &PicturePass,
                    carousel: &Carousel| {
            match mode {
                _ if slot_over(frontend.app()) => frontend.render(compositor, panel),
                Mode::Carts => frontend.render(compositor, panel),
                Mode::Discs => view.draw(overlay, &games, carousel),
                Mode::Playing(_) => picture.draw(panel),
            }
        };
        draw(
            &mode,
            &mut frontend,
            &mut compositor,
            &mut overlay,
            &mut view,
            &picture,
            &carousel,
        );
        if let Some(by) = switch {
            // Keep the screen being left, cross to the other system, and
            // slide the old screen away over it.
            overlay.snapshot();
            mode = match mode {
                Mode::Carts => {
                    write_last(&root, games.get(carousel.selected()));
                    carousel.release_all();
                    Mode::Discs
                }
                _ => {
                    write_last(&root, None);
                    frontend.app_mut().enter_shelves(by);
                    frontend.take_input_back();
                    Mode::Carts
                }
            };
            slide = Some(Slide {
                began: Instant::now(),
                dir: -(by.signum() as f32),
            });
            draw(
                &mode,
                &mut frontend,
                &mut compositor,
                &mut overlay,
                &mut view,
                &picture,
                &carousel,
            );
        }
        if let Some(s) = &slide {
            let t = (s.began.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).min(1.0);
            if t >= 1.0 {
                slide = None;
            } else {
                let eased = 1.0 - (1.0 - t).powi(3);
                let dx = (s.dir * eased * panel.0 as f32).round();
                let mut items = vec![Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: panel.0 as f32,
                    h: panel.1 as f32,
                    colour: [0.0, 0.0, 0.0, 0.45 * (1.0 - eased)],
                }];
                items.extend(overlay.snapshot_draw(dx, 1.0));
                overlay.draw_over(&items);
            }
        }

        let swap = Instant::now();
        let work = swap - began;
        if let Err(e) = surface.swap() {
            fail(format!("tray: swap: {e}"));
        }
        let swap_took = swap.elapsed();
        pacer.swapped(work, swap_took > Duration::from_millis(1));
        if let Mode::Playing(game) = &mut mode {
            if !over {
                game.swapped(swap_took);
            }
        }
        if !drawn {
            drawn = true;
            trace_first_frame();
        }
        if matches!(mode, Mode::Carts) {
            if let Some(left) = MIN_FRAME.checked_sub(began.elapsed()) {
                std::thread::sleep(left);
            }
        }
    }
}
