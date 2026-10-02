# Tests and performance comparisons

Run the repository checks:

```sh
scripts/check.sh
python3 scripts/template_smoke.py
```

Checks cover formatting, Clippy, CPU behavior, gameplay, CLI process output,
documentation examples and HTML docs. The template smoke script generates both
starter projects in temporary directories and checks the actual resulting code.

The core can be tested without any native graphics dependencies:

```sh
cargo test --locked -p rayengine-core
```

Save tests use temporary regular files without a graphics context. They cover a
fixed wire-format fixture, bounds, corruption, explicit game migration,
concurrent complete replacements, and injected partial-write/flush failures.
An injected rename that completes before returning an error verifies ambiguous
outcomes; directory-flush failure verifies that the installed save is retained.

First-person CPU tests cover yaw-relative/normalized motion, configurable sprint,
dt-independent mouse look, pitch limits, input masks/edge consumption, jump/grace,
walls/ceilings/fall limits, camera interpolation and teleport/reconfiguration.
Meadow retains its stone landing, contact, checkpoint and pickup regressions.

Native rendering is separate and requires a display/OpenGL context:

```sh
scripts/native_smoke.sh
RAYENGINE_BACKEND=wayland scripts/native_smoke.sh
# On headless Linux with Xvfb installed:
LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a scripts/native_smoke.sh
```

The native renderer probe checks texture cache/unload behavior, model loading, 2D and 3D
pixel results, UI scaling, portrait resizing and an absolute screenshot path.
The generated mesh probe also verifies indexed/unindexed uploads, replacement,
transforms, tint, failure preserving the old geometry, stale handles, slot reuse,
and resource teardown after both normal exit and initialization failure.
On Linux the GPU failure probe rejects each vertex/index buffer allocation in
turn, checks partial resource cleanup, and verifies the old mesh still renders
after failed replacement. Its test-only dispatch hook is restored before drawing.
The material probe checks textured meshes/models, opaque/cutout/blended pixels,
depth writes, primitive ordering, shader compile/type failures, parameter defaults
and overrides, shared resource lifetime, stale dependencies, and render-state
restoration.
The upload probe sends worker-generated meshes through stale, count, byte and
failure handling, and checks actual old/new geometry pixels.
The UI probe checks cursor capture/release calls and changing game policy,
button state pixels, icons and stale texture handles in wide/portrait/Expand
layouts. CPU tests cover clicks, keyboard focus, drag transitions, resize mapping,
focus-loss resets through paused frames, and explicit gameplay masks.
The diagnostic probe verifies mixed 2D/3D/UI submissions, stale handles,
replacement/unload byte accounting, sampled peaks, phase totals and JSON schema.
The script also runs both demos and the menu example at wide and portrait dimensions and saves PNGs
under `artifacts/smoke/`. Frozen gameplay fixtures check that Arena's shadow is
on an upper platform and Meadow's first-person view stays above the first stone;
their screenshots are saved under `artifacts/regressions/`.
Graphics probes are intentionally ignored by normal
`cargo test`; the smoke script invokes them explicitly. Software OpenGL tests
exercise the native rendering path but do not measure a physical GPU.
The standalone `first_person` example is also rendered at wide and portrait sizes.

Benchmarks use Criterion and stable workload IDs:

```sh
scripts/benchmark.sh save before-change
# Implement a change, keeping the benchmark workload unchanged.
scripts/benchmark.sh compare before-change
```

The suite measures dense component updates (1K, 10K and 100K entities), flat and
parented transform propagation, 2D/3D character collision (1 and 100 boxes),
viewport/pointer/DPI math, fixed timing, 64 actions, timers, event queues,
mesh validation (1K/10K triangles, positions-only or indexed with attributes), and
scripted 240-tick arena/meadow simulation batches. Gameplay fixture creation
and destruction are outside the measured region; every sample starts from
the same state. **These are CPU workloads; no window or GPU is initialized.**
They do not measure actual rendering, asset upload, or an arbitrary game's FPS.

`scripts/benchmark.sh save first-person-v1 first_person` measures one helper tick
with 1/100 static boxes and interpolated camera construction. Each tick copies
the same settled fixture; initialization/fixture allocation is outside timing.
The existing `gameplay/meadow_scripted_240_ticks` ID and script remain unchanged
so the helper extraction can be compared against the earlier demo implementation:

