use crate::quad::Quad;
use crate::shaders::{SPRITE_FRAG, SPRITE_VERT};
use crate::surface::{GfxError, OUT_H, OUT_W};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TexId(usize);

#[cfg(feature = "test-support")]
impl TexId {
    pub const fn from_raw(id: usize) -> Self {
        TexId(id)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Draw {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        colour: [f32; 4],
    },
    Tex {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
    },
    Turned {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
        turn: f32,
    },
    Game,
    Shot {
        tex: TexId,
    },
}

/// A face as the UI knows it: its size in canvas units, and, on a panel
/// that is not the canvas's own size, the panel-sized copy it was shrunk to.
struct Slot {
    tex: gl::types::GLuint,
    w: u32,
    h: u32,
    filter: gl::types::GLenum,
    fitted: Option<Fitted>,
}

#[derive(Copy, Clone, PartialEq, Debug)]
struct Fitted {
    scale: [f32; 2],
    w: u32,
    h: u32,
}

pub struct Sprites {
    prog: gl::types::GLuint,
    white: gl::types::GLuint,
    textures: Vec<Slot>,
    scale: [f32; 2],
    u_rect: gl::types::GLint,
    u_colour: gl::types::GLint,
    u_turn: gl::types::GLint,
}

impl Sprites {
    pub fn new() -> Result<Self, GfxError> {
        let prog = crate::shaders::program(SPRITE_VERT, SPRITE_FRAG)?;
        let white = crate::gl::texture(
            1,
            1,
            gl::NEAREST,
            gl::CLAMP_TO_EDGE,
            gl::RGBA,
            Some(&[255u8; 4]),
        );
        let (u_rect, u_colour, u_turn);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_tex"), 0);
            gl::Uniform2f(
                crate::gl::uniform_location(prog, "u_target"),
                OUT_W as f32,
                OUT_H as f32,
            );
            u_rect = crate::gl::uniform_location(prog, "u_rect");
            u_colour = crate::gl::uniform_location(prog, "u_colour");
            u_turn = crate::gl::uniform_location(prog, "u_turn");
        }
        Ok(Sprites {
            prog,
            white,
            textures: Vec::new(),
            scale: [1.0, 1.0],
            u_rect,
            u_colour,
            u_turn,
        })
    }

    pub fn create_texture(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.push(w, h, rgba, gl::LINEAR)
    }

    pub fn create_texture_nearest(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.push(w, h, rgba, gl::NEAREST)
    }

    /// Canvas pixels per canvas unit. Faces created after this is set are
    /// shrunk to the panel once, rather than by the GPU every frame.
    pub fn set_scale(&mut self, scale: [f32; 2]) {
        self.scale = scale;
    }

    fn fit(
        &self,
        w: u32,
        h: u32,
        rgba: &[u8],
        filter: gl::types::GLenum,
    ) -> (Option<Fitted>, Vec<u8>) {
        let [sx, sy] = self.scale;
        let shrinks = sx < 0.999 || sy < 0.999;
        if filter != gl::LINEAR || !shrinks || sx > 1.001 || sy > 1.001 {
            return (None, Vec::new());
        }
        let (fw, fh) = (
            ((w as f32 * sx).round() as u32).max(1),
            ((h as f32 * sy).round() as u32).max(1),
        );
        let out = crate::resize::area_rgba(rgba, w, h, fw, fh);
        (
            Some(Fitted {
                scale: self.scale,
                w: fw,
                h: fh,
            }),
            out,
        )
    }

    fn push(&mut self, w: u32, h: u32, rgba: &[u8], filter: gl::types::GLenum) -> TexId {
        let (fitted, small) = self.fit(w, h, rgba, filter);
        let tex = match fitted {
            Some(f) => {
                crate::gl::texture(f.w, f.h, filter, gl::CLAMP_TO_EDGE, gl::RGBA, Some(&small))
            }
            None => crate::gl::texture(w, h, filter, gl::CLAMP_TO_EDGE, gl::RGBA, Some(rgba)),
        };
        self.textures.push(Slot {
            tex,
            w,
            h,
            filter,
            fitted,
        });
        TexId(self.textures.len() - 1)
    }

