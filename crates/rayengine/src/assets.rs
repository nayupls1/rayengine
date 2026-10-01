//! Cached asset ownership with typed stable handles and explicit unloading.
//!
//! File handles use append-only indices; generated meshes use versioned slots.
//! Unloaded handles return `None`; creating another asset cannot revive them.
//! Render resources are dropped before the window and sounds before audio closes.

use crate::Error;
use crate::material::{MaterialDesc, UniformId, UniformValue};
use rayengine_core::mesh::MeshData;
use raylib::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub(crate) mod materials;
mod mesh;
use mesh::MeshAssets;

/// Stable handle for a texture owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) usize);
/// Stable handle for a model owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelId(pub(crate) usize);
/// Stable handle for a sound owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub(crate) usize);
/// Stable shader handle owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderId(pub(crate) usize);
/// Stable material-description handle owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialId(pub(crate) usize);

/// Versioned handle for a generated mesh owned by the current game run.
///
/// Replacement keeps the handle. Unloading invalidates it permanently, even
/// when its storage slot is reused. Handles must not be shared between runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId {
    slot: usize,
    generation: u64,
}

/// Runtime asset collection. Load during initialization; draw using typed handles.
pub struct Assets<'audio> {
    textures: Vec<Option<Texture2D>>,
    models: Vec<Option<Model>>,
    sounds: Vec<Option<Sound<'audio>>>,
    meshes: MeshAssets,
    surfaces: materials::MaterialAssets,
    shader_paths: HashMap<(Option<PathBuf>, PathBuf), ShaderId>,
    texture_paths: HashMap<PathBuf, TextureId>,
    model_paths: HashMap<PathBuf, ModelId>,
    sound_paths: HashMap<PathBuf, SoundId>,
    audio: Option<&'audio RaylibAudio>,
}

impl<'audio> Assets<'audio> {
    /// Samples live owned resources. This scans asset slots without GPU queries
    /// or readback; automatic sampling occurs only when diagnostics are enabled.
    /// Logical payload bytes exclude driver overhead, default resources,
    /// imported model textures/animations, and game-owned raw handles.
    pub fn resource_counts(&self) -> crate::diagnostics::ResourceCounts {
        let (meshes, generated_mesh_bytes) = self.meshes.resource_usage();
        let (shaders, materials) = self.surfaces.resource_counts();
        crate::diagnostics::ResourceCounts {
            textures: self.textures.iter().flatten().count() as u64,
            models: self.models.iter().flatten().count() as u64,
            sounds: self.sounds.iter().flatten().count() as u64,
            meshes,
            shaders,
            materials,
            generated_mesh_bytes,
            texture_bytes: self.textures.iter().flatten().fold(0_u64, |sum, texture| {
                sum.saturating_add(texture_payload_bytes(texture.as_ref()))
            }),
            model_geometry_bytes: self
                .models
                .iter()
                .flatten()
                .flat_map(|model| model.meshes())
                .fold(0_u64, |sum, mesh| {
                    sum.saturating_add(mesh_payload_bytes(mesh))
                }),
        }
    }
    pub(crate) fn new(audio: Option<&'audio RaylibAudio>) -> Self {
        Self {
            textures: Vec::new(),
            models: Vec::new(),
            sounds: Vec::new(),
            meshes: MeshAssets::new(),
            surfaces: materials::MaterialAssets::new(),
            shader_paths: HashMap::new(),
            texture_paths: HashMap::new(),
            model_paths: HashMap::new(),
            sound_paths: HashMap::new(),
            audio,
        }
    }

    /// Borrow a loaded texture, or `None` after it has been unloaded.
    pub fn texture(&self, id: TextureId) -> Option<&Texture2D> {
        self.textures.get(id.0).and_then(Option::as_ref)
    }

    /// Borrow a loaded model, or `None` after it has been unloaded.
    pub fn model(&self, id: ModelId) -> Option<&Model> {
        self.models.get(id.0).and_then(Option::as_ref)
    }

