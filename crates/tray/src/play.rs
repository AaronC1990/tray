//! A PlayStation game in play, stepped one frame a swap by tray's loop.
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot::audio::{volume, AudioSink, Ring, GBA_HZ};
use slot::drc::{drc_ratio, drc_target};
use slot::persist::Snapshot;
use slot::resample::Resampler;
use slot_gfx::{FrameLayout, PicturePass};
use slot_input::{Btn, RawEvent};
use slot_retro::{to_xrgb8888, ButtonMask, LibretroCore, PixelFormat, RetroCore};
use slot_store::StateRing;

use crate::library::{content, Game};

pub const CORE: &str = "System/pcsx_rearmed_libretro.so";
pub const SAVES: &str = "Saves/PS";

/// The panel's refresh until it is measured; slot found the RG SP's near
/// this, and the first second of swaps corrects it for this panel.
const FIRST_PERIOD: Duration = Duration::from_micros(16_760);

/// How often the memory card is checked for changes and written out.
const CARD_CHECK_FRAMES: u64 = 60;

pub struct DiscGame {
    /// Shared with the snapshot slot saves and loads states through.
    core: Rc<RefCell<LibretroCore>>,
    ring: Arc<Ring>,
    fps: f64,
    rate: f64,
    device_hz: f64,
    resampler: Resampler,
    out: Vec<i16>,
    period: Duration,
    /// Swap gaps seen while the panel's refresh is still being learned.
    swaps: Option<Vec<Duration>>,
    last_swap: Instant,
    frames: u64,
    slow: u32,
    times: Times,
    work: (Duration, Duration),
    pushed: u64,
    window: (Instant, u64, u64),
    /// Where memory card 1 lives, and what was last written there. The
    /// core hands card 1 to the frontend to keep, as libretro cores do.
    card: PathBuf,
    card_written: Option<Vec<u8>>,
    /// Where the disc in play is remembered.
    disc_file: PathBuf,
}

/// The file a game's memory card is kept in, named after the game rather
/// than any one disc, so all its discs share one card.
pub fn card_path(root: &Path, stem: &str) -> PathBuf {
    root.join(SAVES).join(format!("{stem}.srm"))
}

/// The file that remembers which disc a game was on.
pub fn disc_path(root: &Path, stem: &str) -> PathBuf {
    root.join(SAVES).join(format!("{stem}.disc"))
}

/// The disc `stem` was last on, counting from 0.
pub fn last_disc(root: &Path, stem: &str) -> u32 {
    std::fs::read_to_string(disc_path(root, stem))
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

/// What a button combination asks of the game, beyond playing it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Quit,
    /// Change to the next disc (1) or the one before (-1).
    Disc(i32),
}

/// MENU with START to quit and with L or R to change discs. The buttons
/// pressed with MENU are kept from slot and the game.
#[derive(Default)]
pub struct Combos {
    menu: bool,
    /// Buttons pressed as part of a combination, whose release is kept too.
    held: Vec<Btn>,
}

impl Combos {
    /// What `ev` asks for, and whether it should still go on to slot.
    pub fn take(&mut self, ev: RawEvent) -> (Option<Ask>, bool) {
        let ask = match ev {
            RawEvent::Down(Btn::Menu) => {
                self.menu = true;
                None
            }
            RawEvent::Up(Btn::Menu) => {
                self.menu = false;
                None
            }
            RawEvent::Down(b @ Btn::Start) if self.menu => Some((b, Ask::Quit)),
            RawEvent::Down(b @ Btn::R1) if self.menu => Some((b, Ask::Disc(1))),
            RawEvent::Down(b @ Btn::L1) if self.menu => Some((b, Ask::Disc(-1))),
            RawEvent::Up(b) if self.held.contains(&b) => {
                self.held.retain(|h| *h != b);
                return (None, false);
            }
            _ => None,
        };
        match ask {
            Some((b, ask)) => {
                self.held.push(b);
                (Some(ask), false)
            }
            None => (None, true),
        }
    }
}

/// Where a game's save states are kept, beside slot's own:
/// States/PS/pcsx_rearmed/<game>.
pub fn state_ring(root: &Path, stem: &str) -> StateRing {
    StateRing::at(
        root.join("States")
            .join("PS")
            .join("pcsx_rearmed")
            .join(stem),
    )
}

