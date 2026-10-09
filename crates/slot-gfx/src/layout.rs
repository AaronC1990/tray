use crate::pipeline::{SRC_H, SRC_W};

/// How the game picture is placed on the canvas.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum GameFit {
    /// The source rect is stretched over the whole canvas. On the RG SP's
    /// 720x480 panel a GBA picture lands at exactly 3x this way; on other
    /// panels the scale may be fractional, which the game pass smooths.
    #[default]
    Fill,
    /// The picture is drawn at the largest whole-number scale that fits,
    /// so every game pixel covers the same number of panel pixels. A row or
    /// column may be trimmed from each edge (never more than
    /// `MAX_TRIM_PER_SIDE`) when that buys a whole step of scale, the same
    /// trade the grid already makes for a stretched Game Boy picture.
    Whole,
}

/// The most game pixels trimmed from each edge to reach the next whole
/// scale. A Game Boy picture at 3x on a 640 wide panel is 432 tall against
/// a 427 tall canvas, which costs one row top and bottom.
pub const MAX_TRIM_PER_SIDE: u32 = 1;

/// What the game pass needs to draw a picture: the texture rect spread over
/// the canvas quad, and the part of the texture allowed to show. Anything
/// outside `visible` is drawn black.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct GameView {
    /// x, y, w, h in texture coordinates across the full canvas.
    pub uv: [f32; 4],
    /// x0, y0, x1, y1 in texture coordinates.
    pub visible: [f32; 4],
}

impl GameView {
    pub fn fill(src: [f32; 4]) -> GameView {
        GameView {
            uv: src,
            visible: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// Places one axis at a whole scale. `at` and `len` are the picture's
/// start and length in texels, `out` the canvas length in pixels.
/// Returns (uv start, uv length, visible start, visible end) in texels,
/// or None when even 1x does not fit.
fn axis(at: u32, len: u32, texels: u32, out: u32, scale: u32) -> Option<(f32, f32, f32, f32)> {
    let shown = (out / scale).min(len);
    let mut trim = len - shown;
    if trim % 2 == 1 {
        trim += 1;
    }
    if trim / 2 > MAX_TRIM_PER_SIDE || trim >= len {
        return None;
    }
    let shown = len - trim;
    let first = at + trim / 2;
    let offset_px = (out - shown * scale) / 2;
    let start = first as f32 - offset_px as f32 / scale as f32;
    let span = out as f32 / scale as f32;
    let (lo, hi) = match trim {
        0 => (0.0, texels as f32),
        _ => (first as f32, (first + shown) as f32),
    };
    Some((start, span, lo, hi))
}

/// The view for a picture `pic` (x0, y0, x1, y1 in texture coordinates)
/// drawn with `fit` onto a canvas of `canvas` pixels. `src` is the source
/// rect the Fill mode spreads over the canvas.
pub fn game_view(canvas: (u32, u32), pic: [f32; 4], src: [f32; 4], fit: GameFit) -> GameView {
    if fit == GameFit::Fill || canvas.0 == 0 || canvas.1 == 0 {
        return GameView::fill(src);
    }
    let texel = |v: f32, n: u32| (v * n as f32).round().max(0.0) as u32;
    let (x0, y0) = (texel(pic[0], SRC_W), texel(pic[1], SRC_H));
    let (x1, y1) = (texel(pic[2], SRC_W), texel(pic[3], SRC_H));
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if w == 0 || h == 0 {
        return GameView::fill(src);
    }
    let first = (canvas.0 / w).min(canvas.1 / h).max(1);
    for scale in [first + 1, first] {
        let x = axis(x0, w, SRC_W, canvas.0, scale);
        let y = axis(y0, h, SRC_H, canvas.1, scale);
        if let (Some(x), Some(y)) = (x, y) {
            let (sw, sh) = (SRC_W as f32, SRC_H as f32);
            return GameView {
                uv: [x.0 / sw, y.0 / sh, x.1 / sw, y.1 / sh],
                visible: [x.2 / sw, y.2 / sh, x.3 / sw, y.3 / sh],
            };
        }
    }
    GameView::fill(src)
}
