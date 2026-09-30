//! A draggable menu with keyboard focus and explicit gameplay input routing.
//! Run with: `cargo run -p rayengine --example menu`.
use rayengine::prelude::*;
use rayengine::raylib::prelude::MouseButton;

const PRIMARY: Action = Action(0);
const NEXT: Action = Action(1);
const PREVIOUS: Action = Action(2);
const ACTIVATE: Action = Action(3);
const MENU: Action = Action(4);
const CANCEL: Action = Action(5);
const LEFT: Action = Action(6);
const RIGHT: Action = Action(7);
const UI_ACTIONS: UiActions = UiActions {
    primary: PRIMARY,
    next: NEXT,
    previous: PREVIOUS,
    activate: ACTIVATE,
    cancel: CANCEL,
};
const GAME_ACTIONS: &[Action] = &[PRIMARY, ACTIVATE, LEFT, RIGHT];
const HEADER: UiId = UiId(1);
const RESUME: UiId = UiId(2);
const QUIT: UiId = UiId(3);

struct Menu {
    open: bool,
    ui: UiState,
    panel_offset: Vec2,
    yaw: f32,
    position: f32,
    previous_position: f32,
    alternate_tint: bool,
}

impl Default for Menu {
    fn default() -> Self {
        Self {
            open: true,
            ui: UiState::with_capacity(3),
            panel_offset: Vec2::ZERO,
            yaw: 0.0,
            position: 0.0,
            previous_position: 0.0,
            alternate_tint: false,
        }
    }
}

impl Menu {
    fn panel(&self, size: Vec2) -> Aabb2 {
        UiRect {
            anchor: Vec2::splat(0.5),
            pivot: Vec2::splat(0.5),
            offset: self.panel_offset,
            size: Vec2::new(340.0, 230.0),
        }
        .resolve(size)
    }

    fn regions(&self, size: Vec2) -> [UiRegion; 3] {
        let min = self.panel(size).min;
        let mut header = UiRegion::new(
            HEADER,
            Aabb2 {
                min,
                max: min + Vec2::new(340.0, 44.0),
            },
        );
        header.focusable = false;
        header.draggable = true;
        [
            header,
            UiRegion::new(
                RESUME,
                UiRect::top_left(min + Vec2::new(20.0, 65.0), Vec2::new(300.0, 48.0)).resolve(size),
            ),
            UiRegion::new(
                QUIT,
                UiRect::top_left(min + Vec2::new(20.0, 130.0), Vec2::new(300.0, 48.0))
                    .resolve(size),
            ),
        ]
    }
}

impl Game for Menu {
    fn cursor_mode(&self) -> CursorMode {
        if self.open {
            CursorMode::Free
        } else {
            CursorMode::Captured
        }
    }

    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(PRIMARY, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(NEXT, KeyboardKey::KEY_TAB)
            .bind(NEXT, KeyboardKey::KEY_DOWN)
            .bind(PREVIOUS, KeyboardKey::KEY_UP)
            .bind(ACTIVATE, KeyboardKey::KEY_ENTER)
            .bind(ACTIVATE, KeyboardKey::KEY_SPACE)
            .bind(MENU, KeyboardKey::KEY_ESCAPE)
            .bind(CANCEL, KeyboardKey::KEY_BACKSPACE)
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
    }

    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        let was_open = self.open;
        if ctx.input.pressed(MENU) {
            self.open = !self.open;
        }
        let regions = self.regions(ctx.viewport.logical_size);
        let visible = if self.open { &regions[..] } else { &[] };
        // Updating with an empty list also cancels any old capture/focus.
        let capture = self.ui.update(visible, ctx.ui_input(UI_ACTIONS));
        if self.open {
            let header = self.ui.response(HEADER).unwrap();
            let limit = ((ctx.viewport.logical_size - Vec2::new(340.0, 230.0)) * 0.5
                - Vec2::splat(10.0))
            .max(Vec2::ZERO);
            self.panel_offset = (self.panel_offset + header.drag_delta).clamp(-limit, limit);
            if self.ui.response(RESUME).unwrap().activated {
                self.open = false;
            }
            if self.ui.response(QUIT).unwrap().activated {
                ctx.quit();
            }
        }
        // Keep closing/opening events out of gameplay on this tick too.
        let modal = was_open || self.open;
        let gameplay = ctx.input.routed(
            if modal { GAME_ACTIONS } else { &[] },
            modal || capture.pointer,
        );
        self.previous_position = self.position;
        self.position =
            (self.position + gameplay.axis(LEFT, RIGHT) * ctx.tick.dt * 3.0).clamp(-4.0, 4.0);
        self.yaw += gameplay.pointer_delta().x * 0.003;
        if gameplay.pressed(PRIMARY) || gameplay.pressed(ACTIVATE) {
            self.alternate_tint = !self.alternate_tint;
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(16, 24, 37, 255));
        let x = self.previous_position + (self.position - self.previous_position) * frame.alpha;
        frame.world_3d(
            Camera3D {
                position: Vec3::new(self.yaw.sin() * 8.0, 4.0, self.yaw.cos() * 8.0),
                target: Vec3::ZERO,
                ..Camera3D::default()
            },
            |canvas| {
                canvas.cube(
                    Aabb3::from_center(Vec3::new(0.0, -1.1, 0.0), Vec3::new(12.0, 0.2, 12.0)),
                    Color::new(42, 63, 74, 255),
                );
                canvas.cube(
                    Aabb3::from_center(Vec3::new(x, 0.0, 0.0), Vec3::splat(2.0)),
                    if self.alternate_tint {
                        Color::ORANGE
                    } else {
                        Color::SKYBLUE
                    },
                );
            },
        );
        let regions = self.regions(frame.viewport.logical_size);
        let panel = self.panel(frame.viewport.logical_size);
        frame.ui(|canvas| {
            canvas.text(
                "ESC menu   A/D move   Mouse look   Click/Space change color",
                Vec2::splat(16.0),
                18.0,
                Color::WHITE,
            );
            if self.open {
                canvas.rectangle(panel, Color::new(19, 28, 42, 255));
                canvas.rectangle(regions[0].bounds, Color::new(34, 51, 69, 255));
                canvas.text(
                    "MENU / drag this header",
                    panel.min + Vec2::new(16.0, 12.0),
                    20.0,
                    Color::WHITE,
                );
                // A render frame can occur before the first fixed update.
                for (region, label) in [(regions[1], "Resume"), (regions[2], "Quit")] {
                    if let Some(response) = self.ui.response(region.id) {
                        canvas.button(region.bounds, label, response, UiButtonStyle::default());
                    } else {
                        canvas.rectangle(region.bounds, UiButtonStyle::default().normal);
                        canvas.text(
                            label,
                            region.bounds.min + Vec2::new(16.0, 12.0),
                            20.0,
                            Color::WHITE,
                        );
                    }
                }
                canvas.text(
                    "Tab/Up/Down focus   Enter select",
                    panel.min + Vec2::new(20.0, 196.0),
                    16.0,
                    Color::LIGHTGRAY,
                );
            }
        });
    }
}

fn main() -> Result<(), Error> {
    let mut config = Config::new("rayengine / Menu");
    config.exit_key = None; // Escape belongs to the menu; Quit/close still exits.
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Menu::default())?;
    Ok(())
}
