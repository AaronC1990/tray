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
///
/// On top of that, a look: 0 none, 1 scanlines, 2 a light CRT, 3 an LCD
/// grid, 4 LCD dots. Each works in panel pixels against where the frame's
/// pixels land, so a line or a gap is one panel pixel wide however the
/// frame is scaled, and each holds back where the frame is too big for
/// it to have room, as a 480-line picture is for scanlines.
const PICTURE_FRAG: &str = r#"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D u_tex;
uniform vec2 u_src;
uniform vec2 u_scale;
uniform float u_mode;
varying vec2 v_uv;

vec3 sharp(vec2 t) {
    vec2 cell = floor(t);
    vec2 cd = t - cell - 0.5;
    vec2 range = max(vec2(0.5) - 0.5 / u_scale, vec2(0.0));
    vec2 f = (cd - clamp(cd, -range, range)) * u_scale + 0.5;
    return texture2D(u_tex, (cell + f) / u_src).rgb;
}

/// Whether this panel pixel is the last one down a frame pixel's height.
float last_row(vec2 t) {
    return step(1.0 - 1.0 / u_scale.y, fract(t.y));
}

void main() {
    vec2 t = v_uv * u_src;
    vec3 c = sharp(t);
    // Room for a look: each frame pixel at least about two panel pixels.
    vec2 roomy = step(vec2(1.75), u_scale);
    if (u_mode > 0.5 && u_mode < 1.5) {
        float dark = last_row(t) * roomy.y;
        c = c * mix(1.08, 0.45, dark);
    } else if (u_mode > 1.5 && u_mode < 2.5) {
        // A soft beam across, a dimmer gap between lines, and a faint
        // shadow mask of red, green and blue panel columns.
        vec2 px = vec2(0.5, 0.0) / u_src;
        vec3 soft = 0.25 * (texture2D(u_tex, v_uv - px).rgb
            + texture2D(u_tex, v_uv + px).rgb) + 0.5 * c;
        float dark = last_row(t) * roomy.y;
        c = soft * mix(1.18, 0.6, dark);
        float col = mod(floor(gl_FragCoord.x), 3.0);
        vec3 mask = vec3(0.88);
        if (col < 0.5) { mask.r = 1.06; }
        else if (col < 1.5) { mask.g = 1.06; }
        else { mask.b = 1.06; }
        c *= mask;
    } else if (u_mode > 2.5 && u_mode < 3.5) {
        vec2 edge = step(fract(t), 1.0 / u_scale) * roomy;
        c *= 1.0 - 0.38 * max(edge.x, edge.y);
    } else if (u_mode > 3.5) {
        // Each frame pixel a dot: lit at its top left, dimmer along its
        // last panel column and row, dimmest in the corner they share.
        vec2 far = step(1.0 - 1.0 / u_scale, fract(t)) * roomy;
        c *= 1.06 - 0.24 * (far.x + far.y);
    }
    FRAG_COLOR = vec4(min(c, vec3(1.0)), 1.0);
}
"#;

/// A look over a frame, chosen by its frontend.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum PictureLook {
    #[default]
    Sharp,
    Scanlines,
    Crt,
    Grid,
    Dot,
}

impl PictureLook {
    pub const ALL: [PictureLook; 5] = [
        PictureLook::Sharp,
        PictureLook::Scanlines,
        PictureLook::Crt,
        PictureLook::Grid,
        PictureLook::Dot,
    ];

    fn mode(self) -> f32 {
        self as u8 as f32
    }

    pub fn name(self) -> &'static str {
        match self {
            PictureLook::Sharp => "sharp",
            PictureLook::Scanlines => "scanlines",
            PictureLook::Crt => "crt",
            PictureLook::Grid => "grid",
            PictureLook::Dot => "dot",
        }
    }

    pub fn from_name(name: &str) -> Option<PictureLook> {
        PictureLook::ALL.into_iter().find(|l| l.name() == name)
    }

    /// The next look along, or the one before, round and round.
    pub fn step(self, next: bool) -> PictureLook {
        let n = PictureLook::ALL.len();
        let at = PictureLook::ALL
            .iter()
            .position(|l| *l == self)
            .unwrap_or(0);
        PictureLook::ALL[if next { (at + 1) % n } else { (at + n - 1) % n }]
    }
}

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
    u_mode: gl::types::GLint,
    look: PictureLook,
    quad: Quad,
}

impl PicturePass {
    pub fn new(surface: &dyn Surface) -> Result<Self, GfxError> {
        crate::gl::load(surface);
        let prog = crate::shaders::program(PICTURE_VERT, PICTURE_FRAG)?;
        let tex = crate::gl::texture(1, 1, gl::LINEAR, gl::CLAMP_TO_EDGE, gl::BGRA, None);
        let (u_src, u_scale, u_mode);
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_tex"), 0);
            u_src = crate::gl::uniform_location(prog, "u_src");
            u_scale = crate::gl::uniform_location(prog, "u_scale");
            u_mode = crate::gl::uniform_location(prog, "u_mode");
        }
        Ok(PicturePass {
            prog,
            tex,
            size: (1, 1),
            layout: FrameLayout::Xrgb8888,
            u_src,
            u_scale,
            u_mode,
            look: PictureLook::Sharp,
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

    pub fn set_look(&mut self, look: PictureLook) {
        self.look = look;
    }

    pub fn look(&self) -> PictureLook {
        self.look
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
            gl::Uniform1f(self.u_mode, self.look.mode());
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
