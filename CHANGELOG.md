# Changelog

## 0.0.1

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
