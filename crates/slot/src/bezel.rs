use std::path::{Path, PathBuf};

pub const BEZELS: &str = "System/bezels";

/// Bezels for a Game Boy picture at actual size, which leaves the panel's
/// sides bare. They frame those sides as well as the strip below.
pub const GB_BEZELS: &str = "System/bezels/gb";

fn dims(path: &Path) -> Option<(u32, u32)> {
    if !path.extension()?.eq_ignore_ascii_case("png") {
        return None;
    }
    let (w, h) = path.file_stem()?.to_str()?.split_once('x')?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

pub fn pick(root: &Path, panel: (u32, u32)) -> Option<PathBuf> {
    pick_in(root, BEZELS, panel)
}

pub fn pick_gb(root: &Path, panel: (u32, u32)) -> Option<PathBuf> {
    pick_in(root, GB_BEZELS, panel)
}

fn pick_in(root: &Path, dir: &str, panel: (u32, u32)) -> Option<PathBuf> {
    let mut fits: Vec<((u32, u32), PathBuf)> = std::fs::read_dir(root.join(dir))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !slot_store::is_hidden(p))
        .filter_map(|p| Some((dims(&p)?, p)))
        .filter(|((w, h), _)| {
            u64::from(*w) * u64::from(panel.1) == u64::from(*h) * u64::from(panel.0)
        })
        .collect();
    fits.sort_by_key(|((w, _), p)| (w.abs_diff(panel.0), p.clone()));
    fits.into_iter().next().map(|(_, p)| p)
}

/// Where a bezel's power light sits on a `panel`, and its radius, from the
/// `.light` file beside it: the lens's middle and radius in the bezel
/// image's own pixels, scaled as the image is to the panel.
pub fn light_of(bezel: &Path, panel: (u32, u32)) -> Option<(f32, f32, f32)> {
    let (w, h) = dims(bezel)?;
    let text = std::fs::read_to_string(bezel.with_extension("light")).ok()?;
    let mut it = text.split_whitespace().map(|v| v.parse::<f32>().ok());
    let (x, y, r) = (it.next()??, it.next()??, it.next()??);
    let (sx, sy) = (panel.0 as f32 / w as f32, panel.1 as f32 / h as f32);
    Some((x * sx, y * sy, r * sx.min(sy)))
}

/// The power light's colour for a battery `percent` full: green from 70,
/// amber from 30, and red below. Green when the battery is not known.
pub fn light_colour(percent: Option<u8>) -> [f32; 3] {
    match percent {
        Some(p) if p < 30 => [1.0, 0.16, 0.10],
        Some(p) if p < 70 => [1.0, 0.68, 0.10],
        _ => [0.30, 0.95, 0.40],
    }
}
