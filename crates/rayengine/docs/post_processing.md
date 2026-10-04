# Render targets and post-processing

Targets belong to the current game run. Create a typed `RenderTargetId` through
`InitContext::assets.create_render_target` or `Frame::render_target`, draw into it
with `Frame::with_target`, and sample it through `Canvas2D::render_target`,
`UiCanvas::render_target` or `MaterialDesc::render_target` for mesh/model surfaces.
The runnable example below demonstrates a 3D preview and three full-frame effects.

```rust,no_run
use rayengine::prelude::*;
fn preview(frame: &mut Frame<'_, '_>) -> Result<(), Error> {
    let id = frame.render_target(RenderTargetDesc::fixed(320, 180))?;
    frame.with_target(id, |target| {
        target.clear(Color::BLANK);
        target.world_3d(Camera3D::default(), |world| {
            world.cube(Aabb3::from_center(Vec3::ZERO, Vec3::ONE), Color::ORANGE);
        });
    })?;
    frame.ui(|ui| {
        ui.render_target(id, Aabb2 { min: Vec2::ZERO, max: Vec2::new(320.0, 180.0) }, Color::WHITE);
    });
    frame.assets.unload_render_target(id);
    Ok(())
}
```

Create reusable targets once in initialization rather than every frame.
Initialization registers descriptors; their storage is allocated before the first
active draw. `Frame::render_target` allocates immediately. Targets start transparent,
and contents persist until cleared or resized. Target handles remain stable across
resize; unloading invalidates them permanently, even when a slot is reused.
`StateResources::own_render_target` supports exclusive state ownership.

| Size policy | Pixel dimensions | Resize/DPI behavior |
| --- | --- | --- |
| `Fixed(w, h)` | Explicit pixels | Unchanged |
| `Logical` | Logical window content area, excluding bars | Follows content resize, ignores DPI |
| `Physical` | Physical framebuffer content area, excluding bars | Follows resize and DPI |
| `Reference` | Current logical UI size | Fixed under Fit/IntegerFit; follows Expand coverage |

Logical and physical policies also apply under IntegerFit; choose Reference or
Fixed for a pixel-art target. `TargetFilter::Point` preserves nearest sampling;
`Bilinear` interpolates samples. Dimensions round to the nearest positive pixel.
Drawing into a fixed target uses that target's aspect ratio for cameras; UI retains
the frame's logical units and scales them to the target dimensions.

2D/UI target helpers correct the vertical texture orientation and preserve
premultiplied alpha. Mesh UVs retain native attachment orientation: invert V for
an upright top-left image. Built-in unlit/lit materials convert attachment samples
to straight alpha before surface blending. Custom mesh shaders must do the same
or declare the reserved int `rayenginePremultipliedTexture` and implement its
conversion. A material can sample either a loaded texture or a render target.
An active target is unavailable for sampling; drawing dependent materials returns
false and recursively drawing into the same target returns an error. Unloading an
active target returns false. Target ownership is restored when a callback unwinds,
so catching its panic does not leave the target permanently active. Raw attachment borrows are intended for advanced
interop and use native orientation; never retain weak copies beyond target lifetime.

## Ordered effects and UI

Compile a fragment shader with `shader_from_source(None, fragment)`, register its
parameters with `uniform`, and create a material with ordinary `MaterialParam`
overrides. Set `PostProcessing { materials, ui }` through `Assets::set_post_processing`.
Materials execute in vector order, each reading the preceding output as `texture0`.
Effects require a custom unlit shader with no explicit material texture. Cached
uniform defaults and per-material overrides work as they do for mesh surfaces,
including separate parameters for two passes sharing the same shader.

Changes to chain membership or UI placement take effect on the next frame.
Parameter updates during drawing affect the current frame. The after-draw lifecycle
boundary runs after effects finish, so it can safely clear the chain and release
its dependencies for the next frame. Invalid configurations
fail atomically. Unloading or invalidating a dependency of the active chain makes
the runner return an error with normal shutdown cleanup; clear the chain before
releasing its materials/shaders at a boundary.

