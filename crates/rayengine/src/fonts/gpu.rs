//! Render-thread upload bridge for borrowed RGBA atlas bytes.
#![allow(unsafe_code)]

use super::FontSampling;
use crate::Error;
use raylib::{ffi, prelude::*};
use std::ffi::c_void;

#[cfg(test)]
pub(super) fn environment(_thread: &RaylibThread) -> Vec<String> {
    type GetString = unsafe extern "system" fn(u32) -> *const u8;
    // SAFETY: The render-thread token keeps this query on a live desktop GL
    // context; glGetString returns static, NUL-terminated strings or null.
    unsafe {
        let address = ffi::rlGetProcAddress(c"glGetString".as_ptr());
        if address.is_null() {
            return Vec::new();
        }
        let get: GetString = std::mem::transmute(address);
        [0x1F00, 0x1F01, 0x1F02]
            .into_iter()
            .map(|name| {
                let pointer = get(name);
                if pointer.is_null() {
                    "unavailable".into()
                } else {
                    std::ffi::CStr::from_ptr(pointer.cast())
                        .to_string_lossy()
                        .into_owned()
                }
            })
            .collect()
    }
}

/// Query real desktop GL storage/filtering while preserving the texture binding.
fn texture_info(_thread: &RaylibThread, texture: &Texture2D) -> Result<[i32; 4], Error> {
    type GetInteger = unsafe extern "system" fn(u32, *mut i32);
    type BindTexture = unsafe extern "system" fn(u32, u32);
    type GetLevel = unsafe extern "system" fn(u32, i32, u32, *mut i32);
    type GetParameter = unsafe extern "system" fn(u32, u32, *mut i32);
    let load = |name: &std::ffi::CStr| -> Result<*mut c_void, Error> {
        // SAFETY: This is the live owning render thread and a static C name.
        let address = unsafe { ffi::rlGetProcAddress(name.as_ptr()) };
        if address.is_null() {
            Err(Error::Asset(format!(
                "font atlas: GL procedure {} unavailable",
                name.to_string_lossy()
            )))
        } else {
            Ok(address)
        }
    };
    // SAFETY: Non-null functions use the exact desktop GL signatures/calling
    // convention above. All output pointers are initialized i32s. The texture
    // belongs to this live context; restore binding without changing active unit.
    unsafe {
        let get: GetInteger = std::mem::transmute(load(c"glGetIntegerv")?);
        let bind: BindTexture = std::mem::transmute(load(c"glBindTexture")?);
        let level: GetLevel = std::mem::transmute(load(c"glGetTexLevelParameteriv")?);
        let parameter: GetParameter = std::mem::transmute(load(c"glGetTexParameteriv")?);
        let mut previous = 0;
        let mut info = [0; 4];
        get(0x8069, &mut previous); // GL_TEXTURE_BINDING_2D
        bind(0x0DE1, texture.id); // GL_TEXTURE_2D
        level(0x0DE1, 0, 0x1000, &mut info[0]); // GL_TEXTURE_WIDTH
        level(0x0DE1, 0, 0x1001, &mut info[1]); // GL_TEXTURE_HEIGHT
        parameter(0x0DE1, 0x2801, &mut info[2]); // GL_TEXTURE_MIN_FILTER
        parameter(0x0DE1, 0x2800, &mut info[3]); // GL_TEXTURE_MAG_FILTER
        bind(0x0DE1, previous as u32);
        Ok(info)
    }
}

pub(super) fn upload(
    thread: &RaylibThread,
    pixels: &[u8],
    width: i32,
    height: i32,
    sampling: FontSampling,
) -> Result<Texture2D, Error> {
    assert!(width > 0 && height > 0);
    assert_eq!(pixels.len(), width as usize * height as usize * 4);
    let image = ffi::Image {
        data: pixels.as_ptr().cast_mut().cast(),
        width,
        height,
        mipmaps: 1,
        format: ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
    };
    // SAFETY: The thread token comes from the runner's live GL context. raylib
    // copies these exact, bounded RGBA bytes synchronously and never owns them.
    // The Image descriptor is not wrapped/dropped (its data belongs to Rust).
    // Only the returned texture becomes owned; Texture2D releases it once.
    let texture = unsafe { Texture2D::from_raw(ffi::LoadTextureFromImage(image)) };
    if texture.id == 0 {
        return Err(Error::Asset("font atlas GPU upload failed".into()));
    }
    texture.set_texture_filter(
        thread,
        match sampling {
            FontSampling::Smooth => TextureFilter::TEXTURE_FILTER_BILINEAR,
            FontSampling::Nearest => TextureFilter::TEXTURE_FILTER_POINT,
        },
    );
    let expected_filter = match sampling {
        FontSampling::Smooth => 0x2601,
        FontSampling::Nearest => 0x2600,
    };
    if texture_info(thread, &texture)? != [width, height, expected_filter, expected_filter] {
        return Err(Error::Asset(
            "font atlas GPU storage or sampling initialization failed".into(),
        ));
    }
    Ok(texture)
}
