//! Draws the shelf: the picked disc large and turning in the middle, its
//! neighbours smaller to each side, the title below, and along the bottom a
//! slot-loading drive that discs go into, as slot's carts go into its slot.
use std::path::{Path, PathBuf};

use slot_gfx::{Draw, Overlay, TexId};

use crate::carousel::Carousel;
use crate::disc::{disc_face, label_art, shadow_face, shine_face};
use crate::library::Game;

pub const BIG: u32 = 264;
pub const SMALL: u32 = 150;
const SHADOW_PAD: u32 = 14;
/// How far the neighbours sit from the middle, and the ones past them.
const NEAR: f32 = 230.0;
const FAR: f32 = 200.0;
const REACH: f32 = 2.2;
/// One turn every this many seconds.
const TURN_SECS: f32 = 9.0;
const BACKGROUND: [f32; 3] = [0.075, 0.078, 0.086];
/// Faces kept on the GPU at once; enough for every disc in reach while the
/// row slides, with a few to spare when it turns back.
const POOL: usize = 9;

/// The drive along the bottom, laid out as slot's cart slot is.
const BAND_H: f32 = 52.0;
const LIP_H: f32 = 2.0;
const MOUTH_W: f32 = BIG as f32 + 16.0;
const BAY_W: f32 = MOUTH_W + 18.0;
const SLIT_H: f32 = 9.0;
const RECESS_H: f32 = 40.0;

/// How the shelf is drawn while a disc goes in or out of the drive.
#[derive(Copy, Clone, Debug)]
pub struct Look<'a> {
    /// How far the picked disc is from its place, in panel pixels.
    pub picked: (f32, f32),
    /// One of the view's spare faces to show as the picked disc instead of
    /// its shelf face.
    pub face: Option<usize>,
    /// How much the rest of the shelf shows: the other discs and the labels.
    pub others: f32,
    /// Text in place of the title, and how much the title shows.
    pub title: Option<&'a str>,
    pub title_alpha: f32,
    /// How lit the drive's light is.
    pub light: f32,
}

impl Default for Look<'_> {
    fn default() -> Self {
        Look {
            picked: (0.0, 0.0),
            face: None,
            others: 1.0,
            title: None,
            title_alpha: 1.0,
            light: 0.0,
        }
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) -> Draw {
    Draw::Rect {
        x,
        y,
        w,
        h,
        colour: c,
    }
}

struct Faces {
    game: usize,
    big: TexId,
    small: TexId,
    used: u64,
    /// How far round this disc has turned. A disc stops where it is when
    /// it is no longer picked, rather than snapping back.
    angle: f32,
}

/// The text shown under the discs, as a face.
struct Caption {
    tex: TexId,
    w: u32,
    h: u32,
    text: String,
}

pub struct ShelfView {
    panel: (u32, u32),
    labels: PathBuf,
    pool: Vec<Faces>,
    clock: u64,
    shadow_big: (TexId, u32),
    shadow_small: (TexId, u32),
    shine: TexId,
    title: Caption,
    count: Caption,
    hints: [Caption; 2],
    system: Caption,
    /// Faces for discs that are not the shelf's own, such as the next disc
    /// of a game going in.
    spare: [TexId; 2],
}

/// White text on a clear face `w` by `h`, fitted to it.
pub fn text_face(
    text: &str,
    w: u32,
    h: u32,
    lines: usize,
    max_px: f32,
    min_px: f32,
    rgb: [u8; 3],
) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let Some(font) = slot_ui::text::label_font() else {
        return out;
    };
    let layout = slot_ui::text::fit_box(font, text, w as f32, h as f32, lines, max_px, min_px);
    for (px, a) in out
        .chunks_exact_mut(4)
        .zip(slot_ui::text::coverage(w, h, &layout))
    {
        px.copy_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
    }
    out
}

impl Caption {
    fn new(overlay: &mut Overlay, w: u32, h: u32) -> Self {
        let tex = overlay.create_texture(w, h, &vec![0; (w * h * 4) as usize]);
        Caption {
            tex,
            w,
            h,
            text: String::new(),
        }
    }

