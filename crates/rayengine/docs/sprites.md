# Sprite sheets and animation

`SpriteRegion`, `SpriteFrame`, `AnimationClip` and `AnimationPlayer` live in the
display-independent core and are re-exported by the SDK prelude. Games define
named immutable clips and may share them with `Arc` between independent players.
Each frame has its own `Duration`. Supply simulation time explicitly in
`fixed_update`; drawing and inspecting a frame never advance playback.

```rust
use rayengine::prelude::*;
use std::time::Duration;

let frames = [0, 1, 2].into_iter().map(|column| SpriteFrame {
    region: SpriteRegion::new(column * 16, 0, 16, 16).unwrap(),
    duration: Duration::from_millis(100 + column as u64 * 25),
}).collect();
let clip = AnimationClip::new("walk", frames, PlaybackMode::Loop).unwrap();
let mut player = AnimationPlayer::new(clip);
player.advance(Duration::from_millis(250));
assert_eq!(player.frame_index(), 2);
player.pause();
player.advance(Duration::from_secs(10)); // paused time is discarded
assert_eq!(player.frame_index(), 2);
player.resume();
player.reset(); // preserves pause state and rearms one-shot completion
assert_eq!(player.frame_index(), 0);
```

Exact frame boundaries select the next frame. `Loop` wraps at the clip duration
and emits no completion event. `Once` holds its last frame after reaching the
end, discards excess time, and returns one `AnimationCompleted` from `advance`.
That event retains the completed named clip and can be placed in the game's
`Events<AnimationCompleted>` queue. Later advances, pause/resume, and drawing
cannot emit it again; `reset` or `play` rearms it. `play` starts the selected clip
at frame zero, unpaused, even when selecting the same clip. Change clips only
when the gameplay state changes, rather than restarting them every tick.

Timing uses integer nanoseconds. Even `Duration::MAX` steps skip many loops in
constant time; finding the resulting frame takes logarithmic time in the number
of clip frames. Advancement and frame reads do not allocate. Convert the SDK's
fixed tick with `Duration::from_secs_f32(context.tick.dt)`; durations supplied by
your own simulation should be nonnegative and finite before conversion.

Draw through `Canvas2D::sprite` inside `Frame::world_2d`:

```rust,no_run
use rayengine::{prelude::*, render::Canvas2D, raylib::prelude::RaylibDraw};
fn draw<D: RaylibDraw>(canvas: &mut Canvas2D<'_, D>, texture: TextureId, player: &AnimationPlayer) {
    let size = Vec2::new(32.0, 32.0);
    canvas.sprite(texture, player.frame().region, SpriteTransform {
        position: Vec2::new(100.0, 50.0),
        size,
        origin: size * 0.5,
        rotation: 0.25, // clockwise radians
        flip_x: true,
        ..SpriteTransform::default()
    }, Color::WHITE);
}
```

Source coordinates start at the texture's top-left and are measured in pixels.
Destination position, size and origin use world units. `position` places the
explicit local origin in world space; `origin = size * 0.5` rotates around the
center, while zero rotates around the top-left. Origins may lie outside the
destination. Flips reverse sampling without moving geometry or the pivot.
Tint multiplies the sampled RGBA channels. The existing camera rotation/zoom,
Fit, Expand, high-DPI and IntegerFit presentation policies apply to sprites just
as to other world primitives. IntegerFit presents a reference-resolution target
with nearest filtering. Cached textures use raylib's default nearest filtering;
align the camera and sprite destinations to your pixel grid for crisp artwork.

Validation is explicit: `SpriteRegion::new` rejects zero dimensions and `u32`
endpoint overflow; negative sources are represented with transform flip flags.
`AnimationClip::new` rejects blank names, empty frame lists, zero durations and
total duration overflow. Definitions cannot be mutated after validation.
Actual texture bounds are checked at drawing time, allowing CPU-only clips to
be defined before loading artwork. `Canvas2D::sprite` returns `false` without a
submission for an out-of-bounds region, unloaded texture, nonfinite transform,
nonpositive destination size or rotation that overflows conversion to degrees.
Stale handles retain the existing safe behavior and never revive within a run.
Successful draws use the existing texture submission diagnostics counter.

Run the playable example with
`cargo run -p rayengine --example sprites`. A/D or arrows move and change facing;
Space plays a one-shot swing. Face the golden orb and swing within reach to
collect it. P pauses/resumes, R restarts the current clip, and Escape exits.
The bundled sprite sheet is original MIT-licensed artwork, documented in
`examples/assets/README.md`; no download or working-directory setup is needed.
For skinned 3D models, see [skeletal animation](crate::guides::skeletal_animation).
The complete example below is checked by rustdoc.
