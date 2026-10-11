mod bezel;
mod draw;
mod fbdev;
mod fbo;
mod gl;
mod grade;
#[cfg(target_os = "macos")]
mod headless;
#[cfg(target_os = "linux")]
mod headless_egl;
#[cfg(feature = "host")]
mod host;
mod layout;
mod lcd3x;
mod light;
mod overlay;
mod picture;
mod pipeline;
mod power;
mod quad;
mod resize;
mod shaders;
mod surface;

pub use bezel::{framed, lifted_rect, screen_rect};
pub use draw::{Draw, TexId};
pub use fbdev::{egl_error, panel_mode, panel_size, FbdevSurface};
pub use fbo::{canvas_size, Compositor, BACKDROP};
pub use grade::{blue_light_gain, BLUE_LIGHT_MAX};
#[cfg(target_os = "macos")]
pub use headless::HeadlessSurface;
#[cfg(target_os = "linux")]
pub use headless_egl::HeadlessSurface;
#[cfg(feature = "host")]
pub use host::HostSurface;
pub use layout::{game_view, GameFit, GameView, MAX_TRIM_PER_SIDE};
pub use lcd3x::{lcd3x_factors, lcd3x_mask, ScreenEffect};
pub use light::BezelLight;
pub use overlay::Overlay;
pub use picture::{fit_4x3, FrameLayout, PictureLook, PicturePass};
pub use pipeline::{SCALE, SRC_H, SRC_W, WHOLE_TEXTURE};
pub use power::{screen_brightness, screen_scale, screen_width};
pub use resize::area_rgba;
pub use surface::{blit_rect, blit_rect_fit, fit_rect, fit_scale, GfxError, Surface, OUT_H, OUT_W};