    fn set(
        &mut self,
        overlay: &mut Overlay,
        text: &str,
        lines: usize,
        max_px: f32,
        min_px: f32,
        rgb: [u8; 3],
    ) {
        if self.text == text {
            return;
        }
        self.text = text.to_string();
        let face = text_face(text, self.w, self.h, lines, max_px, min_px, rgb);
        overlay.update_texture(self.tex, self.w, self.h, &face);
    }
}

impl ShelfView {
    pub fn new(overlay: &mut Overlay, panel: (u32, u32), labels: &Path) -> Self {
        let (sb, sb_face) = shadow_face(BIG, SHADOW_PAD);
        let (ss, ss_face) = shadow_face(SMALL, SHADOW_PAD / 2);
        let shadow_big = (overlay.create_texture(sb, sb, &sb_face), sb);
        let shadow_small = (overlay.create_texture(ss, ss, &ss_face), ss);
        let shine = overlay.create_texture(BIG, BIG, &shine_face(BIG));
        let title = Caption::new(overlay, panel.0.saturating_sub(64).max(1), 76);
        let count = Caption::new(overlay, 140, 24);
        let grey = [128, 130, 138];
        let mut hints = [
            Caption::new(overlay, 150, 20),
            Caption::new(overlay, 150, 20),
        ];
        hints[0].set(overlay, "A play", 1, 13.0, 9.0, grey);
        hints[1].set(overlay, "L / R systems", 1, 13.0, 9.0, grey);
        let blank = vec![0u8; (BIG * BIG * 4) as usize];
        let spare = [
            overlay.create_texture(BIG, BIG, &blank),
            overlay.create_texture(BIG, BIG, &blank),
        ];
        let mut system = Caption::new(overlay, 220, 24);
        system.set(overlay, "PlayStation", 1, 16.0, 10.0, grey);
        ShelfView {
            spare,
            system,
            panel,
            labels: labels.to_path_buf(),
            pool: Vec::new(),
            clock: 0,
            shadow_big,
            shadow_small,
            shine,
            title,
            count,
            hints,
        }
    }

    fn faces(
        &mut self,
        overlay: &mut Overlay,
        games: &[Game],
        game: usize,
        keep: &[usize],
    ) -> (TexId, TexId, f32) {
        self.clock += 1;
        if let Some(f) = self.pool.iter_mut().find(|f| f.game == game) {
            f.used = self.clock;
            return (f.big, f.small, f.angle);
        }
        let g = &games[game];
        let colour = slot_ui::label_colour(&g.title);
        let big_art = label_art(&self.labels, &g.stem, BIG);
        let small_art = big_art
            .as_ref()
            .and_then(|_| label_art(&self.labels, &g.stem, SMALL));
        let big = disc_face(BIG, &g.title, colour, big_art.as_deref());
        let small = disc_face(SMALL, &g.title, colour, small_art.as_deref());
        if self.pool.len() < POOL {
            let f = Faces {
                game,
                big: overlay.create_texture(BIG, BIG, &big),
                small: overlay.create_texture(SMALL, SMALL, &small),
                used: self.clock,
                angle: 0.0,
            };
            let ids = (f.big, f.small, 0.0);
            self.pool.push(f);
            return ids;
        }
        let slot = self
            .pool
            .iter_mut()
            .filter(|f| !keep.contains(&f.game))
            .min_by_key(|f| f.used)
            .expect("the pool holds more faces than are ever in reach");
        overlay.update_texture(slot.big, BIG, BIG, &big);
        overlay.update_texture(slot.small, SMALL, SMALL, &small);
        slot.game = game;
        slot.used = self.clock;
        slot.angle = 0.0;
        (slot.big, slot.small, 0.0)
    }

    /// Puts `rgba`, a disc face `BIG` across, in spare face `i`.
    pub fn set_spare(&mut self, overlay: &mut Overlay, i: usize, rgba: &[u8]) {
        overlay.update_texture(self.spare[i], BIG, BIG, rgba);
    }

    /// How far below its place the picked disc must go to be all the way
    /// into the drive.
    pub fn into_drive(&self) -> f32 {
        let slit_bottom = self.band_y() + LIP_H + 5.0 + SLIT_H;
        slit_bottom + BIG as f32 / 2.0 + 16.0 - self.spot().1
    }

