//! Private OpenGL allocation checks for meshes uploaded on raylib's owning thread.
//!
//! Raylib 6 does not report glBufferData failures. Inspect storage sizes rather
//! than just object names. FFI and pointer reads stay inside this backend bridge.
#![allow(unsafe_code)]

use crate::Error;
use rayengine_core::mesh::MeshData;
use raylib::{
    ffi,
    prelude::{Mesh, RaylibThread},
};
use std::ffi::{CStr, c_void};

const ARRAY_BUFFER: u32 = 0x8892;
const ARRAY_BUFFER_BINDING: u32 = 0x8894;
const BUFFER_SIZE: u32 = 0x8764;

type GetInteger = unsafe extern "system" fn(u32, *mut i32);
type BindBuffer = unsafe extern "system" fn(u32, u32);
type GetBufferParameter = unsafe extern "system" fn(u32, u32, *mut i32);

struct Gl<'thread> {
    get_integer: GetInteger,
    bind_buffer: BindBuffer,
    get_buffer_parameter: GetBufferParameter,
    _thread: &'thread RaylibThread,
}

impl<'thread> Gl<'thread> {
    fn load(thread: &'thread RaylibThread) -> Result<Self, Error> {
        // SAFETY: RaylibThread keeps these procedures on the thread with the
        // live graphics context. Each non-null address is cast to the exact
        // OpenGL signature and system calling convention declared by its name.
        unsafe {
            Ok(Self {
                get_integer: std::mem::transmute::<*mut c_void, GetInteger>(load_proc(
                    c"glGetIntegerv",
                )?),
                bind_buffer: std::mem::transmute::<*mut c_void, BindBuffer>(load_proc(
                    c"glBindBuffer",
                )?),
                get_buffer_parameter: std::mem::transmute::<*mut c_void, GetBufferParameter>(
                    load_proc(c"glGetBufferParameteriv")?,
                ),
                _thread: thread,
            })
        }
    }

    fn buffer_size(&self, buffer: u32) -> i32 {
        let mut previous = 0;
        let mut bytes = -1;
        // SAFETY: Functions were loaded for this live render thread; output
        // pointers reference initialized i32 values. Buffer names come from the
        // owned uploaded mesh. Index buffers may also bind to ARRAY_BUFFER;
        // this leaves the VAO's element binding untouched. Restore the binding
        // even when the driver fails to fill the size output.
        unsafe {
            (self.get_integer)(ARRAY_BUFFER_BINDING, &mut previous);
            (self.bind_buffer)(ARRAY_BUFFER, buffer);
            (self.get_buffer_parameter)(ARRAY_BUFFER, BUFFER_SIZE, &mut bytes);
            (self.bind_buffer)(ARRAY_BUFFER, previous as u32);
        }
        bytes
    }
}

fn load_proc(name: &CStr) -> Result<*mut c_void, Error> {
    // SAFETY: Callers hold RaylibThread with an initialized graphics context;
    // the C string lives for this call. The procedure is used only in that context.
    let address = unsafe { ffi::rlGetProcAddress(name.as_ptr()) };
    if address.is_null() {
        Err(Error::Asset(format!(
            "mesh upload: OpenGL procedure {} is unavailable",
            name.to_string_lossy()
        )))
    } else {
        Ok(address)
    }
}

