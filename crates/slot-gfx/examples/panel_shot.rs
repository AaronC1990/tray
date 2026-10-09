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
    let mut shot = Vec::new();
    for y in 0..SRC_H as usize {
        for x in 0..SRC_W as usize {
            let inside = (40..200).contains(&x) && (8..152).contains(&y);
            let v: u8 = if inside {
                if ((x - 40) / 8 + (y - 8) / 8) % 2 == 0 {
                    220
                } else {
                    150
                }
            } else {
                200
            };
            shot.extend_from_slice(&[v / 2, v, v / 2, 0]);
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
    c.set_screen_effect(ScreenEffect::Grid);
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