The pipeline is world → quality resolve/FXAA → optional UI → effects → optional UI
→ viewport presentation. `BeforeEffects` filters UI along with the world;
`AfterEffects` overlays crisp UI at output resolution. Any nonempty chain uses a
separate UI layer, including native and IntegerFit modes; UI calls compose above
all world calls. `Frame::clear` clears both layers. Letterbox bars remain outside
the chain. IntegerFit runs effects at reference resolution with point presentation.
An empty chain follows the existing quality path, with no effect shader passes or
effect targets; native immediate UI ordering is restored.

Shaders receive raylib's usual `fragTexCoord`, `fragColor`, `texture0` and standard
vertex shader. They read and write **premultiplied RGBA**. Preserve alpha and scale
color offsets by alpha so transparent pixels remain transparent. Copy passes use
ONE / ONE_MINUS_SRC_ALPHA over a cleared destination; material surface depth/alpha
policies do not control pass blending. Material tint is supplied as premultiplied vertex color; use white tint for
ordinary effects.

`BuiltinEffect::fragment_source()` provides examples:

| Effect | Registered parameters |
| --- | --- |
| Vignette | float `strength`, clamped to 0..1 |
| ColorGrade | vec3 `gain` and `lift` (identity: ONE and ZERO) |
| Scanlines | float `strength`, clamped to 0..1; float `lines`, at least 1 |

These shaders implement edge darkening, RGB gain/lift grading, and CRT-style
scanline modulation. Register parameter defaults explicitly; unregistered GLSL
uniforms default to zero. Scanlines are a small filter, without bloom or temporal CRT simulation.

## Bounds, cleanup and validation

The quality allocator from rendering-quality support also allocates these targets.
The combined quality, custom-target and effect-target plan is bounded by 8192 per
dimension, the device texture limit, and 512 MiB estimated RGBA+depth storage.
All resized targets are released before replacement allocations. Failed allocations
return explicit errors; the runner still shuts down with the window alive.
`render_target_usage()` reports allocated custom targets/bytes; runtime diagnostics
include custom counts, peak counts and combined target allocation estimates.

A nonempty chain adds two reusable output-sized ping-pong targets regardless of
chain length. Native/IntegerFit additionally acquire a UI and resolve target when
the chain is enabled. Each target estimates 8 bytes per pixel (RGBA8 plus depth).
At 1280×720, native rendering uses 7.03 MiB; adding a chain uses 35.16 MiB, excluding
custom targets. Turning off the chain releases its extra targets before the next
draw. All resources drop before the window closes. Minimized frames pause rendering.

```sh
cargo run -p rayengine --example post_processing -- effects
cargo run -p rayengine --example post_processing -- effects --hidden --frames 30 --screenshot artifacts/post.png
cargo test -p rayengine native_post_processing -- --ignored --test-threads=1
python3 scripts/post_processing_comparison.py --frames 2000 --repeats 5
```

The example uses T to toggle effects and U to choose UI placement. Direct and empty
modes submit identical geometry/text for the benchmark; the empty mode explicitly
sets an empty chain. The comparison rotates run order, checks identical submission
counts, and records repeated frame/render/presentation wall times, allocations,
screenshots, revision, toolchain and GL environment. These are CPU wall times with
driver stalls, not GPU timer measurements. Native probes cover chain order and
shared-shader overrides, premultiplied transparency, both UI placements, 2D/3D
contents and sampling, target invalidation/limits, resizing, runtime toggling and
screenshots across Fit/Expand/IntegerFit, quality modes and simulated DPI factors.
Live desktop DPI transitions are not automated. See the repository's
[measured comparison](https://github.com/nayupls1/rayengine/blob/master/docs/post_processing_comparison.md).
