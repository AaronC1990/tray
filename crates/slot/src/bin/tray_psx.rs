//! tray's PlayStation probe: boots one disc straight to the screen with
//! sound and controls, and no UI, to prove PlayStation runs on the device.
//!
//! Plays the disc named on the command line, or the first one in Games/PS.
//! Hold MENU and press START to quit.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot::audio::open_sink;
use slot::drc::{drc_ratio, drc_target};
use slot::input::DeviceInput;
use slot::resample::Resampler;
use slot_gfx::{FbdevSurface, PicturePass, Surface};
use slot_input::{Btn, InputSource, RawEvent};
use slot_retro::{ButtonMask, LibretroCore, RetroCore};

const CARD: &str = "/mnt/sdcard";
const CORE: &str = "System/pcsx_rearmed_libretro.so";
const GAMES: &str = "Games/PS";
const SAVES: &str = "Saves/PS";

/// Disc formats in the order a folder's games are tried. A playlist names
/// every disc of a multi-disc game, and a cue names a disc's audio tracks,
/// so each beats the raw image it points at. A bare .exe, such as the test
/// card, comes last so any real game plays ahead of it.
const KINDS: [&str; 8] = ["m3u", "cue", "chd", "pbp", "bin", "img", "iso", "exe"];

/// The panel's refresh, measured from swaps; slot found the RG SP's near
/// this, and the first second of swaps corrects it for this panel.
const FIRST_PERIOD: Duration = Duration::from_micros(16_760);

fn first_disc(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !slot_store::is_hidden(p))
        .filter(|p| kind_rank(p).is_some())
        .collect();
    found.sort_by_key(|p| (kind_rank(p), p.clone()));
    for p in &found {
        eprintln!("tray-psx: found {}", p.display());
    }
    found.into_iter().next()
}

fn kind_rank(path: &Path) -> Option<usize> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    KINDS.iter().position(|k| *k == ext)
}

fn bit(btn: Btn) -> u16 {
    match btn {
        Btn::B => ButtonMask::B,
        Btn::Y => ButtonMask::Y,
        Btn::Select => ButtonMask::SELECT,
        Btn::Start => ButtonMask::START,
        Btn::Up => ButtonMask::UP,
        Btn::Down => ButtonMask::DOWN,
        Btn::Left => ButtonMask::LEFT,
        Btn::Right => ButtonMask::RIGHT,
        Btn::A => ButtonMask::A,
        Btn::X => ButtonMask::X,
        Btn::L1 => ButtonMask::L,
        Btn::R1 => ButtonMask::R,
        Btn::L2 => ButtonMask::L2,
        Btn::R2 => ButtonMask::R2,
        _ => 0,
    }
}

fn main() {
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let disc = match std::env::args_os().nth(1) {
        Some(p) => PathBuf::from(p),
        None => match first_disc(&root.join(GAMES)) {
            Some(p) => p,
            None => {
                eprintln!("tray-psx: no disc in {}", root.join(GAMES).display());
                return;
            }
        },
    };
    eprintln!("tray-psx: disc {}", disc.display());

    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => return eprintln!("tray-psx: display: {e}"),
    };
    let panel = surface.window_size();
    eprintln!("tray-psx: panel {}x{}", panel.0, panel.1);
    let mut picture = match PicturePass::new(&surface) {
        Ok(p) => p,
        Err(e) => return eprintln!("tray-psx: picture: {e}"),
    };

    let saves = root.join(SAVES);
    let _ = std::fs::create_dir_all(&saves);
    let mut core = match LibretroCore::open_with(&root.join(CORE), &root.join("BIOS"), &saves) {
        Ok(c) => c,
        Err(e) => return eprintln!("tray-psx: core: {e}"),
    };
    core.set_native_video(true);
    if let Err(e) = core.load(&disc) {
        return eprintln!("tray-psx: load: {e}");
    }
    let av = core.av_info();
    eprintln!("tray-psx: {:.3} fps, {} Hz", av.fps, av.sample_rate);

    let mut sink = open_sink();
    if let Err(e) = sink.open(av.sample_rate as u32) {
        eprintln!("tray-psx: audio: {e}, playing silent");
    }
    let ring = sink.ring();
    let device_hz = match ring.sample_rate() {
        0 => av.sample_rate,
        hz => hz as f64,
    };
    let mut period = FIRST_PERIOD;
    let core_hz = |period: Duration| av.sample_rate / (av.fps * period.as_secs_f64());
    let mut resampler = Resampler::new(core_hz(period), device_hz);
    let mut out = Vec::new();

    let mut input = DeviceInput::open(&root);
    let started = Instant::now();
    let mut pad = 0u16;
    let mut menu = false;
    let mut last_swap = Instant::now();
    let mut swaps = Vec::with_capacity(64);
    let mut slow = 0u32;
    let mut frames = 0u64;
    loop {
        for ev in input.poll(started.elapsed().as_millis() as u64) {
            match ev {
                RawEvent::Down(Btn::Menu) => menu = true,
                RawEvent::Up(Btn::Menu) => menu = false,
                RawEvent::Down(Btn::Start) if menu => {
                    eprintln!("tray-psx: quit after {frames} frames, {slow} slow");
                    return;
                }
                RawEvent::Down(b) => pad |= bit(b),
                RawEvent::Up(b) => pad &= !bit(b),
            }
        }

        let work = Instant::now();
        core.run_frame(ButtonMask(pad));
        let audio = core.take_audio();
        let target = drc_target(ring.capacity_frames());
        resampler.set_ratio(drc_ratio(ring.queued_frames(), target));
        resampler.process(&audio, &mut out);
        ring.push(&out);

        let (px, w, h) = core.frame();
        picture.upload(px, w, h);
        if work.elapsed() > period {
            slow += 1;
        }
        picture.draw(panel);
        if let Err(e) = surface.swap() {
            eprintln!("tray-psx: swap: {e}");
        }
        frames += 1;

        // Learn the panel's real refresh from the first second of swaps,
        // then pace audio to it so the gap to the core's 60 fps never
        // reaches the rate control.
        let now = Instant::now();
        let seen = now - last_swap;
        last_swap = now;
        if (60..120).contains(&frames)
            && seen > Duration::from_millis(12)
            && seen < Duration::from_millis(22)
        {
            swaps.push(seen);
            if swaps.len() == 60 {
                swaps.sort();
                period = swaps[30];
                resampler = Resampler::new(core_hz(period), device_hz);
                eprintln!(
                    "tray-psx: panel refresh {:.3} Hz",
                    1.0 / period.as_secs_f64()
                );
            }
        }
        if frames.is_multiple_of(600) {
            eprintln!(
                "tray-psx: {frames} frames, {slow} slow, {} underruns, {} overruns, {}x{}",
                ring.underruns(),
                ring.overruns(),
                w,
                h
            );
        }
    }
}
