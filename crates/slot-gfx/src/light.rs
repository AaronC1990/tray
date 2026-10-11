use crate::quad::Quad;
use crate::surface::GfxError;

/// A light set into a bezel, lit over it: a lens of `radius` panel pixels
/// at `at` (from the panel's top left), glowing `rgb`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct BezelLight {
    pub at: (f32, f32),
    pub radius: f32,
    pub rgb: [f32; 3],
}

const LIGHT_VERT: &str = r#"
attribute vec2 a_pos;
void main() {
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// A lit lens with a soft edge and a small highlight toward its top left,
/// and a glow round it that fades into the bezel.
const LIGHT_FRAG: &str = r#"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform vec2 u_centre;
uniform float u_radius;
uniform vec3 u_rgb;
uniform vec3 u_gain;
uniform float u_alpha;
void main() {
    vec2 p = gl_FragCoord.xy - u_centre;
    float d = length(p);
    float lens = 1.0 - smoothstep(u_radius - 0.7, u_radius + 0.3, d);
    float glow = 0.42 * exp(-pow(d / (u_radius * 1.6), 2.0)) * (1.0 - lens);
    vec2 h = p - vec2(-0.35, 0.4) * u_radius;
    float shine = (1.0 - smoothstep(0.15 * u_radius, 0.5 * u_radius, length(h))) * lens;
    vec3 colour = mix(u_rgb, vec3(1.0, 0.95, 0.9), 0.55 * shine);
    FRAG_COLOR = vec4(colour * u_gain, max(lens, glow) * u_alpha);
}
"#;

pub struct LightPass {
    prog: gl::types::GLuint,
    u_centre: gl::types::GLint,
    u_radius: gl::types::GLint,
    u_rgb: gl::types::GLint,
    u_gain: gl::types::GLint,
    u_alpha: gl::types::GLint,
}

impl LightPass {
    pub fn new() -> Result<Self, GfxError> {
        let prog = crate::shaders::program(LIGHT_VERT, LIGHT_FRAG)?;
        let at = |name| crate::gl::uniform_location(prog, name);
        Ok(LightPass {
            u_centre: at("u_centre"),
            u_radius: at("u_radius"),
            u_rgb: at("u_rgb"),
            u_gain: at("u_gain"),
            u_alpha: at("u_alpha"),
            prog,
        })
    }

    /// Draws `light` on a `window` panel, `drop` pixels lower than its
    /// place, as the bezel it sits in is drawn.
    pub fn draw(
        &self,
        light: &BezelLight,
        window: (u32, u32),
        drop: i32,
        gain: [f32; 3],
        alpha: f32,
        quad: &Quad,
    ) {
        // GL counts rows from the bottom.
        let cx = light.at.0;
        let cy = window.1 as f32 - light.at.1 - drop as f32;
        let reach = (light.radius * 4.0).ceil();
        let (x, y) = ((cx - reach).floor() as i32, (cy - reach).floor() as i32);
        let side = (2.0 * reach) as i32 + 2;
        unsafe {
            gl::Viewport(x, y, side, side);
            gl::UseProgram(self.prog);
            gl::Uniform2f(self.u_centre, cx, cy);
            gl::Uniform1f(self.u_radius, light.radius);
            gl::Uniform3f(self.u_rgb, light.rgb[0], light.rgb[1], light.rgb[2]);
            gl::Uniform3f(self.u_gain, gain[0], gain[1], gain[2]);
            gl::Uniform1f(self.u_alpha, alpha);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }
        quad.draw();
        unsafe { gl::Disable(gl::BLEND) };
    }
}

impl Drop for LightPass {
    fn drop(&mut self) {
        unsafe { gl::DeleteProgram(self.prog) };
    }
}