    /// Borrows an uploaded generated mesh, or `None` for an unloaded handle.
    pub fn mesh(&self, id: MeshId) -> Option<&Mesh> {
        self.meshes.get(id)
    }

    /// Borrows a material description, or None after unloading.
    pub fn material(&self, id: MaterialId) -> Option<&MaterialDesc> {
        self.surfaces.descriptor(id)
    }

    /// Checks live texture/shader dependencies, alpha policy and parameter types
    /// without creating or replacing a material. Useful before a plugin commits
    /// several resource changes as one transaction.
    pub fn validate_material(&self, desc: &MaterialDesc) -> Result<(), Error> {
        self.surfaces.validate(desc, &self.textures)
    }

    /// Unloads only this description; shared shaders/textures stay alive.
    pub fn unload_material(&mut self, id: MaterialId) -> bool {
        self.surfaces
            .materials
            .get_mut(id.0)
            .and_then(Option::take)
            .is_some()
    }

    /// Unloads an owned shader. Materials/bindings referencing it become invalid.
    pub fn unload_shader(&mut self, id: ShaderId) -> bool {
        let unloaded = self
            .surfaces
            .shaders
            .get_mut(id.0)
            .and_then(Option::take)
            .is_some();
        self.shader_paths.retain(|_, handle| *handle != id);
        unloaded
    }

    /// Replaces CPU material data atomically; failure preserves the old description.
    pub fn replace_material(&mut self, id: MaterialId, desc: MaterialDesc) -> Result<(), Error> {
        if self.material(id).is_none() {
            return Err(Error::Asset("material is unloaded".into()));
        }
        self.surfaces.validate(&desc, &self.textures)?;
        self.surfaces.materials[id.0] = Some(desc);
        Ok(())
    }

    /// Updates a registered shader default, applied before overrides on each draw.
    /// This edits CPU data; actual GPU uniforms are submitted while drawing.
    pub fn set_uniform(&mut self, binding: UniformId, value: UniformValue) -> Result<(), Error> {
        self.surfaces
            .shaders
            .get_mut(binding.shader.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("uniform shader is unloaded".into()))?
            .set(binding, value)
    }

    pub(crate) fn create_material(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        desc: MaterialDesc,
    ) -> Result<MaterialId, Error> {
        self.surfaces.initialize(raylib, thread)?;
        self.surfaces.validate(&desc, &self.textures)?;
        let id = MaterialId(self.surfaces.materials.len());
        self.surfaces.materials.push(Some(desc));
        Ok(id)
    }

    pub(crate) fn shader_source(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        vertex: Option<&str>,
        fragment: &str,
    ) -> Result<ShaderId, Error> {
        self.surfaces.shader(raylib, thread, vertex, fragment)
    }

    pub(crate) fn load_shader(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        vertex: Option<&Path>,
        fragment: &Path,
    ) -> Result<ShaderId, Error> {
        let fragment = asset_path(fragment)?;
        let vertex = vertex.map(asset_path).transpose()?;
        let key = (vertex, fragment);
        if let Some(&id) = self.shader_paths.get(&key) {
            return Ok(id);
        }
        let vertex_source = key.0.as_ref().map(std::fs::read_to_string).transpose()?;
        let fragment_source = std::fs::read_to_string(&key.1)?;
        let id = self.shader_source(raylib, thread, vertex_source.as_deref(), &fragment_source)?;
        self.shader_paths.insert(key, id);
        Ok(id)
    }