```sh
# Before the extraction:
scripts/benchmark.sh save first-person-before-v1 meadow_scripted
# Afterward, on the same machine/toolchain with the same fixtures:
scripts/benchmark.sh compare first-person-before-v1 meadow_scripted
```

The spatial suite measures indexed and linear 2D/3D rays, proximity and camera
visibility queries, and full index rebuilds at 128, 4,096 and 32,768 colliders.
Use `scripts/benchmark.sh save spatial-v1 spatial_` to save only these workloads
with the normal provenance/export workflow. Rebuild cases measure the complete
snapshot update; query cases reuse a built index and visitor callbacks do not
allocate output vectors.

`scripts/benchmark.sh save background-v1 jobs_` measures idle polling, full-queue
rejection, scheduling/receipt latency and an eight-job batch. Synchronous payload
generation is the reference. Payload sizes are 0, 1 KiB and 64 KiB; worker setup
is outside measurement, while payload allocation/drop and scheduling are inside.

`scripts/benchmark.sh save ui-v1 ui_` measures UI hover, paired click updates,
keyboard navigation and three-update drags with 1/32 regions, plus eight-action
masked/unmasked routing. Regions and reusable response storage are prepared
outside measurement; updates use current reference-unit bounds.

`scripts/benchmark.sh save saves-v1 save_container` measures new/reused-buffer
encoding and borrowed decoding at 1 KiB, 64 KiB and 1 MiB. These include the
container CRC; new encoding allocates/drops, while the reusable case warms its
capacity before measurement. Payload fixtures are prepared outside measurement.
Game codecs are excluded because serialization is game-defined.

Filesystem work is a separate opt-in workload with the same export workflow:

```sh
RAYENGINE_SAVE_IO_BENCH=1 scripts/benchmark.sh save saves-io-v1 save_file
RAYENGINE_SAVE_IO_BENCH=1 scripts/benchmark.sh compare saves-io-v1 save_file
# Optional: choose a particular existing device/filesystem for the fixtures.
RAYENGINE_SAVE_BENCH_DIR=/path/on/device RAYENGINE_SAVE_IO_BENCH=1 \
  scripts/benchmark.sh save saves-device-v1 save_file
```

`save_file` measures complete replacement in Atomic and (on Linux) Durable
modes, plus repeated loads at 64 KiB and 1 MiB. Writes include checksum,
temporary-file creation, write, close, rename and, for Durable, both fsync calls.
Loads allocate/validate the complete payload from a warm OS page cache; this is
not a cold-storage benchmark. Unique fixture setup and cleanup are outside
measurement. I/O cases use 10 samples, 250 ms warm-up and 1 second measurement.
They write repeatedly and can be slower on storage with high flush latency.
The default fixture base is `artifacts/benchmarks/save-io-fixtures`; each run
removes only its owned subdirectory. Normal CPU runs perform no save-file I/O.
Metadata records the base path, Linux filesystem type and I/O settings. Compare
on the same device, mount options, cache/load conditions and filesystem; a tmpfs
run does not establish disk durability latency.

Native draw submission has a separate opt-in suite requiring a display:

```sh
scripts/render_benchmark.sh save materials-v1
scripts/render_benchmark.sh compare materials-v1
# Select native Wayland instead:
RAYENGINE_BACKEND=wayland scripts/render_benchmark.sh save wayland-materials-v1
scripts/render_benchmark.sh save uploads-v1 mesh_upload
scripts/render_benchmark.sh save ui-draw-v1 ui_draw
scripts/render_benchmark.sh save diagnostics-v1 diagnostics_
```

`draw_submission/lit_100` compares basic lighting with `opaque_100` using the
same normal-bearing quad, opaque alpha, identity matrix, and white tint/texture.
The lit case evaluates ambient, one directional light, and four points. Light
setup and uniform lookup happen outside measurement; draws reuse the validated
configuration. See [basic lighting](crate::guides::lighting) for the runnable
comparison demo and native pixel probe.

