//! Minimal first-person game. Run with `cargo run -p rayengine --example first_person`.
use rayengine::prelude::*;

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const FORWARD: Action = Action(2);
const BACK: Action = Action(3);
const JUMP: Action = Action(4);
const SPRINT: Action = Action(5);
const RESET: Action = Action(6);
const ACTIONS: FirstPersonActions = FirstPersonActions {
    left: LEFT,
    right: RIGHT,
    forward: FORWARD,
    back: BACK,
    jump: JUMP,
    sprint: Some(SPRINT),
    turn_left: None,
    turn_right: None,
};

struct Walk {
    player: FirstPersonController,
    solids: Vec<Aabb3>,
}
impl Walk {
    fn new() -> Result<Self, FirstPersonError> {
        Ok(Self {
            player: FirstPersonController::new(
                Vec3::new(0.0, 1.0, 6.0),
                Vec3::new(0.8, 1.8, 0.8),
                FirstPersonConfig::default(),
            )?,
            solids: vec![
                Aabb3::from_center(Vec3::new(0.0, -0.5, 0.0), Vec3::new(40.0, 1.0, 40.0)),
                Aabb3::from_center(Vec3::new(0.0, 0.35, -1.0), Vec3::new(3.0, 0.7, 3.0)),
                Aabb3::from_center(Vec3::new(3.0, 0.7, -5.0), Vec3::new(3.0, 1.4, 3.0)),
            ],
        })
    }
}
impl Game for Walk {
    fn cursor_mode(&self) -> CursorMode {
        CursorMode::Captured
    }
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(FORWARD, KeyboardKey::KEY_W)
            .bind(BACK, KeyboardKey::KEY_S)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(SPRINT, KeyboardKey::KEY_LEFT_SHIFT)
            .bind(RESET, KeyboardKey::KEY_R)
    }
    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        // Reset/spawn rules belong to this game, not the helper.
        if context.input.pressed(RESET) || self.player.body.position.y < -15.0 {
            self.player
                .teleport(Vec3::new(0.0, 1.0, 6.0))
                .expect("finite spawn");
        }
        self.player.step(
            FirstPersonInput::from_actions(context.input, ACTIONS),
            context.tick.dt,
            &self.solids,
        );
        // The runner consumes edges and relative motion after this fixed callback.
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::SKYBLUE);
        frame.world_3d(self.player.camera(frame.alpha), |canvas| {
            for (index, &solid) in self.solids.iter().enumerate() {
                canvas.cube(
                    solid,
                    if index == 0 {
                        Color::DARKGREEN
                    } else {
                        Color::BEIGE
                    },
                );
            }
        });
        frame.ui(|ui| {
            ui.circle(ui.logical_size * 0.5, 2.0, Color::WHITE);
            ui.text(
                "WASD move  Mouse look  Space jump  Shift sprint  R reset",
                Vec2::splat(20.0),
                18.0,
                Color::WHITE,
            );
        });
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(Config::new("First-person helper"))
        .with_options(RunOptions::from_env()?)
        .run(Walk::new()?)?;
    Ok(())
}
