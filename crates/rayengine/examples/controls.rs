//! Keyboard/controller movement and a live controls menu.
//! Run: `cargo run -p rayengine --example controls`.
//! Optional game-owned save path: `RAYENGINE_CONTROLS_PATH=/path/controls.json`.
use rayengine::prelude::*;
use rayengine::raylib::prelude::{GamepadAxis, MouseButton};
use std::path::PathBuf;

const PRIMARY: Action = Action(0);
const NEXT: Action = Action(1);
const PREVIOUS: Action = Action(2);
const ACTIVATE: Action = Action(3);
const MENU: Action = Action(4);
const MOVE_X: Axis = Axis(0);
const MOVE_Z: Axis = Axis(1);
const LOOK: Axis = Axis(2);
const BOOST: Axis = Axis(3);
const GAME_AXES: &[Axis] = &[MOVE_X, MOVE_Z, LOOK, BOOST];
const UI_ACTIONS: UiActions = UiActions {
    primary: PRIMARY,
    next: NEXT,
    previous: PREVIOUS,
    activate: ACTIVATE,
    cancel: MENU,
};

struct Controls {
    initial: Bindings,
    path: PathBuf,
    open: bool,
    ui: UiState,
    position: Vec2,
    previous_position: Vec2,
    yaw: f32,
    status: String,
}

