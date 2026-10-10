//! Draws the shelf: the picked disc large and turning in the middle, its
//! neighbours smaller to each side, and the title below.
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
            Caption::new(overlay, 220, 22),
            Caption::new(overlay, 220, 22),
        ];
        hints[0].set(overlay, "A play", 1, 15.0, 10.0, grey);
        hints[1].set(overlay, "Menu + Start exit", 1, 15.0, 10.0, grey);
        ShelfView {
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

    /// Turns the picked disc `dt` seconds on.
    pub fn tick(&mut self, dt: f32, picked: Option<usize>) {
        let tau = std::f32::consts::TAU;
        if let Some(f) = self.pool.iter_mut().find(|f| Some(f.game) == picked) {
            f.angle = (f.angle + dt * tau / TURN_SECS) % tau;
        }
    }

    /// Draws the shelf for `carousel` over `games` and presents nothing;
    /// the caller swaps.
    pub fn draw(&mut self, overlay: &mut Overlay, games: &[Game], carousel: &Carousel) {
        let (pw, ph) = (self.panel.0 as f32, self.panel.1 as f32);
        let mut items = Vec::new();
        let (cx, cy) = (pw / 2.0, (ph * 0.42).round());
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
                let (x, y) = ((x - size / 2.0).round(), (cy - size / 2.0).round());
                // Past halfway the small face is the closer match in size.
                let close = d.abs() < 0.5;
                let (tex, shadow, face) = match close {
                    true => (big, self.shadow_big, BIG),
                    false => (small, self.shadow_small, SMALL),
                };
                let pad = (size * shadow.1 as f32 / face as f32 - size) / 2.0;
                let alpha = 1.0 - 0.45 * near;
                items.push(Draw::Tex {
                    x: x - pad,
                    y: y - pad + size * 0.03,
                    w: size + 2.0 * pad,
                    h: size + 2.0 * pad,
                    tex: shadow.0,
                    alpha,
                });
                items.push(Draw::Turned {
                    x,
                    y,
                    w: size,
                    h: size,
                    tex,
                    alpha,
                    turn,
                });
                if close {
                    items.push(Draw::Tex {
                        x,
                        y,
                        w: size,
                        h: size,
                        tex: self.shine,
                        alpha: 1.0 - near,
                    });
                }
            }
            let game = &games[carousel.selected()];
            self.title
                .set(overlay, &game.title, 2, 30.0, 15.0, [236, 236, 240]);
            let count = format!("{} / {}", carousel.selected() + 1, games.len());
            self.count
                .set(overlay, &count, 1, 16.0, 10.0, [128, 130, 138]);
            items.push(Draw::Tex {
                x: pw - self.count.w as f32 - 18.0,
                y: 14.0,
                w: self.count.w as f32,
                h: self.count.h as f32,
                tex: self.count.tex,
                alpha: 1.0,
            });
        }
        let below = match games.is_empty() {
            true => ((ph - self.title.h as f32) / 2.0).round(),
            false => (cy + BIG as f32 / 2.0 + 22.0).round(),
        };
        items.push(Draw::Tex {
            x: ((pw - self.title.w as f32) / 2.0).round(),
            y: below,
            w: self.title.w as f32,
            h: self.title.h as f32,
            tex: self.title.tex,
            alpha: 1.0,
        });
        // With nothing to play, only the way out is worth showing.
        let hints = match games.is_empty() {
            true => &self.hints[1..],
            false => &self.hints[..],
        };
        let sides: &[f32] = if hints.len() == 1 {
            &[0.0]
        } else {
            &[-1.0, 1.0]
        };
        for (hint, side) in hints.iter().zip(sides) {
            items.push(Draw::Tex {
                x: (pw / 2.0 + side * 110.0 - hint.w as f32 / 2.0).round(),
                y: ph - hint.h as f32 - 14.0,
                w: hint.w as f32,
                h: hint.h as f32,
                tex: hint.tex,
                alpha: 1.0,
            });
        }
        overlay.draw(BACKGROUND, &items);
    }
}