    fn band_y(&self) -> f32 {
        self.panel.1 as f32 - BAND_H
    }

    /// The drive's back, which a disc passes in front of on its way in.
    fn drive_back(&self, out: &mut Vec<Draw>) {
        let pw = self.panel.0 as f32;
        let band = self.band_y();
        let (mouth_x, bay_x) = ((pw - MOUTH_W) / 2.0, (pw - BAY_W) / 2.0);
        let bay_y = band + LIP_H;
        out.push(rect(bay_x, band, BAY_W, LIP_H, slot_ui::housing()));
        out.push(rect(mouth_x, band, MOUTH_W, LIP_H, slot_ui::edge()));
        out.push(rect(bay_x, bay_y, BAY_W, RECESS_H, slot_ui::recess()));
        out.push(rect(
            mouth_x,
            bay_y + 5.0,
            MOUTH_W,
            SLIT_H,
            slot_ui::opening(),
        ));
    }

    /// The drive's front, which hides a disc once it is past the slit.
    fn drive_front(&self, look: &Look, empty: bool, out: &mut Vec<Draw>) {
        let (pw, ph) = (self.panel.0 as f32, self.panel.1 as f32);
        let band = self.band_y();
        let bay_x = (pw - BAY_W) / 2.0;
        let slit_bottom = band + LIP_H + 5.0 + SLIT_H;
        let housing = slot_ui::housing();
        out.push(rect(0.0, band, bay_x, BAND_H, housing));
        out.push(rect(
            bay_x + BAY_W,
            band,
            pw - bay_x - BAY_W,
            BAND_H,
            housing,
        ));
        out.push(rect(bay_x, slit_bottom, BAY_W, ph - slit_bottom, housing));
        let edge = slot_ui::edge();
        out.push(rect(
            bay_x,
            slit_bottom,
            BAY_W,
            1.0,
            [edge[0], edge[1], edge[2], 0.6],
        ));
        if look.light > 0.0 {
            let x = bay_x + BAY_W - 22.0;
            out.push(rect(
                x,
                slit_bottom + 12.0,
                10.0,
                4.0,
                [0.35, 0.95, 0.45, look.light],
            ));
        }
        // The hints sit on the drive either side of the slot.
        for (i, hint) in self.hints.iter().enumerate() {
            if empty && i == 0 {
                continue;
            }
            let x = match i {
                0 => (bay_x - hint.w as f32) / 2.0,
                _ => bay_x + BAY_W + (bay_x - hint.w as f32) / 2.0,
            };
            out.push(Draw::Tex {
                x: x.round(),
                y: (band + (BAND_H - hint.h as f32) / 2.0).round(),
                w: hint.w as f32,
                h: hint.h as f32,
                tex: hint.tex,
                alpha: look.others,
            });
        }
    }

    /// Where the picked disc's middle sits on the panel.
    pub fn spot(&self) -> (f32, f32) {
        let (pw, ph) = (self.panel.0 as f32, self.panel.1 as f32);
        (pw / 2.0, (ph * 0.42).round())
    }

    /// How far round `game`'s disc has turned.
    pub fn angle(&self, game: usize) -> f32 {
        self.pool
            .iter()
            .find(|f| f.game == game)
            .map_or(0.0, |f| f.angle)
    }

    /// Sets how far round `game`'s disc has turned, as when it comes back
    /// to the shelf from the console.
    pub fn set_angle(&mut self, game: usize, angle: f32) {
        if let Some(f) = self.pool.iter_mut().find(|f| f.game == game) {
            f.angle = angle;
        }
    }

    /// Turns the picked disc `dt` seconds on.
    pub fn tick(&mut self, dt: f32, picked: Option<usize>) {
        let tau = std::f32::consts::TAU;
        if let Some(f) = self.pool.iter_mut().find(|f| Some(f.game) == picked) {
            f.angle = (f.angle + dt * tau / TURN_SECS) % tau;
        }
    }

