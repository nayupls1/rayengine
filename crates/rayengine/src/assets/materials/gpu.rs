//! Private render-thread shader/material bridge. Views borrow owned resources.
#![allow(unsafe_code)]

use crate::{Error, material::AlphaMode};
use raylib::{ffi, prelude::*};
use std::{
    ffi::{CStr, CString, c_void},
    marker::PhantomData,
};

fn proc(name: &CStr) -> Result<*mut c_void, Error> {
    // SAFETY: All callers run inside initialized InitContext/Frame callbacks.
    let address = unsafe { ffi::rlGetProcAddress(name.as_ptr()) };
    if address.is_null() {
        Err(Error::Asset(format!(
            "OpenGL procedure {} unavailable",
            name.to_string_lossy()
        )))
    } else {
        Ok(address)
    }
}

pub(super) fn is_custom(shader: &Shader) -> bool {
    // SAFETY: Called with a live context and an owned raylib shader.
    shader.id != 0 && shader.id != unsafe { ffi::rlGetShaderIdDefault() } && !shader.locs.is_null()
}

pub(super) fn uniform_type(shader: &Shader, name: &str) -> Result<u32, Error> {
    type Indices = unsafe extern "system" fn(u32, i32, *const *const std::ffi::c_char, *mut u32);
    type Info = unsafe extern "system" fn(u32, i32, *const u32, u32, *mut i32);
    let name = CString::new(name).map_err(|_| Error::Asset("uniform name contains NUL".into()))?;
    let mut index = u32::MAX;
    let mut kind = 0;
    let mut size = 0;
    // SAFETY: Non-null procedures have the exact OpenGL 3.3 signatures. Shader
    // is alive on this render thread. All counts are one and all pointers target
    // initialized values / a live terminated C string.
    unsafe {
        let indices = std::mem::transmute::<*mut c_void, Indices>(proc(c"glGetUniformIndices")?);
        let info = std::mem::transmute::<*mut c_void, Info>(proc(c"glGetActiveUniformsiv")?);
        indices(shader.id, 1, &name.as_ptr(), &mut index);
        if index == u32::MAX {
            return Err(Error::Asset("uniform is missing or optimized out".into()));
        }
        info(shader.id, 1, &index, 0x8A37, &mut kind); // GL_UNIFORM_TYPE
        info(shader.id, 1, &index, 0x8A38, &mut size); // GL_UNIFORM_SIZE
    }
    if size != 1 {
        return Err(Error::Asset("uniform arrays are not supported".into()));
    }
    Ok(kind as u32)
}

/// Draw a non-owning material view; native pointers cannot escape this call.
pub(super) fn draw<D: RaylibDraw + RaylibDraw3D>(
    raw: &mut D,
    mesh: impl AsRef<ffi::Mesh>,
    shader: &Shader,
    texture: Option<&Texture2D>,
    tint: Color,
    transform: Matrix,
) {
    let mut maps = [ffi::MaterialMap::default(); raylib::consts::MAX_MATERIAL_MAPS as usize];
    // SAFETY: The default white texture is raylib-owned and alive in this context.
    let texture = texture
        .map(|texture| *texture.as_ref())
        .unwrap_or_else(|| ffi::Texture2D {
            id: unsafe { ffi::rlGetTextureIdDefault() },
            width: 1,
            height: 1,
            mipmaps: 1,
            format: 7,
        });
    maps[0] = ffi::MaterialMap {
        texture,
        color: tint,
        value: 0.0,
    };
    // SAFETY: Maps have exactly MAX_MATERIAL_MAPS entries and remain alive until
    // DrawMesh returns. Shader/texture are borrowed from Assets through that call.
    // WeakMaterial never unloads these shared resources. No pointer escapes.
    let material = unsafe {
        WeakMaterial::from_raw(ffi::Material {
            shader: *shader.as_ref(),
            maps: maps.as_mut_ptr(),
            params: [0.0; 4],
        })
    };
    raw.draw_mesh(mesh, material, transform);
}

type GetInt = unsafe extern "system" fn(u32, *mut i32);
type IsEnabled = unsafe extern "system" fn(u32) -> u8;
type Toggle = unsafe extern "system" fn(u32);
type DepthMask = unsafe extern "system" fn(u8);
type BlendFunc = unsafe extern "system" fn(u32, u32, u32, u32);
type BlendEquation = unsafe extern "system" fn(u32, u32);

