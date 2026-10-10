//! Changing discs: the game pauses behind a view of the console from
//! above, the lid lifts, the old disc comes out, the next goes in and the
//! lid shuts. The core sees the lid open and close as it would on the
//! console, and keeps running underneath so the game notices.
use std::path::Path;

use slot_gfx::{Draw, Overlay, TexId};

use crate::disc::{disc_face, label_art, shadow_face};
use crate::library::Game;
use crate::play::DiscGame;
use crate::shelf::{text_face, BIG};

const WELL: u32 = 300;
const LID: u32 = 312;
/// The lid's widest angle open, from flat.
const LID_ANGLE: f32 = 1.4;
const TOP: [f32; 3] = [0.66, 0.66, 0.68];

/// When each part of the swap ends, in seconds from the press.
const SHOWN: f32 = 0.2;
const OPENED: f32 = 0.55;
const TAKEN_OUT: f32 = 0.85;
const PUT_IN: f32 = 1.15;
const SHUT: f32 = 1.5;
const DONE: f32 = 1.7;

/// Coverage of the inside of a circle of radius `r` for a pixel `d` from
/// the middle.
fn inside(d: f32, r: f32) -> f32 {
    (r - d + 0.5).clamp(0.0, 1.0)
}

/// The dark well the disc sits in, with the spindle in the middle.
fn well_face(size: u32) -> Vec<u8> {
    let n = size as usize;
    let r = size as f32 / 2.0;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
            let d = (dx * dx + dy * dy).sqrt();
            let a = inside(d, r);
            if a <= 0.0 {
                continue;
            }
            // Darker toward the wall, lit a little from the top.
            let wall = (d / r).powi(6);
            let lift = -dy / r * 6.0;
            let mut v = 46.0 - 26.0 * wall + lift;
            let spindle = inside(d, 0.1 * r);
            v += (172.0 - v) * spindle;
            let ring = inside(d, 0.16 * r) * (1.0 - inside(d, 0.12 * r));
            v += (70.0 - v) * ring;
            let i = (y * n + x) * 4;
            let c = v.clamp(0.0, 255.0) as u8;
            out[i..i + 4].copy_from_slice(&[c, c, c.saturating_add(3), (a * 255.0) as u8]);
        }
    }
    out
}

/// The lid, seen from above: plain grey plastic with a groove round it.
fn lid_face(size: u32) -> Vec<u8> {
    let n = size as usize;
    let r = size as f32 / 2.0;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - r, y as f32 + 0.5 - r);
            let d = (dx * dx + dy * dy).sqrt();
            let a = inside(d, r);
            if a <= 0.0 {
                continue;
            }
            let light = (-dx - dy) / (2.0 * r);
            let mut v = 196.0 + 18.0 * light;
            let groove = inside(d, 0.88 * r) * (1.0 - inside(d, 0.86 * r));
            v -= 46.0 * groove;
            let edge = 1.0 - inside(d, r - 3.0);
            v -= 40.0 * edge;
            let i = (y * n + x) * 4;
            let c = v.clamp(0.0, 255.0) as u8;
            out[i..i + 4].copy_from_slice(&[c, c, c.saturating_add(2), (a * 255.0) as u8]);
        }
    }
    out
}

/// The faces a swap draws, made once and reused.
pub struct SwapView {
    panel: (u32, u32),
    well: TexId,
    lid: TexId,
    lid_shadow: (TexId, u32),
    disc_shadow: (TexId, u32),
    old: TexId,
    new: TexId,
    caption: TexId,
    caption_size: (u32, u32),
}

impl SwapView {
    pub fn new(overlay: &mut Overlay, panel: (u32, u32)) -> Self {
        let (ls, lsf) = shadow_face(LID, 12);
        let (ds, dsf) = shadow_face(BIG, 8);
        let blank = vec![0u8; (BIG * BIG * 4) as usize];
        let caption_size = (360, 34);
        SwapView {
            panel,
            well: overlay.create_texture(WELL, WELL, &well_face(WELL)),
            lid: overlay.create_texture(LID, LID, &lid_face(LID)),
            lid_shadow: (overlay.create_texture(ls, ls, &lsf), ls),
            disc_shadow: (overlay.create_texture(ds, ds, &dsf), ds),
            old: overlay.create_texture(BIG, BIG, &blank),
            new: overlay.create_texture(BIG, BIG, &blank),
            caption: overlay.create_texture(
                caption_size.0,
                caption_size.1,
                &vec![0; (caption_size.0 * caption_size.1 * 4) as usize],
            ),
            caption_size,
        }
    }
}

/// Disc `index` of `game` as a face: its own label art if the card has it,
/// then the game's, then the game's colour with its title and number.
pub fn disc_label(labels: &Path, game: &Game, index: u32, size: u32) -> Vec<u8> {
    let own = game
        .discs
        .get(index as usize)
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned());
    let art = own
        .and_then(|stem| label_art(labels, &stem, size))
        .or_else(|| label_art(labels, &game.stem, size));
    let text = format!("{} Disc {}", game.title, index + 1);
    disc_face(
        size,
        &text,
        slot_ui::label_colour(&game.title),
        art.as_deref(),
    )
}

pub struct Swap {
    t: f32,
    to: u32,
    opened: bool,
    swapped: bool,
    shut: bool,
}

