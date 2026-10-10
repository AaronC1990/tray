use crate::draw::{Draw, Sprites, TexId};
use crate::quad::Quad;
use crate::surface::{GfxError, Surface};

/// Draws sprites straight onto the panel in its own pixels, for screens
/// laid out for the panel rather than for slot's 720x480 canvas. A face
/// drawn at its own size at a whole position lands texel for texel.
pub struct Overlay {
    sprites: Sprites,
    quad: Quad,
    panel: (u32, u32),
    snapshot: Option<TexId>,
}

impl Overlay {
    pub fn new(surface: &dyn Surface, panel: (u32, u32)) -> Result<Self, GfxError> {
        crate::gl::load(surface);
        let mut sprites = Sprites::new()?;
        sprites.set_target(panel.0 as f32, panel.1 as f32);
        Ok(Overlay {
            sprites,
            quad: Quad::new(),
            panel,
            snapshot: None,
        })
    }

    pub fn create_texture(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.sprites.create_texture(w, h, rgba)
    }

    pub fn update_texture(&mut self, id: TexId, w: u32, h: u32, rgba: &[u8]) {
        self.sprites.update_texture(id, w, h, rgba);
    }

    /// Clears the panel to `clear` and draws `items` over it, in order.
    pub fn draw(&self, clear: [f32; 3], items: &[Draw]) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, self.panel.0 as i32, self.panel.1 as i32);
            gl::ClearColor(clear[0], clear[1], clear[2], 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        self.sprites.draw(items, &self.quad);
    }

    /// Draws `items` over whatever the panel already shows.
    pub fn draw_over(&self, items: &[Draw]) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, self.panel.0 as i32, self.panel.1 as i32);
        }
        self.sprites.draw(items, &self.quad);
    }

    /// Copies what has been drawn to the panel this frame into a face, so
    /// it can be drawn again while the next screen takes over. The copy is
    /// bottom row first, as the panel stores it: `snapshot_draw` places it
    /// the right way up.
    pub fn snapshot(&mut self) -> TexId {
        let (w, h) = self.panel;
        let id = match self.snapshot {
            Some(id) => id,
            None => {
                let id = self
                    .sprites
                    .create_texture(w, h, &vec![0; (w * h * 4) as usize]);
                self.snapshot = Some(id);
                id
            }
        };
        if let Some(tex) = self.sprites.source(id) {
            unsafe {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::BindTexture(gl::TEXTURE_2D, tex);
                // RGB, since the panel may have no alpha to copy, and a copy
                // may only take the channels the panel has.
                gl::CopyTexImage2D(gl::TEXTURE_2D, 0, gl::RGB, 0, 0, w as i32, h as i32, 0);
            }
        }
        id
    }

    /// The draw that shows the last snapshot shifted `dx` panel pixels.
    pub fn snapshot_draw(&self, dx: f32, alpha: f32) -> Option<Draw> {
        let (w, h) = (self.panel.0 as f32, self.panel.1 as f32);
        self.snapshot.map(|tex| Draw::Tex {
            x: dx,
            y: h,
            w,
            h: -h,
            tex,
            alpha,
        })
    }
}
