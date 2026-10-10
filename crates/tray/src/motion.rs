//! Discs going into slot's slot and coming back out, timed and moved as
//! slot moves its carts, with slot's sounds: the picked disc goes in to
//! play, comes out on quitting, and for a disc change the old disc comes
//! out and the next goes in. The core is told when the lid opens and
//! shuts, as a console's drive would tell it.
use std::path::Path;

use slot::audio::Sfx;
use slot_gfx::Compositor;

use crate::disc::{disc_face, label_art};
use crate::library::Game;
use crate::play::DiscGame;
use crate::shelf::{Look, ShelfView, BIG, INSERT_S, IN_S, OUT_S};

fn ease(u: f32) -> f32 {
    slot_ui::ease(u.clamp(0.0, 1.0))
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

/// Whether `t` crossed `at` going from `was`.
fn crossed(was: f32, t: f32, at: f32) -> bool {
    was < at && t >= at
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

/// The picked disc going into the slot as slot seats a cart. The game
/// loads once it is seated, then the screen dips to black and the game
/// comes up.
pub struct Insert {
    t: f32,
    loaded: bool,
}

impl Insert {
    const GONE: f32 = 0.15;

    pub fn start() -> Insert {
        Insert {
            t: 0.0,
            loaded: false,
        }
    }

    /// Moves `dt` seconds on, waiting seated until `loaded`. Returns a
    /// sound to play, if one is due.
    pub fn tick(&mut self, dt: f32) -> Option<Sfx> {
        let was = self.t;
        self.t += dt;
        if !self.loaded {
            self.t = self.t.min(INSERT_S);
        }
        crossed(was, self.t, IN_S - Sfx::Insert.lead()).then_some(Sfx::Insert)
    }

    /// Whether the disc is seated and the game should load now.
    pub fn ready_to_load(&self) -> bool {
        !self.loaded && self.t >= INSERT_S
    }

    pub fn loaded(&mut self) {
        self.loaded = true;
    }

    /// Whether the screen has dipped to black and the game can show.
    pub fn done(&self) -> bool {
        self.loaded && self.t >= INSERT_S + Self::GONE
    }

    pub fn look(&self) -> Look {
        let seat = (self.t / IN_S).min(1.0);
        Look {
            seat,
            recede: seat,
            ..Look::default()
        }
    }

    pub fn veil(&self) -> f32 {
        ease(span(self.t, INSERT_S, INSERT_S + Self::GONE))
    }

    /// The insert as it stands `t` seconds in, for stills.
    pub fn at(mut self, t: f32, loaded: bool) -> Insert {
        self.t = t;
        self.loaded = loaded;
        self
    }
}

/// The disc coming back out after a game, as slot ejects a cart: the shelf
/// comes up out of black with the disc seated, and it comes out to its
/// place while the other discs come back.
pub struct Eject {
    t: f32,
}

impl Eject {
    const SHOWN: f32 = 0.12;

    pub fn start() -> Eject {
        Eject { t: 0.0 }
    }

    pub fn tick(&mut self, dt: f32) -> Option<Sfx> {
        let was = self.t;
        self.t += dt;
        crossed(was, self.t, Self::SHOWN).then_some(Sfx::Eject)
    }

    pub fn done(&self) -> bool {
        self.t >= Self::SHOWN + OUT_S
    }

    pub fn look(&self) -> Look {
        let seat = 1.0 - span(self.t, Self::SHOWN, Self::SHOWN + OUT_S);
        Look {
            seat,
            recede: seat,
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

/// Changing discs mid-game: the game dips to the shelf with the disc
/// seated, it comes out of the slot and slides away, the next slides in
/// and goes into the slot, and the game comes back. The core keeps running
/// underneath so the game sees the lid open and close.
pub struct Swap {
    t: f32,
    to: u32,
    opened: bool,
    swapped: bool,
    shut: bool,
}

impl Swap {
    const SHOWN: f32 = 0.2;
    const OUT: f32 = Self::SHOWN + OUT_S;
    const AWAY: f32 = Self::OUT + 0.25;
    const BACK: f32 = Self::AWAY + 0.25;
    const IN: f32 = Self::BACK + IN_S;
    const HELD: f32 = Self::IN + 0.15;
    const DONE: f32 = Self::HELD + 0.2;
    /// How far a disc slides to be off the shelf, in canvas units.
    const REACH: f32 = 560.0;

    /// Starts changing `game` from disc `from` to `to`, both from 0. Puts
    /// both discs' faces in the shelf's spare faces.
    pub fn start(
        view: &mut ShelfView,
        compositor: &mut Compositor,
        labels: &Path,
        game: &Game,
        from: u32,
        to: u32,
    ) -> Swap {
        view.set_spare(compositor, 0, &disc_label(labels, game, from, BIG));
        view.set_spare(compositor, 1, &disc_label(labels, game, to, BIG));
        Swap {
            t: 0.0,
            to,
            opened: false,
            swapped: false,
            shut: false,
        }
    }

    /// What the slot says while the discs change.
    pub fn caption(&self) -> String {
        format!("Disc {}", self.to + 1)
    }

    /// Moves `dt` seconds on, opening the lid, changing the disc and
    /// shutting the lid in the core as the picture does. Returns a sound
    /// to play, if one is due.
    pub fn tick(&mut self, dt: f32, game: &mut DiscGame) -> Option<Sfx> {
        let was = self.t;
        self.t += dt;
        if !self.opened && self.t >= Self::SHOWN {
            self.opened = true;
            game.set_lid_open(true);
        }
        if !self.swapped && self.t >= Self::AWAY {
            self.swapped = true;
            game.set_disc(self.to);
        }
        if !self.shut && self.t >= Self::IN {
            self.shut = true;
            game.set_lid_open(false);
        }
        if crossed(was, self.t, Self::SHOWN) {
            return Some(Sfx::Eject);
        }
        crossed(was, self.t, Self::IN - Sfx::Insert.lead()).then_some(Sfx::Insert)
    }

    pub fn done(&self) -> bool {
        self.t >= Self::DONE
    }

    /// Whether the shelf shows in place of the game.
    pub fn shows_shelf(&self) -> bool {
        (Self::SHOWN / 2.0..Self::HELD + (Self::DONE - Self::HELD) / 2.0).contains(&self.t)
    }

    pub fn veil(&self) -> f32 {
        dip(self.t, 0.0, Self::SHOWN) + dip(self.t, Self::HELD, Self::DONE)
    }

    pub fn look(&self) -> Look {
        let t = self.t;
        let (seat, dx, face) = match t < Self::AWAY {
            true => (
                1.0 - span(t, Self::SHOWN, Self::OUT),
                ease(span(t, Self::OUT, Self::AWAY)) * Self::REACH,
                0,
            ),
            false => (
                span(t, Self::BACK, Self::IN),
                -(1.0 - ease(span(t, Self::AWAY, Self::BACK))) * Self::REACH,
                1,
            ),
        };
        Look {
            seat,
            recede: 1.0,
            dx,
            face: Some(face),
        }
    }

    /// The swap as it stands `t` seconds in, for drawing a still of it.
    pub fn at(mut self, t: f32) -> Swap {
        self.t = t;
        self
    }
}
