# Runtime diagnostics

Diagnostics are opt-in. Normal runs add no diagnostic clock reads, resource
scans, report allocation or per-frame output. SDK drawing checks an optional
counter; the existing runtime clock and ordinary RunReport counters remain.
Enabled reports retain totals/min/max in constant space, without frame vectors.

```rust,no_run
use rayengine::{App, Config, RunOptions};
use rayengine::diagnostics::DiagnosticsConfig;
let mut diagnostics = DiagnosticsConfig::new("my-game/fixed-scene.v1");
diagnostics.output = Some("artifacts/my-game.json".into());
let app = App::new(Config::new("My game")).with_options(RunOptions {
    frames: Some(300),
    diagnostics: Some(diagnostics),
    ..RunOptions::default()
});
// let report = app.run(game)?;
// let metrics = report.diagnostics.unwrap();
```

For games that use `RunOptions::from_env`, either `--workload ID` or
`--diagnostics PATH` enables collection; `--diagnostics` writes JSON after the
run. A missing workload uses `unspecified`; choose a descriptive stable ID for
comparisons. IDs accept 1–128 ASCII letters/digits and `._-/`. Report output
creates parents and explicitly replaces the requested file. I/O/serialization
and final buffer-flush errors propagate through `App::run`; report output is
ordinary diagnostic output, not a durable game save.

```sh
cargo run -p rayengine-demos --bin meadow -- \
  --hidden --frames 300 --size 960x540 --uncapped \
  --workload meadow/default-view.v1 --diagnostics artifacts/meadow.json
```

## Schema 1

`RunReport::diagnostics` is None when disabled. When enabled, it exposes the same
typed `DiagnosticsReport` that `write_json` exports. Top-level fields are
`schema_version`, `workload`, `settings`, `frames`, `updates`, `dropped_ns`,
`frame`, `update`, `render`, `present`, `draws`, `resources`, `peak_resources`.
Timing objects contain `samples`, `total_ns`, `min_ns`, `max_ns`; units are
nanoseconds. Divide total by nonzero samples for a mean. Counts/totals saturate.

- `frame`: active iteration through presentation and diagnostic sampling.
- `update`: each game fixed-update callback, excluding input routing/edge reset.
- `render`: offscreen target preparation/resizing, clear, and game draw callback.
- `present`: window blit and EndDrawing, including swaps, caps and driver stalls.

Minimized event polling is excluded. Screenshot capture, report serialization,
initialization and teardown are excluded from these timing samples. These are
CPU wall times; deferred GPU work may stall a later operation. There are no GPU
timers, percentiles or arbitrary-game FPS claims. Fixed-update counts depend on
elapsed wall time; a frame limit alone does not produce deterministic simulation.
Use frozen state/input for reproducible native rendering workloads.

`draws` counts successful SDK submission requests and passes. Mesh counts
include the constituent meshes of imported models. Button fill/text/focus
outline count separately. Failed/stale resource requests contribute zero.
The runner's presentation blit is excluded. These are not physical OpenGL draw
calls: raylib batches primitives. Calls through `canvas.raw` are unobservable;
`Frame::with_raylib` counts a raw pass, not its internal submissions.
`Frame::set_draw_counters_enabled` changes counting for that frame and resets
on a mode transition; use it for controlled overhead benchmarks.

`resources` samples live Assets after initialization and every active frame;
`peak_resources` holds per-field sampled maxima, which need not occur together.
Successful replacement counts one live resource. Transient upload overlap or
assets created and unloaded between samples are not high-water measurements.
Counts cover standalone textures, models, sounds, generated meshes, materials,
custom shaders and the SDK material shader. Logical texture bytes include mips;
generated geometry includes fallback UVs; model geometry covers standard
vertex/attribute/index arrays. Bytes exclude driver allocation padding, audio,
shader storage, imported model textures/bones/animations, default raylib assets,
game-owned raw resources, CPU container capacities and the runner's target.
Render-target dimensions are recorded in settings. This is not total VRAM usage.
`Assets::resource_counts` is available separately and scans retained asset slots
without GPU queries/readback. Disabled runs do not call it automatically.

## Comparison and overhead

Settings record compiled backend support, OS/architecture, SDK version, window
and reference sizes, viewport policy, fixed frequency/catch-up, effective
requested cap/vsync and final render size. Driver overrides may affect vsync;
the compiled backend label is not a runtime driver identification. Use the
existing benchmark scripts for revision/toolchain/CPU and renderer provenance.
Keep driver, GPU, backend, target, DPI, scene, input and caps identical.

```sh
scripts/render_benchmark.sh save diagnostics-v1 diagnostics_
scripts/render_benchmark.sh compare diagnostics-v1 diagnostics_
```

`diagnostics_draw/{disabled_100,enabled_100}` submits the same 100 indexed quads
and camera pass at the existing 64×64 native target. Counting is toggled outside
measurement. `diagnostics_resources/owned_fixture` samples the prepared assets
(one quad, one 8×8 RGBA texture, one custom/one SDK shader and five materials).
`diagnostics_timing/{disabled_phase,enabled_phase}` isolates the optional two
clock reads and constant-space timing update around trivial CPU work. These
separate counter, resource and timer costs; they do not claim whole-game overhead.
The original native draw/upload/UI IDs retain their workloads. All exports use
the existing snapshot schema and comparator, which flags renderer/backend/target
and measurement differences. Consult [testing and performance](crate::guides::testing_performance).