    /// Draws the shelf for `carousel` over `games`, as `look` says, and
    /// presents nothing; the caller swaps.
    pub fn draw(
        &mut self,
        overlay: &mut Overlay,
        games: &[Game],
        carousel: &Carousel,
        look: &Look,
    ) {
        let pw = self.panel.0 as f32;
        let ph = self.panel.1 as f32;
        let mut items = Vec::new();
        let (cx, cy) = self.spot();
        let mut picked_items = Vec::new();
        if games.is_empty() {
            self.title.set(
                overlay,
                "No discs in Games/PS",
                2,
                26.0,
                14.0,
                [210, 210, 214],
            );
        } else {
            let placed = carousel.placed(REACH);
            let keep: Vec<usize> = placed.iter().map(|p| p.0).collect();
            for &(game, d) in &placed {
                let (big, small, turn) = self.faces(overlay, games, game, &keep);
                let near = d.abs().min(1.0);
                let size = BIG as f32 + (SMALL as f32 - BIG as f32) * near;
                let x = cx + d.signum() * (near * NEAR + (d.abs() - 1.0).max(0.0) * FAR);
                // Past halfway the small face is the closer match in size.
                let close = d.abs() < 0.5;
                let picked = d == 0.0;
                let (dx, dy) = if picked { look.picked } else { (0.0, 0.0) };
                let (x, y) = (
                    (x - size / 2.0 + dx).round(),
                    (cy - size / 2.0 + dy).round(),
                );
                let spare = if picked { look.face } else { None };
                let (tex, shadow, face) = match (close, spare) {
                    (_, Some(i)) => (self.spare[i], self.shadow_big, BIG),
                    (true, None) => (big, self.shadow_big, BIG),
                    (false, None) => (small, self.shadow_small, SMALL),
                };
                let pad = (size * shadow.1 as f32 / face as f32 - size) / 2.0;
                let alpha = match picked {
                    true => 1.0,
                    false => (1.0 - 0.45 * near) * look.others,
                };
                let out = if picked {
                    &mut picked_items
                } else {
                    &mut items
                };
                out.push(Draw::Tex {
                    x: x - pad,
                    y: y - pad + size * 0.03,
                    w: size + 2.0 * pad,
                    h: size + 2.0 * pad,
                    tex: shadow.0,
                    alpha,
                });
                out.push(Draw::Turned {
                    x,
                    y,
                    w: size,
                    h: size,
                    tex,
                    alpha,
                    turn,
                });
                if close {
                    out.push(Draw::Tex {
                        x,
                        y,
                        w: size,
                        h: size,
                        tex: self.shine,
                        alpha: (1.0 - near) * alpha,
                    });
                }
            }
            let game = &games[carousel.selected()];
            let title = look.title.unwrap_or(&game.title);
            self.title
                .set(overlay, title, 2, 30.0, 15.0, [236, 236, 240]);
            let count = format!("{} / {}", carousel.selected() + 1, games.len());
            self.count
                .set(overlay, &count, 1, 16.0, 10.0, [128, 130, 138]);
            items.push(Draw::Tex {
                x: pw - self.count.w as f32 - 18.0,
                y: 14.0,
                w: self.count.w as f32,
                h: self.count.h as f32,
                tex: self.count.tex,
                alpha: look.others,
            });
        }
        let below = match games.is_empty() {
            true => ((ph - self.title.h as f32) / 2.0).round(),
            false => (cy + BIG as f32 / 2.0 + 18.0).round(),
        };
        items.push(Draw::Tex {
            x: ((pw - self.title.w as f32) / 2.0).round(),
            y: below,
            w: self.title.w as f32,
            h: self.title.h as f32,
            tex: self.title.tex,
            alpha: look.title_alpha,
        });
        items.push(Draw::Tex {
            x: 18.0,
            y: 14.0,
            w: self.system.w as f32,
            h: self.system.h as f32,
            tex: self.system.tex,
            alpha: look.others,
        });
        // The picked disc goes between the drive's back and its front, so it
        // shows above the slit and is hidden past it.
        self.drive_back(&mut items);
        items.extend(picked_items);
        self.drive_front(look, games.is_empty(), &mut items);
        overlay.draw(BACKGROUND, &items);
    }
}
