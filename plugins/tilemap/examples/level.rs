//! Disk-loaded level with atlas drawing, player collision and runtime edits.
use rayengine::{core::manifest::ProjectManifest, prelude::*};
use rayengine_tilemap::{TileId, Tilemap, render::TileAtlas};

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const JUMP: Action = Action(2);
const DROP: Action = Action(3);
const EDIT: Action = Action(4);
struct Demo {
    map: Tilemap,
    atlas: Option<TileAtlas>,
    atlas_path: std::path::PathBuf,
    body: Body2D,
    bridge: TileId,
    drop_timer: f32,
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(LEFT, KeyboardKey::KEY_LEFT)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(RIGHT, KeyboardKey::KEY_RIGHT)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(DROP, KeyboardKey::KEY_S)
            .bind(EDIT, KeyboardKey::KEY_E)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.atlas = Some(TileAtlas::new(ctx.texture(&self.atlas_path)?));
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.body.velocity.x =
            (f32::from(ctx.input.down(RIGHT)) - f32::from(ctx.input.down(LEFT))) * 150.0;
        if ctx.input.pressed(JUMP) && self.body.grounded {
            self.body.velocity.y = -300.0;
        }
        if ctx.input.pressed(DROP) {
            self.drop_timer = 0.25;
        }
        self.drop_timer = (self.drop_timer - ctx.tick.dt).max(0.0);
        self.body.velocity.y += 700.0 * ctx.tick.dt;
        self.map
            .move_body(&mut self.body, ctx.tick.dt, self.drop_timer > 0.0);
        if self.body.position.y > 450.0 {
            self.body = player();
        }
        if ctx.input.pressed(EDIT) {
            let tile = if self.map.tile(0, 8, 8).is_some() {
                None
            } else {
                Some(self.bridge)
            };
            self.map.set_tile(0, 8, 8, tile).unwrap();
            self.map.set_tile(0, 9, 8, tile).unwrap();
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(24, 30, 45, 255));
        let camera = Camera2D {
            target: Vec2::new(320.0, 160.0),
            view_height: 400.0,
            ..Default::default()
        };
        self.atlas
            .unwrap()
            .draw_frame(&self.map, frame, camera)
            .unwrap();
        frame.world_2d(camera, |canvas| {
            canvas.rectangle(self.body.bounds(), Color::WHITE)
        });
        frame.ui(|ui| {
            ui.text(
                "A/D move  SPACE jump  S drop  E bridge",
                Vec2::new(16.0, 16.0),
                18.0,
                Color::WHITE,
            )
        });
    }
}
fn player() -> Body2D {
    Body2D::new(Vec2::new(48.0, 48.0), Vec2::new(20.0, 28.0))
}
fn main() -> Result<(), Error> {
    let project = ProjectManifest::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/rayengine.toml"),
    )
    .and_then(|project| project.resolve(None))
    .map_err(|e| Error::Config(e.to_string()))?;
    let map = Tilemap::load(
        project
            .asset("level.toml")
            .map_err(|e| Error::Config(e.to_string()))?,
    )
    .map_err(|e| Error::Config(e.to_string()))?;
    let atlas_path = project
        .asset(map.atlas_asset().unwrap())
        .map_err(|e| Error::Config(e.to_string()))?;
    let bridge = map.tile(0, 0, 8).unwrap();
    let mut config = Config::new("Tilemap level");
    config.audio = false;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Demo {
            map,
            atlas: None,
            atlas_path,
            body: player(),
            bridge,
            drop_timer: 0.0,
        })?;
    Ok(())
}
