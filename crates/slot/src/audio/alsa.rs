use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

use libloading::Library;

use super::metronome::Metronome;
use super::ring::Ring;
use super::silence::Silence;
use super::sink::{AudioError, AudioSink};

const SND_PCM_STREAM_PLAYBACK: c_int = 0;
const SND_PCM_FORMAT_S16_LE: c_int = 2;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;
const CHANNELS: c_uint = 2;

const LATENCY_US: c_uint = 40_000;

const PERIOD_FRAMES: usize = 512;

const RELEASE_AFTER: Duration = Duration::from_secs(3);

const DEVICES: [&str; 4] = ["plug:default", "default", "plughw:0,0", "hw:0,0"];

type PcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type PcmSetParams =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type PcmWritei = unsafe extern "C" fn(*mut c_void, *const c_void, u64) -> i64;
type PcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type PcmDrop = unsafe extern "C" fn(*mut c_void) -> c_int;
type PcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;
type StrError = unsafe extern "C" fn(c_int) -> *const c_char;

struct Alsa {
    open: PcmOpen,
    set_params: PcmSetParams,
    writei: PcmWritei,
    recover: PcmRecover,
    drop: PcmDrop,
    close: PcmClose,
    strerror: StrError,
    _lib: Library,
}

impl Alsa {
    fn load() -> Result<Self, AudioError> {
        let lib = unsafe { Library::new("libasound.so.2") }
            .or_else(|_| unsafe { Library::new("libasound.so") })
            .map_err(|e| AudioError::Device(format!("libasound: {e}")))?;
        macro_rules! get {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes())
                    .map_err(|e| AudioError::Device(format!("{}: {e}", $name)))?
            };
        }
        unsafe {
            Ok(Alsa {
                open: get!("snd_pcm_open"),
                set_params: get!("snd_pcm_set_params"),
                writei: get!("snd_pcm_writei"),
                recover: get!("snd_pcm_recover"),
                drop: get!("snd_pcm_drop"),
                close: get!("snd_pcm_close"),
                strerror: get!("snd_strerror"),
                _lib: lib,
            })
        }
    }

    fn message(&self, err: c_int) -> String {
        let text = unsafe { (self.strerror)(err) };
        match text.is_null() {
            true => format!("error {err}"),
            false => unsafe { CStr::from_ptr(text) }
                .to_string_lossy()
                .into_owned(),
        }
    }

    fn open_pcm(&self, rate: u32) -> Result<*mut c_void, AudioError> {
        let started = std::time::Instant::now();
        let mut last = AudioError::NoDevice;
        for name in DEVICES {
            let Ok(cname) = CString::new(name) else {
                continue;
            };
            let mut pcm: *mut c_void = std::ptr::null_mut();
            let err = unsafe { (self.open)(&mut pcm, cname.as_ptr(), SND_PCM_STREAM_PLAYBACK, 0) };
            if err < 0 || pcm.is_null() {
                last = AudioError::Device(format!("{name}: {}", self.message(err)));
                continue;
            }
            let err = unsafe {
                (self.set_params)(
                    pcm,
                    SND_PCM_FORMAT_S16_LE,
                    SND_PCM_ACCESS_RW_INTERLEAVED,
                    CHANNELS,
                    rate,
                    1,
                    LATENCY_US,
                )
            };
            if err < 0 {
                unsafe { (self.close)(pcm) };
                last = AudioError::Config(format!("{name}: {}", self.message(err)));
                continue;
            }
            eprintln!(
                "slot: audio {name} at {rate} Hz, opened in {} ms",
                started.elapsed().as_millis()
            );
            return Ok(pcm);
        }
        Err(last)
    }
}

pub struct AlsaSink {
    ring: Arc<Ring>,
    device: Option<Device>,
}

struct Device {
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

impl AlsaSink {
    pub fn new() -> Self {
        AlsaSink {
            ring: Arc::new(Ring::new(0)),
            device: None,
        }
    }