/// Only call for a freshly built Mesh::gen_mesh after MeshData validation.
pub(super) fn verify(thread: &RaylibThread, mesh: &Mesh, data: &MeshData) -> Result<(), Error> {
    if mesh.vaoId == 0 || mesh.vboId.is_null() {
        return Err(Error::Asset(
            "mesh upload: raylib did not allocate vertex-array/buffer objects".into(),
        ));
    }
    let gl = Gl::load(thread)?;
    let vertices = data.positions.len();
    let buffers = [
        (
            "positions",
            ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_POSITION,
            Some(vertices * size_of::<[f32; 3]>()),
        ),
        (
            "texcoords",
            ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_TEXCOORD,
            Some(vertices * size_of::<[f32; 2]>()),
        ),
        (
            "normals",
            ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_NORMAL,
            data.normals
                .as_ref()
                .map(|_| vertices * size_of::<[f32; 3]>()),
        ),
        (
            "colors",
            ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_COLOR,
            data.colors
                .as_ref()
                .map(|_| vertices * size_of::<[u8; 4]>()),
        ),
        (
            "indices",
            ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_INDICES,
            data.indices
                .as_ref()
                .map(|indices| indices.len() * size_of::<u16>()),
        ),
    ];
    for (name, slot, bytes) in buffers {
        let Some(bytes) = bytes else {
            continue;
        };
        // SAFETY: Pinned raylib UploadMesh allocates at least seven vboId entries.
        // The default attribute constants used here are slots 0..=6. This mesh
        // is freshly built/owned, its pointer is non-null, and nothing mutates
        // it until verification completes. Validated byte counts fit i32.
        let buffer = unsafe { *mesh.vboId.add(slot as usize) };
        if buffer == 0 {
            return Err(Error::Asset(format!("mesh upload: missing {name} buffer")));
        }
        let actual = gl.buffer_size(buffer);
        if actual != bytes as i32 {
            return Err(Error::Asset(format!(
                "mesh upload: {name} buffer has {actual} bytes; expected {bytes}"
            )));
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
pub(super) mod fault {
    use super::*;
    use std::cell::Cell;

    type BufferData = unsafe extern "system" fn(u32, isize, *const c_void, u32);
    type IsObject = unsafe extern "system" fn(u32) -> u8;
    // Pinned raylib's bundled OpenGL 3.3 backend dispatches through this GLAD
    // pointer. Tests replace it only with one active context, then restore it.
    unsafe extern "C" {
        static mut glad_glBufferData: Option<BufferData>;
    }

    #[derive(Clone, Copy, Default)]
    struct State {
        original: Option<BufferData>,
        get_integer: Option<GetInteger>,
        reject: usize,
        calls: usize,
        vao: u32,
        buffers: [u32; 5],
    }

    thread_local! { static STATE: Cell<State> = const { Cell::new(State {
        original: None, get_integer: None, reject: 0, calls: 0, vao: 0, buffers: [0; 5],
    }) }; }

    pub(in crate::assets::mesh) struct RejectedUpload<'thread> {
        gl: Gl<'thread>,
        is_buffer: IsObject,
        is_vertex_array: IsObject,
    }

    impl<'thread> RejectedUpload<'thread> {
        pub(in crate::assets::mesh) fn new(thread: &'thread RaylibThread, reject: usize) -> Self {
            let gl = Gl::load(thread).unwrap();
            // SAFETY: Both object queries have the indicated OpenGL signature.
            // The GLAD pointer is initialized by raylib; tests run serially on
            // its owning thread. The guard restores it before drawing/teardown.
            let (original, is_buffer, is_vertex_array) = unsafe {
                (
                    glad_glBufferData.unwrap(),
                    std::mem::transmute::<*mut c_void, IsObject>(load_proc(c"glIsBuffer").unwrap()),
                    std::mem::transmute::<*mut c_void, IsObject>(
                        load_proc(c"glIsVertexArray").unwrap(),
                    ),
                )
            };
            assert!(STATE.get().original.is_none(), "nested GPU fault injection");
            assert!((1..=5).contains(&reject));
            STATE.set(State {
                original: Some(original),
                get_integer: Some(gl.get_integer),
                reject,
                ..State::default()
            });
            // SAFETY: The interceptor has the exact GLAD function signature and
            // the original procedure remains stored until this guard drops.
            unsafe {
                glad_glBufferData = Some(reject_buffer_data);
            }
            Self {
                gl,
                is_buffer,
                is_vertex_array,
            }
        }

        pub(in crate::assets::mesh) fn assert_partial_upload_released(&self) {
            let state = STATE.get();
            assert_eq!(state.calls, 5, "all buffer uploads were attempted");
            assert_ne!(
                state.vao, 0,
                "VAO creation succeeded despite a rejected buffer upload"
            );
            for buffer in state.buffers {
                assert_ne!(
                    buffer, 0,
                    "buffer names were generated before allocation failed"
                );
                // SAFETY: Queries run on the guard's live render context.
                assert_eq!(
                    unsafe { (self.is_buffer)(buffer) },
                    0,
                    "partial buffer leaked"
                );
            }
            // SAFETY: Same context and loaded query as above.
            assert_eq!(
                unsafe { (self.is_vertex_array)(state.vao) },
                0,
                "partial VAO leaked"
            );
        }

        pub(in crate::assets::mesh) fn assert_query_binding_restored(&self, mesh: &Mesh) {
            // SAFETY: The probe passes its live, uploaded owned mesh. These
            // position/UV slots exist in the pinned raylib vboId allocation.
            let (buffer, previous) = unsafe {
                (
                    *mesh
                        .vboId
                        .add(ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_POSITION as usize),
                    *mesh
                        .vboId
                        .add(ffi::RL_DEFAULT_SHADER_ATTRIB_LOCATION_TEXCOORD as usize),
                )
            };
            // SAFETY: Same live context, valid object, initialized outputs.
            unsafe {
                (self.gl.bind_buffer)(ARRAY_BUFFER, previous);
            }
            assert!(self.gl.buffer_size(buffer) > 0);
            let mut actual = 0;
            // SAFETY: Same live context and initialized output.
            unsafe {
                (self.gl.get_integer)(ARRAY_BUFFER_BINDING, &mut actual);
            }
            assert_eq!(actual as u32, previous);
            // SAFETY: Restore neutral array binding after the probe.
            unsafe {
                (self.gl.bind_buffer)(ARRAY_BUFFER, 0);
            }
        }
    }

    impl Drop for RejectedUpload<'_> {
        fn drop(&mut self) {
            let state = STATE.replace(State::default());
            // SAFETY: One serial native test owns this dispatch change. The
            // original pointer came from this live context; restore on unwind too.
            unsafe {
                glad_glBufferData = state.original;
            }
        }
    }

    unsafe extern "system" fn reject_buffer_data(
        target: u32,
        bytes: isize,
        data: *const c_void,
        usage: u32,
    ) {
        let mut state = STATE.get();
        state.calls += 1;
        if let Some(get_integer) = state.get_integer {
            let mut vao = 0;
            let mut buffer = 0;
            // SAFETY: The callback runs synchronously on raylib's owning thread
            // with the captured live procedure and initialized i32 outputs.
            unsafe {
                get_integer(0x85B5, &mut vao); // GL_VERTEX_ARRAY_BINDING
                get_integer(
                    if target == ARRAY_BUFFER {
                        ARRAY_BUFFER_BINDING
                    } else {
                        0x8895
                    },
                    &mut buffer,
                );
            }
            state.vao = vao as u32;
            if let Some(slot) = state.buffers.get_mut(state.calls - 1) {
                *slot = buffer as u32;
            }
        }
        STATE.set(state);
        if state.calls != state.reject
            && let Some(original) = state.original
        {
            // SAFETY: Forward the unchanged arguments to the original procedure.
            unsafe {
                original(target, bytes, data, usage);
            }
        }
        // Reject exactly one allocation without allocating storage or setting
        // a GL error. This proves detection relies on actual storage, not a name
        // or an error flag. Other allocations proceed, exercising partial cleanup.
    }
}
