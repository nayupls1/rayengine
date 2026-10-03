# rayengine

A small Linux-first Rust game SDK over raylib. Games are ordinary Cargo projects
with a shared lifecycle for 2D and 3D, action input, fixed updates, fitted cameras,
logical UI with optional interaction and explicit asset ownership. Version **0.0.2**.

Start with [the quickstart](https://docs.rs/rayengine/latest/rayengine/guides/quickstart/index.html), then read
[game structure](https://docs.rs/rayengine/latest/rayengine/guides/game_structure/index.html) and
[scene switching and state stacks](https://docs.rs/rayengine/latest/rayengine/guides/states/index.html), and
[optional plugins](https://docs.rs/rayengine/latest/rayengine/guides/plugins/index.html). The remaining guides explain
[responsive viewports](https://docs.rs/rayengine/latest/rayengine/guides/responsive/index.html),
[timing and input](https://docs.rs/rayengine/latest/rayengine/guides/timing_input/index.html), [assets](https://docs.rs/rayengine/latest/rayengine/guides/assets/index.html),
[first-person movement](https://docs.rs/rayengine/latest/rayengine/guides/first_person/index.html),
[generated meshes](https://docs.rs/rayengine/latest/rayengine/guides/generated_meshes/index.html),
[materials and shaders](https://docs.rs/rayengine/latest/rayengine/guides/materials/index.html),
[spatial queries](https://docs.rs/rayengine/latest/rayengine/guides/spatial_queries/index.html),
[grid pathfinding](https://docs.rs/rayengine/latest/rayengine/guides/pathfinding/index.html),
[background work](https://docs.rs/rayengine/latest/rayengine/guides/background_work/index.html),
[interactive UI and input routing](https://docs.rs/rayengine/latest/rayengine/guides/interactive_ui/index.html),
[versioned saves](https://docs.rs/rayengine/latest/rayengine/guides/saves/index.html),
[runtime diagnostics](https://docs.rs/rayengine/latest/rayengine/guides/diagnostics/index.html),
[testing and performance](https://docs.rs/rayengine/latest/rayengine/guides/testing_performance/index.html), and the
[agent workflow](https://docs.rs/rayengine/latest/rayengine/guides/agent_workflow/index.html).

Optional `rayengine.toml` project descriptions share a CPU-only loader with the
CLI. See the [project manifest guide](https://docs.rs/rayengine/latest/rayengine/guides/project_manifest/)
for profiles, asset/font declarations, validation and configuration precedence.

`rayengine-core` contains display-independent components and math; access it
through [`core`](https://docs.rs/rayengine-core) or depend on it directly for simulations without raylib.
[`prelude`](https://docs.rs/rayengine/latest/rayengine/prelude/index.html) contains the normal game imports. [`raylib`](https://docs.rs/raylib) remains available for
specialized rendering and other lower-level features.

```no_run
use rayengine::prelude::*;

#[derive(Default)]
struct Hello;

impl Game for Hello {
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(18, 29, 48, 255));
        frame.world_2d(Camera2D::default(), |canvas| {
            canvas.circle(Vec2::ZERO, 40.0, Color::SKYBLUE);
        });
        frame.ui(|ui| ui.text("Hello, rayengine", Vec2::new(24.0, 24.0), 24.0, Color::WHITE));
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("Hello")).run(Hello)?;
    Ok(())
}
```

Custom outline fonts, matching label measurement, per-button font choice,
DPI-aware atlases, and explicit nearest-filtered pixel text are described in the
[font guide](https://docs.rs/rayengine/latest/rayengine/guides/fonts/index.html).
The repository includes a distributable two-font example (`--example fonts`).

The first release provides basic swept character movement and geometric drawing,
not rigid-body dynamics or an advanced 3D rendering pipeline. The interactive
input/image/state testing protocol is deferred. The CLI's JSON project diagnostics
are available now. Desktop Linux is the supported development target; Windows
and macOS are optional and not yet verified by this project's CI.

Basic ambient, directional, and point lighting is opt-in with `Shading::Lit`.
See the [lighting guide](https://docs.rs/rayengine/latest/rayengine/guides/lighting/)
and run `cargo run -p rayengine --example lighting` for a generated/imported
lit/unlit comparison with adjustable lights.
