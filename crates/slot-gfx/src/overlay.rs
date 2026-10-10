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
}
