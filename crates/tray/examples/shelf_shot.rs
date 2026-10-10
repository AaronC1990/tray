//! Renders the shelf for a folder of games to a PNG, as the panel shows it.
//! `shelf_shot <games dir> <labels dir> <out.png> [steps right] [slide]`
use std::path::Path;

use slot_gfx::{HeadlessSurface, Overlay};
use slot_input::{Btn, RawEvent};
use tray::carousel::Carousel;
use tray::library::scan;
use tray::shelf::ShelfView;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let games = scan(Path::new(&a[1]));
    let steps: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let slide: f32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let surface = HeadlessSurface::new().unwrap();
    let panel = (640u32, 480u32);
    let mut overlay = Overlay::new(&surface, panel).unwrap();
    let mut view = ShelfView::new(&mut overlay, panel, Path::new(&a[2]));
    let initials = games
        .iter()
        .map(|g| slot_store::initial(&g.title))
        .collect();
    let mut c = Carousel::new(initials, 0);
    for i in 0..steps {
        c.handle(RawEvent::Down(Btn::Right), i as u64 * 10);
        c.handle(RawEvent::Up(Btn::Right), i as u64 * 10 + 5);
        c.tick(i as u64 * 10 + 6, 10.0);
    }
    if slide < 1.0 {
        c.handle(RawEvent::Down(Btn::Right), 1000);
        c.handle(RawEvent::Up(Btn::Right), 1005);
        // One tick of this length moves the row `slide` of the way.
        c.tick(1006, -(1.0 - slide).ln() / 14.0);
    }
    view.draw(&mut overlay, &games, &c, &tray::shelf::Look::default());
    view.tick(0.7, Some(c.selected()));
    view.draw(&mut overlay, &games, &c, &tray::shelf::Look::default());
    // SLIDE=<dx>: as tray shows the shelf sliding off over another screen,
    // here a plain red one.
    if let Some(dx) = std::env::var("SLIDE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
    {
        overlay.snapshot();
        overlay.draw([0.6, 0.1, 0.1], &[]);
        overlay.draw_over(
            &overlay
                .snapshot_draw(dx, 1.0)
                .into_iter()
                .collect::<Vec<_>>(),
        );
    }
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
    let f = std::fs::File::create(&a[3]).unwrap();
    let mut e = png::Encoder::new(f, 640, 480);
    e.set_color(png::ColorType::Rgba);
    e.write_header().unwrap().write_image_data(&top).unwrap();
}