    pub fn update_texture(&mut self, id: TexId, w: u32, h: u32, rgba: &[u8]) {
        let Some(slot) = self.textures.get(id.0) else {
            return;
        };
        if rgba.len() < (w * h * 4) as usize {
            return;
        }
        let (fitted, small) = self.fit(w, h, rgba, slot.filter);
        let tex = slot.tex;
        if let Some(slot) = self.textures.get_mut(id.0) {
            slot.w = w;
            slot.h = h;
            slot.fitted = fitted;
        }
        let (w, h, rgba) = match fitted {
            Some(f) => (f.w, f.h, &small[..]),
            None => (w, h, rgba),
        };
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                crate::gl::internal_format(gl::RGBA),
                w as i32,
                h as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                rgba.as_ptr() as *const std::ffi::c_void,
            );
        }
    }

    pub fn source(&self, id: TexId) -> Option<gl::types::GLuint> {
        self.textures.get(id.0).map(|s| s.tex)
    }

    /// Lands a rect on whole panel pixels. Edges round to the nearest pixel,
    /// and anything at least one canvas unit across keeps at least one pixel.
    fn snap_rect(&self, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
        let [sx, sy] = self.scale;
        let edge = |a: f32, len: f32, s: f32| {
            let lo = (a * s).round();
            let mut hi = ((a + len) * s).round();
            if len >= 1.0 && hi <= lo {
                hi = lo + 1.0;
            }
            (lo / s, (hi - lo) / s)
        };
        let (x, w) = edge(x, w, sx);
        let (y, h) = edge(y, h, sy);
        (x, y, w, h)
    }

    /// A face drawn at its own size lands on whole panel pixels at its
    /// fitted size, so the GPU copies it rather than resampling it.
    fn snap_face(&self, slot: &Slot, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
        let Some(f) = slot.fitted.filter(|f| f.scale == self.scale) else {
            return (x, y, w, h);
        };
        let natural = (w - slot.w as f32).abs() < 0.5 && (h - slot.h as f32).abs() < 0.5;
        if !natural {
            return (x, y, w, h);
        }
        let [sx, sy] = self.scale;
        (
            (x * sx).round() / sx,
            (y * sy).round() / sy,
            f.w as f32 / sx,
            f.h as f32 / sy,
        )
    }

    fn scaled(&self) -> bool {
        self.scale != [1.0, 1.0]
    }

    pub fn draw(&self, items: &[Draw], quad: &Quad) {
        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::UseProgram(self.prog);
            gl::ActiveTexture(gl::TEXTURE0);
        }
        for item in items {
            let (x, y, w, h, tex, colour, turn) = match *item {
                Draw::Rect { x, y, w, h, colour } if self.scaled() => {
                    let (x, y, w, h) = self.snap_rect(x, y, w, h);
                    (x, y, w, h, self.white, colour, 0.0)
                }
                Draw::Rect { x, y, w, h, colour } => (x, y, w, h, self.white, colour, 0.0),
                Draw::Tex {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                } => match self.textures.get(tex.0) {
                    Some(t) => {
                        let (x, y, w, h) = self.snap_face(t, x, y, w, h);
                        (x, y, w, h, t.tex, [1.0, 1.0, 1.0, alpha], 0.0)
                    }
                    None => continue,
                },
                Draw::Turned {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                    turn,
                } => match self.textures.get(tex.0) {
                    Some(t) => (x, y, w, h, t.tex, [1.0, 1.0, 1.0, alpha], turn),
                    None => continue,
                },
                Draw::Game | Draw::Shot { .. } => continue,
            };
            let (cos, sin) = if turn == 0.0 {
                (1.0, 0.0)
            } else {
                (turn.cos(), turn.sin())
            };
            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, tex);
                gl::Uniform4f(self.u_rect, x, y, w, h);
                gl::Uniform2f(self.u_turn, cos, sin);
                gl::Uniform4f(self.u_colour, colour[0], colour[1], colour[2], colour[3]);
            }
            quad.draw();
        }
        unsafe { gl::Disable(gl::BLEND) };
    }
}

impl Drop for Sprites {
    fn drop(&mut self) {
        unsafe {
            for slot in &self.textures {
                gl::DeleteTextures(1, &slot.tex);
            }
            gl::DeleteTextures(1, &self.white);
            gl::DeleteProgram(self.prog);
        }
    }
}
