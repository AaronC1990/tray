use slot_gfx::{game_view, GameFit, GameView, MAX_TRIM_PER_SIDE, SRC_H, SRC_W, WHOLE_TEXTURE};

const RG_SP: (u32, u32) = (720, 480);
const RG35XXSP: (u32, u32) = (640, 427);

const GBA: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const GB: [f32; 4] = [40.0 / 240.0, 8.0 / 160.0, 200.0 / 240.0, 152.0 / 160.0];

/// Panel pixels per game pixel along x and y.
fn scale(canvas: (u32, u32), v: GameView) -> (f32, f32) {
    (
        canvas.0 as f32 / (v.uv[2] * SRC_W as f32),
        canvas.1 as f32 / (v.uv[3] * SRC_H as f32),
    )
}

/// Where a texel edge lands on the panel, in pixels.
fn edge_px(canvas: (u32, u32), v: GameView, texel: (f32, f32)) -> (f32, f32) {
    let (sx, sy) = scale(canvas, v);
    (
        (texel.0 - v.uv[0] * SRC_W as f32) * sx,
        (texel.1 - v.uv[1] * SRC_H as f32) * sy,
    )
}

#[test]
fn fill_keeps_the_source_rect_exactly_as_before() {
    let src = [0.1, 0.2, 0.3, 0.4];
    for canvas in [RG_SP, RG35XXSP] {
        let v = game_view(canvas, GB, src, GameFit::Fill);
        assert_eq!(v, GameView::fill(src));
        assert_eq!(v.visible, [0.0, 0.0, 1.0, 1.0]);
    }
}

#[test]
fn on_the_rg_sp_whole_is_the_same_picture_fill_already_drew() {
    for pic in [GBA, GB] {
        let v = game_view(RG_SP, pic, WHOLE_TEXTURE, GameFit::Whole);
        assert_eq!(v, GameView::fill(WHOLE_TEXTURE), "picture {pic:?}");
    }
}

#[test]
fn a_game_boy_picture_is_drawn_at_3x_on_a_640_wide_panel() {
    let v = game_view(RG35XXSP, GB, WHOLE_TEXTURE, GameFit::Whole);
    assert_eq!(scale(RG35XXSP, v), (3.0, 3.0));
    let left = edge_px(RG35XXSP, v, (40.0, 0.0)).0;
    let right = edge_px(RG35XXSP, v, (200.0, 0.0)).0;
    assert_eq!((left, right), (80.0, 560.0), "centred across the panel");
}

#[test]
fn the_3x_game_boy_picture_loses_one_row_at_each_edge_and_no_more() {
    let v = game_view(RG35XXSP, GB, WHOLE_TEXTURE, GameFit::Whole);
    let rows = |y: f32| y * SRC_H as f32;
    assert_eq!((rows(v.visible[1]), rows(v.visible[3])), (9.0, 151.0));
    assert_eq!(MAX_TRIM_PER_SIDE, 1);
    let (_, top) = edge_px(RG35XXSP, v, (0.0, 9.0));
    let (_, bottom) = edge_px(RG35XXSP, v, (0.0, 151.0));
    assert_eq!(
        top, 0.0,
        "the first shown row starts on the panel's top edge"
    );
    assert!(bottom <= RG35XXSP.1 as f32);
    assert_eq!(v.visible[0], 0.0, "no columns are trimmed");
    assert_eq!(v.visible[2], 1.0, "no columns are trimmed");
}

#[test]
fn a_gba_picture_is_drawn_at_2x_on_a_640_wide_panel_rather_than_cropped() {
    let v = game_view(RG35XXSP, GBA, WHOLE_TEXTURE, GameFit::Whole);
    assert_eq!(scale(RG35XXSP, v), (2.0, 2.0));
    assert_eq!(v.visible, [0.0, 0.0, 1.0, 1.0], "every GBA pixel shows");
    let (x0, y0) = edge_px(RG35XXSP, v, (0.0, 0.0));
    let (x1, y1) = edge_px(RG35XXSP, v, (240.0, 160.0));
    assert_eq!((x0, x1), (80.0, 560.0));
    assert_eq!((y0, y1), (53.0, 373.0));
}

#[test]
fn every_whole_layout_lands_game_pixels_on_panel_pixel_edges() {
    for canvas in [RG_SP, RG35XXSP, (800, 533), (1280, 853), (480, 320)] {
        for pic in [GBA, GB] {
            let v = game_view(canvas, pic, WHOLE_TEXTURE, GameFit::Whole);
            let (sx, sy) = scale(canvas, v);
            assert!((sx - sx.round()).abs() < 1e-3, "{canvas:?} {pic:?} {sx}");
            assert!((sy - sy.round()).abs() < 1e-3, "{canvas:?} {pic:?} {sy}");
            let (ex, ey) = edge_px(canvas, v, (pic[0] * 240.0, pic[1] * 160.0));
            assert!((ex - ex.round()).abs() < 1e-3, "{canvas:?} {pic:?} x {ex}");
            assert!((ey - ey.round()).abs() < 1e-3, "{canvas:?} {pic:?} y {ey}");
        }
    }
}