/// A thumbnail for slot's state browser: the frame scaled to fill a Game
/// Boy Advance screen's shape, cropping what spills over, as a PNG.
pub fn thumbnail(xrgb: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let (tw, th) = (slot_retro::GBA_W, slot_retro::GBA_H);
    if w == 0 || h == 0 || xrgb.len() < (w * h * 4) as usize {
        return None;
    }
    let scale = (tw as f32 / w as f32).max(th as f32 / h as f32);
    let (ox, oy) = (
        (w as f32 - tw as f32 / scale) / 2.0,
        (h as f32 - th as f32 / scale) / 2.0,
    );
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for y in 0..th {
        let sy = ((oy + (y as f32 + 0.5) / scale) as u32).min(h - 1);
        for x in 0..tw {
            let sx = ((ox + (x as f32 + 0.5) / scale) as u32).min(w - 1);
            let from = ((sy * w + sx) * 4) as usize;
            let to = ((y * tw + x) * 4) as usize;
            out[to..to + 4].copy_from_slice(&xrgb[from..from + 4]);
        }
    }
    slot::thumb::png(&out)
}

/// The game as slot sees it for save states.
struct DiscSnapshot {
    core: Rc<RefCell<LibretroCore>>,
}

impl Snapshot for DiscSnapshot {
    fn state(&self) -> Option<Vec<u8>> {
        self.core.borrow_mut().serialize().ok()
    }

    /// The memory card is kept by tray, beside the game.
    fn save_ram(&self) -> Option<Vec<u8>> {
        None
    }

    fn thumb(&self) -> Option<Vec<u8>> {
        let (xrgb, w, h) = self.core.borrow().frame();
        thumbnail(&xrgb, w, h)
    }

    fn load(&self, state: Vec<u8>) {
        if let Err(e) = self.core.borrow_mut().unserialize(&state) {
            eprintln!("tray: load state: {e}");
        }
    }
}

/// Writes `bytes` to `path` whole or not at all: a card half written when
/// the power goes is worse than the card before it.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("srm.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

impl DiscGame {
    /// Loads `disc` and points `sink` at the core's sample rate. `period`
    /// is the panel's refresh if an earlier game measured it.
    pub fn start(
        root: &Path,
        game: &Game,
        sink: &mut dyn AudioSink,
        period: Option<Duration>,
        resume: Option<Vec<u8>>,
    ) -> Result<Self, String> {
        let disc = content(root, game).map_err(|e| format!("playlist: {e}"))?;
        let disc = disc.as_path();
        eprintln!("tray: play {}", disc.display());
        let saves = root.join(SAVES);
        let _ = std::fs::create_dir_all(&saves);
        let mut core = LibretroCore::open_with(&root.join(CORE), &root.join("BIOS"), &saves)
            .map_err(|e| format!("core: {e}"))?;
        core.set_native_video(true);
        // PCSX ReARMed turns on declared defaults such as threaded rendering
        // only when they are answered; unanswered, it renders on this thread.
        core.set_answer_defaults(true);
        // Options set by hand for looking into a problem, as key=value
        // pairs separated by commas in TRAY_CORE_OPTIONS.
        if let Ok(options) = std::env::var("TRAY_CORE_OPTIONS") {
            for pair in options.split(',').filter(|p| !p.is_empty()) {
                if let Some((key, value)) = pair.split_once('=') {
                    eprintln!("tray: option {key}={value}");
                    core.set_option(key.trim(), value.trim());
                }
            }
        }
        // Start on the disc the game was left on.
        let first = last_disc(root, &game.stem);
        if first > 0 {
            match game.discs.get(first as usize) {
                Some(path) if core.set_first_disc(first, path) => {
                    eprintln!("tray: starting on disc {}", first + 1)
                }
                _ => eprintln!("tray: could not start on disc {}", first + 1),
            }
        }
        core.load(disc).map_err(|e| format!("load: {e}"))?;
        eprintln!(
            "tray: disc {} of {}",
            core.disc_index() + 1,
            core.disc_count()
        );
        let card = card_path(root, &game.stem);
        let card_written = match std::fs::read(&card) {
            Ok(bytes) => match core.load_save_ram(&bytes) {
                Ok(()) => {
                    eprintln!("tray: memory card {}", card.display());
                    Some(bytes)
                }
                Err(e) => {
                    eprintln!("tray: memory card {}: {e}", card.display());
                    None
                }
            },
            Err(_) => None,
        };
        // Pick up where the game was left, as slot resumes a cart.
        if let Some(state) = resume {
            match core.unserialize(&state) {
                Ok(()) => eprintln!("tray: resumed"),
                Err(e) => eprintln!("tray: resume refused, starting fresh: {e}"),
            }
        }
        let av = core.av_info();
        eprintln!("tray: {:.3} fps, {} Hz", av.fps, av.sample_rate);
        if let Err(e) = sink.open(av.sample_rate as u32) {
            eprintln!("tray: audio: {e}, playing silent");
        }
        let ring = sink.ring();
        let device_hz = match ring.sample_rate() {
            0 => av.sample_rate,
            hz => hz as f64,
        };
        let first_period = period.unwrap_or(FIRST_PERIOD);
        let drained = ring.drained_samples();
        let mut game = DiscGame {
            core: Rc::new(RefCell::new(core)),
            fps: av.fps,
            rate: av.sample_rate,
            device_hz,
            resampler: Resampler::new(1.0, 1.0),
            out: Vec::new(),
            period: first_period,
            swaps: period.is_none().then(|| Vec::with_capacity(64)),
            last_swap: Instant::now(),
            frames: 0,
            slow: 0,
            times: Times::default(),
            work: (Duration::ZERO, Duration::ZERO),
            pushed: 0,
            window: (Instant::now(), 0, drained),
            ring,
            card,
            card_written,
            disc_file: disc_path(root, &game.stem),
        };
        game.resampler = Resampler::new(game.core_hz(), device_hz);
        Ok(game)
    }

