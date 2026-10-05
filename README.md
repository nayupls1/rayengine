# rayengine

A Linux-first Rust game SDK and CLI built on raylib. **Version 0.0.3.**
Games are ordinary Cargo projects, with a shared lifecycle and conventions for
2D and 3D. No GUI editor or website toolchain is required.

The SDK includes dense typed entities/components, optional scene parenting,
fixed simulation updates, action input, timers, typed events, interpolated rendering, fitted cameras,
high-DPI viewports, anchored UI, swept character collision, arcade physics, 2D/3D ray selection,
proximity and camera-visibility queries, and cached
texture/model/sound handles, generated meshes, and materials with typed shader
parameters and explicit alpha policies. Raylib is available directly for specialized work.

Optional project descriptions in `rayengine.toml` keep game assets, runtime
defaults, named profiles and game/plugin settings separate from Cargo builds.
New game scaffolds include one. See the [manifest guide](crates/rayengine/docs/project_manifest.md)
for the versioned schema, path resolution and overrides.

Try the games:

```sh
cargo run --release -p rayengine-demos --bin arena
cargo run --release -p rayengine-demos --bin meadow
cargo run --release -p rayengine-demos --bin dungeon
```

**Arena** is a compact platform fighter: A/D or arrows to move, Space/W to
double jump, S to drop through the current upper platform, left click to strike
toward the mouse (left/right), J to strike in the facing direction, T to toggle
the opponent AI, and R to reset. Damage increases knockback, and crossing the
blast zone awards a knockout.

**Meadow** is a first-person 3D exploration platformer: mouse to look around,
WASD to move, Space to jump, Shift to sprint, and R to return to your checkpoint.
Q/E also turn left/right. Escape exits; switching away releases the captured cursor.
Explore the trails, climb the stone course, and collect golden orbs. All three games
use original geometric/pixel art and need no downloaded assets.

**Embervault** is a six-room top-down action dungeon: WASD to move, mouse/J to
strike, Space to dash, E to use healing shrines, Tab for the field journal, and
Escape to pause. Defeat watchers, push a block onto a switch, and recover the
ember from the final Warden. Supports gamepads, checkpoints, rebindable control
presets, audio sliders, and an optional CRT filter. See the
[game guide](examples/games/README.md) for controls, tests, and CLI packaging.

Play the seeded first-person Minecraft voxel demo:

```sh
cargo run -p rayengine-minecraft --bin terrain -- --seed 42 --chunk=-1,2,0
cargo run --locked --release -p rayengine-minecraft --features render --bin minecraft -- --seed 42 --save examples/minecraft/local-saves/world.save
```

The demo streams hills, caves, trees and ores with original fallback textures.
WASD/mouse moves and looks; Space jumps, held LMB mines, RMB places one owned
block, and 1–9 select hotbar slots. E opens inventory and crafting, Escape the
game menu and F3 the debug overlay; F10 quits. Start with an empty inventory and gather logs to craft your first tools.
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

The CLI supports `new`, `new-plugin`, `templates`, `info`, `check`, `build`, `run`,
`package`, `add`/`remove`, `watch`, `clean`, and `doctor`. Add
`--json` for a versioned result, structured errors and preserved Cargo diagnostics.
Scaffolds use the matching crates.io SDK version by default. Install the
published CLI with `cargo install rayengine-cli --version 0.0.3 --locked`
and create a project with `rayengine new my-game --kind 2d`. Pass
`--sdk-path /path/to/rayengine/crates/rayengine` for local engine development.
See the [release workflow and checklist](docs/crates_io_release.md).
The `topdown` and `platformer` templates use the repository-only tilemap plugin;
create them with `--template NAME --sdk-path /path/to/rayengine/crates/rayengine`.
Beacons, particles and tilemap remain repository-only in this release.

