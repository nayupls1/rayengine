# Issue 45 validation evidence

The basic lighting implementation was checked locally with Rust 1.98.1, matching
the CI toolchain, and built with the minimum supported Rust 1.89.0.

Passed commands:

```sh
RUSTUP_TOOLCHAIN=1.98.1 scripts/check.sh
cargo +1.89.0 check --locked --workspace --all-targets --features rayengine-voxel/render,rayengine-minecraft/render
LIBGL_ALWAYS_SOFTWARE=1 scripts/native_smoke.sh
RUSTUP_TOOLCHAIN=1.98.1 LIBGL_ALWAYS_SOFTWARE=1 __GLX_VENDOR_LIBRARY_NAME=mesa cargo test --locked -p rayengine native_lighting -- --ignored --test-threads=1
RUSTUP_TOOLCHAIN=1.98.1 LIBGL_ALWAYS_SOFTWARE=1 __GLX_VENDOR_LIBRARY_NAME=mesa scripts/render_benchmark.sh save lighting-45-mesa 'draw_submission/(opaque|lit)_100'
```

`check.sh` checks formatting, strict Clippy, workspace CPU tests, doctests (including
the new lighting guide/example), and documentation with warnings denied. The
full serial native suite and the lighting demo passed on NVIDIA GeForce RTX 4070,
OpenGL 4.6.0 / NVIDIA 610.57.04, through Xwayland. NVIDIA GLX ignored the initial
software-rendering request; the Mesa-specific probe and benchmark explicitly
selected the Mesa vendor library and used llvmpipe (LLVM 22.1.8, 256 bits), Mesa
26.2.2. The benchmark script records actual renderer information where glxinfo or
glewinfo is available, together with relevant GL environment variables.

The fixture submits the same indexed four-vertex, normal-bearing quad 100 times
per iteration, using identity transforms, default white texture/tint, opaque
alpha, a 64×64 render target, and vsync off. The lit fixture has RGB 0.2 ambient,
one RGB 0.5 directional light, and four RGB 0.1 point lights. Light setup and
uniform lookup are outside the timed draws. Each case warms up for 0.5 seconds,
collects 30 samples over about 2 seconds, and includes driver stalls.

| Submission workload | Mean per 100 draws | 95% mean confidence interval |
| --- | ---: | ---: |
| Unlit `opaque_100` | 170.38 µs | 162.09–182.85 µs |
| Lit `lit_100` | 182.85 µs | 179.07–186.89 µs |

These estimates come from the same run on a shared desktop. The intervals overlap;
they are a reproducible submission comparison, not a regression budget or FPS/GPU
cost claim. Raw mean estimates, environment metadata, exact implementation commit,
and fixture settings are checked in at
[benchmarks/lighting-45-mesa.json](benchmarks/lighting-45-mesa.json). Full Criterion
samples remain in the local `artifacts/benchmarks/lighting-45-mesa/` snapshot;
the benchmark script exports equivalent samples and metadata for future runs.
