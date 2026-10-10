//! Plays one disc until MENU and START are pressed together.
use std::path::Path;
use std::time::{Duration, Instant};

use slot::audio::open_sink;
use slot::drc::{drc_ratio, drc_target};
use slot::resample::Resampler;
use slot_gfx::{FrameLayout, PicturePass, Surface};
use slot_input::{Btn, InputSource, RawEvent};
use slot_retro::{to_xrgb8888, ButtonMask, LibretroCore, PixelFormat, RetroCore};

pub const CORE: &str = "System/pcsx_rearmed_libretro.so";
pub const SAVES: &str = "Saves/PS";

/// The panel's refresh until it is measured; slot found the RG SP's near
/// this, and the first second of swaps corrects it for this panel.
const FIRST_PERIOD: Duration = Duration::from_micros(16_760);

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

/// What playing needs from the shelf that stays the same between games.
pub struct Console<'a> {
    pub surface: &'a mut dyn Surface,
    pub picture: &'a mut PicturePass,
    pub input: &'a mut dyn InputSource,
    /// The clock input times are kept against.
    pub started: Instant,
    pub panel: (u32, u32),
    /// The panel's measured refresh, once a game has measured it.
    pub period: Option<Duration>,
}

/// Plays `disc` until it is quit. The core is unloaded on return, which
/// lets it write its memory cards.
pub fn play(console: &mut Console, root: &Path, disc: &Path) -> Result<(), String> {
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
    let av = core.av_info();
    eprintln!("tray: {:.3} fps, {} Hz", av.fps, av.sample_rate);

    let mut sink = open_sink();
    if let Err(e) = sink.open(av.sample_rate as u32) {
        eprintln!("tray: audio: {e}, playing silent");
    }
    let ring = sink.ring();
    let device_hz = match ring.sample_rate() {
        0 => av.sample_rate,
        hz => hz as f64,
    };
    let known = console.period.is_some();
    let mut period = console.period.unwrap_or(FIRST_PERIOD);
    let core_hz = |period: Duration| av.sample_rate / (av.fps * period.as_secs_f64());
    let mut resampler = Resampler::new(core_hz(period), device_hz);
    let mut out = Vec::new();

    let Console {
        surface,
        picture,
        input,
        started,
        panel,
        ..
    } = console;
    let mut pad = 0u16;
    let mut menu = false;
    let mut last_swap = Instant::now();
    let mut swaps = Vec::with_capacity(64);
    let mut slow = 0u32;
    let mut frames = 0u64;
    let mut times = Times::default();
    let mut pushed = 0u64;
    let mut window = (Instant::now(), 0u64, ring.drained_samples());
    let measured = loop {
        let mut quit = false;
        for ev in input.poll(started.elapsed().as_millis() as u64) {
            match ev {
                RawEvent::Down(Btn::Menu) => menu = true,
                RawEvent::Up(Btn::Menu) => menu = false,
                RawEvent::Down(Btn::Start) if menu => quit = true,
                RawEvent::Down(b) => pad |= bit(b),
                RawEvent::Up(b) => pad &= !bit(b),
            }
        }
        if quit {
            eprintln!("tray: quit after {frames} frames, {slow} slow");
            break swaps.len() == 60;
        }

        let work = Instant::now();
        core.run_frame(ButtonMask(pad));
        let emulated = Instant::now();
        let audio = core.take_audio();
        let target = drc_target(ring.capacity_frames());
        resampler.set_ratio(drc_ratio(ring.queued_frames(), target));
        resampler.process(&audio, &mut out);
        ring.push(&out);
        pushed += out.len() as u64;

        let (px, w, h, format) = core.frame_raw();
        match format {
            PixelFormat::Rgb565 => picture.upload_as(FrameLayout::Rgb565, px, w, h),
            PixelFormat::Xrgb8888 => picture.upload(px, w, h),
            PixelFormat::Xrgb1555 => picture.upload(&to_xrgb8888(format, px, w, h), w, h),
        }
        let uploaded = Instant::now();
        if uploaded - work > period {
            slow += 1;
        }
        picture.draw(*panel);
        if let Err(e) = surface.swap() {
            eprintln!("tray: swap: {e}");
        }
        times.add(emulated - work, uploaded - emulated, uploaded.elapsed());
        frames += 1;

        // Learn the panel's real refresh from the first second of swaps,
        // then pace audio to it so the gap to the core's 60 fps never
        // reaches the rate control. Later games reuse what this learned.
        let now = Instant::now();
        let seen = now - last_swap;
        last_swap = now;
        if !known
            && (60..120).contains(&frames)
            && seen > Duration::from_millis(12)
            && seen < Duration::from_millis(22)
        {
            swaps.push(seen);
            if swaps.len() == 60 {
                swaps.sort();
                period = swaps[30];
                resampler = Resampler::new(core_hz(period), device_hz);
                eprintln!("tray: panel refresh {:.3} Hz", 1.0 / period.as_secs_f64());
            }
        }
        if frames.is_multiple_of(600) {
            eprintln!(
                "tray: {frames} frames, {slow} slow, {} underruns, {} overruns, {}x{}",
                ring.underruns(),
                ring.overruns(),
                w,
                h
            );
            eprintln!("tray:   {}", times.report());
            let (since, was_pushed, was_drained) = window;
            let secs = since.elapsed().as_secs_f64();
            let drained = ring.drained_samples();
            eprintln!(
                "tray:   audio Hz: made {:.0}, played {:.0}, queued {} of {} frames",
                (pushed - was_pushed) as f64 / 2.0 / secs,
                (drained - was_drained) as f64 / 2.0 / secs,
                ring.queued_frames(),
                ring.capacity_frames()
            );
            window = (Instant::now(), pushed, drained);
            times = Times::default();
        }
    };
    if measured {
        console.period = Some(period);
    }
    Ok(())
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
            "ms avg/max: emulate {}, upload {}, draw+swap {}",
            pair(self.emulate),
            pair(self.upload),
            pair(self.present)
        )
    }
}