    fn close(&mut self) {
        if let Some(d) = self.device.take() {
            d.stop.store(true, Ordering::Relaxed);
            let _ = d.join.join();
        }
    }
}

impl Default for AlsaSink {
    fn default() -> Self {
        AlsaSink::new()
    }
}

impl Drop for AlsaSink {
    fn drop(&mut self) {
        self.close();
    }
}

impl AudioSink for AlsaSink {
    fn open(&mut self, sample_rate: u32) -> Result<(), AudioError> {
        self.close();
        let ring = self.ring.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("slot-audio".into())
            .spawn(move || match play(&ring, sample_rate) {
                Ok(mut device) => {
                    let _ = ready_tx.send(Ok(()));
                    device.run(&ring, &flag);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.device = Some(Device { stop, join });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                let _ = join.join();
                Err(AudioError::Device("output thread stopped".into()))
            }
        }
    }

    fn ring(&self) -> Arc<Ring> {
        self.ring.clone()
    }
}

/// An open PCM handle, carried to the thread that opened or will close it.
struct Pcm(*mut c_void);

// SAFETY: a handle is used by one thread at a time; it moves whole between
// the playback thread and the helper that opens or closes it.
unsafe impl Send for Pcm {}

struct Playback {
    alsa: Arc<Alsa>,
    pcm: Option<*mut c_void>,
    rate: u32,
    /// The device being opened in the background, once it reports back.
    opening: Option<mpsc::Receiver<Result<Pcm, AudioError>>>,
    /// The device being closed in the background.
    closing: Option<JoinHandle<()>>,
}

fn play(ring: &Arc<Ring>, sample_rate: u32) -> Result<Playback, AudioError> {
    let alsa = Alsa::load()?;
    let pcm = alsa.open_pcm(sample_rate)?;
    ring.reopen(sample_rate);
    Ok(Playback {
        alsa: Arc::new(alsa),
        pcm: Some(pcm),
        rate: sample_rate,
        opening: None,
        closing: None,
    })
}

impl Playback {
    fn run(&mut self, ring: &Ring, stop: &AtomicBool) {
        let mut buf = vec![0i16; PERIOD_FRAMES * CHANNELS as usize];
        let span = Duration::from_secs_f64(PERIOD_FRAMES as f64 / self.rate as f64);
        let mut silence = Silence::new(RELEASE_AFTER);
        // While the device is let go, the ring still drains in real time, so
        // a game keeps its place in the sound when the device comes back.
        // Sleeping a period each time drains slower than that, since every
        // sleep runs a little long; the metronome keeps to the clock.
        let mut metronome = Metronome::new(span);
        // Opening and closing the device each take a quarter of a second on
        // the H700. Done here, they stall the drain and the ring overflows,
        // dropping the start of whatever sound woke the device. They run on
        // helper threads instead, and the ring drains by the clock meanwhile.
        while !stop.load(Ordering::Relaxed) {
            if !self.take_opened() {
                return;
            }
            ring.fill(&mut buf);
            let hold = silence.hear(&buf, span);
            match (self.pcm, hold) {
                (Some(_), false) => {
                    self.release();
                    eprintln!("slot: audio released after silence");
                }
                (None, true) if self.opening.is_none() => self.open_behind(),
                _ => {}
            }
            match self.pcm {
                Some(pcm) => {
                    metronome.reset();
                    if !self.write(pcm, &buf) {
                        return;
                    }
                }
                None => {
                    let wait = metronome.wait(std::time::Instant::now());
                    if !wait.is_zero() {
                        std::thread::sleep(wait);
                    }
                }
            }
        }
    }

    fn write(&self, pcm: *mut c_void, buf: &[i16]) -> bool {
        let mut written = 0;
        while written < PERIOD_FRAMES {
            let at = written * CHANNELS as usize;
            let frames = unsafe {
                (self.alsa.writei)(
                    pcm,
                    buf[at..].as_ptr() as *const c_void,
                    (PERIOD_FRAMES - written) as u64,
                )
            };
            if frames < 0 {
                let err = unsafe { (self.alsa.recover)(pcm, frames as c_int, 1) };
                if err < 0 {
                    eprintln!("slot: audio: {}", self.alsa.message(err));
                    return false;
                }
                continue;
            }
            written += frames as usize;
        }
        true
    }

    /// Starts opening the device on a helper thread, after any close still
    /// in flight, since the device may refuse to open while it is closing.
    fn open_behind(&mut self) {
        let (tx, rx) = mpsc::channel();
        let alsa = self.alsa.clone();
        let rate = self.rate;
        let closing = self.closing.take();
        let opener = std::thread::Builder::new()
            .name("slot-audio-open".into())
            .spawn(move || {
                if let Some(c) = closing {
                    let _ = c.join();
                }
                let _ = tx.send(alsa.open_pcm(rate).map(Pcm));
            });
        match opener {
            Ok(_) => self.opening = Some(rx),
            Err(e) => eprintln!("slot: audio: open thread: {e}"),
        }
    }

    /// Picks up a device the helper finished opening. False when opening
    /// failed, which ends playback as a failed open always has.
    fn take_opened(&mut self) -> bool {
        let Some(rx) = &self.opening else {
            return true;
        };
        match rx.try_recv() {
            Ok(Ok(pcm)) => {
                self.pcm = Some(pcm.0);
                self.opening = None;
                true
            }
            Ok(Err(e)) => {
                eprintln!("slot: audio: {e}");
                self.opening = None;
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.opening = None;
                true
            }
        }
    }

    /// Lets the device go, closing it on a helper thread.
    fn release(&mut self) {
        let Some(raw) = self.pcm.take() else {
            return;
        };
        let alsa = self.alsa.clone();
        let handle = Pcm(raw);
        let closer = std::thread::Builder::new()
            .name("slot-audio-close".into())
            .spawn(move || close(&alsa, handle));
        match closer {
            Ok(join) => self.closing = Some(join),
            // The closure never ran, so the handle is still open: close it here.
            Err(_) => close(&self.alsa, Pcm(raw)),
        }
    }
}

fn close(alsa: &Alsa, pcm: Pcm) {
    unsafe {
        (alsa.drop)(pcm.0);
        (alsa.close)(pcm.0);
    }
}

impl Playback {
    /// Waits out any open or close still in flight and closes what is open,
    /// so nothing is left holding the device once playback ends.
    fn finish(&mut self) {
        if let Some(pcm) = self.pcm.take() {
            close(&self.alsa, Pcm(pcm));
        }
        if let Some(rx) = self.opening.take() {
            if let Ok(Ok(pcm)) = rx.recv() {
                close(&self.alsa, pcm);
            }
        }
        if let Some(c) = self.closing.take() {
            let _ = c.join();
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.finish();
    }
}