Stable `draw_submission` cases submit 100 indexed quads through the existing
default path and opaque, textured, cutout, parameterized, and blended materials.
The target is 64x64 with vsync disabled. These timings include CPU work, OpenGL
submission and driver stalls; they do not use GPU timers or measure game FPS.
Keep the same GPU, driver, backend and display conditions when comparing. The
script records backend/target settings and `glxinfo -B` when available on X11;
set `RAYENGINE_RENDERER_INFO` to add renderer details. Native exports use the
same snapshot schema and comparator as the CPU suite. Normal benchmark runs
do not initialize this native suite unless `RAYENGINE_RENDER_BENCH=1` is set.
`mesh_upload` cases compare direct and budgeted complete replacement for one
and 1,024 triangles. Queue setup and CPU data cloning are outside the budgeted
measured region; draining includes validation, GPU allocation, callbacks and
staged-data release. These are CPU/driver wall times, without GPU timer queries.
`ui_draw` submits 32 buttons with labels or 32 texture icons into the same fixed
64x64 native target. Layout and interaction preparation are outside measurement;
button response lookup, text measurement and draw submission are included.
Diagnostic overhead workloads compare the same 100 mesh submissions with
counting disabled/enabled, plus resource sampling and optional timer costs.
See [runtime diagnostics](crate::guides::diagnostics) for report coverage.

Criterion baselines live in `target/criterion` (or `CARGO_TARGET_DIR/criterion`).
Every script run exports portable results, samples and metadata under
`artifacts/benchmarks/BASELINE/`. Metadata records revision, dirty files,
toolchain, CPU, operating system and compiler flags. Keep these snapshots when
cleaning the build directory. `save` with the same name replaces Criterion's
active baseline but keeps the timestamped export.

Compare exported snapshots without recompiling:

```sh
python3 scripts/compare_benchmarks.py BEFORE_DIRECTORY AFTER_DIRECTORY
python3 scripts/compare_benchmarks.py BEFORE_DIRECTORY AFTER_DIRECTORY --json
# Optional mean-time regression budget; exits nonzero if exceeded:
python3 scripts/compare_benchmarks.py BEFORE_DIRECTORY AFTER_DIRECTORY --max-regression 10
```

The comparator reports differences in machine/toolchain conditions and missing
workloads. Its threshold checks mean estimates, not statistical significance;
consult Criterion's confidence intervals and change analysis before interpreting
small differences. Shared CI runners are useful for correctness and harness
compilation, but measurements are more comparable on the same idle machine.

To compare commits, use the same Rust version, release flags, benchmark harness,
input fixtures and `CARGO_TARGET_DIR`. Separate Git worktrees let you run an older
implementation without disturbing current work. New workloads need their own
baseline; strict Criterion comparison deliberately fails for missing cases.

The engine uses dense storage and immediate geometry drawing, with no SDK
command-buffer allocation. Transform propagation reuses its scratch storage and
has a dense fast path when there are no parents. Input slots are allocated during
setup. Asset loading, hierarchy edits, formatted strings and raylib's own wrappers
can allocate; avoid calling them blindly in hot loops. Measure the actual game
before adding parallelism, a spatial index or more rendering infrastructure.

## Optional voxel workloads

The CPU snapshot script also runs `rayengine-voxel`'s stable storage, access,
edit, traversal, snapshot and chunk meshing workloads. Filter with `voxel_`:

```sh
scripts/benchmark.sh save voxel-meshing-v1 voxel_
scripts/benchmark.sh compare voxel-meshing-v1 voxel_
scripts/render_benchmark.sh save voxel-render-v1 voxel_
scripts/render_benchmark.sh compare voxel-render-v1 voxel_
```

New mesh IDs need a new baseline; existing storage/query IDs and fixtures retain
their definitions. CPU generation compares culled and greedy modes on solid,
stepped terrain, checkerboard, mixed-tile and cutout chunks. Native upload/draw
cases use the same solid/terrain/checkerboard/cutout geometry, a 64×64 target,
and explicit resource byte reports. Greedy merging preserves tile repetition;
fragmented surfaces can cost more CPU without reducing geometry. Check both
counts and timings rather than assuming merging always improves performance.
The [plugin guide](https://github.com/nayupls1/rayengine/blob/master/plugins/voxel/README.md)
records fixtures and timing boundaries. CPU cases initialize no renderer or
save I/O; native cases include driver work/stalls and are not GPU timers or FPS.
