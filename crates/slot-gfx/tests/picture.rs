use slot_gfx::{fit_4x3, FrameLayout, HeadlessSurface, PicturePass};
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
    shoot_as(FrameLayout::Xrgb8888, &frame(w, h, px), w, h)
}

fn shoot_as(layout: FrameLayout, pixels: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let _g = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let mut pass = PicturePass::new(&surface).ok()?;
    pass.upload_as(layout, pixels, w, h);
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
    for (x, &got) in lit.iter().enumerate() {
        let (a, b) = (x as f32 / scale, (x + 1) as f32 / scale);
        let want: f32 = (a.floor() as usize..=(b - 1e-4).floor() as usize)
            .filter(|t| t % 2 == 0)
            .map(|t| (b.min(t as f32 + 1.0) - a.max(t as f32)).max(0.0))
            .sum::<f32>()
            / (b - a);
        assert!(
            (got - want).abs() <= 0.03,
            "column {x}: {got} where white covers {want}"
        );
    }
}

#[test]
fn a_16_bit_frame_lands_at_2x_with_its_colours_widened() {
    let px565 =
        |x: u32, y: u32| -> u16 { ((x % 32) << 11 | (y % 64) << 5 | ((x + y) % 32)) as u16 };
    let raw: Vec<u8> = (0..240u32)
        .flat_map(|y| (0..320u32).map(move |x| (x, y)))
        .flat_map(|(x, y)| px565(x, y).to_le_bytes())
        .collect();
    let Some(img) = shoot_as(FrameLayout::Rgb565, &raw, 320, 240) else {
        return;
    };
    let widen =
        |v: u16, bits: u32| ((v as f32) * 255.0 / ((1u32 << bits) - 1) as f32).round() as i32;
    for y in (0..240).step_by(7) {
        for x in (0..320).step_by(5) {
            let p = px565(x, y);
            let want = [
                widen(p >> 11, 5),
                widen((p >> 5) & 0x3f, 6),
                widen(p & 0x1f, 5),
            ];
            let got = at(&img, (x * 2 + 1) as usize, (y * 2 + 1) as usize);
            for c in 0..3 {
                assert!(
                    (got[c] as i32 - want[c]).abs() <= 1,
                    "frame pixel {x},{y}: {got:?} against {want:?}"
                );
            }
        }
    }
}

fn shoot_look(look: slot_gfx::PictureLook, w: u32, h: u32) -> Option<Vec<u8>> {
    let _g = GL.lock().unwrap_or_else(PoisonError::into_inner);
    let surface = HeadlessSurface::new().ok()?;
    let mut pass = PicturePass::new(&surface).ok()?;
    pass.upload(&frame(w, h, |_, _| [160, 160, 160]), w, h);
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
    let rows: Vec<&[u8]> = out.chunks_exact(640 * 4).rev().collect();
    Some(rows.concat())
}

#[test]
fn scanlines_darken_every_other_panel_row_of_a_240_line_frame() {
    use slot_gfx::PictureLook;
    let Some(img) = shoot_look(PictureLook::Scanlines, 320, 240) else {
        return;
    };
    for y in (100..140).step_by(2) {
        let (lit, gap) = (at(&img, 300, y)[1], at(&img, 300, y + 1)[1]);
        assert!(
            lit > 160 && gap < 90,
            "rows {y}/{}: {lit} then {gap}",
            y + 1
        );
    }
}

#[test]
fn scanlines_hold_back_on_a_480_line_frame() {
    use slot_gfx::PictureLook;
    let Some(img) = shoot_look(PictureLook::Scanlines, 512, 480) else {
        return;
    };
    let rows: Vec<u8> = (100..110).map(|y| at(&img, 300, y)[1]).collect();
    assert!(
        rows.windows(2).all(|w| w[0] == w[1]),
        "lines in a 480-line frame: {rows:?}"
    );
}

#[test]
fn the_grid_draws_one_panel_pixel_lines_between_frame_pixels() {
    use slot_gfx::PictureLook;
    let Some(img) = shoot_look(PictureLook::Grid, 320, 240) else {
        return;
    };
    // At 2x, the first panel column and row of each frame pixel are dark.
    assert!(at(&img, 300, 101)[1] < at(&img, 301, 101)[1]);
    assert!(at(&img, 301, 100)[1] < at(&img, 301, 101)[1]);
}

#[test]
fn the_crt_look_lights_red_green_and_blue_columns_in_turn() {
    use slot_gfx::PictureLook;
    let Some(img) = shoot_look(PictureLook::Crt, 320, 240) else {
        return;
    };
    let y = 100;
    let lit: Vec<[u8; 3]> = (300..303).map(|x| at(&img, x, y)).collect();
    let strongest: Vec<usize> = lit
        .iter()
        .map(|c| (0..3).max_by_key(|&i| c[i]).unwrap())
        .collect();
    let mut sorted = strongest.clone();
    sorted.sort();
    assert_eq!(sorted, [0, 1, 2], "columns {lit:?}");
}

#[test]
fn looks_step_round_both_ways_and_keep_their_names() {
    use slot_gfx::PictureLook;
    assert_eq!(PictureLook::Sharp.step(false), PictureLook::Dot);
    assert_eq!(PictureLook::Dot.step(true), PictureLook::Sharp);
    for look in PictureLook::ALL {
        assert_eq!(PictureLook::from_name(look.name()), Some(look));
    }
}

#[test]
fn dots_dim_toward_each_frame_pixels_far_corner() {
    use slot_gfx::PictureLook;
    let Some(img) = shoot_look(PictureLook::Dot, 320, 240) else {
        return;
    };
    let (tl, tr, br) = (
        at(&img, 300, 100)[1],
        at(&img, 301, 100)[1],
        at(&img, 301, 101)[1],
    );
    assert!(tl > tr && tr > br, "a dot's pixels: {tl} {tr} {br}");
}
