//! Renders a PPM frame through each picture look at 640x480, for looking
//! at. `look_shot <frame.ppm> <out prefix>`
use slot_gfx::{HeadlessSurface, PictureLook, PicturePass};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let text = String::from_utf8_lossy(&data[..32]).to_string();
    let mut it = text.split_whitespace();
    it.next();
    let w: u32 = it.next().unwrap().parse().unwrap();
    let h: u32 = it.next().unwrap().parse().unwrap();
    let header = data.len() - (w * h * 3) as usize;
    let xrgb: Vec<u8> = data[header..]
        .chunks_exact(3)
        .flat_map(|p| [p[2], p[1], p[0], 0])
        .collect();
    let surface = HeadlessSurface::new().unwrap();
    let mut pass = PicturePass::new(&surface).unwrap();
    pass.upload(&xrgb, w, h);
    for look in PictureLook::ALL {
        pass.set_look(look);
        pass.draw((640, 480));
        let mut out = vec![0u8; 640 * 480 * 4];
        unsafe {
            gl::ReadPixels(
                0,
                0,
                640,
                480,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                out.as_mut_ptr() as *mut _,
            );
        }
        let top: Vec<u8> = out.chunks_exact(640 * 4).rev().flatten().copied().collect();
        let f = std::fs::File::create(format!("{}-{}.png", a[2], look.name())).unwrap();
        let mut e = png::Encoder::new(f, 640, 480);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&top).unwrap();
    }
}
