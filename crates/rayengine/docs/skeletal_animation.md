# Skeletal character animation

Skinned 3D models play imported keyframe clips through raylib's native skinning.
The SDK owns the native clips, checks model/clip compatibility before any pose
reaches raylib, and keeps timing in a display-independent `KeyframePlayer` that
games advance explicitly in `fixed_update`. Character AI, state machines and
clip selection stay game-owned; sprite sheets and `AnimationPlayer` are unchanged.

```rust,no_run
use rayengine::{core::skeletal::KeyframeRate, prelude::*};
use std::time::Duration;

fn init(context: &mut InitContext<'_, '_>) -> Result<ModelAnimator, Error> {
    let model = context.model("assets/character.glb")?;
    // Clips are loaded separately from the mesh, usually from the same file.
    let set = context.model_animations("assets/character.glb", KeyframeRate::GLTF)?;
    let walk = context.assets.find_model_clip(set, "walk")
        .ok_or_else(|| Error::Asset("character has no walk clip".into()))?;
    // Fails unless the model has a valid skeleton with the clip's bone count.
    context.assets.model_animator(model, walk, PlaybackMode::Loop)
}

fn update(animator: &mut ModelAnimator, dt: f32) {
    if let Some(done) = animator.advance(Duration::from_secs_f32(dt)) {
        // A `PlaybackMode::Once` clip reports completion exactly once.
        let _ = done.clip;
    }
}
```

Draw the current pose inside `Frame::world_3d`:

```rust,no_run
use rayengine::{prelude::*, render::Canvas3D, raylib::prelude::{RaylibDraw, RaylibDraw3D}};
fn draw<D: RaylibDraw + RaylibDraw3D>(canvas: &mut Canvas3D<'_, D>, model: ModelId, animator: &ModelAnimator) {
    canvas.animated_model(model, animator, Transform3D {
        position: Vec3::new(1.0, 0.0, 0.0),
        rotation: Quat::from_rotation_y(0.5),
        ..Transform3D::default()
    }, Color::WHITE);
}
```

## Loading and formats

`InitContext::model_animations(path, rate)` loads every clip in one file into a
`ModelAnimationsId` set. The set is cached by canonical path and rate, like
other assets. Clips are addressed by `ModelClipId`, found by file position
(`Assets::model_clip`) or exact stored name (`Assets::find_model_clip`).
`Assets::model_clip_info` reports the name, bone count and `ClipTiming`.

Supported formats are those raylib 6 imports with skeletal clips:

| Format | Rate to pass | Notes |
| --- | --- | --- |
| glTF 2.0 (`.gltf`/`.glb`) | `KeyframeRate::GLTF` (60 per second) | Raylib resamples step, linear and cubic-spline channels at 60 Hz; playback interpolates linearly between those samples. One skin per file. |
| M3D (`.m3d`) | `KeyframeRate::M3D` (one per 17 ms) | Raylib's fixed M3D frame spacing. |
| IQM (`.iqm`) | `KeyframeRate::per_second(authored_fps)` | Raylib discards the authored frame rate, so pass it explicitly. |

The rate only converts time to keyframes; passing the wrong rate plays the clip
faster or slower. The same file loaded with two rates is two independent sets.
Clip names are stored in at most 31 bytes and decoded lossily.

glTF skins must have a parent for their first joint, typically an armature
node. Raylib 6's loader crashes when the root joint is a scene root, so the SDK
parses the glTF JSON first and returns an `Error::Asset` mentioning the "parent
node" instead. Loading is all-or-nothing: a file with no clips, or with any clip
that has no keyframes, missing pose rows or nonfinite transforms, returns an
error and loads none of its clips.

## Compatibility

`Assets::check_model_clip`, `Assets::model_animator` and every animated draw
verify that:

- the model loaded with a skeleton (static meshes report "no skeleton");
- every nonzero vertex weight is finite and refers to an existing bone, and at
  least one mesh is skinned (checked once when the model loads);
- the clip's bone count equals the model's.

