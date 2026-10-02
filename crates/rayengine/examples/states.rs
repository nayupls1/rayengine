//! Title -> gameplay -> pause -> resume, with exclusive mesh teardown.
//! Run: `cargo run -p rayengine --example states`.
use rayengine::core::mesh::MeshData;
use rayengine::prelude::*;

const SELECT: Action = Action(0);
const PAUSE: Action = Action(1);
const TITLE: Action = Action(2);
const LEFT: Action = Action(3);
const RIGHT: Action = Action(4);

fn request(commands: &mut StateCommands, transition: Transition) {
    // Each callback in this example makes at most one request.
    assert!(commands.request(transition).is_ok());
}

struct Title;
impl State for Title {
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        if ctx.input.pressed(SELECT) {
            request(commands, Transition::Replace(Box::new(Play::default())));
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>, _: &mut StateCommands) {
        frame.clear(Color::new(18, 29, 48, 255));
        frame.ui(|ui| {
            ui.text("STATE STACK", Vec2::new(60.0, 80.0), 40.0, Color::WHITE);
            ui.text("Enter: start", Vec2::new(60.0, 150.0), 24.0, Color::SKYBLUE);
        });
    }
}

#[derive(Default)]
struct Play {
    mesh: Option<MeshId>,
    x: f32,
    previous_x: f32,
    seconds: f32,
}
impl State for Play {
    fn enter(
        &mut self,
        ctx: &mut InitContext<'_, '_>,
        owned: &mut StateResources,
    ) -> Result<(), Error> {
        // A generated mesh is unique to this play session. Cached/shared handles
        // would remain game-owned instead of being registered here.
        let data = MeshData::new(vec![
            Vec3::new(-0.8, 0.0, 0.0),
            Vec3::new(0.8, 0.0, 0.0),
            Vec3::new(0.0, 1.6, 0.0),
        ]);
        self.mesh = Some(owned.own_mesh(ctx.mesh(&data)?));
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        if ctx.input.pressed(PAUSE) {
            request(commands, Transition::Push(Box::new(Pause)));
            return;
        }
        self.previous_x = self.x;
        self.x = (self.x + ctx.input.axis(LEFT, RIGHT) * ctx.tick.dt * 3.0).clamp(-4.0, 4.0);
        self.seconds += ctx.tick.dt;
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>, _: &mut StateCommands) {
        frame.clear(Color::new(18, 29, 48, 255));
        let x = self.previous_x + (self.x - self.previous_x) * frame.alpha;
        frame.world_3d(
            Camera3D {
                position: Vec3::new(0.0, 3.0, 9.0),
                target: Vec3::ZERO,
                ..Camera3D::default()
            },
            |world| {
                world.mesh(
                    self.mesh.unwrap(),
                    Transform3D::at(Vec3::new(x, 0.0, 0.0)),
                    Color::SKYBLUE,
                );
            },
        );
        let timer = format!("Simulation: {:.1}s", self.seconds);
        frame.ui(|ui| {
            ui.text(&timer, Vec2::splat(24.0), 24.0, Color::WHITE);
            ui.text(
                "A/D: move   Esc: pause",
                Vec2::new(24.0, 60.0),
                20.0,
                Color::WHITE,
            );
        });
    }
}

struct Pause;
impl State for Pause {
    fn policy(&self) -> StatePolicy {
        StatePolicy {
            draw_below: true,
            ..StatePolicy::default()
        }
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        if ctx.input.pressed(TITLE) {
            request(commands, Transition::Reset(Box::new(Title)));
        } else if ctx.input.pressed(PAUSE) || ctx.input.pressed(SELECT) {
            request(commands, Transition::Pop);
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>, _: &mut StateCommands) {
        let size = frame.viewport.logical_size;
        frame.ui(|ui| {
            ui.rectangle(
                Aabb2 {
                    min: Vec2::ZERO,
                    max: size,
                },
                Color::new(0, 0, 0, 170),
            );
            ui.text("PAUSED", Vec2::new(60.0, 140.0), 40.0, Color::WHITE);
            ui.text(
                "Enter/Esc: resume   T: title",
                Vec2::new(60.0, 200.0),
                24.0,
                Color::WHITE,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let mut config = Config::new("rayengine / States");
    config.exit_key = None;
    let bindings = Bindings::new()
        .bind(SELECT, KeyboardKey::KEY_ENTER)
        .bind(PAUSE, KeyboardKey::KEY_ESCAPE)
        .bind(TITLE, KeyboardKey::KEY_T)
        .bind(LEFT, KeyboardKey::KEY_A)
        .bind(RIGHT, KeyboardKey::KEY_D);
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(StateStack::new(Title, bindings))?;
    Ok(())
}
