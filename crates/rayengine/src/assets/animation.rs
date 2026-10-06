//! Owned skeletal animation clips and model/clip compatibility.

use super::{ModelId, asset_path, path_string};
use crate::Error;
use rayengine_core::skeletal::{
    ClipTiming, KeyframePlayer, KeyframeRate, PlaybackMode, check_skeleton,
};
use raylib::prelude::*;
use std::{collections::HashMap, path::Path, path::PathBuf};

mod native;
pub(crate) use native::{Skin, apply_pose, inspect_skin};

/// Stable handle for the clips loaded from one animation file in the current run.
///
/// Clips share their file's native allocation, so the whole set unloads at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelAnimationsId(pub(crate) usize);

/// One clip within a loaded [`ModelAnimationsId`]. Becomes stale when its set unloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelClipId {
    set: ModelAnimationsId,
    index: u32,
}

impl ModelClipId {
    /// Owning animation set.
    pub fn set(self) -> ModelAnimationsId {
        self.set
    }
    /// Zero-based clip position within its file.
    pub fn index(self) -> usize {
        self.index as usize
    }
}

/// Validated description of an imported clip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelClipInfo {
    name: String,
    bones: u32,
    timing: ClipTiming,
}

impl ModelClipInfo {
    /// Name stored in the file (at most 31 bytes, lossily decoded), possibly empty.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Bones in each keyframe; must equal the model's skeleton bone count.
    pub fn bones(&self) -> u32 {
        self.bones
    }
    /// Keyframe count and the rate supplied when loading.
    pub fn timing(&self) -> ClipTiming {
        self.timing
    }
}

/// Playback cursor for one model instance. Advance it with simulation time and
/// draw its current pose with [`crate::render::Canvas3D::animated_model`].
pub type ModelAnimator = KeyframePlayer<ModelClipId>;

/// A clip sampled at a fractional keyframe in `0.0..=keyframes - 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPose {
    /// Clip whose keyframes are sampled.
    pub clip: ModelClipId,
    /// Keyframe position; the fraction interpolates toward the next keyframe.
    pub keyframe: f32,
}

impl From<&ModelAnimator> for ModelPose {
    fn from(animator: &ModelAnimator) -> Self {
        Self {
            clip: animator.clip(),
            keyframe: animator.keyframe(),
        }
    }
}
impl From<ModelAnimator> for ModelPose {
    fn from(animator: ModelAnimator) -> Self {
        Self::from(&animator)
    }
}

struct ClipSet {
    native: ModelAnimations,
    clips: Vec<ModelClipInfo>,
    bytes: u64,
}

#[derive(Default)]
pub(crate) struct AnimationAssets {
    sets: Vec<Option<ClipSet>>,
    paths: HashMap<(PathBuf, KeyframeRate), ModelAnimationsId>,
}

impl AnimationAssets {
    pub(crate) fn load(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        path: &Path,
        rate: KeyframeRate,
    ) -> Result<ModelAnimationsId, Error> {
        let path = asset_path(path)?;
        let key = (path, rate);
        if let Some(&id) = self.paths.get(&key) {
            return Ok(id);
        }
        let context = |message: String| Error::Asset(format!("{}: {message}", key.0.display()));
        preflight_gltf(&key.0).map_err(context)?;
        let native = raylib
            .load_model_animations(thread, path_string(&key.0)?)
            .map_err(|e| context(e.to_string()))?;
        let mut clips = Vec::with_capacity(native.len());
        let mut bytes = 0_u64;
        for (index, clip) in native.iter().enumerate() {
            let (keyframes, bones) = (clip.keyframeCount, clip.boneCount);
            native::check_clip(clip).map_err(|e| context(format!("animation {index}: {e}")))?;
            let name: Vec<u8> = clip
                .name
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect();
            let timing = ClipTiming::new(keyframes as u32, rate)
                .map_err(|e| context(format!("animation {index}: {e}")))?;
            // Each keyframe owns one Transform per bone plus its row pointer.
            bytes = bytes.saturating_add((keyframes as u64).saturating_mul(
                (bones as u64).saturating_mul(size_of::<raylib::ffi::Transform>() as u64)
                    + size_of::<usize>() as u64,
            ));
            clips.push(ModelClipInfo {
                name: String::from_utf8_lossy(&name).into_owned(),
                bones: bones as u32,
                timing,
            });
        }
        let id = ModelAnimationsId(self.sets.len());
        self.sets.push(Some(ClipSet {
            native,
            clips,
            bytes,
        }));
        self.paths.insert(key, id);
        Ok(id)
    }

    pub(crate) fn unload(&mut self, id: ModelAnimationsId) -> bool {
        self.paths.retain(|_, handle| *handle != id);
        self.sets.get_mut(id.0).and_then(Option::take).is_some()
    }

    fn set(&self, id: ModelAnimationsId) -> Option<&ClipSet> {
        self.sets.get(id.0).and_then(Option::as_ref)
    }

    pub(crate) fn clip_count(&self, id: ModelAnimationsId) -> Option<usize> {
        self.set(id).map(|set| set.clips.len())
    }

    pub(crate) fn clip(&self, id: ModelAnimationsId, index: usize) -> Option<ModelClipId> {
        (index < self.clip_count(id)?).then_some(ModelClipId {
            set: id,
            index: index as u32,
        })
    }

    pub(crate) fn find(&self, id: ModelAnimationsId, name: &str) -> Option<ModelClipId> {
        let index = self.set(id)?.clips.iter().position(|c| c.name == name)?;
        self.clip(id, index)
    }

    pub(crate) fn info(&self, clip: ModelClipId) -> Option<&ModelClipInfo> {
        self.set(clip.set)?.clips.get(clip.index as usize)
    }

