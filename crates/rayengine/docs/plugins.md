# Optional Cargo plugins

A plugin is an ordinary Rust library dependency. [`crate::Plugin`] offers optional
`init`, `fixed_update`, `draw`, and explicit `unload` hooks over a typed shared
state. The game owns each instance and invokes hooks in the order it wants.
The engine adds no registry, automatic scheduling, allocation, or plugin dispatch
to the runtime loop. Games without plugins keep the same execution path.

Keep engine crates in `crates/`, repository plugins in `plugins/<name>/`, and
game content/rules in `examples/<game>/`. The included `plugins/beacons` package
is named `rayengine-beacons`; the engine does not depend on it. Third-party
plugins can live in any Cargo project and need no registration with rayengine.
Plugins can implement only these hooks, provide their own typed methods, or both.
A custom interface often makes sense for sharing one canvas or supplying masked
input. There is no mandatory plugin trait object or `Send` bound.

## Explicit composition

```no_run
use rayengine::prelude::*;

#[derive(Default)]
struct State { ticks: u64, observed: u64 }
struct Clock;
struct Observer;
impl Plugin<State> for Clock {
    fn fixed_update(&mut self, state: &mut State, _: &mut Update<'_, '_>) {
        state.ticks += 1;
    }
}
impl Plugin<State> for Observer {
    fn fixed_update(&mut self, state: &mut State, _: &mut Update<'_, '_>) {
        state.observed = state.ticks;
    }
}
struct MyGame { state: State, clock: Clock, observer: Observer }
impl Game for MyGame {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.clock.init(&mut self.state, ctx)?;
        self.observer.init(&mut self.state, ctx)?;
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.clock.fixed_update(&mut self.state, ctx);
        // Observer sees the result of Clock in this same tick.
        self.observer.fixed_update(&mut self.state, ctx);
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::BLACK);
        self.clock.draw(&self.state, frame);
        self.observer.draw(&self.state, frame);
        frame.ui(|ui| ui.text("Game HUD", Vec2::splat(20.0), 20.0, Color::WHITE));
    }
}
```

Use `Plugin<()>` for independent state, a game-defined struct for shared state,
or `Plugin<MyView<'_>>` for a temporary view borrowing only relevant game fields.
The trait accepts unsized state too. A plugin can implement several state
adapters; use qualified calls if type inference is ambiguous. Dependencies between
plugins remain normal Rust references/data and explicit call order. No plugin
may assume another is installed unless its constructor/API expresses that need.

The game controls bindings and cursor capture. Give plugins caller-selected action
IDs/configuration instead of claiming global numeric IDs. Input edges and motion
are consumed after the whole game's fixed callback; every plugin receives the
same sample if passed the same `Update`. For UI masking, pass an `InputView` to a
plugin-specific method or place the routed view in shared state. Plugins should
not change the runner's input-consumption or pause policy behind the game's back.

## Public extension surface

| Need | Public API and ownership |
| --- | --- |
| Entities/components | Game-owned `Scene` or `World` in shared state; custom typed components |
| Actions, timing, viewport | `Update` exposes `input`, `tick`, `viewport`, pointer/focus, and `quit()` |
| GPU/audio assets | `InitContext` loads resources; `Frame` uploads/replaces meshes and shaders; both expose assets |
| Runtime asset inspection | `Update.assets` is read-only; `Assets::resource_counts()` samples owned resources |
| Drawing | `Frame` world/UI passes; canvas `raw` and `Frame::with_raylib` expose specialized drawing |
| Native initialization | `InitContext.raylib` and `thread`; borrowed only during the call |
| Jobs and uploads | `core::jobs::JobPool` for CPU payloads; `MeshUploadQueue` and `UploadBudget` on the render thread |
| Saves | `rayengine::save` accepts game/plugin-defined opaque payloads and schema versions |
| Diagnostics | Existing opt-in runtime reports and frame submission counters; explicit plugin benchmarks |

Update assets are intentionally read-only: GPU uploads happen during init or
rendering. The contexts are borrowed, not stored or sent to workers. Workers own
CPU inputs/results and observe cancellation/revisions; they never call raylib.
GPU resources remain on the window's owning thread. Plugins can contain job
pools, queues, and their own status/events without requiring a new engine service
container. See the corresponding SDK guides for each primitive's guarantees.

