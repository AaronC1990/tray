use crate::quad::Quad;
use crate::surface::{GfxError, Surface};

/// The largest 4:3 rect that fits the panel, centred, in whole pixels.
/// A PlayStation shows every resolution it draws, 256 to 640 wide, at 4:3.
pub fn fit_4x3(panel: (u32, u32)) -> (i32, i32, i32, i32) {
    let (pw, ph) = panel;
    let (w, h) = if pw * 3 >= ph * 4 {
        (ph * 4 / 3, ph)
    } else {
        (pw, pw * 3 / 4)
    };
    (
        ((pw - w) / 2) as i32,
        ((ph - h) / 2) as i32,
        w as i32,
        h as i32,
    )
}

const PICTURE_VERT: &str = r#"
attribute vec2 a_pos;
varying vec2 v_uv;
void main() {
    v_uv = vec2(a_pos.x, 1.0 - a_pos.y);
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// Sharp bilinear: each frame pixel is solid in its middle and blends with
/// its neighbour only across the one panel pixel it shares with it, so
/// pixels stay the same size at any scale. At a whole scale every panel
/// pixel samples a texel's centre, which is plain nearest sampling.
const PICTURE_FRAG: &str = r#"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D u_tex;
uniform vec2 u_src;
uniform vec2 u_scale;
varying vec2 v_uv;
void main() {
    vec2 t = v_uv * u_src;
    vec2 cell = floor(t);
    vec2 cd = t - cell - 0.5;
    vec2 range = max(vec2(0.5) - 0.5 / u_scale, vec2(0.0));
    vec2 f = (cd - clamp(cd, -range, range)) * u_scale + 0.5;
    FRAG_COLOR = vec4(texture2D(u_tex, (cell + f) / u_src).rgb, 1.0);
}
"#;

/// Draws one frame of any size across the panel's 4:3 area, for cores
/// whose picture is not slot's fixed 240x160.
/// How a frame's pixels are laid out in memory.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FrameLayout {
    /// Four bytes a pixel: B, G, R, X.
    Xrgb8888,
    /// Two bytes a pixel, little endian, red in the top five bits. The GPU
    /// takes these as they are, half the upload of the same frame widened.
    Rgb565,
}

impl FrameLayout {
    fn bytes(self) -> u32 {
        match self {
            FrameLayout::Xrgb8888 => 4,
            FrameLayout::Rgb565 => 2,
        }
    }

    fn gl(self) -> (gl::types::GLenum, gl::types::GLenum) {
        match self {
            FrameLayout::Xrgb8888 => (gl::BGRA, gl::UNSIGNED_BYTE),
            FrameLayout::Rgb565 => (gl::RGB, gl::UNSIGNED_SHORT_5_6_5),
        }
    }
}

pub struct PicturePass {
    prog: gl::types::GLuint,
    tex: gl::types::GLuint,
    size: (u32, u32),
    layout: FrameLayout,
    u_src: gl::types::GLint,
    u_scale: gl::types::GLint,
    quad: Quad,
}

impl PicturePass {
    pub fn new(surface: &dyn Surface) -> Result<Self, GfxError> {
        crate::gl::load(surface);
        let prog = crate::shaders::program(PICTURE_VERT, PICTURE_FRAG)?;
        let tex = crate::gl::texture(1, 1, gl::LINEAR, gl::CLAMP_TO_EDGE, gl::BGRA, None);
        let (u_src, u_scale);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_tex"), 0);
            u_src = crate::gl::uniform_location(prog, "u_src");
            u_scale = crate::gl::uniform_location(prog, "u_scale");
        }
        Ok(PicturePass {
            prog,
            tex,
            size: (1, 1),
            layout: FrameLayout::Xrgb8888,
            u_src,
            u_scale,
            quad: Quad::new(),
        })
    }

    /// Takes a frame as XRGB8888 (B, G, R, X in memory), row 0 at the top.
    pub fn upload(&mut self, xrgb: &[u8], w: u32, h: u32) {
        self.upload_as(FrameLayout::Xrgb8888, xrgb, w, h);
    }

    /// Takes a frame of packed rows in `layout`, row 0 at the top.
    pub fn upload_as(&mut self, layout: FrameLayout, pixels: &[u8], w: u32, h: u32) {
        let bytes = (w * h * layout.bytes()) as usize;
        if w == 0 || h == 0 || pixels.len() < bytes {
            return;
        }
        let (format, kind) = layout.gl();
        let data = pixels.as_ptr() as *const std::ffi::c_void;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            if (w, h) == self.size && layout == self.layout {
                gl::TexSubImage2D(
                    gl::TEXTURE_2D,
                    0,
                    0,
                    0,
                    w as i32,
                    h as i32,
                    format,
                    kind,
                    data,
                );
                return;
            }
            let internal = match (crate::gl::es(), layout) {
                (true, _) => format as gl::types::GLint,
                (false, FrameLayout::Xrgb8888) => gl::RGBA8 as gl::types::GLint,
                (false, FrameLayout::Rgb565) => gl::RGB8 as gl::types::GLint,
            };
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                internal,
                w as i32,
                h as i32,
                0,
                format,
                kind,
                data,
            );
        }
        self.size = (w, h);
        self.layout = layout;
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Clears the panel to black and draws the frame over its 4:3 area.
    pub fn draw(&self, panel: (u32, u32)) {
        let (x, y, w, h) = fit_4x3(panel);
        let (fw, fh) = (self.size.0 as f32, self.size.1 as f32);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, panel.0 as i32, panel.1 as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::Viewport(x, y, w, h);
            gl::UseProgram(self.prog);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
            gl::Uniform2f(self.u_src, fw, fh);
            gl::Uniform2f(self.u_scale, w as f32 / fw, h as f32 / fh);
        }
        self.quad.draw();
    }
}

impl Drop for PicturePass {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.tex);
            gl::DeleteProgram(self.prog);
        }
    }
}
