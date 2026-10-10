//! Renders stills of the disc change at the given times, as the panel
//! shows them over a game. `swap_shot <out prefix> <t>...`
use std::path::{Path, PathBuf};

use slot_gfx::{Draw, HeadlessSurface, Overlay};
use tray::library::Game;
use tray::swap::{Swap, SwapView};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let surface = HeadlessSurface::new().unwrap();
    let panel = (640u32, 480u32);
    let mut overlay = Overlay::new(&surface, panel).unwrap();
    let mut view = SwapView::new(&mut overlay, panel);
    let game = Game {
        path: PathBuf::from("Final Fantasy IX (USA) (Disc 1).chd"),
        stem: "Final Fantasy IX (USA)".into(),
        title: "Final Fantasy IX".into(),
        discs: (1..=4)
            .map(|n| PathBuf::from(format!("Final Fantasy IX (USA) (Disc {n}).chd")))
            .collect(),
        grouped: true,
    };
    for t in &a[2..] {
        let t: f32 = t.parse().unwrap();
        let swap = Swap::start(&mut view, &mut overlay, Path::new("/none"), &game, 0, 1, 4).at(t);
        // A stand-in for the game's picture underneath.
        overlay.draw(
            [0.2, 0.3, 0.5],
            &[Draw::Rect {
                x: 80.0,
                y: 0.0,
                w: 480.0,
                h: 480.0,
                colour: [0.3, 0.5, 0.3, 1.0],
            }],
        );
        swap.draw(&view, &overlay);
        let mut px = vec![0u8; 640 * 480 * 4];
        unsafe {
            gl::ReadPixels(
                0,
                0,
                640,
                480,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                px.as_mut_ptr() as *mut _,
            );
        }
        let top: Vec<u8> = px.chunks_exact(640 * 4).rev().flatten().copied().collect();
        let f = std::fs::File::create(format!("{}-{t}.png", a[1])).unwrap();
        let mut e = png::Encoder::new(f, 640, 480);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&top).unwrap();
    }
}