Raylib matches bones by index, not name, so a clip must also use the same joint
order as the model. Clips exported from the same rig satisfy this. Bone counts
are the only structural check the backend makes; a clip from a different rig
with the same count is accepted and poses the wrong joints.

## Timing

`ClipTiming::duration` spans the first keyframe to the last:
`(keyframes - 1) / rate`. A 61-keyframe glTF clip lasts exactly one second.
Looping clips wrap from the last keyframe back to the first, so author loops
whose last pose matches the first. Raylib samples glTF clips at whole 60 Hz
steps and drops any trailing partial step, so keep glTF clip lengths whole
multiples of 1/60 s (author at 30 or 60 fps). Otherwise the authored final pose
is never sampled and loops hitch at the wrap. A one-keyframe clip is a static pose.

`KeyframePlayer` follows the [sprite animation](crate::guides::sprites)
conventions. Time is tracked in integer nanoseconds without drift.

- `Loop` wraps without completion events. Huge steps wrap in constant time.
- `Once` holds the last keyframe, discards excess time, and returns one
  `ClipCompleted` from `advance`. `reset` or `play` rearms it.
- `pause` discards advanced time until `resume`. `reset` keeps the pause state.
- `keyframe()` is a fractional position in `0.0..=keyframes - 1`; the fraction
  interpolates linearly toward the next keyframe.

Players are small `Copy` values. Create one per character instance, or create
fresh players once and copy one in to switch clips. Drawing never advances time.
`ModelPose { clip, keyframe }` draws an arbitrary sampled pose when a game
drives keyframes itself.

## Drawing and sharing models

`Canvas3D::animated_model` and `animated_model_material` apply the pose right
before drawing. Several instances can share one `ModelId` with different
clips and keyframes in the same frame. Posing uses raylib's default CPU
skinning, which rewrites and re-uploads the model's vertex buffers on every
animated draw. Cost scales with skinned vertices times animated draws, so keep
crowd characters low-poly.

`try_animated_model` and `try_animated_model_material_matrix` return
`Ok(false)` for stale model, clip or material handles, and errors for
incompatible clips or keyframes outside `0.0..=keyframes - 1` (including NaN).
`try_animated_model` also rejects nonfinite transforms and zero rotations. The
material variant follows `model_material`: it validates the matrix and normals
only for lit materials. The non-`try` variants return `false` instead of errors.
Successful draws count `model_poses` in [diagnostics](crate::guides::diagnostics).

A plain `Canvas3D::model` draw of a skinned model shows whatever pose was last
applied to it (the bind pose before any animated draw). Use `animated_model`
for each skinned instance.

## Ownership and cleanup

`Assets::unload_model_animations` releases every clip in a set at once, because
the clips share one native allocation. Its clip handles become stale forever and
a reload returns a new set. Models and clip sets are independent: unloading
either one makes animated draws that use it return `false` without touching
the other. Remaining sets drop with the other graphics resources before the
window closes. `ResourceCounts` reports live `model_animations`, `model_clips`
and keyframe payload `model_animation_bytes`.

## Limits

- **Blending:** not supported. Each draw samples one clip. Raylib 6 can blend
  two clips natively, but the SDK does not expose it yet. Cross-fades,
  additive layers and per-bone masks are out of scope. Switch clips at natural
  boundaries, such as completion of a one-shot.
- **Bone attachments:** not supported. There is no API to read a bone's world
  transform, so props cannot be parented to a hand. Model props as part of the
  skinned mesh instead, rigidly weighted to one bone.
- **Root motion:** clips do not move the instance. Gameplay owns the
  `Transform3D`, as the example does while walking.
- GPU skinning, morph targets, multiple skins per file and inverse kinematics
  are not supported.

## Example

`cargo run -p rayengine --example character` plays an original lamplighter
generated by `scripts/generate_character_model.py`. It walks between two
lanterns on autopilot and waves to light each one. A/D take manual control, E
waves within reach of a lantern, P pauses and R restarts. The model is original
MIT-licensed artwork, described in `examples/assets/README.md`. The complete
example below is checked by rustdoc.
