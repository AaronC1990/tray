//! Renders one GB frame the way the device composes it on a 640x480 panel
//! (canvas, then bezel), reading back the final framebuffer.
use slot_gfx::{Compositor, GameFit, HeadlessSurface, ScreenEffect, SRC_H, SRC_W};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (out, bezel, fit) = (&args[1], &args[2], &args[3]);
    let surface = HeadlessSurface::new().unwrap();
    let mut c = Compositor::new(&surface).unwrap();
    let panel = (640u32, 480u32);
    let img = std::fs::read(bezel).unwrap();
    let dec = png::Decoder::new(std::io::Cursor::new(img));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    r.next_frame(&mut buf).unwrap();
    c.set_bezel(640, 480, &buf);
    let scene = std::env::var("GB_SCENE")
        .ok()
        .map(|p| std::fs::read(p).unwrap());
    let mut shot = Vec::new();
    for y in 0..SRC_H as usize {
        for x in 0..SRC_W as usize {
            let inside = (40..200).contains(&x) && (8..152).contains(&y);
            let px = match (&scene, inside) {
                (Some(rgb), true) => {
                    let o = ((y - 8) * 160 + (x - 40)) * 3;
                    [rgb[o + 2], rgb[o + 1], rgb[o], 0]
                }
                (Some(_), false) => [0, 0, 0, 0],
                (None, _) => {
                    let v: u8 = match (inside, ((x / 8) + (y / 8)) % 2 == 0) {
                        (false, _) => 200,
                        (true, true) => 220,
                        (true, false) => 150,
                    };
                    [v / 2, v, v / 2, 0]
                }
            };
            shot.extend_from_slice(&px);
        }
    }
    let gb = [40.0 / 240.0, 8.0 / 160.0, 200.0 / 240.0, 152.0 / 160.0];
    c.set_picture(gb);
    c.set_game_source_rect([0.0, 0.0, 1.0, 1.0]);
    c.set_game_fit(if fit == "whole" {
        GameFit::Whole
    } else {
        GameFit::Fill
    });
    c.fit(panel);
    c.set_screen_effect(match std::env::var("EFFECT").as_deref() {
        Ok("off") => ScreenEffect::None,
        _ => ScreenEffect::Grid,
    });
    c.set_screen_power(1.0);
    c.begin_frame();
    c.upload_game(&shot);
    c.draw_game();
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
    let mut top = Vec::with_capacity(px.len());
    for row in px.chunks_exact(640 * 4).rev() {
        top.extend_from_slice(row);
    }
    let f = std::fs::File::create(out).unwrap();
    let mut e = png::Encoder::new(f, 640, 480);
    e.set_color(png::ColorType::Rgba);
    e.write_header().unwrap().write_image_data(&top).unwrap();
}
