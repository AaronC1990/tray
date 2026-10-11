//! Renders the Game Boy bezel with its power light lit in each colour, as
//! the panel shows it. `light_shot <bezel.png> <out prefix>`
use slot_gfx::{BezelLight, Compositor, HeadlessSurface};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let surface = HeadlessSurface::new().unwrap();
    let mut c = Compositor::new(&surface).unwrap();
    let panel = (640u32, 480u32);
    let img = std::fs::read(&a[1]).unwrap();
    let mut r = png::Decoder::new(std::io::Cursor::new(img))
        .read_info()
        .unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    r.next_frame(&mut buf).unwrap();
    c.set_gb_bezel(640, 480, &buf);
    c.use_gb_bezel(true);
    c.fit(panel);
    c.set_screen_power(1.0);
    for (name, rgb) in [
        ("green", [0.30, 0.95, 0.40]),
        ("amber", [1.0, 0.68, 0.10]),
        ("red", [1.0, 0.16, 0.10]),
    ] {
        c.set_gb_light(Some(BezelLight {
            at: (40.0, 132.0),
            radius: 4.0,
            rgb,
        }));
        c.begin_frame();
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
        let f = std::fs::File::create(format!("{}-{name}.png", a[2])).unwrap();
        let mut e = png::Encoder::new(f, 640, 480);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&top).unwrap();
    }
}
