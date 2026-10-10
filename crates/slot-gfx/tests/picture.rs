use slot_gfx::{fit_4x3, HeadlessSurface, PicturePass};
use std::sync::{Mutex, PoisonError};

static GL: Mutex<()> = Mutex::new(());

#[test]
fn a_4_by_3_panel_is_filled_and_a_wide_one_is_pillarboxed() {
    assert_eq!(fit_4x3((640, 480)), (0, 0, 640, 480));
    assert_eq!(fit_4x3((720, 480)), (40, 0, 640, 480));
    assert_eq!(fit_4x3((640, 640)), (0, 80, 640, 480));
}

fn frame(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let [r, g, b] = px(x, y);
            [b, g, r, 0]
        })
        .collect()
}

fn shoot(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 3]) -> Option<Vec<u8>> {
    let _g = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let mut pass = PicturePass::new(&surface).ok()?;
    pass.upload(&frame(w, h, px), w, h);
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
    // GL rows are bottom up; turn them top down.
    let rows: Vec<&[u8]> = out.chunks_exact(640 * 4).rev().collect();
    Some(rows.concat())
}

fn at(img: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * 640 + x) * 4;
    [img[o], img[o + 1], img[o + 2]]
}

#[test]
fn a_320_by_240_frame_lands_at_exactly_2x_on_the_rg35xxsp() {
    let colour = |x: u32, y: u32| {
        [
            (x * 7 % 256) as u8,
            (y * 13 % 256) as u8,
            ((x ^ y) & 1) as u8 * 200,
        ]
    };
    let Some(img) = shoot(320, 240, colour) else {
        return;
    };
    for y in 0..240 {
        for x in 0..320 {
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                assert_eq!(
                    at(&img, (x * 2 + dx) as usize, (y * 2 + dy) as usize),
                    colour(x, y),
                    "frame pixel {x},{y}"
                );
            }
        }
    }
}

#[test]
fn the_top_row_of_the_frame_is_drawn_at_the_top_of_the_panel() {
    let Some(img) = shoot(
        320,
        240,
        |_, y| if y == 0 { [255, 0, 0] } else { [0, 0, 255] },
    ) else {
        return;
    };
    assert_eq!(at(&img, 100, 0), [255, 0, 0]);
    assert_eq!(at(&img, 100, 479), [0, 0, 255]);
}

#[test]
fn a_256_wide_frame_still_fills_the_screen_with_even_pixels() {
    // 256x224 is 2.5x across and 2.14x down on a 640x480 panel.
    let Some(img) = shoot(256, 224, |x, _| if x % 2 == 0 { [255; 3] } else { [0; 3] }) else {
        return;
    };
    let lit: Vec<f32> = (0..640)
        .map(|x| at(&img, x, 200)[0] as f32 / 255.0)
        .collect();
    let scale = 2.5f32;
    for x in 0..640usize {
        let (a, b) = (x as f32 / scale, (x + 1) as f32 / scale);
        let want: f32 = (a.floor() as usize..=(b - 1e-4).floor() as usize)
            .filter(|t| t % 2 == 0)
            .map(|t| (b.min(t as f32 + 1.0) - a.max(t as f32)).max(0.0))
            .sum::<f32>()
            / (b - a);
        assert!(
            (lit[x] - want).abs() <= 0.03,
            "column {x}: {} where white covers {want}",
            lit[x]
        );
    }
}
