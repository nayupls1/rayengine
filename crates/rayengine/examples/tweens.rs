//! Tweened world objects and UI: a sliding door, a bobbing crate, a damage
//! flash with screen shake, and a toast that slides in when the door opens.
//! Run with `cargo run -p rayengine --example tweens`.
use rayengine::prelude::*;
use std::time::Duration;

const DOOR: Action = Action(0);
const HIT: Action = Action(1);
const PAUSE: Action = Action(2);
const DOOR_OPENED: TweenId = TweenId(1);
const DOOR_CLOSED: TweenId = TweenId(2);
const WHITE: [u8; 4] = [235, 235, 245, 255];
const RED: [u8; 4] = [255, 70, 70, 255];

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

fn color([r, g, b, a]: [u8; 4]) -> Color {
    Color::new(r, g, b, a)
}

/// The toast slides in, waits, then slides out: a sequence of same-typed tweens.
fn toast() -> Sequence<[Tween<Vec2>; 2]> {
    let (hidden, shown) = (Vec2::new(-260.0, 470.0), Vec2::new(16.0, 470.0));
    let mut toast = Sequence::new([
        Tween::new(hidden, shown, ms(450)).with_ease(Ease::BackOut),
        Tween::new(shown, hidden, ms(350))
            .with_ease(Ease::CubicIn)
            .with_delay(ms(1500)),
    ]);
    toast.finish(); // start hidden
    toast
}

struct Tweens {
    // Simulation-owned tweens advance with fixed ticks and are interpolated.
    door: Tween<f32>,
    door_previous: f32,
    open: bool,
    crate_bob: Tween<Vec2>,
    crate_previous: Vec2,
    flash: Sequence<[Tween<[u8; 4]>; 2]>,
    shake: Shake,
    completed: Events<TweenCompleted>,
    paused: bool,
    // Presentation-only tweens advance once per rendered frame.
    toast: Sequence<[Tween<Vec2>; 2]>,
    toast_text: &'static str,
}

impl Tweens {
    fn new() -> Self {
        let mut door = Tween::new(0.0, 0.0, ms(700)).with_ease(Ease::CubicInOut);
        door.finish();
        let mut flash = Sequence::new([
            Tween::new(WHITE, RED, ms(60)),
            Tween::new(RED, WHITE, ms(300)).with_ease(Ease::QuadOut),
        ]);
        flash.finish();
        let crate_bob = Tween::new(Vec2::new(150.0, 60.0), Vec2::new(150.0, 40.0), ms(900))
            .with_ease(Ease::SineInOut)
            .with_mode(TweenMode::PingPong);
        Self {
            door,
            door_previous: 0.0,
            open: false,
            crate_previous: crate_bob.value(),
            crate_bob,
            flash,
            shake: Shake::new(ShakeConfig {
                max_offset: Vec2::splat(14.0),
                ..ShakeConfig::default()
            })
            .expect("valid shake"),
            completed: Events::with_capacity(4),
            paused: false,
            toast: toast(),
            toast_text: "",
        }
    }
}

impl Game for Tweens {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(DOOR, KeyboardKey::KEY_SPACE)
            .bind(HIT, KeyboardKey::KEY_H)
            .bind(PAUSE, KeyboardKey::KEY_P)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        let dt = Duration::from_secs_f32(context.tick.dt);
        self.door_previous = self.door.value();
        self.crate_previous = self.crate_bob.value();
        if context.input.pressed(PAUSE) {
            self.paused = !self.paused;
            if self.paused {
                self.door.pause();
                self.crate_bob.pause();
                self.flash.pause();
            } else {
                self.door.resume();
                self.crate_bob.resume();
                self.flash.resume();
            }
        }
        if context.input.pressed(DOOR) && !self.paused {
            // Interrupting is smooth: continue from wherever the door is now.
            self.open = !self.open;
            self.door = self
                .door
                .with_id(if self.open { DOOR_OPENED } else { DOOR_CLOSED });
            self.door.retarget(if self.open { 1.0 } else { 0.0 });
        }
        if context.input.pressed(HIT) && !self.paused {
            self.flash.reset();
            self.shake.add_trauma(0.45);
        }
        if let Some(done) = self.door.advance(dt) {
            self.completed.send(done);
        }
        self.crate_bob.advance(dt);
        self.flash.advance(dt);
        if !self.paused {
            self.shake.advance(dt);
        }
        for done in self.completed.drain() {
            self.toast_text = if done.id == DOOR_OPENED {
                "Door opened"
            } else {
                "Door closed"
            };
            self.toast.reset();
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        // UI-only animation uses render-frame time, so it stays smooth at any tick rate.
        self.toast.advance(frame.delta);
        let alpha = frame.alpha;
        let door = f32::interpolate(self.door_previous, self.door.value(), alpha);
        let crate_at = Vec2::interpolate(self.crate_previous, self.crate_bob.value(), alpha);
        let tint = color(self.flash.value().unwrap_or(WHITE));
        let camera = self.shake.apply_2d(Camera2D::default());
        frame.clear(Color::new(24, 28, 38, 255));
        frame.world_2d(camera, |canvas| {
            let floor = Aabb2 {
                min: Vec2::new(-480.0, 100.0),
                max: Vec2::new(480.0, 270.0),
            };
            canvas.rectangle(floor, Color::new(46, 58, 70, 255));
            // Door frame and a panel that slides up into the wall.
            canvas.rectangle(
                Aabb2 {
                    min: Vec2::new(-170.0, -110.0),
                    max: Vec2::new(-70.0, 100.0),
                },
                Color::new(14, 16, 22, 255),
            );
            canvas.rectangle(
                Aabb2 {
                    min: Vec2::new(-165.0, -105.0 - door * 190.0),
                    max: Vec2::new(-75.0, 100.0 - door * 190.0),
                },
                Color::new(142, 96, 58, 255),
            );
            canvas.rectangle(
                Aabb2 {
                    min: Vec2::new(-180.0, -300.0),
                    max: Vec2::new(-60.0, -110.0),
                },
                Color::new(60, 66, 82, 255),
            );
            // Training dummy flashes when hit.
            canvas.rectangle(
                Aabb2::from_center(Vec2::new(10.0, 50.0), Vec2::new(36.0, 100.0)),
                tint,
            );
            canvas.circle(Vec2::new(10.0, -20.0), 24.0, tint);
            // Floating crate bobs forever with a ping-pong tween.
            canvas.rectangle(
                Aabb2::from_center(crate_at, Vec2::splat(48.0)),
                Color::new(196, 150, 72, 255),
            );
        });
        let toast_at = self.toast.value().unwrap_or_default();
        let text = self.toast_text;
        let paused = self.paused;
        frame.ui(|ui| {
            ui.text(
                "Space door   H hit   P pause world",
                Vec2::new(16.0, 16.0),
                20.0,
                Color::WHITE,
            );
            if paused {
                ui.text("Paused", Vec2::new(16.0, 44.0), 20.0, Color::GOLD);
            }
            ui.rectangle(
                Aabb2 {
                    min: toast_at,
                    max: toast_at + Vec2::new(240.0, 48.0),
                },
                Color::new(36, 44, 64, 235),
            );
            ui.text(text, toast_at + Vec2::new(16.0, 14.0), 20.0, Color::WHITE);
        });
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Tweens"))
        .with_options(RunOptions::from_env()?)
        .run(Tweens::new())?;
    Ok(())
}
