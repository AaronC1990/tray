//! Renders stills of the disc shelf as slot's frame shows it: still, or a
//! disc going in, coming out or being changed. Slot's wallpaper, clock and
//! name are left out; its slot is drawn.
//! `motion_shot <games dir> <out prefix> <shelf|insert|eject|swap> <t>...`
use std::path::Path;

use slot_gfx::{Compositor, Draw, HeadlessSurface};
use tray::carousel::Carousel;
use tray::library::scan;
use tray::motion::{Eject, Insert, Swap};
use tray::shelf::{Look, ShelfView};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let games = scan(Path::new(&a[1]));
    let surface = HeadlessSurface::new().unwrap();
    let panel = (640u32, 480u32);
    let mut c = Compositor::new(&surface).unwrap();
    c.fit(panel);
    let labels = Path::new("/none");
    let mut view = ShelfView::new(&mut c, labels);
    let initials = games
        .iter()
        .map(|g| slot_store::initial(&g.title))
        .collect();
    let carousel = Carousel::new(initials, 0);
    for t in &a[4..] {
        let t: f32 = t.parse().unwrap();
        let (look, veil) = match a[3].as_str() {
            "insert" => {
                let m = Insert::start().at(t, t > 0.73);
                (m.look(), m.veil())
            }
            "eject" => {
                let m = Eject::start().at(t);
                (m.look(), m.veil())
            }
            "swap" => {
                let m = Swap::start(&mut view, &mut c, labels, &games[0], 0, 1).at(t);
                (m.look(), m.veil())
            }
            _ => (Look::default(), 0.0),
        };
        let d = view.draws(&mut c, &games, &carousel, &look);
        let mut items = vec![Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: 720.0,
            h: 480.0,
            colour: [0.12, 0.13, 0.16, 1.0],
        }];
        items.extend(d.row);
        items.extend(d.back);
        items.extend(d.seated);
        items.extend(d.front);
        items.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: 720.0,
            h: 480.0,
            colour: [0.0, 0.0, 0.0, veil],
        });
        c.begin_frame();
        c.draw_list(&items);
        c.end_frame(panel);
        let mut px = vec![0u8; 640 * 480 * 4];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
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
