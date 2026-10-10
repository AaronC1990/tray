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
use tray::motion::{Eject, Insert, Swap};
use tray::play::{Ask, DiscGame, SAVES};
use tray::shelf::{Look, ShelfView};

const CARD: &str = "/mnt/sdcard";
const GAMES: &str = "Games/PS";
const LABELS: &str = "Labels/PS";
/// Which system was last on screen, so tray starts where it was left.
const LAST: &str = ".tray-last";
/// How long one screen takes to slide off and uncover the next.
const SLIDE: Duration = Duration::from_millis(280);

/// How long a game takes to dip to black before its disc comes out, and to
/// come up out of black once it has gone in.
const OUTRO: f32 = 0.15;
const INTRO: f32 = 0.25;

enum Mode {
    /// slot's shelves and games.
    Carts,
    Discs,
    /// A disc going into the drive; the game loads once it is in.
    Inserting {
        which: usize,
        anim: Insert,
        /// Whether the seated disc has been on screen, so loading, which
        /// holds the screen still, starts with it showing.
        shown_shut: bool,
        game: Option<Box<DiscGame>>,
    },
    Playing(Box<Play>),
    /// A disc coming back out to the shelf.
    Ejecting {
        anim: Eject,
    },
}

struct Play {
    game: Box<DiscGame>,
    /// Which game on the shelf it is.
    which: usize,
    swap: Option<Swap>,
    /// Seconds since the game came up.
    intro: f32,
    /// Seconds into dipping to black before quitting, once asked to quit.
    outro: Option<f32>,
}

/// A plain black veil over the whole panel.
fn veil(panel: (u32, u32), alpha: f32) -> Draw {
    Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: panel.0 as f32,
        h: panel.1 as f32,
        colour: [0.0, 0.0, 0.0, alpha.clamp(0.0, 1.0)],
    }
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
    let labels = root.join(LABELS);
    let mut view = ShelfView::new(&mut overlay, panel, &labels);
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
            match std::mem::replace(&mut mode, Mode::Discs) {
                Mode::Playing(play) => {
                    play.game.finish(frontend.audio_sink());
                }
                Mode::Inserting {
                    game: Some(game), ..
                } => {
                    game.finish(frontend.audio_sink());
                }
                _ => {}
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
                            next = Some(Mode::Inserting {
                                which: i,
                                anim: Insert::start(&view),
                                shown_shut: false,
                                game: None,
                            });
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
            Mode::Inserting {
                which,
                anim,
                shown_shut,
                game,
            } => {
                if !over {
                    anim.tick(dt);
                }
                if anim.ready_to_load() {
                    if *shown_shut {
                        match DiscGame::start(&root, &games[*which], frontend.audio_sink(), period)
                        {
                            Ok(g) => {
                                *game = Some(Box::new(g));
                                anim.loaded();
                            }
                            Err(e) => {
                                eprintln!("tray: {}: {e}", games[*which].path.display());
                                next = Some(Mode::Ejecting {
                                    anim: Eject::start(&view),
                                });
                            }
                        }
                        // Loading held the screen; don't count it as time
                        // the animation should catch up on.
                        last_tick = Instant::now();
                    } else {
                        *shown_shut = true;
                    }
                }
                if anim.done() {
                    if let Some(game) = game.take() {
                        next = Some(Mode::Playing(Box::new(Play {
                            game,
                            which: *which,
                            swap: None,
                            intro: 0.0,
                            outro: None,
                        })));
                    }
                }
            }
            Mode::Playing(play) => {
                let Play {
                    game,
                    which,
                    swap,
                    intro,
                    outro,
                } = play.as_mut();
                // A disc change or quitting takes the buttons until done.
                let ask = match (swap.is_some(), outro.is_some()) {
                    (false, false) => mine.into_iter().find_map(|ev| game.handle(ev)),
                    _ => None,
                };
                match ask {
                    Some(Ask::Quit) => {
                        game.release_all();
                        *outro = Some(0.0);
                    }
                    Some(Ask::Disc(by)) => {
                        let (from, count) = (game.disc(), game.disc_count());
                        let to = from as i64 + by as i64;
                        if count > 1 && (0..count as i64).contains(&to) {
                            game.release_all();
                            *swap = Some(Swap::start(
                                &mut view,
                                &mut overlay,
                                &labels,
                                &games[*which],
                                from,
                                to as u32,
                                count,
                                panel.0 as f32,
                            ));
                        }
                    }
                    None => {}
                }
                if over {
                    // The game waits, silent, while slot's screen is up,
                    // with its card written in case the power goes next.
                    game.release_all();
                    game.save_card();
                } else if let Some(t) = outro {
                    *t += dt;
                    if *t >= OUTRO {
                        next = Some(Mode::Ejecting {
                            anim: Eject::start(&view),
                        });
                    }
                } else {
                    *intro += dt;
                    game.frame(&mut picture, frontend.app().output_volume());
                    if swap.as_mut().is_some_and(|s| s.tick(dt, game)) {
                        *swap = None;
                    }
                }
            }
            Mode::Ejecting { anim } => {
                if !over {
                    anim.tick(dt);
                }
                if anim.done() {
                    next = Some(Mode::Discs);
                }
            }
        }
        if let Some(to) = next {
            match std::mem::replace(&mut mode, to) {
                Mode::Playing(play) => {
                    period = play.game.finish(frontend.audio_sink()).or(period);
                    last_tick = Instant::now();
                }
                Mode::Ejecting { .. } => carousel.release_all(),
                _ => {}
            }
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
            if slot_over(frontend.app()) {
                return frontend.render(compositor, panel);
            }
            let shelf = |view: &mut ShelfView, overlay: &mut Overlay, look: &Look| {
                view.draw(overlay, &games, carousel, look)
            };
            let dark = match mode {
                Mode::Carts => {
                    frontend.render(compositor, panel);
                    0.0
                }
                Mode::Discs => {
                    shelf(view, overlay, &Look::default());
                    0.0
                }
                Mode::Inserting { anim, .. } => {
                    shelf(view, overlay, &anim.look());
                    anim.veil()
                }
                Mode::Ejecting { anim, .. } => {
                    shelf(view, overlay, &anim.look());
                    anim.veil()
                }
                Mode::Playing(play) => {
                    match &play.swap {
                        Some(s) if s.shows_shelf() => shelf(view, overlay, &s.look()),
                        _ => picture.draw(panel),
                    }
                    let swap = play.swap.as_ref().map_or(0.0, Swap::veil);
                    let ends = match play.outro {
                        Some(t) => t / OUTRO,
                        None => 1.0 - play.intro / INTRO,
                    };
                    swap.max(ends)
                }
            };
            if dark > 0.0 {
                overlay.draw_over(&[veil(panel, dark)]);
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
                let mut items = vec![veil(panel, 0.45 * (1.0 - eased))];
                items.extend(overlay.snapshot_draw(dx, 1.0));
                overlay.draw_over(&items);
            }
        }

        let swap_at = Instant::now();
        let work = swap_at - began;
        if let Err(e) = surface.swap() {
            fail(format!("tray: swap: {e}"));
        }
        let swap_took = swap_at.elapsed();
        pacer.swapped(work, swap_took > Duration::from_millis(1));
        if let Mode::Playing(play) = &mut mode {
            if !over && play.outro.is_none() {
                play.game.swapped(swap_took);
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
