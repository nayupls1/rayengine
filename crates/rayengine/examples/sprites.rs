//! Original pixel-art sprite playground: move, face and swing at the golden orb.
//! Run with `cargo run -p rayengine --example sprites`.
use rayengine::prelude::*;
use std::{sync::Arc, time::Duration};

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const SWING: Action = Action(2);
const PAUSE: Action = Action(3);
const RESET: Action = Action(4);

struct Sprites {
    texture: Option<TextureId>,
    idle: Arc<AnimationClip>,
    walk: Arc<AnimationClip>,
    swing: Arc<AnimationClip>,
    player: AnimationPlayer,
    x: f32,
    previous_x: f32,
    facing_left: bool,
    orb_x: f32,
    score: u32,
}

impl Sprites {
    fn new() -> Self {
        let clip = |name, timings: &[(u32, u64)], mode| {
            Arc::new(
                AnimationClip::new(
                    name,
                    timings
                        .iter()
                        .map(|&(index, ms)| SpriteFrame {
                            region: SpriteRegion::new(index * 16, 0, 16, 16).unwrap(),
                            duration: Duration::from_millis(ms),
                        })
                        .collect(),
                    mode,
                )
                .unwrap(),
            )
        };
        let idle = clip("idle", &[(0, 450), (1, 250)], PlaybackMode::Loop);
        let walk = clip("walk", &[(2, 110), (3, 110)], PlaybackMode::Loop);
        let swing = clip(
            "swing",
            &[(4, 70), (5, 80), (6, 120), (7, 140)],
            PlaybackMode::Once,
        );
        Self {
            texture: None,
            player: AnimationPlayer::new(Arc::clone(&idle)),
            idle,
            walk,
            swing,
            x: 0.0,
            previous_x: 0.0,
            facing_left: false,
            orb_x: 70.0,
            score: 0,
        }
    }
}

impl Game for Sprites {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(LEFT, KeyboardKey::KEY_LEFT)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(RIGHT, KeyboardKey::KEY_RIGHT)
            .bind(SWING, KeyboardKey::KEY_SPACE)
            .bind(PAUSE, KeyboardKey::KEY_P)
            .bind(RESET, KeyboardKey::KEY_R)
    }

    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.texture = Some(context.texture(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/assets/sprites.png"
        ))?);
        Ok(())
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        if context.input.pressed(PAUSE) {
            if self.player.is_paused() {
                self.player.resume();
            } else {
                self.player.pause();
            }
        }
        if context.input.pressed(RESET) {
            self.player.reset();
        }
        self.previous_x = self.x;
        let direction = context.input.axis(LEFT, RIGHT);
        // Pause the playground too, so the held frame and pose are easy to inspect.
        if self.player.is_paused() {
            return;
        }
        if self.player.clip().name() != "swing" {
            self.x = (self.x + direction * context.tick.dt * 85.0).clamp(-140.0, 140.0);
            if direction != 0.0 {
                self.facing_left = direction < 0.0;
            }
            let desired = if direction == 0.0 {
                &self.idle
            } else {
                &self.walk
            };
            if self.player.clip().name() != desired.name() {
                self.player.play(Arc::clone(desired));
            }
            if context.input.pressed(SWING) {
                self.player.play(Arc::clone(&self.swing));
            }
        }
        if let Some(completed) = self
            .player
            .advance(Duration::from_secs_f32(context.tick.dt))
        {
            // Completion is reported once. Reward a swing only in range and facing the orb.
            let delta = self.orb_x - self.x;
            if completed.clip.name() == "swing"
                && delta.abs() < 28.0
                && (delta < 0.0) == self.facing_left
            {
                self.score += 1;
                self.orb_x = if self.orb_x > 0.0 { -70.0 } else { 70.0 };
            }
            self.player.play(Arc::clone(&self.idle));
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(21, 29, 43, 255));
        let x = self.previous_x + (self.x - self.previous_x) * frame.alpha;
        frame.world_2d(
            Camera2D {
                view_height: 180.0,
                ..Camera2D::default()
            },
            |canvas| {
                canvas.rectangle(
                    Aabb2 {
                        min: Vec2::new(-200.0, 60.0),
                        max: Vec2::new(200.0, 90.0),
                    },
                    Color::new(38, 68, 69, 255),
                );
                canvas.line(
                    Vec2::new(-200.0, 60.0),
                    Vec2::new(200.0, 60.0),
                    2.0,
                    Color::new(77, 124, 96, 255),
                );
                canvas.circle(Vec2::new(self.orb_x, 48.0), 4.0, Color::GOLD);
                if let Some(texture) = self.texture {
                    canvas.sprite(
                        texture,
                        self.player.frame().region,
                        SpriteTransform {
                            position: Vec2::new(x, 60.0),
                            size: Vec2::splat(32.0),
                            origin: Vec2::new(16.0, 32.0),
                            flip_x: self.facing_left,
                            ..SpriteTransform::default()
                        },
                        Color::WHITE,
                    );
                }
            },
        );
        frame.ui(|ui| {
            ui.text(
                "A/D move   Space swing   P pause   R restart",
                Vec2::new(8.0, 8.0),
                10.0,
                Color::WHITE,
            );
            ui.text(
                &format!(
                    "Orbs: {}   {}{}",
                    self.score,
                    self.player.clip().name(),
                    if self.player.is_paused() {
                        " (paused)"
                    } else {
                        ""
                    }
                ),
                Vec2::new(8.0, 23.0),
                10.0,
                Color::GOLD,
            );
            ui.text(
                "Face the orb and swing within reach",
                Vec2::new(8.0, 38.0),
                10.0,
                Color::LIGHTGRAY,
            );
        });
    }
}

fn main() -> Result<(), Error> {
    let mut config = Config::new("rayengine / Sprites");
    config.reference_size = Vec2::new(320.0, 180.0);
    config.scale_mode = ScaleMode::IntegerFit;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Sprites::new())?;
    Ok(())
}
