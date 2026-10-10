//! A PlayStation game in play, stepped one frame a swap by tray's loop.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot::audio::{volume, AudioSink, Ring, GBA_HZ};
use slot::drc::{drc_ratio, drc_target};
use slot::resample::Resampler;
use slot_gfx::{FrameLayout, PicturePass};
use slot_input::{Btn, RawEvent};
use slot_retro::{to_xrgb8888, ButtonMask, LibretroCore, PixelFormat, RetroCore};

pub const CORE: &str = "System/pcsx_rearmed_libretro.so";
pub const SAVES: &str = "Saves/PS";

/// The panel's refresh until it is measured; slot found the RG SP's near
/// this, and the first second of swaps corrects it for this panel.
const FIRST_PERIOD: Duration = Duration::from_micros(16_760);

/// How often the memory card is checked for changes and written out.
const CARD_CHECK_FRAMES: u64 = 60;

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

pub struct DiscGame {
    core: LibretroCore,
    ring: Arc<Ring>,
    fps: f64,
    rate: f64,
    device_hz: f64,
    resampler: Resampler,
    out: Vec<i16>,
    pad: u16,
    menu: bool,
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
}

/// The file a game's memory card is kept in: named after the file the game
/// started from, so every disc of a playlist shares one card.
pub fn card_path(root: &Path, disc: &Path) -> PathBuf {
    let stem = disc.file_stem().unwrap_or_default().to_string_lossy();
    root.join(SAVES).join(format!("{stem}.srm"))
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
        disc: &Path,
        sink: &mut dyn AudioSink,
        period: Option<Duration>,
    ) -> Result<Self, String> {
        eprintln!("tray: play {}", disc.display());
        let saves = root.join(SAVES);
        let _ = std::fs::create_dir_all(&saves);
        let mut core = LibretroCore::open_with(&root.join(CORE), &root.join("BIOS"), &saves)
            .map_err(|e| format!("core: {e}"))?;
        core.set_native_video(true);
        // PCSX ReARMed turns on declared defaults such as threaded rendering
        // only when they are answered; unanswered, it renders on this thread.
        core.set_answer_defaults(true);
        core.load(disc).map_err(|e| format!("load: {e}"))?;
        let card = card_path(root, disc);
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
        let first = period.unwrap_or(FIRST_PERIOD);
        let drained = ring.drained_samples();
        let mut game = DiscGame {
            core,
            fps: av.fps,
            rate: av.sample_rate,
            device_hz,
            resampler: Resampler::new(1.0, 1.0),
            out: Vec::new(),
            pad: 0,
            menu: false,
            period: first,
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

    /// Takes one button event. True when it asks to quit.
    pub fn handle(&mut self, ev: RawEvent) -> bool {
        match ev {
            RawEvent::Down(Btn::Menu) => self.menu = true,
            RawEvent::Up(Btn::Menu) => self.menu = false,
            RawEvent::Down(Btn::Start) if self.menu => return true,
            RawEvent::Down(b) => self.pad |= bit(b),
            RawEvent::Up(b) => self.pad &= !bit(b),
        }
        false
    }

    /// Lets go of every button, as when the game loses them for a while.
    pub fn release_all(&mut self) {
        self.pad = 0;
        self.menu = false;
    }

    /// Runs one frame, queues its sound at `volume` and uploads its picture.
    pub fn frame(&mut self, picture: &mut PicturePass, volume_level: u8) {
        let work = Instant::now();
        self.core.run_frame(ButtonMask(self.pad));
        let emulated = Instant::now();
        let mut audio = self.core.take_audio();
        volume::apply(&mut audio, volume_level);
        let target = drc_target(self.ring.capacity_frames());
        self.resampler
            .set_ratio(drc_ratio(self.ring.queued_frames(), target));
        self.resampler.process(&audio, &mut self.out);
        self.ring.push(&self.out);
        self.pushed += self.out.len() as u64;

        let (px, w, h, format) = self.core.frame_raw();
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
        let Some(now) = self.core.save_ram() else {
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
        let (_, w, h, _) = self.core.frame_raw();
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
