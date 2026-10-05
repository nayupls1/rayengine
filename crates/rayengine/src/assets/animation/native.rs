//! Private skinning bridge on raylib's owning thread.
//!
//! Raylib trusts each vertex's bone indices and the caller's keyframe when it
//! poses a model. Validate the skin once at load, then submit only checked
//! model/clip/keyframe triples. Pointer reads and FFI calls stay in this module.
#![allow(unsafe_code)]

use raylib::{
    ffi,
    prelude::{Model, ModelAnimation, RaylibThread},
};

/// Animation capability of a loaded model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Skin {
    /// No skeleton; clips cannot pose it.
    Static,
    /// A skeleton with this many bones and at least one validated skinned mesh.
    Bones(u32),
    /// A skeleton whose skin data would make posing unsafe or meaningless.
    Invalid(String),
}

pub(crate) fn inspect_skin(model: &Model) -> Skin {
    let raw: &ffi::Model = model.as_ref();
    let skeleton = &raw.skeleton;
    if raw.boneMatrices.is_null()
        || raw.currentPose.is_null()
        || skeleton.bones.is_null()
        || skeleton.bindPose.is_null()
        || skeleton.boneCount <= 0
    {
        return Skin::Static;
    }
    let bones = skeleton.boneCount as u32;
    let meshes: &[ffi::Mesh] = if raw.meshes.is_null() || raw.meshCount <= 0 {
        &[]
    } else {
        // SAFETY: A live loaded Model owns `meshCount` initialized meshes at `meshes`.
        unsafe { std::slice::from_raw_parts(raw.meshes, raw.meshCount as usize) }
    };
    let mut skinned = 0;
    for (index, mesh) in meshes.iter().enumerate() {
        if mesh.boneIndices.is_null() || mesh.boneWeights.is_null() || mesh.vertexCount <= 0 {
            // Raylib skips meshes without skin data; they stay in the bind pose.
            continue;
        }
        if mesh.vertices.is_null() {
            return Skin::Invalid(format!("skinned mesh {index} has no vertex positions"));
        }
        let influences = mesh.vertexCount as usize * 4;
        // SAFETY: Raylib allocates four u8 indices and four f32 weights per vertex
        // for every mesh with skin data; the model owns both arrays while borrowed.
        let (indices, weights) = unsafe {
            (
                std::slice::from_raw_parts(mesh.boneIndices, influences),
                std::slice::from_raw_parts(mesh.boneWeights, influences),
            )
        };
        // Raylib reads the bone matrix for every influence whose weight is not
        // exactly zero (including NaN), so those indices must be in range.
        for (&bone, &weight) in indices.iter().zip(weights) {
            if weight != 0.0 && (!weight.is_finite() || u32::from(bone) >= bones) {
                return Skin::Invalid(format!(
                    "skinned mesh {index} has an influence on bone {bone} with weight {weight}, \
                     but the skeleton has {bones} bones"
                ));
            }
        }
        skinned += 1;
    }
    if skinned == 0 {
        return Skin::Invalid("the skeleton has no skinned meshes".into());
    }
    Skin::Bones(bones)
}

/// Requires keyframes and bones, non-null pose rows and finite transforms.
/// (raylib-rs 6.0's `frame_poses_iter` misreads row pointers, so read them here.)
pub(crate) fn check_clip(clip: &ModelAnimation) -> Result<(), String> {
    let raw: &ffi::ModelAnimation = clip.as_ref();
    let (keyframes, bones) = (raw.keyframeCount, raw.boneCount);
    if keyframes <= 0 || bones <= 0 || raw.keyframePoses.is_null() {
        return Err(format!(
            "needs keyframes and bones ({keyframes} keyframes, {bones} bones)"
        ));
    }
    // SAFETY: A live loaded clip owns `keyframeCount` row pointers.
    let rows = unsafe { std::slice::from_raw_parts(raw.keyframePoses, keyframes as usize) };
    for &row in rows {
        if row.is_null() {
            return Err("has a missing keyframe pose".into());
        }
        // SAFETY: Each non-null row owns `boneCount` transforms.
        let pose = unsafe { std::slice::from_raw_parts(row, bones as usize) };
        let finite = |v: &[f32]| v.iter().all(|c| c.is_finite());
        // A nonfinite pose would silently corrupt every skinned vertex.
        if !pose.iter().all(|t| {
            finite(&[t.translation.x, t.translation.y, t.translation.z])
                && finite(&[t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w])
                && finite(&[t.scale.x, t.scale.y, t.scale.z])
        }) {
            return Err("has a nonfinite pose".into());
        }
    }
    Ok(())
}

/// Writes one interpolated pose into the model's bone matrices and, with the
/// default CPU skinning, its animated vertex buffers.
///
/// Callers must pass the model's own `Skin::Bones(n)` result from
/// [`inspect_skin`], a clip with exactly `n` bones whose keyframes were checked
/// at load, and a finite keyframe in `0.0..=keyframeCount - 1`.
pub(crate) fn apply_pose(
    _thread: &RaylibThread,
    model: &Model,
    clip: &ModelAnimation,
    keyframe: f32,
) {
    let (model, clip): (&ffi::Model, &ffi::ModelAnimation) = (model.as_ref(), clip.as_ref());
    debug_assert_eq!(model.skeleton.boneCount, clip.boneCount);
    debug_assert!(keyframe >= 0.0 && keyframe <= (clip.keyframeCount - 1) as f32);
    // SAFETY: RaylibThread proves this runs on the graphics thread. The caller
    // validated equal bone counts, in-range vertex bone indices and a keyframe
    // within the clip, so raylib's pose reads and matrix writes stay in bounds.
    // Both structs are copied by value but alias storage owned by live assets.
    unsafe { ffi::UpdateModelAnimation(*model, *clip, keyframe) }
}
