# rayengine 0.0.1

Linux-first Rust SDK and CLI over raylib, with no editor. Windows and macOS
are optional targets; browser and mobile are outside the current scope.

## Crate boundaries

- `rayengine-core`: CPU-only entities/components, transforms and hierarchy,
  fixed simulation timing, action input, viewport math, cameras, collision,
  UI layout, validated CPU mesh data, and versioned save containers. No window, audio device, C toolchain,
  or GPU required.
- `rayengine`: raylib runtime, rendering and asset ownership. One shared
  lifecycle for 2D and 3D. Rendering stays on raylib's owning thread.
- `rayengine-cli`: create, inspect and check ordinary Cargo game projects.
- `plugins/<name>`: optional Cargo libraries selected by games; engine crates
  never depend on these. `plugins/beacons` demonstrates typed lifecycle hooks,
  generated meshes, and independently configured instances.
- `rayengine-demos`: a 2D arena fighter and a 3D exploration platformer.

## Design contract

Game code owns its state and custom components. `hecs` supplies dense typed
component storage; scene parenting is optional. `glam` supplies CPU math.
The engine controls input sampling, fixed updates and rendering, with explicit
hooks for game behavior and direct access to raylib when needed.

2D and 3D share lifecycle, entities, actions, timing, UI and asset management.
Their transforms, cameras, colliders and drawing types are distinct.

The default viewport fits a reference aspect ratio inside the window. The
visible game area and 3D vertical field of view remain stable through resizing;
bars absorb aspect changes. An expand policy is explicit and optional. UI and
pointer conversion use the same viewport. Rendering uses physical framebuffer
pixels while UI uses logical units. Pixel art can select integer scaling.

Simulation has a fixed timestep and bounded catch-up. Input edges survive
render frames without an update and are consumed once by a fixed update.
Games can interpolate between simulation states during rendering.

Optional `Plugin<State>` hooks borrow explicit shared state and the same public
init/update/frame contexts as games. Games own instances, action IDs, dependencies,
call order, failure policies, and removal. There is no registry or runtime loader,
and the runner performs no automatic plugin dispatch. CPU jobs remain plugin-owned;
GPU uploads/removal stay on the owning thread. Rustdoc describes the full public
extension surface and `rayengine new-plugin` creates a normal standalone library.

## First release

The first release includes basic collision queries and character movement,
not a rigid-body physics engine. Demos use geometric art so they need no
external assets. Texture/model/audio and generated mesh ownership are part of
the SDK. Generated mesh upload/replacement occurs on the render thread;
CPU geometry can be produced independently. The core also owns explicit 2D/3D
collider snapshots for ray, proximity, and camera-visibility queries, with no
renderer dependency. Materials supply shared shaders,
textures, cached typed uniforms, and explicit opaque/cutout/blended policies for
generated meshes and imported models. Game code orders transparent draws;
CPU jobs and mesh staging have explicit bounded queues. Upload budgets are
applied on the render thread; cancellation and game-defined revisions reject
obsolete work. Automatic world streaming remains game policy.

An optional CPU first-person controller combines action/mouse look, yaw-relative
movement, configurable jump/gravity/grace, Body3D collision and an interpolated
eye camera. Meadow uses it while retaining checkpoint, reset and pickup rules.
Games can pass direct tick input or action views masked by their UI.

UI interaction is an optional CPU state machine over game-owned regions and IDs.
Pointer capture, keyboard focus, clicks and drag events use reference units;
games explicitly mask actions/look motion before gameplay. The SDK draws buttons
and icons and updates cursor capture as menus change. Menu layout, drop targets
and inventory rules remain game-owned.

Persistence accepts bounded opaque payloads with separate engine/game versions
and corruption checks. File writes replace from unique siblings; Linux durable
writes flush both file and directory. Errors distinguish pre-replacement,
ambiguous rename and installed-but-unconfirmed durability outcomes. Games own
serialization, migration, locations, backups and recovery. File I/O is explicit
and blocking; games can schedule owned snapshots through the optional CPU jobs.

Tests cover behavior without a display; rendering smoke checks exercise raylib
separately. Benchmarks retain named baselines and record toolchain and machine
metadata. A benchmark result must identify whether it measures CPU systems
or rendering: a fast CPU benchmark does not establish GPU performance. Optional
save-file benchmarks record their filesystem and directory separately from
container CPU workloads.
Runtime diagnostics are optional constant-space wall-time summaries, SDK
submission counters and sampled owned resources. Disabled runs skip additional
timers/scans. Native benchmarks compare enabled/disabled counters while retaining
the existing workload IDs and provenance/export workflow. Counts are not driver
draw calls and payload byte estimates are not total VRAM.

The interactive agent testing protocol (input → image/state → next input) is
deferred. CLI JSON diagnostics are tool output, not a game automation protocol.

Public API reference and guides are built by `cargo doc`. Examples and
documentation tests are checked alongside implementation changes.