    pub(crate) fn uniform(
        &mut self,
        id: ShaderId,
        name: &str,
        value: UniformValue,
    ) -> Result<UniformId, Error> {
        self.surfaces
            .shaders
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("shader is unloaded".into()))?
            .register(id, name, value)
    }

    pub(crate) fn material_pass(&self) -> Option<materials::SurfaceGuard> {
        self.surfaces.pass()
    }

    pub(crate) fn mesh_material(
        &mut self,
        mesh: MeshId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Mesh, materials::Prepared<'_>)> {
        let mesh = self.meshes.get(mesh)?;
        let material = self.surfaces.prepare(material, &self.textures, tint)?;
        Some((mesh, material))
    }

    pub(crate) fn model_material(
        &mut self,
        model: ModelId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Model, materials::Prepared<'_>)> {
        let model = self.models.get(model.0)?.as_ref()?;
        let material = self.surfaces.prepare(material, &self.textures, tint)?;
        Some((model, material))
    }

    /// Frees a generated mesh on the render thread; false if already unloaded.
    ///
    /// The default material is released when the last generated mesh unloads.
    pub fn unload_mesh(&mut self, id: MeshId) -> bool {
        self.meshes.unload(id)
    }

    pub(crate) fn upload_mesh(
        &mut self,
        raylib: &RaylibHandle,
        thread: &RaylibThread,
        data: &MeshData,
    ) -> Result<MeshId, Error> {
        self.meshes.upload(raylib, thread, data)
    }

    pub(crate) fn replace_mesh(
        &mut self,
        thread: &RaylibThread,
        id: MeshId,
        data: &MeshData,
    ) -> Result<(), Error> {
        self.meshes.replace(thread, id, data)
    }

    pub(crate) fn mesh_for_draw(
        &mut self,
        id: MeshId,
        tint: Color,
    ) -> Option<(&Mesh, WeakMaterial)> {
        self.meshes.for_draw(id, tint)
    }

    /// Borrow a loaded sound, or `None` after it has been unloaded.
    pub fn sound(&self, id: SoundId) -> Option<&Sound<'audio>> {
        self.sounds.get(id.0).and_then(Option::as_ref)
    }

    /// Plays a sound. Returns false for an unloaded handle.
    pub fn play(&self, id: SoundId) -> bool {
        if let Some(sound) = self.sound(id) {
            sound.play();
            true
        } else {
            false
        }
    }

    /// Unloads a texture immediately. Its handle remains invalid forever in this run.
    pub fn unload_texture(&mut self, id: TextureId) {
        if let Some(slot) = self.textures.get_mut(id.0) {
            *slot = None;
        }
        self.texture_paths.retain(|_, handle| *handle != id);
    }

    /// Unloads a model immediately.
    pub fn unload_model(&mut self, id: ModelId) {
        if let Some(slot) = self.models.get_mut(id.0) {
            *slot = None;
        }
        self.model_paths.retain(|_, handle| *handle != id);
    }

    /// Unloads a sound immediately.
    pub fn unload_sound(&mut self, id: SoundId) {
        if let Some(slot) = self.sounds.get_mut(id.0) {
            *slot = None;
        }
        self.sound_paths.retain(|_, handle| *handle != id);
    }

    pub(crate) fn load_texture(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        path: &Path,
    ) -> Result<TextureId, Error> {
        let path = asset_path(path)?;
        if let Some(&id) = self.texture_paths.get(&path) {
            return Ok(id);
        }
        let texture = raylib
            .load_texture(thread, path_string(&path)?)
            .map_err(|e| Error::Asset(format!("{}: {e}", path.display())))?;
        let id = TextureId(self.textures.len());
        self.textures.push(Some(texture));
        self.texture_paths.insert(path, id);
        Ok(id)
    }

    pub(crate) fn load_model(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        path: &Path,
    ) -> Result<ModelId, Error> {
        let path = asset_path(path)?;
        if let Some(&id) = self.model_paths.get(&path) {
            return Ok(id);
        }
        let model = raylib
            .load_model(thread, path_string(&path)?)
            .map_err(|e| Error::Asset(format!("{}: {e}", path.display())))?;
        let id = ModelId(self.models.len());
        self.models.push(Some(model));
        self.model_paths.insert(path, id);
        Ok(id)
    }

    pub(crate) fn load_sound(&mut self, path: &Path) -> Result<SoundId, Error> {
        let audio = self
            .audio
            .ok_or_else(|| Error::Asset("enable Config::audio before loading sounds".into()))?;
        let path = asset_path(path)?;
        if let Some(&id) = self.sound_paths.get(&path) {
            return Ok(id);
        }
        let sound = audio
            .new_sound(path_string(&path)?)
            .map_err(|e| Error::Asset(format!("{}: {e}", path.display())))?;
        let id = SoundId(self.sounds.len());
        self.sounds.push(Some(sound));
        self.sound_paths.insert(path, id);
        Ok(id)
    }
}

