//! The disc shelf, drawn into slot's own frame as one more shelf: the
//! picked disc large in the middle, its neighbours smaller to each side as
//! slot's carts are, and slot's slot below, which discs go into as carts
//! do. Laid out in slot's canvas units.
use std::path::{Path, PathBuf};

use slot::app::{EJECT_S, SEATED_AT};
use slot_gfx::{Compositor, Draw, TexId, OUT_H, OUT_W};

use crate::carousel::Carousel;
use crate::disc::{disc_face, label_art, shadow_face, shine_face};
use crate::library::Game;

/// A disc's size on the shelf, and seated in the slot, which fits the
/// slot's mouth as a seated cart does.
pub const BIG: u32 = 300;
pub const SEATED: u32 = slot_ui::SEATED_W;
/// Slot's shelf: neighbours shrink and fade, one cart's width apart, and
/// slide apart by this much as a cart goes in.
const SIDE_SCALE: f32 = 0.78;
const SIDE_ALPHA: f32 = 0.55;
const PART: f32 = 130.0;
const SMALL: u32 = (BIG as f32 * SIDE_SCALE) as u32;
const SHADOW_PAD: u32 = 14;
const REACH: f32 = 2.2;
/// One turn every this many seconds.
const TURN_SECS: f32 = 9.0;
/// Faces kept on the GPU at once; enough for every disc in reach while the
/// row slides, with a few to spare when it turns back.
const POOL: usize = 9;

/// Slot's slot: the lip a cart catches on, and where a seated cart's top
/// sits, just inside it.
const LIP_Y: f32 = OUT_H as f32 - slot_ui::MOUTH_H;
const SEATED_Y: f32 = LIP_Y + slot_ui::LIP_H + 4.0;
/// The catch, as slot's cart takes it: in to the lip, a short creep, then
/// the rest of the way.
const CATCH_IN: f32 = 0.42;
const CATCH_OUT: f32 = 0.62;
const CREEP: f32 = 0.03;

pub use slot::app::INSERT_S;
/// How long a disc takes to go in, and to come out, as a cart does.
pub const IN_S: f32 = SEATED_AT;
pub const OUT_S: f32 = EJECT_S;

fn ease(u: f32) -> f32 {
    slot_ui::ease(u.clamp(0.0, 1.0))
}

fn travel(seat: f32, catch: f32) -> f32 {
    if seat < CATCH_IN {
        catch * ease(seat / CATCH_IN)
    } else if seat < CATCH_OUT {
        catch + CREEP * (seat - CATCH_IN) / (CATCH_OUT - CATCH_IN)
    } else {
        let caught = catch + CREEP;
        caught + (1.0 - caught) * ease((seat - CATCH_OUT) / (1.0 - CATCH_OUT))
    }
}

fn shrink(offset: f32) -> f32 {
    1.0 + (SIDE_SCALE - 1.0) * offset.abs().min(1.0)
}

/// How the shelf is drawn while a disc goes in or out of the slot.
#[derive(Copy, Clone, Debug, Default)]
pub struct Look {
    /// How far into the slot the picked disc is: 0 on the shelf, 1 seated.
    pub seat: f32,
    /// How far the other discs have drawn apart and faded, as slot's carts
    /// do while one goes in.
    pub recede: f32,
    /// A sideways shift of the picked disc, for a disc changing places.
    pub dx: f32,
    /// One of the view's spare faces to show as the picked disc instead of
    /// its shelf face.
    pub face: Option<usize>,
}

/// The shelf as draws: what stands on it, and what is passing through the
/// slot, which goes between the slot's back and its front.
pub struct ShelfDraws {
    pub row: Vec<Draw>,
    pub seated: Vec<Draw>,
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

pub struct ShelfView {
    labels: PathBuf,
    pool: Vec<Faces>,
    clock: u64,
    shadow_big: (TexId, u32),
    shadow_small: (TexId, u32),
    shine: TexId,
    /// Faces for discs that are not the shelf's own, such as the next disc
    /// of a game going in.
    spare: [TexId; 2],
}

impl ShelfView {
    pub fn new(compositor: &mut Compositor, labels: &Path) -> Self {
        let (sb, sb_face) = shadow_face(BIG, SHADOW_PAD);
        let (ss, ss_face) = shadow_face(SMALL, SHADOW_PAD * SMALL / BIG);
        let blank = vec![0u8; (BIG * BIG * 4) as usize];
        ShelfView {
            labels: labels.to_path_buf(),
            pool: Vec::new(),
            clock: 0,
            shadow_big: (compositor.create_texture(sb, sb, &sb_face), sb),
            shadow_small: (compositor.create_texture(ss, ss, &ss_face), ss),
            shine: compositor.create_texture(BIG, BIG, &shine_face(BIG)),
            spare: [
                compositor.create_texture(BIG, BIG, &blank),
                compositor.create_texture(BIG, BIG, &blank),
            ],
        }
    }

