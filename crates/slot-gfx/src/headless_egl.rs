use std::ffi::{c_char, c_void, CString};

use libloading::Library;

use crate::fbdev::egl_error;
use crate::surface::{GfxError, Surface, OUT_H, OUT_W};

const EGL_NONE: i32 = 0x3038;
const EGL_ALPHA_SIZE: i32 = 0x3021;
const EGL_BLUE_SIZE: i32 = 0x3022;
const EGL_GREEN_SIZE: i32 = 0x3023;
const EGL_RED_SIZE: i32 = 0x3024;
const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_WIDTH: i32 = 0x3057;
const EGL_RENDERABLE_TYPE: i32 = 0x3040;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_PBUFFER_BIT: i32 = 0x0001;
const EGL_OPENGL_ES2_BIT: i32 = 0x0004;
const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;

type Ptr = *mut c_void;
type GetPlatformDisplay = unsafe extern "C" fn(u32, Ptr, *const isize) -> Ptr;
type Initialize = unsafe extern "C" fn(Ptr, *mut i32, *mut i32) -> u32;
type BindApi = unsafe extern "C" fn(u32) -> u32;
type ChooseConfig = unsafe extern "C" fn(Ptr, *const i32, *mut Ptr, i32, *mut i32) -> u32;
type CreatePbufferSurface = unsafe extern "C" fn(Ptr, Ptr, *const i32) -> Ptr;
type CreateContext = unsafe extern "C" fn(Ptr, Ptr, Ptr, *const i32) -> Ptr;
type MakeCurrent = unsafe extern "C" fn(Ptr, Ptr, Ptr, Ptr) -> u32;
type DestroySurface = unsafe extern "C" fn(Ptr, Ptr) -> u32;
type DestroyContext = unsafe extern "C" fn(Ptr, Ptr) -> u32;
type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;
type GetError = unsafe extern "C" fn() -> i32;
type Terminate = unsafe extern "C" fn(Ptr) -> u32;

/// An offscreen GLES2 context for tests on Linux, through Mesa's surfaceless
/// EGL platform. It speaks the same GL dialect as the device, so shader and
/// pipeline tests exercise the code the SP actually runs.
pub struct HeadlessSurface {
    make_current: MakeCurrent,
    destroy_surface: DestroySurface,
    destroy_context: DestroyContext,
    get_proc_address: GetProcAddress,
    terminate: Terminate,
    display: Ptr,
    surface: Ptr,
    context: Ptr,
    _egl: Library,
    gles: Library,
}

unsafe fn sym<T: Copy>(lib: &Library, name: &str) -> Result<T, GfxError> {
    lib.get::<T>(name.as_bytes())
        .map(|s| *s)
        .map_err(|e| GfxError::Context(format!("{name}: {e}")))
}

fn open(name: &str) -> Result<Library, GfxError> {
    unsafe { Library::new(name) }.map_err(|e| GfxError::Context(format!("{name}: {e}")))
}

impl HeadlessSurface {
    pub fn new() -> Result<Self, GfxError> {
        let egl = open("libEGL.so.1").or_else(|_| open("libEGL.so"))?;
        let gles = open("libGLESv2.so.2").or_else(|_| open("libGLESv2.so"))?;
        unsafe {
            let get_error: GetError = sym(&egl, "eglGetError")?;
            let fail = |what: &str| egl_error(what, get_error());
            let get_platform_display: GetPlatformDisplay = sym(&egl, "eglGetPlatformDisplay")?;
            let initialize: Initialize = sym(&egl, "eglInitialize")?;
            let bind_api: BindApi = sym(&egl, "eglBindAPI")?;
            let choose_config: ChooseConfig = sym(&egl, "eglChooseConfig")?;
            let create_pbuffer: CreatePbufferSurface = sym(&egl, "eglCreatePbufferSurface")?;
            let create_context: CreateContext = sym(&egl, "eglCreateContext")?;
            let make_current: MakeCurrent = sym(&egl, "eglMakeCurrent")?;

            let display = get_platform_display(
                EGL_PLATFORM_SURFACELESS_MESA,
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            if display.is_null() {
                return Err(fail("eglGetPlatformDisplay"));
            }
            if initialize(display, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                return Err(fail("eglInitialize"));
            }
            if bind_api(EGL_OPENGL_ES_API) == 0 {
                return Err(fail("eglBindAPI"));
            }
            let attrs = [
                EGL_SURFACE_TYPE,
                EGL_PBUFFER_BIT,
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_ALPHA_SIZE,
                8,
                EGL_RENDERABLE_TYPE,
                EGL_OPENGL_ES2_BIT,
                EGL_NONE,
            ];
            let mut config: Ptr = std::ptr::null_mut();
            let mut found = 0;
            if choose_config(display, attrs.as_ptr(), &mut config, 1, &mut found) == 0 || found == 0
            {
                return Err(fail("eglChooseConfig"));
            }
            let size = [EGL_WIDTH, OUT_W as i32, EGL_HEIGHT, OUT_H as i32, EGL_NONE];
            let surface = create_pbuffer(display, config, size.as_ptr());
            if surface.is_null() {
                return Err(fail("eglCreatePbufferSurface"));
            }
            let ctx_attrs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let context = create_context(display, config, std::ptr::null_mut(), ctx_attrs.as_ptr());
            if context.is_null() {
                return Err(fail("eglCreateContext"));
            }
            if make_current(display, surface, surface, context) == 0 {
                return Err(fail("eglMakeCurrent"));
            }
            Ok(HeadlessSurface {
                make_current,
                destroy_surface: sym(&egl, "eglDestroySurface")?,
                destroy_context: sym(&egl, "eglDestroyContext")?,
                get_proc_address: sym(&egl, "eglGetProcAddress")?,
                terminate: sym(&egl, "eglTerminate")?,
                display,
                surface,
                context,
                _egl: egl,
                gles,
            })
        }
    }
}

impl Surface for HeadlessSurface {
    fn make_current(&mut self) -> Result<(), GfxError> {
        match unsafe { (self.make_current)(self.display, self.surface, self.surface, self.context) }
        {
            0 => Err(GfxError::Context("eglMakeCurrent refused".into())),
            _ => Ok(()),
        }
    }

    fn window_size(&self) -> (u32, u32) {
        (OUT_W, OUT_H)
    }

    fn swap(&mut self) -> Result<(), GfxError> {
        unsafe { gl::Flush() };
        Ok(())
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        let Ok(sym) = CString::new(name) else {
            return std::ptr::null();
        };
        unsafe {
            if let Ok(f) = self.gles.get::<*const c_void>(sym.as_bytes_with_nul()) {
                return f.into_raw().into_raw() as *const c_void;
            }
            (self.get_proc_address)(sym.as_ptr())
        }
    }
}

impl Drop for HeadlessSurface {
    fn drop(&mut self) {
        unsafe {
            (self.make_current)(
                self.display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            (self.destroy_context)(self.display, self.context);
            (self.destroy_surface)(self.display, self.surface);
            (self.terminate)(self.display);
        }
    }
}
