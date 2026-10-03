use rayengine::{core::manifest::ProjectManifest, prelude::*};
use rayengine_tilemap::Tilemap;

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const UP: Action = Action(2);
const DOWN: Action = Action(3);
const JUMP: Action = Action(4);

struct MyGame {
    map: Tilemap,
    body: Body2D,
    previous: Vec2,
}

impl Game for MyGame {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(UP, KeyboardKey::KEY_W)
            .bind(DOWN, KeyboardKey::KEY_S)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.previous = self.body.position;
        self.body.velocity.x = context.input.axis(LEFT, RIGHT) * 200.0;
        self.body.velocity.y = context.input.axis(UP, DOWN) * 200.0;
        self.map.move_body(&mut self.body, context.tick.dt, false);
        if self.body.position.y > 450.0 {
            self.body = player();
            self.previous = self.body.position;
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(18, 29, 48, 255));
        let position = self.previous.lerp(self.body.position, frame.alpha);
        let camera = Camera2D {
            target: Vec2::new(320.0, 192.0),
            view_height: 440.0,
            ..Default::default()
        };
        frame.world_2d(camera, |canvas| {
            self.map.visit_region(
                Aabb2::from_center(Vec2::new(320.0, 192.0), Vec2::new(640.0, 384.0)),
                |tile| {
                    canvas.rectangle(
                        tile.bounds,
                        if tile.flags.one_way {
                            Color::GOLD
                        } else {
                            Color::DARKGRAY
                        },
                    )
                },
            );
            canvas.rectangle(
                Aabb2::from_center(position, self.body.half_size * 2.0),
                Color::SKYBLUE,
            );
        });
        frame.ui(|ui| {
            ui.text(
                "W/A/S/D move   ESC exit",
                Vec2::new(24.0, 24.0),
                20.0,
                Color::WHITE,
            );
        });
    }
}

fn player() -> Body2D {
    Body2D::new(Vec2::new(64.0, 64.0), Vec2::new(20.0, 28.0))
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
    let project = ProjectManifest::load_optional(&manifest)
        .and_then(|manifest| {
            manifest.ok_or_else(|| {
                rayengine::core::manifest::ManifestError("rayengine.toml is required".into())
            })
        })
        .and_then(|manifest| manifest.resolve(profile.as_deref()))
        .map_err(|e| Error::Config(e.to_string()))?;
    let map = Tilemap::load(
        project
            .asset("level.toml")
            .map_err(|e| Error::Config(e.to_string()))?,
    )
    .map_err(|e| Error::Config(e.to_string()))?;
    let config =
        Config::new("Top-down Game").with_optional_project(manifest, profile.as_deref())?;
    let body = player();
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(MyGame {
            map,
            previous: body.position,
            body,
        })?;
    Ok(())
}
