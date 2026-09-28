//! Cached asset ownership with typed stable handles and explicit unloading.
//!
//! File handles use append-only indices; generated meshes use versioned slots.
//! Unloaded handles return `None`; creating another asset cannot revive them.
//! Render resources are dropped before the window and sounds before audio closes.

use crate::Error;
use rayengine_core::mesh::MeshData;
use raylib::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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
    texture_paths: HashMap<PathBuf, TextureId>,
    model_paths: HashMap<PathBuf, ModelId>,
    sound_paths: HashMap<PathBuf, SoundId>,
    audio: Option<&'audio RaylibAudio>,
}

impl<'audio> Assets<'audio> {
    pub(crate) fn new(audio: Option<&'audio RaylibAudio>) -> Self {
        Self {
            textures: Vec::new(),
            models: Vec::new(),
            sounds: Vec::new(),
            meshes: MeshAssets::new(),
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
