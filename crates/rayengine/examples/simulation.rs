//! Pause and fast-forward a simulation while its UI remains interactive.
//! Run with `cargo run -p rayengine --example simulation`.
use rayengine::prelude::*;
use rayengine::raylib::prelude::MouseButton;
use std::time::Duration;

const HOST_STEP: Duration = Duration::from_millis(10);
const PRIMARY: Action = Action(0);
const NEXT: Action = Action(1);
const PREVIOUS: Action = Action(2);
const ACTIVATE: Action = Action(3);
const CANCEL: Action = Action(4);
const PAUSE: Action = Action(5);
const SLOW: Action = Action(6);
const NORMAL: Action = Action(7);
const FAST: Action = Action(8);
const REVERSE: Action = Action(9);
const BOOST: Action = Action(10);
const UI_ACTIONS: UiActions = UiActions {
    primary: PRIMARY,
    next: NEXT,
    previous: PREVIOUS,
    activate: ACTIVATE,
    cancel: CANCEL,
};

struct Simulation {
    clock: SimulationClock,
    ui: UiState,
    phase: f32,
    previous_phase: f32,
    direction: f32,
    pending_reverse: bool,
    reversals: u64,
    production: Timer,
    produced: u64,
    discarded: Duration,
    presentation: Duration,
}

impl Simulation {
    fn new() -> Self {
        Self {
            clock: SimulationClock::new(100, 8),
            ui: UiState::with_capacity(4),
            phase: 0.0,
            previous_phase: 0.0,
            direction: 1.0,
            pending_reverse: false,
            reversals: 0,
            production: Timer::repeating(Duration::from_secs(1)),
            produced: 0,
            discarded: Duration::ZERO,
            presentation: Duration::ZERO,
        }
    }

    fn regions(size: Vec2) -> [UiRegion; 4] {
        std::array::from_fn(|i| {
            UiRegion::new(
                UiId(i as u64),
                UiRect::top_left(
                    Vec2::new(16.0 + i as f32 * 156.0, 82.0),
                    Vec2::new(144.0, 44.0),
                )
                .resolve(size),
            )
        })
    }

    fn update(&mut self, input: &Input, ui_input: UiInput, size: Vec2) {
        // UI uses the host cadence, even when there are zero simulation steps.
        let regions = Self::regions(size);
        self.ui.update(&regions, ui_input);
        let activated = |i| self.ui.response(UiId(i)).is_some_and(|r| r.activated);
        if input.pressed(PAUSE) || activated(0) {
            self.clock.set_paused(!self.clock.is_paused());
        }
        for (i, action, speed) in [(1, SLOW, 0.25), (2, NORMAL, 1.0), (3, FAST, 8.0)] {
            if input.pressed(action) || activated(i) {
                self.clock.set_speed(speed).expect("valid example speed");
                self.clock.set_paused(false);
            }
        }

        // This game rejects commands while paused and clears old commands on
        // pause/focus reset. At slow speed, keep one pending reverse until a
        // simulation tick actually runs. Repeated pending presses coalesce.
        if self.clock.is_paused() || input.reset_pending() {
            self.pending_reverse = false;
        } else {
            self.pending_reverse |= input.pressed(REVERSE);
        }
        let plan = self.clock.advance(HOST_STEP);
        self.discarded = self.discarded.saturating_add(plan.dropped);
        for tick in plan.ticks(self.clock.step()) {
            // Taking a command consumes its edge once across all eight fast ticks.
            if std::mem::take(&mut self.pending_reverse) {
                self.direction = -self.direction;
                self.reversals += 1;
            }
            self.previous_phase = self.phase;
            let boost = if input.down(BOOST) { 2.0 } else { 1.0 };
            self.phase += self.direction * boost * tick.dt;
            self.produced += u64::from(self.production.advance(self.clock.step()));
        }
    }
}

