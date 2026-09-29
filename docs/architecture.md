# rayengine 0.0.1

Linux-first Rust SDK and CLI over raylib, with no editor. Windows and macOS
are optional targets; browser and mobile are outside the current scope.

## Crate boundaries

- `rayengine-core`: CPU-only entities/components, transforms and hierarchy,
  fixed simulation timing, action input, viewport math, cameras, collision,
  UI layout, and validated CPU mesh data. No window, audio device, C toolchain,
  or GPU required.
- `rayengine`: raylib runtime, rendering and asset ownership. One shared
  lifecycle for 2D and 3D. Rendering stays on raylib's owning thread.
- `rayengine-cli`: create, inspect and check ordinary Cargo game projects.
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

## First release

The first release includes basic collision queries and character movement,
not a rigid-body physics engine. Demos use geometric art so they need no
external assets. Texture/model/audio and generated mesh ownership are part of
the SDK. Generated mesh upload/replacement occurs on the render thread;
CPU geometry can be produced independently. Materials supply shared shaders,
textures, cached typed uniforms, and explicit opaque/cutout/blended policies for
generated meshes and imported models. Game code orders transparent draws;
asynchronous upload scheduling remains separate work.

Tests cover behavior without a display; rendering smoke checks exercise raylib
separately. Benchmarks retain named baselines and record toolchain and machine
metadata. A benchmark result must identify whether it measures CPU systems
or rendering: a fast CPU benchmark does not establish GPU performance.

The interactive agent testing protocol (input → image/state → next input) is
deferred. CLI JSON diagnostics are tool output, not a game automation protocol.

Public API reference and guides are built by `cargo doc`. Examples and
documentation tests are checked alongside implementation changes.
