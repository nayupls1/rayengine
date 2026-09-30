# rayengine

A Linux-first Rust game SDK and CLI built on raylib. **Version 0.0.1.**
Games are ordinary Cargo projects, with a shared lifecycle and conventions for
2D and 3D. No GUI editor or website toolchain is required.

The SDK includes dense typed entities/components, optional scene parenting,
fixed simulation updates, action input, timers, typed events, interpolated rendering, fitted cameras,
high-DPI viewports, anchored UI, swept character collision, 2D/3D ray selection,
proximity and camera-visibility queries, and cached
texture/model/sound handles, generated meshes, and materials with typed shader
parameters and explicit alpha policies. Raylib is available directly for specialized work.

Try the games:

```sh
cargo run --release -p rayengine-demos --bin arena
cargo run --release -p rayengine-demos --bin meadow
```

**Arena** is a compact platform fighter: A/D or arrows to move, Space/W to
double jump, S to drop through the current upper platform, left click to strike
toward the mouse (left/right), J to strike in the facing direction, T to toggle
the opponent AI, and R to reset. Damage increases knockback, and crossing the
blast zone awards a knockout.

**Meadow** is a first-person 3D exploration platformer: mouse to look around,
WASD to move, Space to jump, Shift to sprint, and R to return to your checkpoint.
Q/E also turn left/right. Escape exits; switching away releases the captured cursor.
Explore the trails, climb the stone course, and collect golden orbs. Both games
use geometric art and need no downloaded assets.

Create a game:

```sh
cargo run -p rayengine-cli -- doctor
cargo run -p rayengine-cli -- new ../my-game --kind 2d
# Or --kind 3d
cargo run --manifest-path ../my-game/Cargo.toml
```

The CLI supports `new`, `new-plugin`, `info`, `check`, `build`, `run`, and `doctor`. Add
`--json` for a versioned result, structured errors and preserved Cargo diagnostics.
Scaffolds use a local SDK path, inferred from this checkout or supplied with
`--sdk-path /path/to/rayengine/crates/rayengine`; nothing is published to crates.io yet.

Optional extensions live under `plugins/`; games select them through Cargo.
Try `cargo run -p rayengine-beacons --example composition`, or create a library
with `cargo run -p rayengine-cli -- new-plugin ../my-game/plugins/my-plugin`.
The game owns plugin instances and calls their typed hooks explicitly.

Rust **1.89+**, CMake, a C compiler, libclang and Linux graphics/audio development
libraries are required. See [installation and quickstart](crates/rayengine/docs/quickstart.md).
The default backend is X11/XWayland; enable `--features rayengine/wayland` for
native Wayland when running the demos. The core crate has no native dependencies.

Read and export the docs:

```sh
cargo doc --workspace --no-deps
# Open target/doc/rayengine/index.html
```

Guides are plain Markdown included in rustdoc, with checked Rust examples:

- [Game structure and corresponding 2D/3D primitives](crates/rayengine/docs/game_structure.md)
- [Optional plugins and authoring](crates/rayengine/docs/plugins.md)
- [Responsive viewports, cameras and UI](crates/rayengine/docs/responsive.md)
- [Timing, input and character movement](crates/rayengine/docs/timing_input.md)
- [Reusable first-person controller](crates/rayengine/docs/first_person.md)
- [Assets and ownership](crates/rayengine/docs/assets.md)
- [Generated meshes](crates/rayengine/docs/generated_meshes.md)
- [Materials and shaders](crates/rayengine/docs/materials.md)
- [Spatial queries](crates/rayengine/docs/spatial_queries.md)
- [Background work and upload budgets](crates/rayengine/docs/background_work.md)
- [Interactive UI and input routing](crates/rayengine/docs/interactive_ui.md)
- [Versioned saves and reliable file replacement](crates/rayengine/docs/saves.md)
- [Runtime diagnostics](crates/rayengine/docs/diagnostics.md)
- [Tests and performance comparisons](crates/rayengine/docs/testing_performance.md)
- [Agent workflow and JSON contract](crates/rayengine/docs/agent_workflow.md)
- [Architecture](docs/architecture.md)

Validate changes and compare performance:

```sh
scripts/check.sh
python3 scripts/template_smoke.py
scripts/native_smoke.sh                  # requires a display/OpenGL context
scripts/benchmark.sh save before-change
scripts/benchmark.sh compare before-change
scripts/render_benchmark.sh save materials-v1 # optional native draw benchmark
```

Try the draggable menu, keyboard focus, and dynamic cursor capture example:

```sh
cargo run -p rayengine --example menu
```

Escape toggles the menu. Tab/Up/Down moves focus; Enter/Space selects. Drag the
header to move the panel. Gameplay receives explicitly masked input while the
menu is open.

Benchmark snapshots include samples, revision, toolchain and machine metadata.
The CPU suite measures primitives and gameplay simulation; the opt-in native
suite measures draw submission wall time, including driver stalls. Neither is a
GPU timer or a game's FPS. Graphics probes check rendering separately.
Shared CI runs correctness checks, native render probes,
template checks, minimum-Rust checks and benchmark compilation.

The first release keeps physics and rendering small: axis-aligned static
collision and character movement, geometric drawing, textures and model access.
There is no rigid-body solver, automatic world streaming, advanced lighting,
navigation, networking or editor. Linux is the tested target; Windows/macOS are
optional, and browser/mobile are outside the current scope.

The [interactive agent testing protocol](https://github.com/nayupls1/rayengine/issues/1)
is tracked separately and deferred beyond 0.0.1.
