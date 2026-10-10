//! Renders stills of a disc going into the drive, coming out, or being
//! changed, as the panel shows them.
//! `motion_shot <games dir> <out prefix> <insert|eject|swap> <t>...`
use std::path::Path;

use slot_gfx::{Draw, HeadlessSurface, Overlay};
use tray::carousel::Carousel;
use tray::library::scan;
use tray::motion::{Eject, Insert, Swap};
use tray::shelf::ShelfView;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let games = scan(Path::new(&a[1]));
    let surface = HeadlessSurface::new().unwrap();
    let panel = (640u32, 480u32);
    let mut overlay = Overlay::new(&surface, panel).unwrap();
    let labels = Path::new("/none");
    let mut view = ShelfView::new(&mut overlay, panel, labels);
    let initials = games
        .iter()
        .map(|g| slot_store::initial(&g.title))
        .collect();
    let carousel = Carousel::new(initials, 0);
    let black = |alpha: f32| Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: 640.0,
        h: 480.0,
        colour: [0.0, 0.0, 0.0, alpha],
    };
    for t in &a[4..] {
        let t: f32 = t.parse().unwrap();
        let veil = match a[3].as_str() {
            "insert" => {
                let m = Insert::start(&view).at(t, t > 0.73);
                view.draw(&mut overlay, &games, &carousel, &m.look());
                m.veil()
            }
            "eject" => {
                let m = Eject::start(&view).at(t);
                view.draw(&mut overlay, &games, &carousel, &m.look());
                m.veil()
            }
            _ => {
                let m =
                    Swap::start(&mut view, &mut overlay, labels, &games[0], 0, 1, 2, 640.0).at(t);
                match m.shows_shelf() {
                    true => view.draw(&mut overlay, &games, &carousel, &m.look()),
                    false => overlay.draw([0.2, 0.3, 0.5], &[]),
                }
                m.veil()
            }
        };
        overlay.draw_over(&[black(veil)]);
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
        let f = std::fs::File::create(format!("{}-{}-{t}.png", a[2], a[3])).unwrap();
        let mut e = png::Encoder::new(f, 640, 480);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&top).unwrap();
    }
}