    /// The core's sample rate as played: it makes 60 frames a second of
    /// sound, but the panel shows a little under 60.
    fn core_hz(&self) -> f64 {
        self.rate / (self.fps * self.period.as_secs_f64())
    }

    /// The panel's refresh as measured, once it has been.
    pub fn period(&self) -> Option<Duration> {
        self.swaps.is_none().then_some(self.period)
    }

    /// What slot saves and loads states through.
    pub fn snapshot(&self) -> Box<dyn Snapshot> {
        Box::new(DiscSnapshot {
            core: self.core.clone(),
        })
    }

    pub fn disc_count(&self) -> u32 {
        self.core.borrow().disc_count()
    }

    /// The disc in, counting from 0.
    pub fn disc(&self) -> u32 {
        self.core.borrow().disc_index()
    }

    pub fn set_lid_open(&mut self, open: bool) {
        if !self.core.borrow_mut().set_lid_open(open) {
            eprintln!(
                "tray: the lid would not {}",
                if open { "open" } else { "close" }
            );
        }
    }

    /// Changes to disc `index` and remembers it for next time.
    pub fn set_disc(&mut self, index: u32) {
        if !self.core.borrow_mut().set_disc(index) {
            eprintln!("tray: disc {} refused", index + 1);
            return;
        }
        eprintln!("tray: disc {} in", index + 1);
        let _ = std::fs::write(&self.disc_file, format!("{index}\n"));
    }

    /// Runs one frame with `pad` held, queues its sound at `volume_level`
    /// and uploads its picture.
    pub fn frame(&mut self, picture: &mut PicturePass, volume_level: u8, pad: ButtonMask) {
        let mut core = self.core.borrow_mut();
        let work = Instant::now();
        core.run_frame(pad);
        let emulated = Instant::now();
        let mut audio = core.take_audio();
        volume::apply(&mut audio, volume_level);
        let target = drc_target(self.ring.capacity_frames());
        self.resampler
            .set_ratio(drc_ratio(self.ring.queued_frames(), target));
        self.resampler.process(&audio, &mut self.out);
        self.ring.push(&self.out);
        self.pushed += self.out.len() as u64;

        let (px, w, h, format) = core.frame_raw();
        match format {
            PixelFormat::Rgb565 => picture.upload_as(FrameLayout::Rgb565, px, w, h),
            PixelFormat::Xrgb8888 => picture.upload(px, w, h),
            PixelFormat::Xrgb1555 => picture.upload(&to_xrgb8888(format, px, w, h), w, h),
        }
        let uploaded = Instant::now();
        if uploaded - work > self.period {
            self.slow += 1;
        }
        self.work = (emulated - work, uploaded - emulated);
    }