    fn faces(
        &mut self,
        compositor: &mut Compositor,
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
                big: compositor.create_texture(BIG, BIG, &big),
                small: compositor.create_texture(SMALL, SMALL, &small),
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
        compositor.update_texture(slot.big, BIG, BIG, &big);
        compositor.update_texture(slot.small, SMALL, SMALL, &small);
        slot.game = game;
        slot.used = self.clock;
        slot.angle = 0.0;
        (slot.big, slot.small, 0.0)
    }

    /// Puts `rgba`, a disc face `BIG` across, in spare face `i`.
    pub fn set_spare(&mut self, compositor: &mut Compositor, i: usize, rgba: &[u8]) {
        compositor.update_texture(self.spare[i], BIG, BIG, rgba);
    }

    /// Turns the picked disc `dt` seconds on.
    pub fn tick(&mut self, dt: f32, picked: Option<usize>) {
        let tau = std::f32::consts::TAU;
        if let Some(f) = self.pool.iter_mut().find(|f| Some(f.game) == picked) {
            f.angle = (f.angle + dt * tau / TURN_SECS) % tau;
        }
    }

    /// Where the picked disc is, `seat` of the way into the slot, starting
    /// from `size0` across with its middle at `cx`: as slot moves a cart,
    /// down to the lip, a short catch, then on in while shrinking to fit.
    fn seated_at(seat: f32, size0: f32, cx: f32) -> (f32, f32, f32) {
        let foot0 = slot_ui::rest_y(BIG as f32) + BIG as f32;
        let seated_foot = SEATED_Y + SEATED as f32;
        let catch = (LIP_Y - foot0) / (seated_foot - foot0);
        let travel = travel(seat.clamp(0.0, 1.0), catch);
        let k = (travel / catch).min(1.0);
        let size = size0 + (SEATED as f32 - size0) * k;
        let mid = cx + (OUT_W as f32 / 2.0 - cx) * k;
        let y = foot0 + (seated_foot - foot0) * travel - size;
        (mid - size / 2.0, y, size)
    }

    /// The shelf for `carousel` over `games`, as `look` says.
    pub fn draws(
        &mut self,
        compositor: &mut Compositor,
        games: &[Game],
        carousel: &Carousel,
        look: &Look,
    ) -> ShelfDraws {
        let mut row = Vec::new();
        let mut seated = Vec::new();
        if games.is_empty() {
            return ShelfDraws { row, seated };
        }
        let cx = OUT_W as f32 / 2.0;
        let foot = slot_ui::rest_y(BIG as f32) + BIG as f32;
        let recede = look.recede.clamp(0.0, 1.0);
        let placed = carousel.placed(REACH);
        let keep: Vec<usize> = placed.iter().map(|p| p.0).collect();
        for &(game, d) in &placed {
            let (big, small, turn) = self.faces(compositor, games, game, &keep);
            let picked = d.abs() < 0.5 && game == carousel.selected();
            let scale = shrink(d);
            let size0 = BIG as f32 * scale;
            let mid = cx + d * BIG as f32;
            let (x, y, size, alpha) = match picked {
                true => {
                    let (x, y, size) = Self::seated_at(look.seat, size0, mid);
                    (x + look.dx, y, size, 1.0)
                }
                false => {
                    let away = d.signum() * (1.0 + d.abs());
                    let t = d.abs().min(1.0);
                    let alpha = (1.0 + (SIDE_ALPHA - 1.0) * t) * (1.0 - recede);
                    let x = mid + away * PART * recede - size0 / 2.0;
                    (x, foot - size0, size0, alpha)
                }
            };
            if alpha <= 0.0 || x + size <= 0.0 || x >= OUT_W as f32 {
                continue;
            }
            let spare = if picked { look.face } else { None };
            let (tex, shadow, face) = match (d.abs() < 0.5, spare) {
                (_, Some(i)) => (self.spare[i], self.shadow_big, BIG),
                (true, None) => (big, self.shadow_big, BIG),
                (false, None) => (small, self.shadow_small, SMALL),
            };
            let out = if picked { &mut seated } else { &mut row };
            let pad = (size * shadow.1 as f32 / face as f32 - size) / 2.0;
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
            if picked {
                out.push(Draw::Tex {
                    x,
                    y,
                    w: size,
                    h: size,
                    tex: self.shine,
                    alpha,
                });
            }
        }
        ShelfDraws { row, seated }
    }
}