fn texture_payload_bytes(texture: &raylib::ffi::Texture2D) -> u64 {
    let (mut width, mut height) = (texture.width, texture.height);
    let mut bytes = 0_u64;
    for _ in 0..texture.mipmaps {
        bytes = bytes.saturating_add(texture_level_bytes(width, height, texture.format));
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    bytes
}

fn texture_level_bytes(width: i32, height: i32, format: i32) -> u64 {
    use raylib::ffi::PixelFormat::*;
    // GetPixelDataSize uses a signed int result and fractional bytes/pixel,
    // which undercounts compressed mip tails and can overflow large textures.
    // Keep the raw format as an integer rather than transmuting an enum.
    let layouts = [
        (PIXELFORMAT_UNCOMPRESSED_GRAYSCALE, 1, 1, 1),
        (PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA, 1, 1, 2),
        (PIXELFORMAT_UNCOMPRESSED_R5G6B5, 1, 1, 2),
        (PIXELFORMAT_UNCOMPRESSED_R8G8B8, 1, 1, 3),
        (PIXELFORMAT_UNCOMPRESSED_R5G5B5A1, 1, 1, 2),
        (PIXELFORMAT_UNCOMPRESSED_R4G4B4A4, 1, 1, 2),
        (PIXELFORMAT_UNCOMPRESSED_R8G8B8A8, 1, 1, 4),
        (PIXELFORMAT_UNCOMPRESSED_R32, 1, 1, 4),
        (PIXELFORMAT_UNCOMPRESSED_R32G32B32, 1, 1, 12),
        (PIXELFORMAT_UNCOMPRESSED_R32G32B32A32, 1, 1, 16),
        (PIXELFORMAT_UNCOMPRESSED_R16, 1, 1, 2),
        (PIXELFORMAT_UNCOMPRESSED_R16G16B16, 1, 1, 6),
        (PIXELFORMAT_UNCOMPRESSED_R16G16B16A16, 1, 1, 8),
        (PIXELFORMAT_COMPRESSED_DXT1_RGB, 4, 4, 8),
        (PIXELFORMAT_COMPRESSED_DXT1_RGBA, 4, 4, 8),
        (PIXELFORMAT_COMPRESSED_DXT3_RGBA, 4, 4, 16),
        (PIXELFORMAT_COMPRESSED_DXT5_RGBA, 4, 4, 16),
        (PIXELFORMAT_COMPRESSED_ETC1_RGB, 4, 4, 8),
        (PIXELFORMAT_COMPRESSED_ETC2_RGB, 4, 4, 8),
        (PIXELFORMAT_COMPRESSED_ETC2_EAC_RGBA, 4, 4, 16),
        (PIXELFORMAT_COMPRESSED_PVRT_RGB, 4, 4, 8),
        (PIXELFORMAT_COMPRESSED_PVRT_RGBA, 4, 4, 8),
        (PIXELFORMAT_COMPRESSED_ASTC_4x4_RGBA, 4, 4, 16),
        (PIXELFORMAT_COMPRESSED_ASTC_8x8_RGBA, 8, 8, 16),
    ];
    let Some((_, block_width, block_height, block_bytes)) = layouts
        .into_iter()
        .find(|(pixel_format, ..)| *pixel_format as i32 == format)
    else {
        return 0;
    };
    if width <= 0 || height <= 0 {
        return 0;
    }
    let (mut width, mut height) = (width as u64, height as u64);
    if format == PIXELFORMAT_COMPRESSED_PVRT_RGB as i32
        || format == PIXELFORMAT_COMPRESSED_PVRT_RGBA as i32
    {
        // PVRTC 4bpp has a minimum 2x2 block footprint (8x8 texels).
        width = width.max(8);
        height = height.max(8);
    }
    width
        .div_ceil(block_width)
        .saturating_mul(height.div_ceil(block_height))
        .saturating_mul(block_bytes)
}

fn mesh_payload_bytes(mesh: &impl AsRef<raylib::ffi::Mesh>) -> u64 {
    let mesh = mesh.as_ref();
    let vertices = mesh.vertexCount.max(0) as u64;
    let per_vertex = [
        (mesh.vertices.is_null(), 12),
        (mesh.texcoords.is_null(), 8),
        (mesh.texcoords2.is_null(), 8),
        (mesh.normals.is_null(), 12),
        (mesh.tangents.is_null(), 16),
        (mesh.colors.is_null(), 4),
    ]
    .into_iter()
    .filter(|(is_null, _)| !is_null)
    .map(|(_, size)| size)
    .sum::<u64>();
    vertices
        .saturating_mul(per_vertex)
        .saturating_add(if mesh.indices.is_null() {
            0
        } else {
            (mesh.triangleCount.max(0) as u64).saturating_mul(6)
        })
}

fn asset_path(path: &Path) -> Result<PathBuf, Error> {
    let canonical = path.canonicalize()?;
    path_string(&canonical)?;
    Ok(canonical)
}

pub(crate) fn path_string(path: &Path) -> Result<&str, Error> {
    path.to_str().filter(|p| !p.contains('\0')).ok_or_else(|| {
        Error::Asset(format!(
            "raylib needs a UTF-8 path without NUL: {}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod diagnostics_tests {
    use super::*;
    use raylib::ffi::PixelFormat::*;

    #[test]
    fn compressed_texture_bytes_include_complete_blocks_and_mip_tails() {
        let texture = |width, height, mipmaps, format| raylib::ffi::Texture2D {
            id: 0,
            width,
            height,
            mipmaps,
            format: format as i32,
        };
        // 8x2, 4x1, 2x1, 1x1: two blocks, then one per mip.
        assert_eq!(
            texture_payload_bytes(&texture(8, 2, 4, PIXELFORMAT_COMPRESSED_DXT1_RGB)),
            40
        );
        assert_eq!(
            texture_payload_bytes(&texture(8, 2, 4, PIXELFORMAT_COMPRESSED_DXT5_RGBA)),
            80
        );
        assert_eq!(
            texture_level_bytes(5, 7, PIXELFORMAT_COMPRESSED_ETC2_RGB as i32),
            32
        );
        assert_eq!(
            texture_level_bytes(1, 1, PIXELFORMAT_COMPRESSED_ASTC_8x8_RGBA as i32),
            16
        );
        assert_eq!(
            texture_level_bytes(9, 1, PIXELFORMAT_COMPRESSED_ASTC_8x8_RGBA as i32),
            32
        );
        assert_eq!(
            texture_level_bytes(1, 1, PIXELFORMAT_COMPRESSED_PVRT_RGBA as i32),
            32
        );
    }

    #[test]
    fn uncompressed_texture_bytes_keep_wide_totals_and_mips() {
        let texture = raylib::ffi::Texture2D {
            id: 0,
            width: 8,
            height: 8,
            mipmaps: 4,
            format: PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
        };
        assert_eq!(texture_payload_bytes(&texture), 340);
        assert_eq!(
            texture_level_bytes(16384, 16384, PIXELFORMAT_UNCOMPRESSED_R32G32B32A32 as i32),
            4_294_967_296
        );
        assert_eq!(texture_level_bytes(8, 8, -1), 0);
        assert_eq!(texture_level_bytes(0, 8, texture.format), 0);
    }
}
