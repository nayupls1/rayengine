use rayengine::prelude::*;

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const JUMP: Action = Action(2);

struct MyGame {
    body: Body2D,
    previous: Vec2,
    floor: Aabb2,
}

impl MyGame {
    fn new() -> Self {
        Self {
            body: Body2D::new(Vec2::ZERO, Vec2::new(32.0, 48.0)),
            previous: Vec2::ZERO,
            floor: Aabb2::from_center(Vec2::new(0.0, 180.0), Vec2::new(850.0, 40.0)),
        }
    }
}

impl Game for MyGame {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.previous = self.body.position;
        self.body.velocity.x = context.input.axis(LEFT, RIGHT) * 250.0;
        if context.input.pressed(JUMP) && self.body.grounded {
            self.body.velocity.y = -550.0;
        }
        self.body.velocity.y += 1400.0 * context.tick.dt;
        self.body.move_and_slide(context.tick.dt, &[self.floor]);
        if self.body.position.y > 500.0 {
            self.body.position = Vec2::ZERO;
            self.previous = Vec2::ZERO;
            self.body.velocity = Vec2::ZERO;
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(18, 29, 48, 255));
        let position = self.previous.lerp(self.body.position, frame.alpha);
        frame.world_2d(Camera2D::default(), |canvas| {
            canvas.rectangle(self.floor, Color::new(56, 78, 99, 255));
            canvas.rectangle(
                Aabb2::from_center(position, self.body.half_size * 2.0),
                Color::SKYBLUE,
            );
        });
        frame.ui(|ui| {
            ui.text("MY 2D GAME", Vec2::new(24.0, 24.0), 30.0, Color::WHITE);
            ui.text(
                "A/D move   SPACE jump   ESC exit",
                Vec2::new(24.0, 65.0),
                16.0,
                Color::LIGHTGRAY,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let manifest = std::env::var_os("RAYENGINE_MANIFEST")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let bundled = std::env::current_exe().ok().and_then(|path| {
                path.parent()?
                    .parent()
                    .map(|root| root.join("rayengine.toml"))
            });
            bundled
                .filter(|path| path.is_file())
                .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        });
    let profile = std::env::var("RAYENGINE_PROFILE").ok();
    let config = Config::new("My 2D Game").with_optional_project(manifest, profile.as_deref())?;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(MyGame::new())?;
    Ok(())
}