fn key_axis(negative: KeyboardKey, positive: KeyboardKey) -> AxisBinding {
    AxisBinding::new(AxisSource::Buttons {
        negative: negative.into(),
        positive: positive.into(),
    })
}
fn pad_axis(axis: GamepadAxis) -> AxisBinding {
    AxisBinding::new(AxisSource::Gamepad { device: 0, axis })
}
fn defaults() -> Result<Bindings, Error> {
    Bindings::new()
        .bind(PRIMARY, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
        .bind(NEXT, KeyboardKey::KEY_TAB)
        .bind(NEXT, KeyboardKey::KEY_DOWN)
        .bind(PREVIOUS, KeyboardKey::KEY_UP)
        .bind(ACTIVATE, KeyboardKey::KEY_ENTER)
        .bind(MENU, KeyboardKey::KEY_ESCAPE)
        .bind_axis(MOVE_X, key_axis(KeyboardKey::KEY_A, KeyboardKey::KEY_D))?
        .bind_axis(MOVE_X, pad_axis(GamepadAxis::GAMEPAD_AXIS_LEFT_X))?
        .bind_axis(MOVE_Z, key_axis(KeyboardKey::KEY_W, KeyboardKey::KEY_S))?
        .bind_axis(MOVE_Z, pad_axis(GamepadAxis::GAMEPAD_AXIS_LEFT_Y))?
        .bind_axis(LOOK, key_axis(KeyboardKey::KEY_Q, KeyboardKey::KEY_E))?
        .bind_axis(LOOK, pad_axis(GamepadAxis::GAMEPAD_AXIS_RIGHT_X))?
        .bind_axis(BOOST, pad_axis(GamepadAxis::GAMEPAD_AXIS_RIGHT_TRIGGER))
}
fn regions(size: Vec2) -> [UiRegion; 6] {
    let min = size * 0.5 - Vec2::new(185.0, 175.0);
    std::array::from_fn(|i| {
        UiRegion::new(
            UiId(i as u64),
            UiRect::top_left(
                min + Vec2::new(0.0, i as f32 * 48.0),
                Vec2::new(370.0, 40.0),
            )
            .resolve(size),
        )
    })
}

impl Game for Controls {
    fn bindings(&self) -> Bindings {
        self.initial.clone()
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        let was_open = self.open;
        if ctx.input.pressed(MENU) {
            self.open = !self.open;
        }
        let regions = regions(ctx.viewport.logical_size);
        self.ui.update(
            if self.open { &regions[..] } else { &[] },
            ctx.ui_input(UI_ACTIONS),
        );
        if self.open {
            for i in 0..regions.len() {
                if !self
                    .ui
                    .response(UiId(i as u64))
                    .is_some_and(|r| r.activated)
                {
                    continue;
                }
                let result = match i {
                    0 => {
                        self.open = false;
                        Ok(())
                    }
                    1 => {
                        // Choose presets at runtime, retaining the controller source.
                        let arrows = ctx.bindings.axis_bindings(MOVE_X).iter().any(|b| {
                            matches!(
                                b.source,
                                AxisSource::Buttons {
                                    negative: Button::Key(KeyboardKey::KEY_A),
                                    ..
                                }
                            )
                        });
                        let (left, right, forward, back) = if arrows {
                            (
                                KeyboardKey::KEY_LEFT,
                                KeyboardKey::KEY_RIGHT,
                                KeyboardKey::KEY_UP,
                                KeyboardKey::KEY_DOWN,
                            )
                        } else {
                            (
                                KeyboardKey::KEY_A,
                                KeyboardKey::KEY_D,
                                KeyboardKey::KEY_W,
                                KeyboardKey::KEY_S,
                            )
                        };
                        ctx.bindings
                            .rebind_axis(
                                MOVE_X,
                                vec![
                                    key_axis(left, right),
                                    pad_axis(GamepadAxis::GAMEPAD_AXIS_LEFT_X),
                                ],
                            )
                            .and_then(|()| {
                                ctx.bindings.rebind_axis(
                                    MOVE_Z,
                                    vec![
                                        key_axis(forward, back),
                                        pad_axis(GamepadAxis::GAMEPAD_AXIS_LEFT_Y),
                                    ],
                                )
                            })
                    }
                    2 => {
                        let sources = ctx
                            .bindings
                            .axis_bindings(LOOK)
                            .iter()
                            .map(|source| {
                                let mut source = *source;
                                source.inverted = !source.inverted;
                                source
                            })
                            .collect();
                        ctx.bindings.rebind_axis(LOOK, sources)
                    }
                    3 => {
                        let sources = ctx
                            .bindings
                            .axis_bindings(LOOK)
                            .iter()
                            .map(|source| {
                                let mut source = *source;
                                source.sensitivity = if source.sensitivity >= 2.0 {
                                    0.5
                                } else {
                                    source.sensitivity * 2.0
                                };
                                source
                            })
                            .collect();
                        ctx.bindings.rebind_axis(LOOK, sources)
                    }
                    4 => ctx.bindings.save(&self.path),
                    _ => Bindings::load(&self.path).and_then(|loaded| {
                        // This game's menu needs these actions to remain reachable.
                        if [PRIMARY, NEXT, PREVIOUS, ACTIVATE, MENU]
                            .iter()
                            .any(|a| loaded.buttons(*a).is_empty())
                        {
                            return Err(Error::Config(
                                "Loaded settings must retain the menu actions".into(),
                            ));
                        }
                        ctx.bindings.replace(loaded.config())
                    }),
                };
                self.status = match result {
                    Ok(()) => match i {
                        1 => "Movement keys switched (WASD / arrows)".into(),
                        2 => "Look inversion toggled".into(),
                        3 => format!(
                            "Look sensitivity: {}",
                            ctx.bindings
                                .axis_bindings(LOOK)
                                .first()
                                .map_or(1.0, |b| b.sensitivity)
                        ),
                        4 => format!("Saved {}", self.path.display()),
                        5 => format!("Loaded {}", self.path.display()),
                        _ => "ESC opens the controls menu".into(),
                    },
                    Err(error) => error.to_string(),
                };
            }
        }
        let modal = was_open || self.open;
        let gameplay =
            ctx.input
                .routed(&[], modal)
                .with_blocked_axes(if modal { GAME_AXES } else { &[] });
        self.previous_position = self.position;
        let movement =
            Vec2::new(gameplay.value(MOVE_X), gameplay.value(MOVE_Z)).clamp_length_max(1.0);
        self.position = (self.position
            + movement * (3.0 + 3.0 * gameplay.value(BOOST)) * ctx.tick.dt)
            .clamp(Vec2::splat(-4.0), Vec2::splat(4.0));
        self.yaw += gameplay.value(LOOK) * 2.0 * ctx.tick.dt;
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(16, 24, 37, 255));
        let position = self.previous_position.lerp(self.position, frame.alpha);
        frame.world_3d(
            Camera3D {
                position: Vec3::new(self.yaw.sin() * 10.0, 8.0, self.yaw.cos() * 10.0),
                target: Vec3::ZERO,
                ..Camera3D::default()
            },
            |canvas| {
                canvas.cube(
                    Aabb3::from_center(Vec3::new(0.0, -0.6, 0.0), Vec3::new(12.0, 0.2, 12.0)),
                    Color::DARKGREEN,
                );
                canvas.cube(
                    Aabb3::from_center(Vec3::new(position.x, 0.0, position.y), Vec3::ONE),
                    Color::SKYBLUE,
                );
            },
        );
        let regions = regions(frame.viewport.logical_size);
        frame.ui(|canvas| {
            canvas.text(
                "WASD move / Q,E look / ESC controls",
                Vec2::splat(16.0),
                18.0,
                Color::WHITE,
            );
            canvas.text(
                "Pad 0: left stick move / right stick look / right trigger boost",
                Vec2::new(16.0, 40.0),
                16.0,
                Color::LIGHTGRAY,
            );
            if self.open {
                let labels = [
                    "Resume",
                    "Switch movement keys",
                    "Invert look",
                    "Cycle look sensitivity",
                    "Save bindings",
                    "Load bindings",
                ];
                for (region, label) in regions.iter().zip(labels) {
                    if let Some(response) = self.ui.response(region.id) {
                        canvas.button(region.bounds, label, response, UiButtonStyle::default());
                    } else {
                        canvas.rectangle(region.bounds, UiButtonStyle::default().normal);
                        canvas.text(
                            label,
                            region.bounds.min + Vec2::new(12.0, 10.0),
                            18.0,
                            Color::WHITE,
                        );
                    }
                }
            }
            canvas.text(
                &self.status,
                Vec2::new(16.0, canvas.logical_size.y - 30.0),
                16.0,
                Color::WHITE,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let mut config = Config::new("rayengine / Controls");
    config.exit_key = None;
    let path = std::env::var_os("RAYENGINE_CONTROLS_PATH")
        .map_or_else(|| PathBuf::from("controls.json"), PathBuf::from);
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Controls {
            initial: defaults()?,
            path,
            open: true,
            ui: UiState::with_capacity(6),
            position: Vec2::ZERO,
            previous_position: Vec2::ZERO,
            yaw: 0.0,
            status: "Tab/Up/Down focus / Enter select / release pad triggers once to enable boost"
                .into(),
        })?;
    Ok(())
}
