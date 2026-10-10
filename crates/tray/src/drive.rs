//! The disc shelf's slot: a slot-loading drive in a gloss black front, in
//! the manner of a PS3's, laid across the bottom of slot's frame where its
//! cart slot sits. Discs go in through a thin slit; a small light shows
//! while one is read, and an eject mark sits beside the slit.
use slot_gfx::{Compositor, Draw, TexId, OUT_H, OUT_W};

/// The drive's band, the same height as slot's slot band.
pub const BAND_Y: f32 = OUT_H as f32 - slot_ui::MOUTH_H;
const BAND_H: u32 = slot_ui::MOUTH_H as u32;
/// The slit, a little below the band's glossy top edge. A disc shows in
/// front of the band above it and goes behind the band past it.
const SLIT_TOP: u32 = 14;
const SLIT_H: u32 = 5;
const SLIT_W: f32 = 258.0;
pub const SLIT_Y: f32 = BAND_Y + SLIT_TOP as f32;
/// Where the slot's name is printed: on the band below the slit.
pub const NAME_Y: f32 = SLIT_Y + SLIT_H as f32 + 6.0;
const LIGHT_W: f32 = 8.0;
const LIGHT_H: f32 = 3.0;

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far a point is from a line segment, for the slit's rounded ends.
fn to_segment(px: f32, py: f32, ax: f32, bx: f32, y: f32) -> f32 {
    let x = px.clamp(ax, bx);
    ((px - x).powi(2) + (py - y).powi(2)).sqrt()
}

/// Coverage of the eject mark, a triangle over a bar, for a pixel at
/// (x, y) from the mark's top left, 10 by 9.
fn eject_mark(x: f32, y: f32) -> f32 {
    let tri = {
        // Apex at (5, 0), base from (0, 5.5) to (10, 5.5).
        let half = 5.0 * (y / 5.5);
        let inside = (0.0..=5.5).contains(&y) as u8 as f32;
        inside * smooth(-0.6, 0.6, half - (x - 5.0).abs())
    };
    let bar = ((7.0..=9.0).contains(&y) && (0.0..=10.0).contains(&x)) as u8 as f32;
    tri.max(bar)
}

/// The band as RGBA, `OUT_W` by the band's height.
pub fn drive_face() -> Vec<u8> {
    let (w, h) = (OUT_W as usize, BAND_H as usize);
    let cx = w as f32 / 2.0;
    let (slit_l, slit_r) = (cx - SLIT_W / 2.0, cx + SLIT_W / 2.0);
    let slit_mid = SLIT_TOP as f32 + SLIT_H as f32 / 2.0;
    let mark = (slit_r + 16.0, slit_mid - 4.5);
    let light = (slit_l - 16.0 - LIGHT_W, slit_mid - LIGHT_H / 2.0);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Gloss black, a touch lighter at the top.
            let down = fy / h as f32;
            let mut v = 30.0 - 20.0 * down;
            // A soft reflection along the upper part, strongest mid-band.
            let across = 0.55 + 0.45 * (std::f32::consts::PI * (fx - cx) / w as f32).cos();
            v += 16.0 * across * (1.0 - smooth(2.0, 12.0, fy)) * smooth(0.0, 2.0, fy);
            // A bright edge along the top, as where gloss meets the light.
            if y == 0 {
                v = 118.0;
            } else if y == 1 {
                v = v.max(62.0);
            }
            // The slit: a dark rounded gap, shadowed above, lit below.
            let d = to_segment(fx, fy, slit_l + 2.5, slit_r - 2.5, slit_mid);
            let gap = 1.0 - smooth(SLIT_H as f32 / 2.0 - 0.6, SLIT_H as f32 / 2.0 + 0.6, d);
            let lip_below = (fy - slit_mid - SLIT_H as f32 / 2.0).clamp(0.0, 1.5);
            if fx > slit_l && fx < slit_r && lip_below > 0.0 && lip_below < 1.5 {
                v = v.max(52.0);
            }
            let shadow = (fx > slit_l && fx < slit_r && fy < slit_mid - 2.0 && fy > slit_mid - 4.0)
                as u8 as f32;
            v -= 8.0 * shadow;
            v += (3.0 - v) * gap;
            // The eject mark, quietly grey.
            let m = eject_mark(fx - mark.0, fy - mark.1);
            v += (92.0 - v) * m * 0.8;
            // The light's window, dark until lit.
            let in_light =
                fx >= light.0 && fx < light.0 + LIGHT_W && fy >= light.1 && fy < light.1 + LIGHT_H;
            if in_light {
                v = 22.0;
            }
            let c = v.clamp(0.0, 255.0) as u8;
            let i = (y * w + x) * 4;
            out[i..i + 4].copy_from_slice(&[c, c, c.saturating_add(3), 255]);
        }
    }
    out
}

/// The drive's two halves on the GPU: above the slit, which a disc passes
/// in front of, and from the slit down, which hides it.
pub struct Drive {
    back: TexId,
    front: TexId,
}

impl Drive {
    pub fn new(compositor: &mut Compositor) -> Drive {
        let face = drive_face();
        let row = OUT_W as usize * 4;
        let split = SLIT_TOP as usize * row;
        let back = compositor.create_texture(OUT_W, SLIT_TOP, &face[..split]);
        let front = compositor.create_texture(OUT_W, BAND_H - SLIT_TOP, &face[split..]);
        Drive { back, front }
    }

    pub fn back(&self) -> Vec<Draw> {
        vec![Draw::Tex {
            x: 0.0,
            y: BAND_Y,
            w: OUT_W as f32,
            h: SLIT_TOP as f32,
            tex: self.back,
            alpha: 1.0,
        }]
    }

    /// The front, with the light lit as much as `light` says.
    pub fn front(&self, light: f32) -> Vec<Draw> {
        let mut out = vec![Draw::Tex {
            x: 0.0,
            y: SLIT_Y,
            w: OUT_W as f32,
            h: (BAND_H - SLIT_TOP) as f32,
            tex: self.front,
            alpha: 1.0,
        }];
        if light > 0.0 {
            let cx = OUT_W as f32 / 2.0;
            let mid = SLIT_Y + SLIT_H as f32 / 2.0;
            out.push(Draw::Rect {
                x: cx - SLIT_W / 2.0 - 16.0 - LIGHT_W,
                y: mid - LIGHT_H / 2.0,
                w: LIGHT_W,
                h: LIGHT_H,
                colour: [0.30, 0.95, 0.45, light],
            });
        }
        out
    }
}
