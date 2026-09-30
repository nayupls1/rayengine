# rayengine

A small Linux-first Rust game SDK over raylib. Games are ordinary Cargo projects
with a shared lifecycle for 2D and 3D, action input, fixed updates, fitted cameras,
logical UI with optional interaction and explicit asset ownership. Version **0.0.1**.

Start with [the quickstart](crate::guides::quickstart), then read
[game structure](crate::guides::game_structure). The remaining guides explain
[responsive viewports](crate::guides::responsive),
[timing and input](crate::guides::timing_input), [assets](crate::guides::assets),
[generated meshes](crate::guides::generated_meshes),
[materials and shaders](crate::guides::materials),
[spatial queries](crate::guides::spatial_queries),
[background work](crate::guides::background_work),
[interactive UI and input routing](crate::guides::interactive_ui),
[testing and performance](crate::guides::testing_performance), and the
[agent workflow](crate::guides::agent_workflow).

`rayengine-core` contains display-independent components and math; access it
through [`core`] or depend on it directly for simulations without raylib.
[`prelude`] contains the normal game imports. [`raylib`] remains available for
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

The first release provides basic swept character movement and geometric drawing,
not rigid-body dynamics or an advanced 3D rendering pipeline. The interactive
input/image/state testing protocol is deferred. The CLI's JSON project diagnostics
are available now. Desktop Linux is the supported development target; Windows
and macOS are optional and not yet verified by this project's CI.
