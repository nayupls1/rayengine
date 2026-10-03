# rayengine-particles

Optional, game-owned bounded particle effects, introduced in 0.0.2.
The tested SDK requirement is the workspace's 0.0.2 API, Rust 1.89+, desktop Linux.
The engine never depends on this crate and performs no particle lifecycle work
unless a game explicitly creates and ticks an instance. This crate is not part
of the initial four-crate publication set; use a Cargo path dependency.

```toml
[dependencies]
rayengine-particles = { path = "../rayengine/plugins/particles" }
# Add features = ["render"] for SDK sprite/billboard adapters.
```

The default build depends only on `rayengine-core` and needs no display or native
raylib toolchain. Independent emitters have no registry, ECS scan, or shared RNG.

```rust
use rayengine_particles::{Emitter, EmitterConfig};
let mut sparks = Emitter::new(EmitterConfig {
    capacity: 128, max_spawn: 32, seed: 42, ..Default::default()
})?;
assert_eq!(sparks.burst(1000), 32);
sparks.step(1.0 / 60.0)?; // pass explicit fixed-tick seconds, never render dt
sparks.stop(); // no births; existing particles keep aging
sparks.step(1.0)?;
assert!(sparks.is_empty());
sparks.reset(); // clear, restore seed and phase, resume
assert_eq!(sparks.burst(8), 8);
drop(sparks); // remove all CPU storage
# Ok::<(), rayengine_particles::ParticleError>(())
```

Configuration validates finite motion, nonnegative variations/rates/sizes,
positive ordered lifetime, and straight RGBA in 0..=1. Variation is uniform per
axis. Constant acceleration is integrated analytically per tick; size and color
interpolate linearly over each sampled lifetime. Splitting a tick changes birth
quantization: continuous births occur at tick end with age zero. Identical seeds,
configuration and call sequence reproduce CPU state. Ages accumulate in f64 so
small fixed ticks do not round prematurely to a f32 lifetime boundary. There is
no wall clock or backend randomness. `set_position` moves future births, leaving
live particles in world space. Finite motion that overflows f32 storage retires that particle.

## Bounds and saturation

`capacity` bounds the live vector; `max_spawn` bounds births and RNG work per
`burst` or `step` call. Both must be in 1..=1,000,000. Storage is reserved once;
steady-state simulation and rendering do not allocate. Each independent emitter
owns its own limit; the game controls the total number of emitters and calls.
Rejected births are dropped, never queued or substituted for older particles.
Continuous emission carries only a fractional birth remainder, even when
saturated; freeing slots does not release an old backlog. Stopped time accrues
no emissions. Bursts do not consume the continuous emission phase. `burst`
returns admitted births; `step` retires particles before admitting new births.
Zero dt only collapses interpolation history; invalid dt changes no state.

## Rendering and lifecycle

Enable `render`, wrap an emitter in `render::ParticleEffect`, and use the existing
`Plugin<render::ParticleView>` hooks explicitly from the game's lifecycle. The
game owns cameras, input, pause policy, hook order and resources. The effect
exposes burst/start/stop/reset, fixed stepping and origin changes. Removing an effect means
calling `Plugin::unload` at a render-thread preparation boundary and dropping it.
Unload is idempotent and permits reinit; it resets CPU state and releases only
its own quad/material. Failed init leaves neither committed; duplicate init is
rejected. Keep initialized effects paired with their original run's assets.
Dropping alone retains GPU handles until the SDK asset collection drops with a
live native context, including on propagated game initialization failure.

Textured effects take a `ParticleSprite` borrowing a game-owned `TextureId` and
an optional `SpriteRegion`; unload never destroys that
texture. None draws square untextured particles. Unloaded texture handles skip
textured drawing. Both adapters validate atlas bounds at initialization. 2D reuses the SDK
`Canvas2D::sprite` API; 3D maps the same region to the reusable quad UVs.
`TextureId::into()` selects a whole texture.

2D draws XY sprites oldest to newest using the existing viewport/camera scaling
and scoped straight-alpha blending that preserves destination alpha. 3D draws camera-facing quads, sorted far
to near by camera-space depth with birth-order ties. It uses `AlphaMode::Blend`:
depth testing enabled, depth writes disabled. Draw opaque/cutout geometry first,
then particles, then HUD. Sorting is per emitter; the game must order overlapping
emitters/other transparent objects appropriately. These adapters do not promise
correct global transparency for interleaved emitters. `draw_2d`/`draw_3d` can share
a caller-owned canvas with other effects. Colors use straight alpha, so supply
straight-alpha textures. Billboards have square world-unit size. Render calls
interpolate previous/current positions and ages using `Frame::alpha`; paused
games can call `step(0.0)` to collapse history. Degenerate 3D cameras skip drawing.

## Demo and validation

`cargo run -p rayengine-particles --features render --example effects` demonstrates
independent sparks, smoke and pickup bursts in 2D and 3D (Tab toggles view,
Space bursts, S stops/resumes smoke, R resets). The soft sprite is procedural,
distributable under this crate's MIT license, and has no file/network dependencies.
The runner supports `--hidden --frames 120 --size 800x1000 --screenshot path.png`.

```sh
cargo test --locked -p rayengine-particles
cargo test --locked -p rayengine-particles --features render
LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a cargo test --locked -p rayengine-particles --features render native_particles -- --ignored --test-threads=1
RUSTDOCFLAGS='-D warnings' cargo doc --locked -p rayengine-particles --features render --no-deps
scripts/benchmark.sh save particles-v1 particles_
scripts/benchmark.sh compare particles-v1 particles_
```

CPU tests cover deterministic/reset behavior, saturation, sustained load,
fixed-tick motion, lifetime, appearance and invalid input. Ignored serial native
probes cover textured/untextured pixels, blending, depth ordering/writes, fitted
wide/portrait viewports, stale handles and unload/reinit. Criterion workloads
compare idle, full-capacity updates, continuous saturation and oversized bursts
at 128/4,096/32,768 capacity. Construction is outside measured regions and every
sample starts from the same seeded state. These are CPU costs, not GPU/FPS claims.
See [performance evidence](crate::guides::performance) for measured bounds/comparisons.
