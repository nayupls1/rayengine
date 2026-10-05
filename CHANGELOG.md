# Changelog

## Unreleased

- Exact CPU raycasts, translation casts and overlap visitors for 2D boxes/circles
  and 3D boxes/spheres, with symmetric filters, exclusions, opt-in triggers and
  deterministic first hits over current world bodies; checked docs and a headless example.

- Game-owned CPU simulation timelines with pause, validated speed controls,
  fixed timesteps, fractional time carry and bounded discard; checked guide and
  interactive example keep UI responsive and consume simulation commands once.

## 0.0.3

- Opt-in cached music streams with looping, frame-clock fades/crossfades and
  explicit pause/resume; named master/music/sfx and custom buses with volume,
  mute, transient ducking and serializable settings for versioned saves.
- Independent one-shot volume/pitch/pan, reusable overlapping voices and optional
  per-sound concurrency caps; state-owned stream cleanup and resource diagnostics.
- Checked audio guide, CPU bus/fade/save tests, native playback probes and a
  runnable scene/pause/settings example with original procedural WAV assets.
- Versioned engine-owned render targets with fixed/logical/physical/reference sizing,
  point/bilinear sampling, 2D/3D rendering and shared material textures.
- Ordered material post-processing with UI before/after effects, vignette, color
  grade and scanline shaders, resize/DPI allocation bounds, checked docs/example,
  native pixel probes and direct-versus-empty-chain performance comparison.
- Screenshots capture the final rendered frame before buffer swap.

- Display-independent tweens in `rayengine-core`: 22 pure easing functions,
  typed `f32`/`Vec2`/`Vec3`/`Vec4`/`Quat`/RGBA tweens with delay, loop/ping-pong,
  finite cycles, retargeting, pause/resume/reset/cancel/finish and one-shot
  `TweenCompleted` events; drift-free integer-nanosecond timing.
- Allocation-free sequences and parallel groups over arrays, `Vec`s and tuples,
  with leftover time carried between members and nesting.
- Seeded trauma-based screen shake for `Camera2D` and `Camera3D`.
- `Frame::delta` exposes render-frame wall time for presentation-only animation;
  new tweens guide and playable example animating UI and world objects.
- Grid A* pathfinding in the core: four/eight-way movement, per-cell costs,
  diagonal corner rules, borrowed `NavGrid` cells, reusable search buffers and
  resumable expansion budgets.
- Cost- and clearance-aware path smoothing, a path follower for 2D/3D character
  bodies, and distance fields for many agents chasing one target.
- `rayengine-tilemap` maps implement `NavGrid` (solid tiles block) and expose
  `grid_layout()`; `GridLayout` supports rectangular cells.
- Checked pathfinding guide, an agents-around-walls example, unit tests against
  a reference search, and `pathfinding_*` Criterion workloads.

- Deterministic 2D/3D arcade physics worlds with swept contacts, collision
  layers, triggers, mass-weighted separation, restitution and moving platforms;
  checked physics and first-person controller guides and playable examples.
- Optional repository-only `rayengine-tilemap` plugin with validated layered
  TOML levels, collision/trigger queries, conservative chunk culling and sprite
  rendering; navigation integration uses the core grid API.
- CLI lifecycle commands: template discovery, topdown/platformer starters,
  plugin add/remove, bounded file watching and restart, Linux packaging with
  relocated assets/fonts and license notices, clean and prerequisite hints.
  Tilemap starters and unpublished plugins require a repository SDK or explicit
  compatible local plugin paths.
- Embervault: an original six-room dungeon showcase combining tilemaps,
  physics, navigation, sprites, particles, tweens, state stacks, UI, shaders,
  streamed music and durable saves, with gameplay/native/bundle probes.
- Coordinated 0.0.3 versions for the four published crates; beacons, particles,
  tilemap and game examples remain repository-only.

## 0.0.2

- Configurable native/2× supersampled world rendering and offscreen FXAA, native
  UI composition, checked manifest profiles, bounded allocations and visual/performance probes.
- Owned custom fonts with typed handles, target-aware sharp UI atlases, shared
  measurement/spacing, per-button selection, explicit pixel sampling, named
  manifest/profile integration, and a licensed two-font comparison example.
- Minecraft demo: game menu, F3-only debug overlay, icon-based inventory and
  recipe cards, day/night lighting with a lit voxel shader, held torches, lit
  pickups and particle block debris.
- Add opt-in Lambert materials with ambient, directional, and up to four point
  lights, shared generated/imported geometry APIs, checked normal/transform
  validation, an adjustable demo, native pixel probes, and submission benchmarks.
- Optional game-owned state stacks with deferred transitions, independent
  update/draw/input routing, explicit resource cleanup and a title/pause example.
- Validated sprite-sheet regions with world-space rotation, explicit pivots,
  flipping and tint through cached texture handles.
- CPU-only named animation clips with per-frame timing, looping and one-shot
  playback, pause/resume/reset and single completion events.
- Original MIT-licensed pixel-art playground, checked documentation, focused
  playback tests and a native sprite drawing probe across viewport policies.
- Add optional versioned `rayengine.toml` descriptions with profiles, shared
  CLI/runtime validation, manifest-relative assets/font declarations, extension
  namespaces, scaffold examples and inspection/check integration.
- Add the independently optional `rayengine-particles` Cargo plugin with bounded,
  seeded CPU emitters, fixed-tick emission/bursts, lifetime variation, motion and
  size/color evolution, and explicit stop/reset/removal.
- Add optional 2D atlas sprites and transparent camera-facing 3D billboards using
  existing asset/material/viewport APIs, with procedural sparks/smoke/pickup demo.
- Add scoped 2D alpha blending that preserves target alpha and restores native
  state, simulation/native coverage and bounded-load Criterion comparisons.
- Coordinated 0.0.2 crate versions; `rayengine-particles` stays repository-only.

## 0.0.1

- Manually dispatched crates.io releases gated by the full CI suite, coordinated
  crate dry runs, package contents/license checks and installed consumer probes.
- CLI starters use the matching crates.io SDK version by default, with explicit
  `--sdk-path` support for checkout development.
- Optional voxel plugin with CPU storage, signed queries, meshing and streaming,
  plus the SDK render adapter; repository-only Minecraft survival demo and saves.

- Linux-first Cargo workspace with a display-independent core, raylib SDK,
  project CLI, and playable 2D/3D examples.
- Dense typed components, optional validated transform hierarchy, stable entity
  generations and a fast path for unparented scenes.
- Fixed simulation timing, bounded catch-up, interpolation and action input
  transitions that are consumed once per simulation tick.
- Explicit one-shot/repeating timers and typed event queues with reused storage.
- Responsive Fit/Expand/IntegerFit viewports, stable camera height/FOV, logical
  UI anchors and pointer conversion with letterbox rejection.
- Swept 2D/3D character movement against static boxes.
- Cached texture/model/sound ownership and stable typed handles; audio is opt-in.
- CLI project scaffolding, inspection, checks, builds, runs and prerequisite
  diagnostics, with versioned JSON success/error output.
- Arena platform fighter with training dummy/optional AI, double jumps,
  damage-based knockback and knockouts.
- Meadow exploration platformer with camera-relative movement, jumping,
  collectible orbs and checkpoints.
- Markdown guides and Rust examples exported through rustdoc.
- Behavior, gameplay, CLI and native rendering tests; Criterion baselines and
  exported benchmark samples with machine/toolchain/revision provenance.

Interactive agent pause/input/image/state automation is deferred to issue #1.
