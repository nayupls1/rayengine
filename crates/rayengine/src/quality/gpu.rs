//! Small context-bound quality bridge: device checks and UI blend factors.
#![allow(unsafe_code)]
use crate::Error;
use raylib::{ffi, prelude::*};
use std::ffi::c_void;

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
