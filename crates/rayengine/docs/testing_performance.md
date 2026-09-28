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
The script also runs both demos at wide and portrait dimensions and saves PNGs
under `artifacts/smoke/`. Frozen gameplay fixtures check that Arena's shadow is
on an upper platform and Meadow's first-person view stays above the first stone;
their screenshots are saved under `artifacts/regressions/`.
Graphics probes are intentionally ignored by normal
`cargo test`; the smoke script invokes them explicitly. Software OpenGL tests
exercise the native rendering path but do not measure a physical GPU.

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