## Resources, failures, and removal

`init` is fallible; propagate with `?` to abort startup. `fixed_update` and `draw`
match the game's infallible hooks. A plugin needing recoverable errors should
expose status/events or fallible custom methods; the game decides whether to
retry, disable it, display an error, or request exit.

Successful earlier initializations are not rolled back if a later plugin fails.
When the error escapes `Game::init`, the runner drops game/plugin fields before
SDK assets, audio, and the window. CPU jobs must stop/join in their owner's `Drop`;
`JobPool` already does this. Remaining SDK resources are released by `Assets`.
If you catch an init error and continue, follow that plugin's documented partial
initialization cleanup policy. Avoid committing shared state until admission or
resource creation succeeds when feasible.

Asset IDs are handles, not automatic resource owners. Dropping a plugin alone
does not remove its scene entities or unload its SDK assets. The optional
`unload(state, assets)` hook lets a game detach it explicitly while a live native
context exists. Call it on the owning thread during init or a game-chosen frame
preparation boundary, before drawing users of the removed state. It is not called
automatically at shutdown. Avoid unloading cached textures/shaders shared with
other plugins; agree ownership or let the run's asset collection retain them.
Document whether unload is idempotent and whether reinitialization is supported.

## Create and use a plugin

From this checkout:

```sh
cargo run -p rayengine-cli -- new-plugin ../my-game/plugins/my-plugin --name my-plugin
# Optional: --sdk-path /path/to/rayengine/crates/rayengine
cargo check --manifest-path ../my-game/plugins/my-plugin/Cargo.toml
```

The scaffold is a standalone Cargo library with `MyPlugin` and `PluginState`.
It never overwrites an existing destination or edits the consuming game's
manifest. It includes an empty `[workspace]` so it can be checked independently.
Generated games exclude the `plugins/` directory from their workspace, allowing nested
standalone plugins as dependencies. For an existing workspace, either exclude
the plugin's path in its root `[workspace].exclude`, or remove the plugin's
`[workspace]` table and add its path to the root `members`. Do this before adding
a nested plugin dependency; two overlapping workspace roots are invalid. See
[Cargo workspace rules](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-members-and-exclude-fields).

Add a dependency to the game's manifest:

```toml
[dependencies]
my-plugin = { path = "plugins/my-plugin" }
```

Then import `my_plugin::{MyPlugin, PluginState}`, store them in your game, and
call hooks as above. Normal Cargo aliases/features control which plugins compile;
there is no automatic folder scan. `--json new-plugin ...` returns the existing
versioned CLI result with `kind: "plugin"` and `src/lib.rs` in its file list.

For plugins in this repository, start from `plugins/beacons/Cargo.toml`, use
workspace package/dependency/lint settings, and explicitly add the directory to
root workspace members. Keep core CPU-only; a plugin with substantial simulation
can split its own CPU and SDK packages under its `plugins/<name>/` directory.
Depend on `rayengine-core` directly when no native SDK APIs are needed.

Run the working example and export all documentation:

```sh
cargo run -p rayengine-beacons --example composition
cargo doc --workspace --no-deps
# target/doc/rayengine_beacons/index.html includes the complete example
```

## Compatibility and author checklist

Use the same SDK source/version as the game, especially for path/git dependencies;
duplicate SDK copies have distinct Rust types. This SDK is pre-1.0: select the
Cargo requirement appropriate to the APIs used, and document tested versions,
features, MSRV, and platform support. There is no stable dynamic ABI or runtime
plugin loader. Plugin versions can evolve independently of the SDK.

Document configuration, required shared state, call order, action allocation,
input routing, thread restrictions, init/reinit/failure semantics, and resource
ownership/removal. Include a small playable composition, CPU tests that run
without a display, checked rustdoc, and serial native tests for GPU behavior.
Add stable workload IDs for expensive systems and use the existing metadata and
comparison scripts when evaluating changes. Keep default operation bounded and
avoid per-frame allocations in steady-state paths. A voxel plugin owns chunks,
meshing, and streaming; recipes and specific survival rules belong in the game.
