//! Disc faces, drawn in code: a printed label, the clear plastic around the
//! hub, and the hole. A game's own label art, when the card has it, is
//! printed on the label in place of its colour and title.
use std::path::Path;

/// Where each part of a disc ends, as a fraction of its radius.
const HOLE: f32 = 0.115;
const HUB: f32 = 0.31;
const STACK_RING: (f32, f32) = (0.215, 0.235);
const RIM: f32 = 0.975;

/// Coverage of the inside of a circle of radius `r` for a pixel whose centre
/// is `d` from the middle, so every edge is smooth at any size.
fn inside(d: f32, r: f32) -> f32 {
    (r - d + 0.5).clamp(0.0, 1.0)
}

/// Light text on a dark label, dark text on a light one.
fn ink_for(colour: [u8; 3]) -> [u8; 3] {
    let [r, g, b] = colour.map(|c| c as f32);
    match 0.299 * r + 0.587 * g + 0.114 * b > 150.0 {
        true => [24, 24, 28],
        false => [246, 244, 238],
    }
}

/// A disc `size` pixels across, RGBA with straight alpha. `art`, when given,
/// is `size` by `size` RGBA and fills the label; otherwise the label is
/// `colour` with `title` printed below the hub.
pub fn disc_face(size: u32, title: &str, colour: [u8; 3], art: Option<&[u8]>) -> Vec<u8> {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    let r = size as f32 / 2.0;
    let ink = ink_for(colour);
    let text = match art {
        Some(_) => None,
        None => Some(title_coverage(size, title)),
    };
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
            let d = (dx * dx + dy * dy).sqrt();
            let disc = inside(d, r) * (1.0 - inside(d, HOLE * r));
            if disc <= 0.0 {
                continue;
            }
            let label = inside(d, RIM * r) * (1.0 - inside(d, HUB * r));
            let i = (y * n + x) * 4;
            let printed: [f32; 3] = match (art, &text) {
                (Some(a), _) => [a[i] as f32, a[i + 1] as f32, a[i + 2] as f32],
                (None, Some(cov)) => {
                    // A soft fall of light across the print, so a flat
                    // colour still reads as a surface.
                    let shade = 1.06 - 0.12 * (y as f32 / n as f32);
                    let base = colour.map(|c| (c as f32 * shade).min(255.0));
                    let a = cov[y * n + x] as f32 / 255.0;
                    [0, 1, 2].map(|c| base[c] + (ink[c] as f32 - base[c]) * a)
                }
                (None, None) => colour.map(|c| c as f32),
            };
            // The hub and rim are clear plastic over a silvered layer.
            let ring = d >= STACK_RING.0 * r && d < STACK_RING.1 * r;
            let plastic: ([f32; 3], f32) = match (d < HUB * r, ring) {
                (true, true) => ([226.0, 229.0, 234.0], 0.80),
                (true, false) => ([196.0, 201.0, 208.0], 0.55),
                _ => ([206.0, 208.0, 212.0], 1.0),
            };
            let rgb = [0, 1, 2].map(|c| plastic.0[c] + (printed[c] - plastic.0[c]) * label);
            let alpha = disc * (plastic.1 + (1.0 - plastic.1) * label);
            out[i] = rgb[0].round() as u8;
            out[i + 1] = rgb[1].round() as u8;
            out[i + 2] = rgb[2].round() as u8;
            out[i + 3] = (alpha * 255.0).round() as u8;
        }
    }
    out
}

/// The title's coverage over the whole face, set in the band below the hub
/// where the label is widest.
fn title_coverage(size: u32, title: &str) -> Vec<u8> {
    let n = size as usize;
    let mut out = vec![0u8; n * n];
    let Some(font) = slot_ui::text::label_font() else {
        return out;
    };
    let r = size as f32 / 2.0;
    let (bw, bh) = ((1.15 * r) as u32, (0.42 * r) as u32);
    if bw == 0 || bh == 0 {
        return out;
    }
    let max_px = (0.15 * r).max(8.0);
    let layout = slot_ui::text::fit_box(
        font,
        title,
        bw as f32,
        bh as f32,
        2,
        max_px,
        (max_px * 0.5).max(7.0),
    );
    let block = slot_ui::text::coverage(bw, bh, &layout);
    let (ox, oy) = ((r - bw as f32 / 2.0) as usize, (r + 0.36 * r) as usize);
    for (row, line) in block.chunks_exact(bw as usize).enumerate() {
        let y = oy + row;
        if y >= n {
            break;
        }
        let at = y * n + ox;
        out[at..at + line.len().min(n - ox)].copy_from_slice(&line[..line.len().min(n - ox)]);
    }
    out
}

/// The game's label art from `dir`, named after its file, scaled to cover
/// a `size` square. PNG or JPEG.
pub fn label_art(dir: &Path, stem: &str, size: u32) -> Option<Vec<u8>> {
    ["png", "jpg", "jpeg"]
        .iter()
        .map(|ext| dir.join(format!("{stem}.{ext}")))
        .find(|p| p.is_file())
        .and_then(|p| slot_ui::cover(&p, size, size))
}

/// A soft shadow for a disc `size` across, on a face `pad` larger each side.
pub fn shadow_face(size: u32, pad: u32) -> (u32, Vec<u8>) {
    let n = size + 2 * pad;
    let c = n as f32 / 2.0;
    let r = size as f32 / 2.0;
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            let t = ((r + pad as f32 - d) / (2.0 * pad as f32)).clamp(0.0, 1.0);
            let a = t * t * (3.0 - 2.0 * t) * 0.55;
            out[((y * n + x) * 4 + 3) as usize] = (a * 255.0).round() as u8;
        }
    }
    (n, out)
}

/// The glint a light above leaves on a disc. It is drawn still over a
/// turning disc, as a real glint stays put while the disc spins under it.
pub fn shine_face(size: u32) -> Vec<u8> {
    let n = size as usize;
    let r = size as f32 / 2.0;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
            let d = (dx * dx + dy * dy).sqrt();
            let on = inside(d, RIM * r) * (1.0 - inside(d, HUB * r));
            if on <= 0.0 {
                continue;
            }
            let angle = dy.atan2(dx) - std::f32::consts::FRAC_PI_4;
            let glint = (2.0 * angle).cos().max(0.0).powi(10);
            let i = (y * n + x) * 4;
            out[i..i + 3].copy_from_slice(&[255, 255, 255]);
            out[i + 3] = (on * glint * 0.16 * 255.0).round() as u8;
        }
    }
    out
}
