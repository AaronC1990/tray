use crate::surface::GfxError;

const VERT_PREAMBLE: &str = "#version 330 core\n#define attribute in\n#define varying out\n";

const FRAG_PREAMBLE: &str = "#version 330 core\n#define varying in\n\
                             #define texture2D texture\nout vec4 FRAG_COLOR;\n";

const VERT_PREAMBLE_ES: &str = "";
const FRAG_PREAMBLE_ES: &str = "#define FRAG_COLOR gl_FragColor\n";

pub fn program(vert: &str, frag: &str) -> Result<gl::types::GLuint, GfxError> {
    let (vp, fp) = match crate::gl::es() {
        true => (VERT_PREAMBLE_ES, FRAG_PREAMBLE_ES),
        false => (VERT_PREAMBLE, FRAG_PREAMBLE),
    };
    crate::gl::program(&format!("{vp}{vert}"), &format!("{fp}{frag}"))
}

pub const RECT_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_rect;
uniform vec2 u_target;
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    vec2 p = (u_rect.xy + a_pos * u_rect.zw) / u_target;
    gl_Position = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}
"#;

pub const SPRITE_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_rect;
uniform vec2 u_target;
uniform vec2 u_turn;
varying vec2 v_uv;
void main() {
    v_uv = a_pos;
    vec2 mid = u_rect.zw * 0.5;
    vec2 local = a_pos * u_rect.zw - mid;
    vec2 turned = vec2(u_turn.x * local.x - u_turn.y * local.y,
                       u_turn.y * local.x + u_turn.x * local.y);
    vec2 p = (u_rect.xy + a_pos * u_rect.zw + (turned - local)) / u_target;
    gl_Position = vec4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}
"#;

