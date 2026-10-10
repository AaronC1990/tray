use slot_gfx::area_rgba;

fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    std::iter::repeat_n(px, (w * h) as usize)
        .flatten()
        .collect()
}

#[test]
fn a_flat_image_stays_the_same_colour_at_any_size() {
    let out = area_rgba(&solid(720, 30, [10, 120, 230, 255]), 720, 30, 640, 27);
    assert_eq!(out.len(), 640 * 27 * 4);
    assert!(out.chunks_exact(4).all(|p| p == [10, 120, 230, 255]));
}

#[test]
fn transparent_pixels_do_not_darken_an_opaque_edge() {
    // White text on a transparent background, the usual face. A naive
    // average would pull the edge toward the transparent pixels' black.
    let (w, h) = (9u32, 1u32);
    let mut src = solid(w, h, [0, 0, 0, 0]);
    for x in 0..4usize {
        src[x * 4..x * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    let out = area_rgba(&src, w, h, 8, 1);
    for p in out.chunks_exact(4).filter(|p| p[3] > 0) {
        assert_eq!(&p[..3], &[255, 255, 255], "edge went grey: {p:?}");
    }
}

#[test]
fn coverage_is_kept_when_shrinking() {
    let (w, h) = (720u32, 1u32);
    let src: Vec<u8> = (0..w)
        .flat_map(|x| {
            if x % 2 == 0 {
                [255, 255, 255, 255]
            } else {
                [255, 255, 255, 0]
            }
        })
        .collect();
    let out = area_rgba(&src, w, h, 640, 1);
    let total: u32 = out.chunks_exact(4).map(|p| p[3] as u32).sum();
    let want = 640 * 255 / 2;
    assert!(total.abs_diff(want) <= 640, "alpha {total} against {want}");
}