impl Game for Simulation {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(PRIMARY, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(NEXT, KeyboardKey::KEY_TAB)
            .bind(NEXT, KeyboardKey::KEY_DOWN)
            .bind(PREVIOUS, KeyboardKey::KEY_UP)
            .bind(ACTIVATE, KeyboardKey::KEY_ENTER)
            .bind(CANCEL, KeyboardKey::KEY_BACKSPACE)
            .bind(PAUSE, KeyboardKey::KEY_P)
            .bind(SLOW, KeyboardKey::KEY_ONE)
            .bind(NORMAL, KeyboardKey::KEY_TWO)
            .bind(FAST, KeyboardKey::KEY_THREE)
            .bind(REVERSE, KeyboardKey::KEY_R)
            .bind(BOOST, KeyboardKey::KEY_B)
    }

    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.update(
            ctx.input,
            ctx.ui_input(UI_ACTIONS),
            ctx.viewport.logical_size,
        );
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        // Presentation uses unscaled render time. The world uses its own alpha;
        // the host's frame.alpha belongs to a different timeline.
        self.presentation = self.presentation.saturating_add(frame.delta);
        let phase = self.previous_phase + (self.phase - self.previous_phase) * self.clock.alpha();
        frame.clear(Color::new(20, 28, 38, 255));
        frame.world_2d(Camera2D::default(), |canvas| {
            canvas.rectangle(
                Aabb2::from_center(Vec2::new(0.0, 80.0), Vec2::new(680.0, 20.0)),
                Color::DARKGRAY,
            );
            for offset in [0.0, 2.1, 4.2] {
                canvas.rectangle(
                    Aabb2::from_center(
                        Vec2::new((phase + offset).sin() * 290.0, 45.0),
                        Vec2::splat(48.0),
                    ),
                    Color::SKYBLUE,
                );
            }
        });
        let regions = Self::regions(frame.viewport.logical_size);
        frame.ui(|canvas| {
            canvas.text(
                "P pause | 1 slow | 2 normal | 3 fast | R reverse | Hold B boost",
                Vec2::splat(16.0),
                18.0,
                Color::WHITE,
            );
            canvas.text(
                &format!(
                    "{}  {:.2}x | Sim {:.2}s | Produced {} | Commands {} | Discarded {:.2}s",
                    if self.clock.is_paused() {
                        "Paused"
                    } else {
                        "Running"
                    },
                    self.clock.speed(),
                    self.clock.elapsed().as_secs_f64(),
                    self.produced,
                    self.reversals,
                    self.discarded.as_secs_f64(),
                ),
                Vec2::new(16.0, 48.0),
                18.0,
                Color::GOLD,
            );
            for (region, label) in regions.iter().zip([
                if self.clock.is_paused() {
                    "Resume"
                } else {
                    "Pause"
                },
                "Slow 0.25x",
                "Normal 1x",
                "Fast 8x",
            ]) {
                if let Some(response) = self.ui.response(region.id) {
                    canvas.button(region.bounds, label, response, UiButtonStyle::default());
                } else {
                    canvas.rectangle(region.bounds, Color::DARKGRAY);
                    canvas.text(
                        label,
                        region.bounds.min + Vec2::new(12.0, 12.0),
                        18.0,
                        Color::WHITE,
                    );
                }
            }
            let pulse = (self.presentation.as_secs_f32() * 3.0).sin();
            canvas.circle(Vec2::new(30.0, 155.0), 8.0 + pulse * 3.0, Color::LIME);
            canvas.text(
                "UI animation continues while paused. Tab / Enter or click controls.",
                Vec2::new(48.0, 147.0),
                18.0,
                Color::LIGHTGRAY,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let mut config = Config::new("rayengine / Simulation timeline");
    config.fixed_hz = 100; // HOST_STEP is exact; avoid converting f32 dt back to Duration.
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Simulation::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(game: &mut Simulation, input: &mut Input) {
        game.update(
            input,
            UiInput::from_actions(input, None, UI_ACTIONS, true),
            Vec2::new(960.0, 540.0),
        );
        input.consume_edges();
    }

    #[test]
    fn one_command_is_consumed_once_in_eight_steps_and_held_boost_persists() {
        let mut game = Simulation::new();
        let mut input = Input::default();
        input.set(FAST, true);
        input.set(REVERSE, true);
        input.set(BOOST, true);
        update(&mut game, &mut input);
        assert_eq!(game.clock.ticks(), 8);
        assert_eq!(game.reversals, 1);
        assert!((game.phase + 0.16).abs() < 0.00001);
        update(&mut game, &mut input);
        assert_eq!(game.reversals, 1);
        assert!((game.phase + 0.32).abs() < 0.00001);
    }

    #[test]
    fn commands_survive_updates_with_no_simulation_tick() {
        let mut game = Simulation::new();
        let mut input = Input::default();
        input.set(SLOW, true);
        input.set(REVERSE, true);
        update(&mut game, &mut input);
        assert_eq!(game.clock.ticks(), 0);
        assert!(game.pending_reverse);
        input.set(REVERSE, false);
        for _ in 0..3 {
            update(&mut game, &mut input);
        }
        assert_eq!(game.clock.ticks(), 1);
        assert_eq!(game.reversals, 1);
        assert!(!game.pending_reverse);
    }

    #[test]
    fn paused_world_keeps_keyboard_ui_interactive() {
        let mut game = Simulation::new();
        let mut input = Input::default();
        input.set(PAUSE, true);
        update(&mut game, &mut input);
        for _ in 0..100 {
            update(&mut game, &mut input);
        }
        assert!(game.clock.is_paused());
        assert_eq!(game.clock.ticks(), 0);
        assert_eq!(game.produced, 0);
        // Focus the Pause/Resume button and activate it through ordinary UI.
        input.set(NEXT, true);
        update(&mut game, &mut input);
        input.set(ACTIVATE, true);
        update(&mut game, &mut input);
        assert!(!game.clock.is_paused());
        assert_eq!(game.clock.ticks(), 1);
    }

    #[test]
    fn pause_and_focus_reset_clear_pending_commands() {
        for reset in [false, true] {
            let mut game = Simulation::new();
            let mut input = Input::default();
            input.set(SLOW, true);
            input.set(REVERSE, true);
            update(&mut game, &mut input);
            assert!(game.pending_reverse);
            if reset {
                input.release_all();
            } else {
                input.set(PAUSE, true);
            }
            update(&mut game, &mut input);
            assert!(!game.pending_reverse);
            input.set(FAST, true);
            update(&mut game, &mut input);
            assert_eq!(game.reversals, 0);
        }
    }
}
