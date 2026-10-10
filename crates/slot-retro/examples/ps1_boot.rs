//! Boots PlayStation content through a libretro core with no UI, and writes
//! chosen frames as PPM images. Proves a core, its content and the bridge's
//! native video work before any frontend is involved.
//!
//! Usage: ps1_boot <core.so> <content> <system_dir> <out_dir> [frames...]
use slot_retro::{ButtonMask, LibretroCore, RetroCore};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (core, content, system, out) = (&args[1], &args[2], &args[3], &args[4]);
    let shots: Vec<u32> = args[5..].iter().map(|a| a.parse().unwrap()).collect();
    let last = shots.iter().copied().max().unwrap_or(120);
    std::fs::create_dir_all(out).unwrap();

    let mut c = LibretroCore::open_with(Path::new(core), Path::new(system), Path::new(out))
        .expect("open core");
    c.set_native_video(true);
    if std::env::var_os("PS1_DEFAULTS").is_some() {
        c.set_answer_defaults(true);
    }
    if std::env::var_os("PS1_LIST_OPTIONS").is_some() {
        let mut keys: Vec<_> = c.declared_options().iter().collect();
        keys.sort();
        for (k, v) in keys {
            println!("option {k}: {}", v.join("|"));
        }
    }
    println!("needs full path: {}", c.needs_full_path());
    c.load(Path::new(content)).expect("load content");
    let av = c.av_info();
    println!("fps {:.3}, sample rate {}", av.fps, av.sample_rate);

    let mut audio = 0usize;
    for n in 1..=last {
        c.run_frame(ButtonMask::default());
        audio += c.take_audio().len();
        if shots.contains(&n) {
            let (px, w, h) = c.frame();
            let mut ppm = format!("P6 {w} {h} 255\n").into_bytes();
            for p in px.chunks_exact(4) {
                ppm.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            let path = format!("{out}/frame{n:04}.ppm");
            std::fs::write(&path, ppm).unwrap();
            println!("frame {n}: {w}x{h} -> {path}");
        }
    }
    println!("audio samples over {last} frames: {audio}");
    let threads = std::fs::read_dir("/proc/self/task")
        .map(|d| d.count())
        .unwrap_or(0);
    println!("threads: {threads}");
}
