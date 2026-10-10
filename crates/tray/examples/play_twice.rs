//! Plays a disc twice in one process, quitting each after a set number of
//! frames, to show the core loads again cleanly after it is unloaded.
//! `play_twice <card root> <disc> <frames>`; writes each run's last frame
//! size and a checksum of the panel.
use std::path::Path;
use std::time::Instant;

use slot_gfx::{HeadlessSurface, PicturePass};
use slot_input::{Btn, InputSource, RawEvent};
use tray::play::{play, Console};

struct QuitAfter(u32, u32);

impl InputSource for QuitAfter {
    fn poll(&mut self, _now: u64) -> Vec<RawEvent> {
        self.1 += 1;
        match self.1 == self.0 {
            true => vec![RawEvent::Down(Btn::Menu), RawEvent::Down(Btn::Start)],
            false => Vec::new(),
        }
    }
}

fn panel_sum() -> u64 {
    let mut px = vec![0u8; 640 * 480 * 4];
    unsafe {
        gl::ReadPixels(0, 0, 640, 480, gl::RGBA, gl::UNSIGNED_BYTE, px.as_mut_ptr() as *mut _);
    }
    px.iter().map(|&b| b as u64).sum()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let frames: u32 = a[3].parse().unwrap();
    let mut surface = HeadlessSurface::new().unwrap();
    let mut picture = PicturePass::new(&surface).unwrap();
    for run in 0..2 {
        let mut input = QuitAfter(frames, 0);
        let mut console = Console {
            surface: &mut surface,
            picture: &mut picture,
            input: &mut input,
            started: Instant::now(),
            panel: (640, 480),
            period: None,
        };
        let r = play(&mut console, Path::new(&a[1]), Path::new(&a[2]));
        println!("run {run}: {r:?}, frame {:?}, panel sum {}", picture.size(), panel_sum());
    }
}
