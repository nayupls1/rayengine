//! Run with `cargo run -p rayengine-beacons --example composition`.
use rayengine::prelude::*;
use rayengine_beacons::{Beacon, BeaconWorld};

const PAUSE: Action = Action(0);

struct Demo {
    world: BeaconWorld,
    blue: Beacon,
    orange: Beacon,
    paused: bool,
}
impl Demo {
    fn new() -> Result<Self, Error> {
        Ok(Self {
            world: BeaconWorld::default(),
            blue: Beacon::new(Vec3::new(-2.0, 1.0, 0.0), Color::SKYBLUE, 1.5)?,
            orange: Beacon::new(Vec3::new(2.0, 1.0, 0.0), Color::ORANGE, -1.0)?,
            paused: false,
        })
    }
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new().bind(PAUSE, KeyboardKey::KEY_SPACE)
    }
    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.blue.init(&mut self.world, context)?;
        self.orange.init(&mut self.world, context)?;
        Ok(())
    }
    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        // The game owns input and decides whether either plugin advances.
        if context.input.pressed(PAUSE) {
            self.paused = !self.paused;
        }
        if !self.paused {
            self.blue.fixed_update(&mut self.world, context);
            self.orange.fixed_update(&mut self.world, context);
        } else {
            // Collapse interpolation history so paused render frames stay still.
            self.blue.step(&mut self.world, 0.0);
            self.orange.step(&mut self.world, 0.0);
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(18, 29, 48, 255));
        frame.world_3d(self.world.camera, |canvas| {
            canvas.cube(
                Aabb3::from_center(Vec3::new(0.0, -0.25, 0.0), Vec3::new(10.0, 0.5, 6.0)),
                Color::DARKGREEN,
            );
        });
        // Explicit plugin ordering; the game draws its HUD last.
        self.blue.draw(&self.world, frame);
        self.orange.draw(&self.world, frame);
        frame.ui(|ui| {
            ui.text(
                "Two Cargo plugin instances",
                Vec2::splat(20.0),
                24.0,
                Color::WHITE,
            );
            ui.text(
                if self.paused {
                    "SPACE resume  ESC exit"
                } else {
                    "SPACE pause  ESC exit"
                },
                Vec2::new(20.0, 55.0),
                18.0,
                Color::LIGHTGRAY,
            );
        });
    }
}
fn main() -> Result<(), Error> {
    let mut config = Config::new("Plugin composition");
    config.audio = false;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Demo::new()?)?;
    Ok(())
}
