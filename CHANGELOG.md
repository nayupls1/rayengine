# Changelog

## Unreleased (0.0.2)

- Configurable native/2× supersampled world rendering and offscreen FXAA, native
  UI composition, checked manifest profiles, bounded allocations and visual/performance probes.

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