fn ease(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

fn span(t: f32, from: f32, to: f32) -> f32 {
    ((t - from) / (to - from)).clamp(0.0, 1.0)
}

impl Swap {
    /// Starts changing `game` from disc `from` to `to`, both from 0.
    pub fn start(
        view: &mut SwapView,
        overlay: &mut Overlay,
        labels: &Path,
        game: &Game,
        from: u32,
        to: u32,
        count: u32,
    ) -> Swap {
        overlay.update_texture(view.old, BIG, BIG, &disc_label(labels, game, from, BIG));
        overlay.update_texture(view.new, BIG, BIG, &disc_label(labels, game, to, BIG));
        let (cw, ch) = view.caption_size;
        let caption = format!("Disc {} of {count}", to + 1);
        overlay.update_texture(
            view.caption,
            cw,
            ch,
            &text_face(&caption, cw, ch, 1, 24.0, 12.0, [40, 40, 44]),
        );
        Swap {
            t: 0.0,
            to,
            opened: false,
            swapped: false,
            shut: false,
        }
    }

    /// Moves the swap `dt` seconds on, opening the lid, changing the disc
    /// and shutting the lid in the core as the picture does. True once done.
    pub fn tick(&mut self, dt: f32, game: &mut DiscGame) -> bool {
        self.t += dt;
        if !self.opened && self.t >= SHOWN {
            self.opened = true;
            game.set_lid_open(true);
        }
        if !self.swapped && self.t >= TAKEN_OUT {
            self.swapped = true;
            game.set_disc(self.to);
        }
        if !self.shut && self.t >= SHUT {
            self.shut = true;
            game.set_lid_open(false);
        }
        self.t >= DONE
    }

    /// Draws the console over whatever is on the panel.
    pub fn draw(&self, view: &SwapView, overlay: &Overlay) {
        let t = self.t;
        let (pw, ph) = (view.panel.0 as f32, view.panel.1 as f32);
        // The game dips to black, the console comes up out of black, and
        // the same on the way back: layers faded together would show
        // through each other.
        let half = SHOWN / 2.0;
        let back = SHUT + (DONE - SHUT) / 2.0;
        let scene = (half..back).contains(&t);
        let veil = match t {
            t if t < half => span(t, 0.0, half),
            t if t < SHOWN => 1.0 - span(t, half, SHOWN),
            t if t < SHUT => 0.0,
            t if t < back => span(t, SHUT, back),
            t => 1.0 - span(t, back, DONE),
        };
        let veil = Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: pw,
            h: ph,
            colour: [0.0, 0.0, 0.0, ease(veil)],
        };
        if !scene {
            overlay.draw_over(&[veil]);
            return;
        }
        let shown = 1.0;
        let (cx, cy) = (pw / 2.0, (ph * 0.45).round());
        let mut items = vec![Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: pw,
            h: ph,
            colour: [TOP[0], TOP[1], TOP[2], shown],
        }];
        let square = |size: f32, x: f32, y: f32| (x - size / 2.0, y - size / 2.0, size);
        let (x, y, s) = square(WELL as f32, cx, cy);
        items.push(Draw::Tex {
            x,
            y,
            w: s,
            h: s,
            tex: view.well,
            alpha: shown,
        });

        // The disc in the well: the old one until it comes out, then the
        // new one sliding in.
        let out = ease(span(t, OPENED, TAKEN_OUT));
        let into = 1.0 - ease(span(t, TAKEN_OUT, PUT_IN));
        let (tex, dx, lift) = match t < TAKEN_OUT {
            true => (view.old, out * pw * 0.8, (out * 3.0).min(1.0)),
            false => (view.new, -into * pw * 0.8, (into * 3.0).min(1.0)),
        };
        let size = BIG as f32 * (1.0 + 0.06 * lift);
        let shadow = view.disc_shadow;
        let ss = size * shadow.1 as f32 / BIG as f32;
        let (x, y, s) = square(ss, cx + dx, cy + 6.0 * lift + 3.0);
        items.push(Draw::Tex {
            x,
            y,
            w: s,
            h: s,
            tex: shadow.0,
            alpha: shown * (0.4 + 0.6 * lift),
        });
        let (x, y, s) = square(size, cx + dx, cy);
        items.push(Draw::Tex {
            x: x.round(),
            y: y.round(),
            w: s,
            h: s,
            tex,
            alpha: shown,
        });

        // The lid swings up about its hinge at the back, so from above it
        // narrows toward the top edge as it opens.
        let open = ease(span(t, SHOWN, OPENED)) * (1.0 - ease(span(t, PUT_IN, SHUT)));
        let squash = (open * LID_ANGLE).cos();
        let lid = LID as f32;
        let hinge = cy - lid / 2.0;
        let ls = view.lid_shadow;
        let lss = lid * ls.1 as f32 / LID as f32;
        let pad = (lss - lid) / 2.0;
        items.push(Draw::Tex {
            x: cx - lss / 2.0,
            y: hinge - pad + 4.0,
            w: lss,
            h: lss * squash,
            tex: ls.0,
            alpha: shown * squash,
        });
        items.push(Draw::Tex {
            x: cx - lid / 2.0,
            y: hinge,
            w: lid,
            h: (lid * squash).max(1.0),
            tex: view.lid,
            alpha: shown,
        });

        let (cw, ch) = view.caption_size;
        items.push(Draw::Tex {
            x: ((pw - cw as f32) / 2.0).round(),
            y: (cy + WELL as f32 / 2.0 + 28.0).round(),
            w: cw as f32,
            h: ch as f32,
            tex: view.caption,
            alpha: shown,
        });
        items.push(veil);
        overlay.draw_over(&items);
    }

    /// The swap as it stands `t` seconds in, for drawing a still of it.
    pub fn at(mut self, t: f32) -> Swap {
        self.t = t;
        self
    }
}
