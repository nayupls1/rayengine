//! Cached asset ownership with typed stable handles and explicit unloading.
//!
//! Handles are append-only indices and are never recycled within a run. Unloaded
//! handles return `None`; loading another asset cannot accidentally revive them.
//! Render resources are dropped before the window and sounds before audio closes.

use crate::Error;
use raylib::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Stable handle for a texture owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub(crate) usize);
/// Stable handle for a model owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelId(pub(crate) usize);
/// Stable handle for a sound owned by the current game run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub(crate) usize);

/// Runtime asset collection. Load during initialization; draw using typed handles.
pub struct Assets<'audio> {
    textures: Vec<Option<Texture2D>>,
    models: Vec<Option<Model>>,
    sounds: Vec<Option<Sound<'audio>>>,
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