    pub(crate) fn native(&self, clip: ModelClipId) -> Option<(&ModelAnimation, &ModelClipInfo)> {
        let set = self.set(clip.set)?;
        Some((
            set.native.get(clip.index as usize)?,
            set.clips.get(clip.index as usize)?,
        ))
    }

    /// Live sets, clips and keyframe payload bytes.
    pub(crate) fn usage(&self) -> (u64, u64, u64) {
        self.sets
            .iter()
            .flatten()
            .fold((0, 0, 0), |(sets, clips, bytes), set| {
                (
                    sets + 1,
                    clips + set.clips.len() as u64,
                    bytes.saturating_add(set.bytes),
                )
            })
    }
}

/// Rejects glTF layouts that crash raylib 6's animation loader instead of
/// failing: it reads the first skin joint's parent node unconditionally.
/// Other formats and malformed files are left to the native loader.
fn preflight_gltf(path: &Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let bytes = match extension.as_deref() {
        Some("gltf" | "glb") => std::fs::read(path).map_err(|e| e.to_string())?,
        _ => return Ok(()),
    };
    gltf_root_joint_check(&bytes)
}

fn gltf_root_joint_check(bytes: &[u8]) -> Result<(), String> {
    let json = if bytes.starts_with(b"glTF") {
        // GLB: 12-byte header, then the JSON chunk's length, type and payload.
        let word = |at: usize| {
            bytes
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        let (length, kind) = (word(12), bytes.get(16..20));
        match (length, kind) {
            (Some(length), Some(b"JSON")) => bytes
                .get(20..20usize.saturating_add(length))
                .ok_or("GLB JSON chunk is truncated")?,
            _ => return Err("GLB has no leading JSON chunk".into()),
        }
    } else {
        bytes
    };
    let document: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("invalid glTF JSON: {e}"))?;
    let Some(skin) = document["skins"].get(0) else {
        return Ok(());
    };
    let Some(root) = skin["joints"].get(0).and_then(serde_json::Value::as_u64) else {
        return Err("the glTF skin has no joints".into());
    };
    let has_parent = document["nodes"].as_array().is_some_and(|nodes| {
        nodes.iter().any(|node| {
            node["children"]
                .as_array()
                .is_some_and(|children| children.iter().any(|c| c.as_u64() == Some(root)))
        })
    });
    if !has_parent {
        return Err(
            "the first glTF skin joint must have a parent node (such as an armature); \
             raylib cannot load animations for a scene-root joint"
                .into(),
        );
    }
    Ok(())
}

/// Checks a live model skin against a live clip, naming both on failure.
pub(crate) fn check(model: ModelId, skin: &Skin, info: &ModelClipInfo) -> Result<(), Error> {
    let bones = match skin {
        Skin::Static => 0,
        Skin::Bones(bones) => *bones,
        Skin::Invalid(reason) => {
            return Err(Error::Asset(format!(
                "model {} cannot be animated: {reason}",
                model.0
            )));
        }
    };
    check_skeleton(bones, info.bones).map_err(|e| {
        Error::Asset(format!(
            "clip \"{}\" is incompatible with model {}: {e}",
            info.name, model.0
        ))
    })
}

/// Rejects keyframes outside the clip, which the backend would index out of bounds.
pub(crate) fn check_pose(pose: ModelPose, info: &ModelClipInfo) -> Result<(), Error> {
    let last = (info.timing.keyframes() - 1) as f32;
    if !(pose.keyframe.is_finite() && (0.0..=last).contains(&pose.keyframe)) {
        return Err(Error::Asset(format!(
            "pose keyframe {} is outside clip \"{}\" (0..={last})",
            pose.keyframe, info.name
        )));
    }
    Ok(())
}

/// Creates a player after checking that the clip can pose the model.
pub(crate) fn animator(
    model: ModelId,
    skin: &Skin,
    clip: ModelClipId,
    info: &ModelClipInfo,
    mode: PlaybackMode,
) -> Result<ModelAnimator, Error> {
    check(model, skin, info)?;
    Ok(KeyframePlayer::new(clip, info.timing, mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    #[test]
    fn gltf_preflight_rejects_scene_root_joints_before_native_loading() {
        let fixture = |name: &str| Path::new(FIXTURES).join(name);
        assert_eq!(preflight_gltf(&fixture("pendulum.glb")), Ok(()));
        let error = preflight_gltf(&fixture("root_joint.glb")).unwrap_err();
        assert!(error.contains("parent node"), "{error}");
        // Other formats are left to the native loader.
        assert_eq!(preflight_gltf(&fixture("missing.iqm")), Ok(()));
        assert!(preflight_gltf(&fixture("missing.GLB")).is_err());
        // Text glTF, skinless files, empty skins and malformed containers.
        let parented = br#"{"nodes":[{"children":[1]},{}],"skins":[{"joints":[1]}]}"#;
        assert_eq!(gltf_root_joint_check(parented), Ok(()));
        assert_eq!(gltf_root_joint_check(br#"{"nodes":[{}]}"#), Ok(()));
        assert!(gltf_root_joint_check(br#"{"skins":[{"joints":[]}]}"#).is_err());
        assert!(gltf_root_joint_check(br#"{"nodes":[{}],"skins":[{"joints":[0]}]}"#).is_err());
        assert!(gltf_root_joint_check(b"not json").is_err());
        assert!(gltf_root_joint_check(b"glTF\x02\0\0\0").is_err());
        let mut truncated = b"glTF\x02\0\0\0\0\0\0\0".to_vec();
        truncated.extend_from_slice(&64_u32.to_le_bytes());
        truncated.extend_from_slice(b"JSON{}");
        assert!(gltf_root_joint_check(&truncated).is_err());
    }
}
