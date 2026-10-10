//! Discs going into the drive at the bottom of the shelf and coming back
//! out, as slot's carts go into its slot: the picked disc slides down into
//! the slit to play, rises back out on quitting, and for a disc change the
//! old disc comes out and the next goes in. The core is told when the lid
//! opens and shuts, as a console's drive would tell it.
use std::path::Path;

use crate::disc::{disc_face, label_art};
use crate::library::Game;
use crate::play::DiscGame;
use crate::shelf::{Look, ShelfView, BIG};

fn ease(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Slow to start and quick to finish, like a disc pulled in by the drive.
fn ease_in(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * u
}

/// Quick to start and slow to finish, like a disc pushed out by the drive.
fn ease_out(u: f32) -> f32 {
    1.0 - ease_in(1.0 - u)
}

fn span(t: f32, from: f32, to: f32) -> f32 {
    ((t - from) / (to - from)).clamp(0.0, 1.0)
}

/// Black rising to 1 over the first half of `from..to` and falling back
/// over the second.
fn dip(t: f32, from: f32, to: f32) -> f32 {
    let mid = (from + to) / 2.0;
    ease(span(t, from, mid)) * (1.0 - ease(span(t, mid, to)))
}

/// Disc `index` of `game` as a face: its own label art if the card has it,
/// then the game's, then the game's colour with its title, and its number
/// when the game has more than one disc.
pub fn disc_label(labels: &Path, game: &Game, index: u32, size: u32) -> Vec<u8> {
    let own = game
        .discs
        .get(index as usize)
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned());
    let art = own
        .and_then(|stem| label_art(labels, &stem, size))
        .or_else(|| label_art(labels, &game.stem, size));
    let text = match game.discs.len() > 1 {
        true => format!("{} Disc {}", game.title, index + 1),
        false => game.title.clone(),
    };
    disc_face(
        size,
        &text,
        slot_ui::label_colour(&game.title),
        art.as_deref(),
    )
}

/// The picked disc going into the drive. It slides down into the slit
/// while the rest of the shelf fades, the drive's light comes on, and the
/// game loads. Then the screen dips to black and the game comes up.
pub struct Insert {
    t: f32,
    depth: f32,
    loaded: bool,
}

impl Insert {
    /// As slot seats a cart: in, then a beat with it seated.
    const IN: f32 = 0.45;
    const SEATED: f32 = 0.73;
    const GONE: f32 = 0.15;

    pub fn start(view: &ShelfView) -> Insert {
        Insert {
            t: 0.0,
            depth: view.into_drive(),
            loaded: false,
        }
    }

    /// Moves `dt` seconds on. It waits with the disc seated until `loaded`.
    pub fn tick(&mut self, dt: f32) {
        self.t += dt;
        if !self.loaded {
            self.t = self.t.min(Self::SEATED);
        }
    }

    /// Whether the disc is in and the game should load now.
    pub fn ready_to_load(&self) -> bool {
        !self.loaded && self.t >= Self::SEATED
    }

    pub fn loaded(&mut self) {
        self.loaded = true;
    }

    /// Whether the screen has dipped to black and the game can show.
    pub fn done(&self) -> bool {
        self.loaded && self.t >= Self::SEATED + Self::GONE
    }

    pub fn look(&self) -> Look<'static> {
        let t = self.t;
        let fade = 1.0 - ease(span(t, 0.0, Self::IN * 0.7));
        Look {
            picked: (0.0, ease_in(span(t, 0.0, Self::IN)) * self.depth),
            others: fade,
            title_alpha: fade,
            light: if t >= Self::IN { 1.0 } else { 0.0 },
            ..Look::default()
        }
    }

    pub fn veil(&self) -> f32 {
        ease(span(self.t, Self::SEATED, Self::SEATED + Self::GONE))
    }

    /// The insert as it stands `t` seconds in, for stills.
    pub fn at(mut self, t: f32, loaded: bool) -> Insert {
        self.t = t;
        self.loaded = loaded;
        self
    }
}

/// The disc coming back out after a game: the shelf comes up out of black
/// with the disc in the drive, the disc rises out to its place, and the
/// rest of the shelf fades back in.
pub struct Eject {
    t: f32,
    depth: f32,
}

impl Eject {
    const SHOWN: f32 = 0.15;
    const HOLD: f32 = 0.3;
    const OUT: f32 = 0.75;
    const DONE: f32 = 0.95;