    /// Notes a swap after a frame: learns the panel's refresh from the
    /// first second of them and logs how play is going every ten seconds.
    pub fn swapped(&mut self, presented: Duration) {
        self.times.add(self.work.0, self.work.1, presented);
        self.frames += 1;
        let now = Instant::now();
        let seen = now - self.last_swap;
        self.last_swap = now;
        if let Some(swaps) = &mut self.swaps {
            if (60..120).contains(&self.frames)
                && seen > Duration::from_millis(12)
                && seen < Duration::from_millis(22)
            {
                swaps.push(seen);
            }
            if swaps.len() == 60 {
                swaps.sort();
                self.period = swaps[30];
                self.swaps = None;
                self.resampler = Resampler::new(self.core_hz(), self.device_hz);
                eprintln!(
                    "tray: panel refresh {:.3} Hz",
                    1.0 / self.period.as_secs_f64()
                );
            }
        }
        if self.frames.is_multiple_of(CARD_CHECK_FRAMES) {
            self.save_card();
        }
        if self.frames.is_multiple_of(600) {
            self.report();
        }
    }

    /// Writes memory card 1 out if the game changed it since last time.
    pub fn save_card(&mut self) {
        let Some(now) = self.core.borrow().save_ram() else {
            return;
        };
        if self.card_written.as_deref() == Some(&now[..]) {
            return;
        }
        match write_whole(&self.card, &now) {
            Ok(()) => {
                eprintln!("tray: saved memory card {}", self.card.display());
                self.card_written = Some(now);
            }
            Err(e) => eprintln!("tray: memory card {}: {e}", self.card.display()),
        }
    }

    fn report(&mut self) {
        let (_, w, h, _) = self.core.borrow().frame_raw();
        eprintln!(
            "tray: {} frames, {} slow, {} underruns, {} overruns, {w}x{h}",
            self.frames,
            self.slow,
            self.ring.underruns(),
            self.ring.overruns(),
        );
        eprintln!("tray:   {}", self.times.report());
        let (since, was_pushed, was_drained) = self.window;
        let secs = since.elapsed().as_secs_f64();
        let drained = self.ring.drained_samples();
        eprintln!(
            "tray:   audio Hz: made {:.0}, played {:.0}, queued {} of {} frames",
            (self.pushed - was_pushed) as f64 / 2.0 / secs,
            (drained - was_drained) as f64 / 2.0 / secs,
            self.ring.queued_frames(),
            self.ring.capacity_frames()
        );
        self.window = (Instant::now(), self.pushed, drained);
        self.times = Times::default();
    }

    /// Ends play: writes the memory card, unloads the core, and
    /// gives `sink` back at slot's rate. Returns the measured refresh.
    pub fn finish(mut self, sink: &mut dyn AudioSink) -> Option<Duration> {
        self.save_card();
        eprintln!(
            "tray: quit after {} frames, {} slow",
            self.frames, self.slow
        );
        let period = self.period();
        if Rc::strong_count(&self.core) > 1 {
            eprintln!("tray: the core is still lent out; let slot go of it first");
        }
        drop(self.core);
        if let Err(e) = sink.open(GBA_HZ) {
            eprintln!("tray: audio: {e}");
        }
        period
    }
}

/// Where each frame's time went, over the frames since the last report.
#[derive(Default)]
struct Times {
    frames: u32,
    emulate: (Duration, Duration),
    upload: (Duration, Duration),
    present: (Duration, Duration),
}

impl Times {
    fn add(&mut self, emulate: Duration, upload: Duration, present: Duration) {
        self.frames += 1;
        for (slot, t) in [
            (&mut self.emulate, emulate),
            (&mut self.upload, upload),
            (&mut self.present, present),
        ] {
            slot.0 += t;
            slot.1 = slot.1.max(t);
        }
    }

    fn report(&self) -> String {
        let n = self.frames.max(1);
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let pair = |(sum, max): (Duration, Duration)| format!("{:.1}/{:.1}", ms(sum / n), ms(max));
        format!(
            "ms avg/max: emulate {}, upload {}, swap {}",
            pair(self.emulate),
            pair(self.upload),
            pair(self.present)
        )
    }
}