pub const GAME_FRAG: &str = r#"
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform sampler2D u_game;
uniform sampler2D u_paper;
uniform vec2 u_src;
uniform vec4 u_uv;
uniform float u_bright;
uniform float u_mode;
uniform float u_paper_size;
uniform vec4 u_rect;
uniform vec2 u_fbo;
uniform vec4 u_pic;
uniform vec4 u_vis;
uniform vec2 u_texel_px;
uniform float u_whole;
varying vec2 v_uv;
const vec3 offsets = vec3(3.141592654) * vec3(1.0 / 2.0, 1.0 / 2.0 - 2.0 / 3.0, 1.0 / 2.0 - 4.0 / 3.0);
const vec3 luma_coeff = vec3(0.2126, 0.7152, 0.0722);
float dot_weight(vec2 f, vec2 o, float bloom) {
    return exp(-4.0 * length(f - (o + vec2(0.5))) * bloom);
}
float dot_factor(vec2 f, float bloom) {
    float sum = 0.0;
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            sum += dot_weight(f, vec2(float(i), float(j)), bloom);
        }
    }
    return mix(1.2 * dot_weight(f, vec2(0.0), bloom), sum, 0.85);
}
vec3 texel(vec2 t) {
    return texture2D(u_game, (t + vec2(0.5)) / u_src).rgb;
}
vec3 sharp(vec2 t) {
    vec2 range = max(vec2(0.5) - 0.5 / u_texel_px, vec2(0.0));
    vec2 cd = fract(t) - 0.5;
    vec2 p = floor(t) + (cd - clamp(cd, -range, range)) * u_texel_px;
    vec2 i = floor(p);
    vec2 w = p - i;
    vec3 top = mix(texel(i), texel(i + vec2(1.0, 0.0)), w.x);
    vec3 bot = mix(texel(i + vec2(0.0, 1.0)), texel(i + vec2(1.0, 1.0)), w.x);
    return mix(top, bot, w.y);
}
vec3 lcd_stripes(float t, float h) {
    vec3 a = vec3(6.283185307 * (t - h)) + offsets;
    vec3 b = vec3(6.283185307 * (t + h)) + offsets;
    return (cos(a) - cos(b)) / (6.283185307 * 2.0 * h);
}
float lcd_lines(float t, float h) {
    float a = 6.283185307 * (t - h);
    float b = 6.283185307 * (t + h);
    return (cos(a) - cos(b)) / (6.283185307 * 2.0 * h);
}
vec2 grid_run(vec2 x) {
    return floor(x) / 3.0 + min(fract(x), vec2(1.0 / 3.0));
}
void main() {
    vec2 uv = u_uv.xy + v_uv * u_uv.zw;
    vec2 f = fract(uv * u_src);
    bool whole = u_whole > 0.5;
    bool roomy = min(u_texel_px.x, u_texel_px.y) > 2.999;
    bool fine = whole && roomy;
    vec2 h = 0.5 / u_texel_px;
    vec3 c = whole ? texture2D(u_game, uv).rgb : sharp(uv * u_src);
    bool inside = all(greaterThanEqual(uv, u_pic.xy)) && all(lessThan(uv, u_pic.zw));
    if (!inside) {
    } else if (u_mode > 3.5) {
        c = pow(c, vec3(1.05));
        vec2 d = abs(f - 0.5);
        float x2 = max(d.x, d.y);
        x2 = x2 * x2;
        float x4 = x2 * x2;
        float w = 48.0 * (x4 - (8.0 / 3.0) * x4 * x2);
        w = w * (w + (1.0 - w) * 0.65) * 0.65;
        float luma = dot(c, luma_coeff);
        w = w * luma;
        c = mix(c, vec3(1.0), w);
        vec3 bg = texture2D(u_paper, (floor(gl_FragCoord.xy) + 0.5) / u_paper_size).rgb * c;
        c = mix(c, bg, dot(c, luma_coeff));
    } else if (u_mode > 2.5) {
        float bloom = mix(1.05, 0.95, dot(c, vec3(0.30, 0.59, 0.11)));
        if (max(u_texel_px.x, u_texel_px.y) < 2.5) {
            vec2 q = 0.5 * h;
            float k = dot_factor(fract(f + vec2(-q.x, -q.y)), bloom)
                    + dot_factor(fract(f + vec2(q.x, -q.y)), bloom)
                    + dot_factor(fract(f + vec2(-q.x, q.y)), bloom)
                    + dot_factor(fract(f + vec2(q.x, q.y)), bloom);
            c *= 0.25 * k;
        } else {
            float mid = dot_weight(f, vec2(0.0), bloom);
            float sum = 0.0;
            for (int j = -1; j <= 1; j++) {
                for (int i = -1; i <= 1; i++) {
                    sum += dot_weight(f, vec2(float(i), float(j)), bloom);
                }
            }
            c = mix(1.2 * c * mid, c * sum, 0.85);
        }
    } else if (u_mode > 1.5) {
        vec2 hp = 0.5 * u_uv.zw * u_src / (u_rect.zw * u_fbo);
        vec2 edge = max(step(f - hp, vec2(1e-4)), step(vec2(1.0) + 1e-4, f + hp));
        if (!fine) {
            vec2 t = uv * u_src;
            edge = (grid_run(t + h) - grid_run(t - h)) / (2.0 * h);
        }
        c *= 1.0 - 0.75 * max(edge.x, edge.y);
    } else if (u_mode > 0.5) {
        vec2 angle = f * 6.283185307;
        float yfactor = (16.0 + sin(angle.y)) / 17.0;
        vec3 xfactors = (4.0 + sin(angle.x + offsets)) / 5.0;
        if (!roomy) {
            vec2 t = uv * u_src;
            yfactor = (16.0 + lcd_lines(t.y, h.y)) / 17.0;
            xfactors = (4.0 + lcd_stripes(t.x, h.x)) / 5.0;
        }
        c *= yfactor * xfactors;
    }
    bool shown = all(greaterThanEqual(uv, u_vis.xy)) && all(lessThan(uv, u_vis.zw));
    if (!shown) {
        c = vec3(0.0);
    }
    FRAG_COLOR = vec4(c * u_bright, 1.0);
}
"#;

pub const SPRITE_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec4 u_colour;
varying vec2 v_uv;
void main() {
    FRAG_COLOR = texture2D(u_tex, v_uv) * u_colour;
}
"#;

pub const BLIT_VERT: &str = r#"
attribute vec2 a_pos;
uniform vec4 u_uv;
varying vec2 v_uv;
void main() {
    v_uv = u_uv.xy + a_pos * u_uv.zw;
    gl_Position = vec4(a_pos * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub const BLIT_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec3 u_gain;
varying vec2 v_uv;
void main() {
    FRAG_COLOR = vec4(texture2D(u_tex, v_uv).rgb * u_gain, 1.0);
}
"#;

pub const BEZEL_FRAG: &str = r#"
precision mediump float;
uniform sampler2D u_tex;
uniform vec3 u_gain;
uniform float u_alpha;
varying vec2 v_uv;
void main() {
    vec4 c = texture2D(u_tex, v_uv);
    FRAG_COLOR = vec4(c.rgb * u_gain, c.a * u_alpha);
}
"#;
