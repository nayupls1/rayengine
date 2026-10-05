//! Cached asset ownership with typed stable handles and explicit unloading.
//!
//! File handles use append-only indices; generated meshes use versioned slots.
//! Unloaded handles return `None`; creating another asset cannot revive them.
//! Render resources are dropped before the window and sounds before audio closes.

use crate::Error;
pub use crate::fonts::FontId;
use crate::lighting::Lighting;
use crate::material::{MaterialDesc, Shading, UniformId, UniformValue};
use rayengine_core::mesh::MeshData;
use raylib::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

mod animation;
pub(crate) mod materials;
mod mesh;
pub(crate) use animation::apply_pose;
pub use animation::{ModelAnimationsId, ModelAnimator, ModelClipId, ModelClipInfo, ModelPose};
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
    pub(crate) targets: crate::targets::TargetAssets,
    pub(crate) post_processing: crate::post_processing::PostProcessing,
    pub(crate) target_reservation: u64,
    pub(crate) fonts: crate::fonts::FontAssets,
    pub(crate) textures: Vec<Option<Texture2D>>,
    models: Vec<Option<Model>>,
    model_lit_normals: Vec<bool>,
    model_skins: Vec<animation::Skin>,
    animations: animation::AnimationAssets,
    pub(crate) sounds: Vec<Option<Sound<'audio>>>,
    pub(crate) mixer: std::cell::RefCell<crate::audio::AudioMixer<'audio>>,
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
    /// imported model textures, and game-owned raw handles.
    pub fn resource_counts(&self) -> crate::diagnostics::ResourceCounts {
        let (meshes, generated_mesh_bytes) = self.meshes.resource_usage();
        let (shaders, materials) = self.surfaces.resource_counts();
        let (fonts, font_atlases, font_bytes) = self.fonts.usage();
        let (music_streams, sound_instances) = self.mixer.borrow().resource_counts();
        let (model_animations, model_clips, model_animation_bytes) = self.animations.usage();
        crate::diagnostics::ResourceCounts {
            render_targets: self.targets.usage().0,
            render_target_bytes: self.targets.usage().1,
            fonts,
            font_atlases,
            font_bytes,
            textures: self.textures.iter().flatten().count() as u64,
            models: self.models.iter().flatten().count() as u64,
            model_animations,
            model_clips,
            model_animation_bytes,
            sounds: self.sounds.iter().flatten().count() as u64,
            music_streams,
            sound_instances,
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
            targets: crate::targets::TargetAssets::default(),
            post_processing: crate::post_processing::PostProcessing::default(),
            target_reservation: 0,
            fonts: crate::fonts::FontAssets::new(),
            textures: Vec::new(),
            models: Vec::new(),
            model_lit_normals: Vec::new(),
            model_skins: Vec::new(),
            animations: animation::AnimationAssets::default(),
            sounds: Vec::new(),
            mixer: std::cell::RefCell::new(crate::audio::AudioMixer::new(audio)),
            meshes: MeshAssets::new(),
            surfaces: materials::MaterialAssets::new(),
            shader_paths: HashMap::new(),
            texture_paths: HashMap::new(),
            model_paths: HashMap::new(),
            sound_paths: HashMap::new(),
            audio,
        }
    }

    /// Measures the exact logical layout used by custom text drawing, without
    /// allocating GPU resources. Stale handles and invalid text return errors.
    pub fn measure_text(
        &self,
        text: &str,
        style: crate::fonts::TextStyle,
    ) -> Result<crate::fonts::TextMetrics, Error> {
        self.fonts.measure(text, style)
    }

    /// Releases every atlas for this font. Returns false for an unloaded handle.
    /// Call between passes, so queued native drawing has already been flushed.
    pub fn unload_font(&mut self, id: crate::fonts::FontId) -> bool {
        self.fonts.unload(id)
    }

    pub(crate) fn load_font(
        &mut self,
        thread: &RaylibThread,
        path: &Path,
        options: crate::fonts::FontOptions,
    ) -> Result<crate::fonts::FontId, Error> {
        self.fonts.load(thread, path, options)
    }

    /// Registers an engine-owned target. Storage is allocated before the next draw.
    /// Viewport-relative targets are recreated on resize/DPI changes and start transparent.
    pub fn create_render_target(
        &mut self,
        desc: crate::targets::RenderTargetDesc,
    ) -> Result<crate::targets::RenderTargetId, Error> {
        self.targets.create(desc)
    }
    /// Releases a target and invalidates its handle. An active target cannot be unloaded.
    pub fn unload_render_target(&mut self, id: crate::targets::RenderTargetId) -> bool {
        self.targets.unload(id)
    }
    /// Borrows the target color attachment. None before allocation, while drawing
    /// into it, or after unloading. Native sampling has vertically inverted UVs.
    pub fn render_target_texture(
        &self,
        id: crate::targets::RenderTargetId,
    ) -> Option<&WeakTexture2D> {
        self.targets.texture(id)
    }
    /// Current allocated custom target count and RGBA+depth bytes.
    pub fn render_target_usage(&self) -> (u64, u64) {
        self.targets.usage()
    }
    /// Selects an ordered chain for the next frame. Validation is atomic; materials
    /// and shaders remain owned by Assets and may be reused when toggling effects.
    pub fn set_post_processing(
        &mut self,
        chain: crate::post_processing::PostProcessing,
    ) -> Result<(), Error> {
        self.validate_post_processing(&chain)?;
        self.post_processing = chain;
        Ok(())
    }
    pub(crate) fn post_blit(
        &mut self,
        material: MaterialId,
        raw: &mut impl RaylibDraw,
        source: &WeakTexture2D,
        size: (u32, u32),
    ) -> Result<(), Error> {
        let mut surface = self
            .surfaces
            .prepare(material, &self.textures, &self.targets, Color::WHITE)
            .ok_or_else(|| Error::Asset("post-processing dependency is unloaded".into()))?;
        surface.blit(raw, source, size);
        Ok(())
    }
    /// Current frame-effect configuration.
    pub fn post_processing(&self) -> &crate::post_processing::PostProcessing {
        &self.post_processing
    }
    pub(crate) fn validate_post_processing(
        &self,
        chain: &crate::post_processing::PostProcessing,
    ) -> Result<(), Error> {
        for id in &chain.materials {
            let desc = self
                .material(*id)
                .ok_or_else(|| Error::Asset("post-processing material is unloaded".into()))?;
            self.validate_material(desc)?;
            if desc.shader.is_none()
                || desc.shading != Shading::Unlit
                || desc.texture.is_some()
                || desc.render_target.is_some()
            {
                return Err(Error::Asset("post-processing requires a custom unlit shader with no material texture (texture0 is the previous pass)".into()));
            }
        }
        Ok(())
    }

    /// Borrow a loaded texture, or `None` after it has been unloaded.
    pub fn texture(&self, id: TextureId) -> Option<&Texture2D> {
        self.textures.get(id.0).and_then(Option::as_ref)
    }

    /// Borrow a loaded model, or `None` after it has been unloaded.
    pub fn model(&self, id: ModelId) -> Option<&Model> {
        self.models.get(id.0).and_then(Option::as_ref)
    }

    /// Describes a loaded clip, or `None` after its set has been unloaded.
    pub fn model_clip_info(&self, clip: ModelClipId) -> Option<&ModelClipInfo> {
        self.animations.info(clip)
    }

    /// Number of clips in a loaded set, in file order; `None` after unloading.
    pub fn model_clip_count(&self, set: ModelAnimationsId) -> Option<usize> {
        self.animations.clip_count(set)
    }

    /// Clip at a zero-based file position; `None` if out of range or unloaded.
    pub fn model_clip(&self, set: ModelAnimationsId, index: usize) -> Option<ModelClipId> {
        self.animations.clip(set, index)
    }

    /// First clip with exactly this stored name; `None` if absent or unloaded.
    pub fn find_model_clip(&self, set: ModelAnimationsId, name: &str) -> Option<ModelClipId> {
        self.animations.find(set, name)
    }

    /// Checks that a clip can pose a model: both are loaded, the model has a
    /// skeleton with in-range skinned vertex influences, and the bone counts match.
    pub fn check_model_clip(&self, model: ModelId, clip: ModelClipId) -> Result<(), Error> {
        let (skin, info) = self.model_clip_pair(model, clip)?;
        animation::check(model, skin, info)
    }

    /// Creates a playback cursor at the clip's first keyframe after the same
    /// checks as [`Self::check_model_clip`]. Timing comes from the clip's load rate.
    pub fn model_animator(
        &self,
        model: ModelId,
        clip: ModelClipId,
        mode: rayengine_core::skeletal::PlaybackMode,
    ) -> Result<ModelAnimator, Error> {
        let (skin, info) = self.model_clip_pair(model, clip)?;
        animation::animator(model, skin, clip, info, mode)
    }

    fn model_clip_pair(
        &self,
        model: ModelId,
        clip: ModelClipId,
    ) -> Result<(&animation::Skin, &ModelClipInfo), Error> {
        if self.model(model).is_none() {
            return Err(Error::Asset("animated model is unloaded".into()));
        }
        let info = self
            .model_clip_info(clip)
            .ok_or_else(|| Error::Asset("animation clip is unloaded".into()))?;
        Ok((&self.model_skins[model.0], info))
    }

    /// Checks and resolves a pose for drawing: `Ok(None)` for stale handles.
    pub(crate) fn posed_model(
        &self,
        model: ModelId,
        pose: ModelPose,
    ) -> Result<Option<(&Model, &raylib::prelude::ModelAnimation)>, Error> {
        let (Some(native_model), Some((clip, info))) =
            (self.model(model), self.animations.native(pose.clip))
        else {
            return Ok(None);
        };
        animation::check(model, &self.model_skins[model.0], info)?;
        animation::check_pose(pose, info)?;
        Ok(Some((native_model, clip)))
    }

    pub(crate) fn posed_model_material(
        &mut self,
        model: ModelId,
        material: MaterialId,
        pose: ModelPose,
        tint: Color,
    ) -> Result<
        Option<(
            &Model,
            &raylib::prelude::ModelAnimation,
            materials::Prepared<'_>,
        )>,
        Error,
    > {
        if self.posed_model(model, pose)?.is_none() {
            return Ok(None);
        }
        let (Some(native), Some((clip, _))) = (
            self.models.get(model.0).and_then(Option::as_ref),
            self.animations.native(pose.clip),
        ) else {
            return Ok(None);
        };
        Ok(self
            .surfaces
            .prepare(material, &self.textures, &self.targets, tint)
            .map(|prepared| (native, clip, prepared)))
    }

    /// Releases every clip loaded from one file. Its clip handles become stale
    /// forever; models are unaffected. Returns false if already unloaded.
    pub fn unload_model_animations(&mut self, set: ModelAnimationsId) -> bool {
        self.animations.unload(set)
    }

    pub(crate) fn load_model_animations(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        path: &Path,
        rate: rayengine_core::skeletal::KeyframeRate,
    ) -> Result<ModelAnimationsId, Error> {
        self.animations.load(raylib, thread, path, rate)
    }

    /// Borrows an uploaded generated mesh, or `None` for an unloaded handle.
    pub fn mesh(&self, id: MeshId) -> Option<&Mesh> {
        self.meshes.get(id)
    }

    /// Borrows shared world-space light settings for this run.
    pub fn lighting(&self) -> &Lighting {
        self.surfaces.lighting()
    }

    /// Atomically validates and replaces lights. Failure preserves previous settings.
    /// Call in init or before world_3d; changed uniforms upload on the next lit draw.
    pub fn set_lighting(&mut self, lighting: Lighting) -> Result<(), Error> {
        self.surfaces.set_lighting(lighting)
    }

    pub(crate) fn validate_lit_draw(
        &self,
        mesh: Option<MeshId>,
        model: Option<ModelId>,
        material: MaterialId,
        transform: rayengine_core::glam::Mat4,
    ) -> Result<(), Error> {
        let Some(desc) = self.material(material) else {
            return Ok(());
        };
        if desc.shading == Shading::Lit {
            // Preserve Ok(false) for stale dependencies, even if transform/normals
            // are also invalid. prepare() will reject the stale resource handle.
            if desc.texture.is_some_and(|id| self.texture(id).is_none())
                || desc
                    .render_target
                    .is_some_and(|id| self.targets.texture(id).is_none())
            {
                return Ok(());
            }
            let valid = if let Some(id) = mesh {
                if self.mesh(id).is_none() {
                    return Ok(());
                }
                self.meshes.has_lit_normals(id)
            } else if let Some(id) = model {
                if self.model(id).is_none() {
                    return Ok(());
                }
                self.model_lit_normals[id.0]
            } else {
                false
            };
            crate::lighting::validate_transform(transform)?;
            if !valid {
                return Err(Error::Asset("lit geometry requires one finite, nonzero normal per vertex, with representable squared length; supply MeshData::normals or export model normals".into()));
            }
        }
        Ok(())
    }

    /// Borrows a material description, or None after unloading.
    pub fn material(&self, id: MaterialId) -> Option<&MaterialDesc> {
        self.surfaces.descriptor(id)
    }

    /// Checks live texture/shader dependencies, alpha policy and parameter types
    /// without creating or replacing a material. Useful before a plugin commits
    /// several resource changes as one transaction.
    pub fn validate_material(&self, desc: &MaterialDesc) -> Result<(), Error> {
        self.surfaces.validate(desc, &self.textures, &self.targets)
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
        self.surfaces
            .validate(&desc, &self.textures, &self.targets)?;
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
        self.surfaces
            .validate(&desc, &self.textures, &self.targets)?;
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
        let material = self
            .surfaces
            .prepare(material, &self.textures, &self.targets, tint)?;
        Some((mesh, material))
    }

    pub(crate) fn model_material(
        &mut self,
        model: ModelId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Model, materials::Prepared<'_>)> {
        let model = self.models.get(model.0)?.as_ref()?;
        let material = self
            .surfaces
            .prepare(material, &self.textures, &self.targets, tint)?;
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

    /// Borrow the original native voice, or `None` after unloading.
    /// Raw playback bypasses bus mixing and concurrency controls. Its native
    /// volume/pitch/pan are never changed by the mixer.
    pub fn sound(&self, id: SoundId) -> Option<&Sound<'audio>> {
        self.sounds.get(id.0).and_then(Option::as_ref)
    }

    /// Plays/restarts the original voice, preserving its native volume/pitch/pan.
    /// This legacy voice bypasses bus mixing. The optional concurrency cap applies.
    /// Returns false for an unloaded handle or a reached cap (restarts are allowed).
    /// Use `play_sound` for independent mixer-managed instances and bus routing.
    pub fn play(&self, id: SoundId) -> bool {
        self.sound(id)
            .is_some_and(|sound| self.mixer.borrow().play_legacy(id, sound))
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

    /// Stops/releases a sound and all its overlapping instances immediately.
    pub fn unload_sound(&mut self, id: SoundId) {
        if let Some(slot) = self.sounds.get_mut(id.0) {
            *slot = None;
        }
        if let Some(pool) = self.mixer.get_mut().sounds.get_mut(id.0) {
            *pool = None;
        }
        self.sound_paths.retain(|_, handle| *handle != id);
    }

    pub(crate) fn upload_texture_image(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        image: &Image,
    ) -> Result<TextureId, Error> {
        let texture = raylib
            .load_texture_from_image(thread, image)
            .map_err(|e| Error::Asset(format!("generated texture: {e}")))?;
        let id = TextureId(self.textures.len());
        self.textures.push(Some(texture));
        Ok(id)
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
        self.model_lit_normals.push(
            !model.meshes().is_empty()
                && model.meshes().iter().all(|mesh| {
                    let normals = mesh.normals();
                    normals.len() == mesh.vertexCount as usize
                        && !normals.is_empty()
                        && normals.iter().all(|n| {
                            crate::lighting::valid_normal(rayengine_core::glam::Vec3::new(
                                n.x, n.y, n.z,
                            ))
                        })
                }),
        );
        self.model_skins.push(animation::inspect_skin(&model));
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
        let wave = audio
            .new_wave(path_string(&path)?)
            .map_err(|e| Error::Asset(format!("{}: {e}", path.display())))?;
        let sound = audio
            .new_sound_from_wave(&wave)
            .map_err(|e| Error::Asset(format!("{}: {e}", path.display())))?;
        let id = SoundId(self.sounds.len());
        self.sounds.push(Some(sound));
        self.mixer
            .get_mut()
            .sounds
            .push(Some(crate::audio::SoundPool::new(wave)));
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

pub(crate) fn asset_path(path: &Path) -> Result<PathBuf, Error> {
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
