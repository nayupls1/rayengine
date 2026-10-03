//! Small context-bound quality bridge: device checks and UI blend factors.
#![allow(unsafe_code)]
use crate::Error;
use raylib::{ffi, prelude::*};
use std::ffi::c_void;

pub(super) fn load_target(
    _: &mut RaylibHandle,
    _: &RaylibThread,
    size: (u32, u32),
) -> Result<RenderTexture2D, Error> {
    type GenTextures = unsafe extern "system" fn(i32, *mut u32);
    type BindTexture = unsafe extern "system" fn(u32, u32);
    type TexImage =
        unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
    // Resolve everything before creating resources so errors cannot leak names.
    // SAFETY: Called only on the initialized OpenGL 3.3 render thread.
    let addresses = unsafe {
        [c"glGenTextures", c"glBindTexture", c"glTexImage2D"]
            .map(|name| ffi::rlGetProcAddress(name.as_ptr()))
    };
    if addresses.iter().any(|address| address.is_null()) {
        return Err(Error::Backend(
            "render texture allocation unavailable".into(),
        ));
    }
    // SAFETY: Procedures have the OpenGL signatures above. The plan bounds sizes
    // to positive i32 values. The owning wrapper releases color, depth and FBO
    // on both success and validation failure, matching raylib's own ownership.
    unsafe {
        let generate = std::mem::transmute::<*mut c_void, GenTextures>(addresses[0]);
        let bind = std::mem::transmute::<*mut c_void, BindTexture>(addresses[1]);
        let image = std::mem::transmute::<*mut c_void, TexImage>(addresses[2]);
        let id = ffi::rlLoadFramebuffer();
        if id == 0 {
            return Err(Error::Backend(
                "render framebuffer allocation failed".into(),
            ));
        }
        let mut color = ffi::Texture2D {
            id: 0,
            width: size.0 as i32,
            height: size.1 as i32,
            mipmaps: 1,
            format: ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
        };
        generate(1, &mut color.id);
        bind(0x0DE1, color.id); // GL_TEXTURE_2D
        // Explicit sized RGBA8 storage avoids raylib's null-data rlLoadTexture
        // path, which produces an empty color attachment in GCC 14 release builds.
        image(
            0x0DE1,
            0,
            0x8058,
            color.width,
            color.height,
            0,
            0x1908,
            0x1401,
            std::ptr::null(),
        );
        bind(0x0DE1, 0);
        ffi::rlTextureParameters(color.id, 0x2801, 0x2600); // MIN_FILTER: NEAREST
        ffi::rlTextureParameters(color.id, 0x2800, 0x2600); // MAG_FILTER: NEAREST
        let depth = ffi::Texture2D {
            id: ffi::rlLoadTextureDepth(color.width, color.height, true),
            format: 19, // Same depth metadata as LoadRenderTexture.
            ..color
        };
        ffi::rlFramebufferAttach(id, color.id, 0, 100, 0);
        ffi::rlFramebufferAttach(id, depth.id, 100, 200, 0);
        Ok(RenderTexture2D::from_raw(ffi::RenderTexture2D {
            id,
            texture: color,
            depth,
        }))
    }
}

pub(super) fn max_dimension(_: &RaylibThread) -> Result<u32, Error> {
    type GetInt = unsafe extern "system" fn(u32, *mut i32);
    // SAFETY: Called only after initialization on the context's render thread.
    let address = unsafe { ffi::rlGetProcAddress(c"glGetIntegerv".as_ptr()) };
    if address.is_null() {
        return Err(Error::Backend("glGetIntegerv unavailable".into()));
    }
    let mut limit = 0;
    // SAFETY: OpenGL 3.3 glGetIntegerv has this signature; limit is writable.
    unsafe {
        let get = std::mem::transmute::<*mut c_void, GetInt>(address);
        get(0x0D33, &mut limit); // GL_MAX_TEXTURE_SIZE
    }
    if limit <= 0 {
        return Err(Error::Backend("invalid device texture limit".into()));
    }
    Ok(limit as u32)
}

pub(super) fn complete(target: &RenderTexture2D, _: &RaylibThread) -> bool {
    // SAFETY: Target is owned, live and checked before any drawing pass begins.
    // rlFramebufferComplete restores the default FBO after checking the target.
    unsafe { ffi::rlFramebufferComplete(target.id) }
}

/// Scoped straight-source alpha blending into premultiplied offscreen targets.
/// Kept separate from the drawing guard so public raw pass types stay unchanged.
pub(crate) struct CoverageBlend<'thread>(std::marker::PhantomData<&'thread RaylibThread>);
pub(crate) fn coverage_blend(_: &RaylibThread) -> CoverageBlend<'_> {
    // SAFETY: Called on the live render thread within a texture drawing pass.
    // RGB becomes premultiplied; alpha accumulates coverage (ONE), avoiding
    // coverage squared. Begin/EndBlendMode flush batches at the scope boundaries.
    unsafe {
        ffi::rlSetBlendFactorsSeparate(0x0302, 0x0303, 1, 0x0303, 0x8006, 0x8006);
        ffi::BeginBlendMode(BlendMode::BLEND_CUSTOM_SEPARATE as i32);
    }
    CoverageBlend(std::marker::PhantomData)
}
impl Drop for CoverageBlend<'_> {
    fn drop(&mut self) {
        // SAFETY: The borrowed thread token keeps this guard within the live pass.
        unsafe {
            ffi::EndBlendMode();
        }
    }
}

pub(crate) fn graphics_info(_: &RaylibThread) -> [String; 3] {
    type GetString = unsafe extern "system" fn(u32) -> *const u8;
    // SAFETY: Queried on a live OpenGL context; function signature is OpenGL's.
    let address = unsafe { ffi::rlGetProcAddress(c"glGetString".as_ptr()) };
    if address.is_null() {
        return std::array::from_fn(|_| "unavailable".into());
    }
    // SAFETY: Non-null GL procedure has the stated signature. Returned strings
    // belong to the live driver; null is checked, data is copied before returning.
    unsafe {
        let get = std::mem::transmute::<*mut c_void, GetString>(address);
        [0x1F00, 0x1F01, 0x1F02].map(|name| {
            let value = get(name);
            if value.is_null() {
                "unavailable".into()
            } else {
                std::ffi::CStr::from_ptr(value.cast())
                    .to_string_lossy()
                    .into_owned()
            }
        })
    }
}
