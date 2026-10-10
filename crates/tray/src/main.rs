//! tray: slot's Game Boy shelves and a PlayStation disc shelf in one
//! frontend. L and R move between systems; slot runs its own shelves and
//! games exactly as it does alone, and tray runs the discs.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot::app::{set_outside_shelf, App, Outside, Phase};
use slot::frontend::Frontend;
use slot::input::DeviceInput;
use slot::pacer::{Pacer, MIN_FRAME, STEP_TIMEOUT};
use slot_gfx::{Compositor, Draw, FbdevSurface, Overlay, PicturePass, Surface};
use slot_input::{Btn, InputSource, RawEvent};
use slot_power::{trace_first_frame, DevicePlatform};
use slot_store::initial;
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
/// What slot's slot says when the disc shelf comes up.
const NAME: &str = "PlayStation";

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

/// Buttons slot keeps whichever screen is up: power and the lid for doze
/// and the power menu, and volume, which slot sets for every game.
fn slot_keeps(ev: &RawEvent) -> bool {
    let (RawEvent::Down(b) | RawEvent::Up(b)) = ev;
    matches!(b, Btn::Power | Btn::Lid | Btn::VolUp | Btn::VolDown)
}

/// On the disc shelf, MENU also goes to slot, for its settings menu, as on
/// slot's own shelves.
fn slot_keeps_on_shelf(ev: &RawEvent) -> bool {
    let (RawEvent::Down(b) | RawEvent::Up(b)) = ev;
    slot_keeps(ev) || *b == Btn::Menu
}

/// Whether slot has a screen of its own up over the discs: asleep, its
/// power menu, its settings, or anything else but its shelf.
fn slot_over(app: &App) -> bool {
    !matches!(app.phase(), Phase::Shelf) || app.power_menu().is_some()
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
    let overlay = match Overlay::new(&surface, panel) {
        Ok(o) => o,
        Err(e) => fail(format!("tray: overlay: {e}")),
    };
    let labels = root.join(LABELS);
    let mut view = ShelfView::new(&mut compositor, &labels);
    let initials = games.iter().map(|g| initial(&g.title)).collect();
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
    if matches!(mode, Mode::Discs) {
        frontend.app_mut().say_in_slot(NAME);
    }
    let mut period: Option<Duration> = None;
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
        let (to_slot, mine): (Vec<RawEvent>, Vec<RawEvent>) = match (carts, &mode) {
            (true, _) => (events, Vec::new()),
            (false, Mode::Discs) => events.into_iter().partition(slot_keeps_on_shelf),
            (false, _) => events.into_iter().partition(slot_keeps),
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
        let mut sound = None;
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
                                anim: Insert::start(),
                                shown_shut: false,
                                game: None,
                            });
                            break;
                        }
                        Some(Pick::Switch(by)) if frontend.app().has_carts() => {
                            switch = Some(by);
                            break;
                        }
                        Some(Pick::Letter(c)) => frontend.app_mut().say_in_slot(&c.to_string()),
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
                    sound = anim.tick(dt);
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
                                    anim: Eject::start(),
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
                            let s = Swap::start(
                                &mut view,
                                &mut compositor,
                                &labels,
                                &games[*which],
                                from,
                                to as u32,
                            );
                            frontend.app_mut().say_in_slot(&s.caption());
                            *swap = Some(s);
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
                            anim: Eject::start(),
                        });
                    }
                } else {
                    *intro += dt;
                    game.frame(&mut picture, frontend.app().output_volume());
                    if let Some(s) = swap {
                        sound = s.tick(dt, game);
                        if s.done() {
                            *swap = None;
                        }
                    }
                }
            }
            Mode::Ejecting { anim } => {
                if !over {
                    sound = anim.tick(dt);
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
        if let Some(by) = switch {
            // Across to the other system at once, as slot moves between its
            // own shelves, with the new shelf's name in the slot.
            mode = match mode {
                Mode::Carts => {
                    write_last(&root, games.get(carousel.selected()));
                    carousel.release_all();
                    frontend.app_mut().say_in_slot(NAME);
                    Mode::Discs
                }
                _ => {
                    write_last(&root, None);
                    frontend.app_mut().enter_shelves(by);
                    frontend.take_input_back();
                    Mode::Carts
                }
            };
        }
        if let Some(s) = sound {
            frontend.play_sfx(s);
        }
        if matches!(mode, Mode::Carts) {
            frontend.step_emulator(pacer.period, STEP_TIMEOUT);
        }

        // The disc shelf is drawn by slot, into its own frame, as one of
        // its shelves; a game is drawn straight to the panel.
        let look = match &mode {
            Mode::Carts => None,
            Mode::Discs => Some(Look::default()),
            Mode::Inserting { anim, .. } => Some(anim.look()),
            Mode::Ejecting { anim } => Some(anim.look()),
            Mode::Playing(play) => play
                .swap
                .as_ref()
                .filter(|s| s.shows_shelf())
                .map(Swap::look),
        };
        let outside = look.map(|look| {
            let d = view.draws(&mut compositor, &games, &carousel, &look);
            Outside {
                row: d.row,
                seated: d.seated,
            }
        });
        let shelf_up = outside.is_some();
        match (&mode, outside) {
            (Mode::Carts, _) => frontend.app_mut().set_outside(None),
            (_, Some(o)) => frontend.app_mut().set_outside(Some(o)),
            _ => {}
        }
        let game_up = matches!(mode, Mode::Playing(_)) && !shelf_up;
        match game_up && !slot_over(frontend.app()) {
            true => picture.draw(panel),
            false => frontend.render(&mut compositor, panel),
        }
        let dark = match &mode {
            Mode::Carts | Mode::Discs => 0.0,
            Mode::Inserting { anim, .. } => anim.veil(),
            Mode::Ejecting { anim } => anim.veil(),
            Mode::Playing(play) => {
                let swap = play.swap.as_ref().map_or(0.0, Swap::veil);
                let ends = match play.outro {
                    Some(t) => t / OUTRO,
                    None => 1.0 - play.intro / INTRO,
                };
                swap.max(ends)
            }
        };
        if dark > 0.0 && !slot_over(frontend.app()) {
            overlay.draw_over(&[veil(panel, dark)]);
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