Optional extensions live under `plugins/`; games select them through Cargo.
Try `cargo run -p rayengine-beacons --example composition`, or create a library
with `cargo run -p rayengine-cli -- new-plugin ../my-game/plugins/my-plugin --sdk-path "$PWD/crates/rayengine"`.
The game owns plugin instances and calls their typed hooks explicitly.
`plugins/particles` adds bounded CPU particle effects with optional 2D/3D
rendering; try `cargo run -p rayengine-particles --features render --example effects`.

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
- [Optional scene switching and state stacks](crates/rayengine/docs/states.md)
- [Optional plugins and authoring](crates/rayengine/docs/plugins.md)
- [Voxel storage, generation interfaces, meshing and streaming](plugins/voxel/README.md)
- [Minecraft terrain, survival, crafting, respawn and world saves](examples/minecraft/README.md)
- [Minecraft 0.0.1 release, Linux bundle and complete validation](docs/minecraft_release.md)
- [Render quality, supersampling and anti-aliasing](crates/rayengine/docs/render_quality.md)
- [Responsive viewports, cameras and UI](crates/rayengine/docs/responsive.md)
- [Timing, input and character movement](crates/rayengine/docs/timing_input.md)
- [Paused and accelerated simulation](crates/rayengine/docs/simulation.md)
- [Arcade physics, layers, triggers and moving platforms](crates/rayengine/docs/physics.md)
- [Static directional hit geometry](crates/rayengine/docs/directional_hits.md)
- [Reusable first-person controller](crates/rayengine/docs/first_person.md)
- [Assets and ownership](crates/rayengine/docs/assets.md)
- [Streamed music, buses and fades](crates/rayengine/docs/audio.md)
- [Custom fonts, text measurement, and pixel text](crates/rayengine/docs/fonts.md)
- [Sprite sheets and CPU animation](crates/rayengine/docs/sprites.md)
- [Skeletal 3D character animation](crates/rayengine/docs/skeletal_animation.md)
- [Tweens, easing and screen shake](crates/rayengine/docs/tweens.md)
- [Generated meshes](crates/rayengine/docs/generated_meshes.md)
- [Materials and shaders](crates/rayengine/docs/materials.md)
- [Basic lighting](crates/rayengine/docs/lighting.md)
- [Layered tilemaps, level format and collision](plugins/tilemap/README.md)
- [Particle effects plugin](plugins/particles/README.md)
- [Versioned project manifests](crates/rayengine/docs/project_manifest.md)
- [Spatial queries](crates/rayengine/docs/spatial_queries.md)
- [Grid pathfinding](crates/rayengine/docs/pathfinding.md)
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

Try the title/gameplay/pause state stack example:

```sh
cargo run -p rayengine --example states
cargo run -p rayengine --example audio
```

Play Embervault, the original six-room dungeon showcase:
`cargo run -p rayengine-demos --bin dungeon`. It combines tilemaps, physics,
pathfinding, sprites, particles, tweens, menus, shaders, music and durable saves.
See [controls, save behavior and Linux packaging](examples/games/README.md).

In the state stack example, Enter starts a session. Escape pauses with the world visible; Enter/Escape
resumes, and T while paused returns to title and releases session resources.

Try the draggable menu, keyboard focus, and dynamic cursor capture example:

```sh
cargo run -p rayengine --example menu
```

Escape toggles the menu. Tab/Up/Down moves focus; Enter/Space selects. Drag the
header to move the panel. Gameplay receives explicitly masked input while the
menu is open.

Try analog controller movement and live binding settings:

```sh
cargo run -p rayengine --example controls
```

The controls menu switches movement keys, inverts look, adjusts sensitivity,
and explicitly saves/loads settings. Keyboard and controller sources share the
same movement axes. See [timing and input](crates/rayengine/docs/timing_input.md)
for normalization, fixed-tick sampling, routing, and game-owned persistence.

Try the original pixel-art sprite playground with idle, walking and one-shot
sword animations: `cargo run -p rayengine --example sprites`. A/D or arrows move;
Space swings at the golden orb, P pauses, and R restarts the current clip.

Watch an original skinned lamplighter walk between lanterns and wave to light
them: `cargo run -p rayengine --example character`. A/D take control, E waves
near a lantern, P pauses and R restarts. See
[skeletal animation](crates/rayengine/docs/skeletal_animation.md).

Try tweened world objects and UI: `cargo run -p rayengine --example tweens`.
Space slides a door, H flashes a training dummy and shakes the camera, and P
pauses the world tweens; each door completion slides in a UI toast.

Watch agents navigate around walls with grid A*, path smoothing and a shared
distance field: `cargo run -p rayengine --example pathfinding`. Click to move
the target; R resets. See [grid pathfinding](crates/rayengine/docs/pathfinding.md).

Benchmark snapshots include samples, revision, toolchain and machine metadata.
The CPU suite measures primitives and gameplay simulation; the opt-in native
suite measures draw submission wall time, including driver stalls. Neither is a
GPU timer or a game's FPS. Graphics probes check rendering separately.
Shared CI runs correctness checks, native render probes,
template checks, minimum-Rust checks and benchmark compilation.

Physics stays small and predictable: axis-aligned boxes, circles/spheres,
continuous translation, mass-weighted separation, layers, triggers and moving
platforms. The independent swept character helpers remain available. Rendering
includes geometric drawing, textures and model access.
The game owns optional voxel streaming. Navigation is limited to grid pathfinding.
There is no rigid-body solver, advanced lighting, navigation mesh, networking or editor.
Linux is the tested target; Windows/macOS are
optional, and browser/mobile are outside the current scope.

The [interactive agent testing protocol](https://github.com/nayupls1/rayengine/issues/1)
is tracked separately and deferred beyond 0.0.3.

Basic ambient, directional, and point lighting is opt-in with `Shading::Lit`.
See the [lighting guide](https://docs.rs/rayengine/latest/rayengine/guides/lighting/)
and run `cargo run -p rayengine --example lighting` for a generated/imported
lit/unlit comparison with adjustable lights.
