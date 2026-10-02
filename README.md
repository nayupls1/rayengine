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

Play the seeded first-person Minecraft voxel demo:

```sh
cargo run -p rayengine-minecraft --bin terrain -- --seed 42 --chunk=-1,2,0
cargo run --locked --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42 --save examples/minecraft/local-saves/world.save
```

The demo streams hills, caves, trees and ores with original fallback textures.
WASD/mouse moves and looks; Space jumps, held LMB mines, RMB places one owned
block, and 1–9 select hotbar slots. E/Escape opens inventory and crafting; F10
quits. Start with an empty inventory and gather logs to craft your first tools.
Collision and reach queries use the resident edited world. Extract local textures once with
`python3 scripts/import_minecraft_textures.py /path/to/client.jar`, then add
`--textures examples/minecraft/local-assets/minecraft` to load the ordinary PNGs.
The game has no archive dependency; `--bin textures` inspects a PNG folder as JSON.
Its game-owned recipe and CPU fixtures are documented in
[the terrain guide](examples/minecraft/README.md).
Add `--save examples/minecraft/local-saves/world.save` to retain world/player
progress. Autosave, F5 and graceful exit checkpoint the selected slot; reopening
uses its recorded seed.

Create a game:

```sh
cargo run -p rayengine-cli -- doctor
cargo run -p rayengine-cli -- new ../my-game --kind 2d --sdk-path "$PWD/crates/rayengine"
# Or --kind 3d
cargo run --manifest-path ../my-game/Cargo.toml
```

The CLI supports `new`, `new-plugin`, `info`, `check`, `build`, `run`, and `doctor`. Add
`--json` for a versioned result, structured errors and preserved Cargo diagnostics.
Scaffolds use the matching crates.io SDK version by default. Once 0.0.1 is
published, install with `cargo install rayengine-cli --version 0.0.1 --locked`
and create a project with `rayengine new my-game --kind 2d`. Pass
`--sdk-path /path/to/rayengine/crates/rayengine` for local engine development.
See the [release workflow and checklist](docs/crates_io_release.md).

Optional extensions live under `plugins/`; games select them through Cargo.
Try `cargo run -p rayengine-beacons --example composition`, or create a library
with `cargo run -p rayengine-cli -- new-plugin ../my-game/plugins/my-plugin --sdk-path "$PWD/crates/rayengine"`.
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
- [Voxel storage, generation interfaces, meshing and streaming](plugins/voxel/README.md)
- [Minecraft terrain, survival, crafting, respawn and world saves](examples/minecraft/README.md)
- [Minecraft 0.0.1 release, Linux bundle and complete validation](docs/minecraft_release.md)
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
The game owns optional voxel streaming. There is no rigid-body solver, advanced lighting,
navigation, networking or editor. Linux is the tested target; Windows/macOS are
optional, and browser/mobile are outside the current scope.

The [interactive agent testing protocol](https://github.com/nayupls1/rayengine/issues/1)
is tracked separately and deferred beyond 0.0.1.

Basic ambient, directional, and point lighting is opt-in with `Shading::Lit`.
See the [lighting guide](https://docs.rs/rayengine/latest/rayengine/guides/lighting/)
and run `cargo run -p rayengine --example lighting` for a generated/imported
lit/unlit comparison with adjustable lights.