/// Context-local function table. Never sent to workers or retained between runs.
#[derive(Clone, Copy)]
pub(super) struct RenderState {
    get: GetInt,
    enabled: IsEnabled,
    enable: Toggle,
    disable: Toggle,
    depth: DepthMask,
    blend: BlendFunc,
    equation: BlendEquation,
    _thread: PhantomData<RaylibThread>,
}

impl RenderState {
    pub(super) fn load(_: &RaylibThread) -> Result<Self, Error> {
        // SAFETY: Live render context, exact OpenGL names/signatures/ABI. The
        // private table lives in Assets; its thread marker prevents worker use.
        unsafe {
            Ok(Self {
                get: std::mem::transmute::<*mut c_void, GetInt>(proc(c"glGetIntegerv")?),
                enabled: std::mem::transmute::<*mut c_void, IsEnabled>(proc(c"glIsEnabled")?),
                enable: std::mem::transmute::<*mut c_void, Toggle>(proc(c"glEnable")?),
                disable: std::mem::transmute::<*mut c_void, Toggle>(proc(c"glDisable")?),
                depth: std::mem::transmute::<*mut c_void, DepthMask>(proc(c"glDepthMask")?),
                blend: std::mem::transmute::<*mut c_void, BlendFunc>(proc(c"glBlendFuncSeparate")?),
                equation: std::mem::transmute::<*mut c_void, BlendEquation>(proc(
                    c"glBlendEquationSeparate",
                )?),
                _thread: PhantomData,
            })
        }
    }

    pub(super) fn begin(self) -> SurfaceGuard {
        let mut values = [0; 7];
        // SAFETY: Function table belongs to this live render context; each
        // queried enum writes one initialized GLint. Snapshot once per pass.
        unsafe {
            for (slot, parameter) in values
                .iter_mut()
                .zip([0x0B72, 0x80C9, 0x80C8, 0x80CB, 0x80CA, 0x8009, 0x883D])
            {
                (self.get)(parameter, slot);
            }
            SurfaceGuard {
                state: self,
                values,
                blending: (self.enabled)(0x0BE2),
                current: None,
            }
        }
    }
}

/// Restores depth-write/blend state before legacy drawing and on unwind/pass exit.
pub(crate) struct SurfaceGuard {
    state: RenderState,
    values: [i32; 7],
    blending: u8,
    current: Option<bool>,
}

impl SurfaceGuard {
    pub(crate) fn apply(&mut self, mode: AlphaMode) {
        let blend = mode == AlphaMode::Blend;
        if self.current == Some(blend) {
            return;
        }
        // SAFETY: Private guard cannot outlive the active camera pass/context.
        unsafe {
            // Raylib primitives are batched, while DrawMesh is immediate. Submit
            // preceding primitives with their original depth/blend state before
            // changing it, preserving SDK call order across the two paths.
            ffi::rlDrawRenderBatchActive();
            (self.state.depth)(u8::from(!blend));
            if blend {
                (self.state.enable)(0x0BE2);
                (self.state.equation)(0x8006, 0x8006); // GL_FUNC_ADD
                (self.state.blend)(0x0302, 0x0303, 1, 0x0303);
            } else {
                (self.state.disable)(0x0BE2);
            }
        }
        self.current = Some(blend);
    }

    pub(crate) fn legacy(&mut self) {
        if self.current.take().is_none() {
            return;
        }
        // SAFETY: Restore the exact pass-entry state through the same live table.
        unsafe {
            (self.state.depth)(self.values[0] as u8);
            if self.blending != 0 {
                (self.state.enable)(0x0BE2);
            } else {
                (self.state.disable)(0x0BE2);
            }
            (self.state.blend)(
                self.values[1] as u32,
                self.values[2] as u32,
                self.values[3] as u32,
                self.values[4] as u32,
            );
            (self.state.equation)(self.values[5] as u32, self.values[6] as u32);
        }
    }
}

impl Drop for SurfaceGuard {
    fn drop(&mut self) {
        self.legacy();
    }
}

#[cfg(test)]
pub(super) fn snapshot(thread: &RaylibThread) -> [i32; 8] {
    let state = RenderState::load(thread).unwrap().begin();
    let [a, b, c, d, e, f, g] = state.values;
    [a, b, c, d, e, f, g, i32::from(state.blending)]
}
