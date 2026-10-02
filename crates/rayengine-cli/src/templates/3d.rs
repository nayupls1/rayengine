use rayengine::prelude::*;

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const FORWARD: Action = Action(2);
const BACK: Action = Action(3);
const JUMP: Action = Action(4);

struct MyGame {
    body: Body3D,
    previous: Vec3,
    floor: Aabb3,
}

impl MyGame {
    fn new() -> Self {
        let position = Vec3::new(0.0, 2.0, 0.0);
        Self {
            body: Body3D::new(position, Vec3::new(0.8, 1.8, 0.8)),
            previous: position,
            floor: Aabb3::from_center(Vec3::new(0.0, -1.0, 0.0), Vec3::new(40.0, 2.0, 40.0)),
        }
    }
}

impl Game for MyGame {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(FORWARD, KeyboardKey::KEY_W)
            .bind(BACK, KeyboardKey::KEY_S)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.previous = self.body.position;
        let axis = Vec2::new(
            context.input.axis(LEFT, RIGHT),
            context.input.axis(FORWARD, BACK),
        )
        .clamp_length_max(1.0);
        self.body.velocity.x = axis.x * 6.0;
        self.body.velocity.z = axis.y * 6.0;
        if context.input.pressed(JUMP) && self.body.grounded {
            self.body.velocity.y = 10.0;
        }
        self.body.velocity.y -= 26.0 * context.tick.dt;
        self.body.move_and_slide(context.tick.dt, &[self.floor]);
        if self.body.position.y < -20.0 {
            self.body.position = Vec3::new(0.0, 2.0, 0.0);
            self.previous = self.body.position;
            self.body.velocity = Vec3::ZERO;
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(208, 231, 237, 255));
        let position = self.previous.lerp(self.body.position, frame.alpha);
        frame.world_3d(
            Camera3D {
                target: position,
                position: position + Vec3::new(8.0, 6.0, 8.0),
                ..Camera3D::default()
            },
            |canvas| {
                canvas.cube(self.floor, Color::new(126, 169, 114, 255));
                canvas.cube(
                    Aabb3::from_center(position, self.body.half_size * 2.0),
                    Color::ORANGE,
                );
            },
        );
        frame.ui(|ui| {
            ui.text("MY 3D GAME", Vec2::new(24.0, 24.0), 30.0, Color::DARKGREEN);
            ui.text(
                "WASD move   SPACE jump   ESC exit",
                Vec2::new(24.0, 65.0),
                16.0,
                Color::DARKGREEN,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let manifest = std::env::var_os("RAYENGINE_MANIFEST")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let profile = std::env::var("RAYENGINE_PROFILE").ok();
    let config = Config::new("My 3D Game").with_optional_project(manifest, profile.as_deref())?;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(MyGame::new())?;
    Ok(())
}