    pub fn start(view: &ShelfView) -> Eject {
        Eject {
            t: 0.0,
            depth: view.into_drive(),
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.t += dt;
    }

    pub fn done(&self) -> bool {
        self.t >= Self::DONE
    }

    pub fn look(&self) -> Look<'static> {
        let t = self.t;
        let out = ease_out(span(t, Self::HOLD, Self::OUT));
        let back = ease(span(t, Self::OUT - 0.15, Self::DONE));
        Look {
            picked: (0.0, (1.0 - out) * self.depth),
            others: back,
            title_alpha: back,
            light: if t < Self::HOLD { 1.0 } else { 0.0 },
            ..Look::default()
        }
    }

    pub fn veil(&self) -> f32 {
        1.0 - ease(span(self.t, 0.0, Self::SHOWN))
    }

    /// The eject as it stands `t` seconds in, for stills.
    pub fn at(mut self, t: f32) -> Eject {
        self.t = t;
        self
    }
}

/// Changing discs mid-game: the game dips to the shelf's drive, the old
/// disc rises out and slides away, the next slides in and goes down into
/// the drive, and the game comes back. The core keeps running underneath
/// so the game sees the lid open and close.
pub struct Swap {
    t: f32,
    to: u32,
    depth: f32,
    width: f32,
    caption: String,
    opened: bool,
    swapped: bool,
    shut: bool,
}

impl Swap {
    const SHOWN: f32 = 0.2;
    const RISEN: f32 = 0.55;
    const TAKEN: f32 = 0.8;
    const BROUGHT: f32 = 1.05;
    const IN: f32 = 1.4;
    const DONE: f32 = 1.6;

    /// Starts changing `game` from disc `from` to `to`, both from 0. Puts
    /// both discs' faces in the shelf's spare faces.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        view: &mut ShelfView,
        overlay: &mut slot_gfx::Overlay,
        labels: &Path,
        game: &Game,
        from: u32,
        to: u32,
        count: u32,
        panel_w: f32,
    ) -> Swap {
        view.set_spare(overlay, 0, &disc_label(labels, game, from, BIG));
        view.set_spare(overlay, 1, &disc_label(labels, game, to, BIG));
        Swap {
            t: 0.0,
            to,
            depth: view.into_drive(),
            width: panel_w,
            caption: format!("Disc {} of {count}", to + 1),
            opened: false,
            swapped: false,
            shut: false,
        }
    }

    /// Moves `dt` seconds on, opening the lid, changing the disc and
    /// shutting the lid in the core as the picture does. True once done.
    pub fn tick(&mut self, dt: f32, game: &mut DiscGame) -> bool {
        self.t += dt;
        if !self.opened && self.t >= Self::SHOWN {
            self.opened = true;
            game.set_lid_open(true);
        }
        if !self.swapped && self.t >= Self::TAKEN {
            self.swapped = true;
            game.set_disc(self.to);
        }
        if !self.shut && self.t >= Self::IN {
            self.shut = true;
            game.set_lid_open(false);
        }
        self.t >= Self::DONE
    }

    /// Whether the shelf shows in place of the game.
    pub fn shows_shelf(&self) -> bool {
        let half = Self::SHOWN / 2.0;
        let back = Self::IN + (Self::DONE - Self::IN) / 2.0;
        (half..back).contains(&self.t)
    }

    pub fn veil(&self) -> f32 {
        dip(self.t, 0.0, Self::SHOWN) + dip(self.t, Self::IN, Self::DONE)
    }

    pub fn look(&self) -> Look<'_> {
        let t = self.t;
        let reach = self.width * 0.75;
        let (picked, face) = match t < Self::TAKEN {
            true => {
                let up = 1.0 - ease_out(span(t, Self::SHOWN, Self::RISEN));
                let away = ease_in(span(t, Self::RISEN, Self::TAKEN));
                ((away * reach, up * self.depth), 0)
            }
            false => {
                let come = 1.0 - ease_out(span(t, Self::TAKEN, Self::BROUGHT));
                let down = ease_in(span(t, Self::BROUGHT, Self::IN));
                ((-come * reach, down * self.depth), 1)
            }
        };
        Look {
            picked,
            face: Some(face),
            others: 0.0,
            title: Some(&self.caption),
            title_alpha: 1.0,
            light: if (Self::RISEN..Self::BROUGHT).contains(&t) {
                0.0
            } else {
                1.0
            },
        }
    }

    /// The swap as it stands `t` seconds in, for drawing a still of it.
    pub fn at(mut self, t: f32) -> Swap {
        self.t = t;
        self
    }
}
