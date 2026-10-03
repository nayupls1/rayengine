//! Arcade physics playground: `cargo run -p rayengine --example physics`.
use rayengine::prelude::*;
use std::collections::BTreeMap;

const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const JUMP: Action = Action(2);
const RESET: Action = Action(3);

struct Playground {
    world: PhysicsWorld2D,
    player: BodyId,
    platform: BodyId,
    trigger: BodyId,
    events: Events<TriggerEvent>,
    previous: BTreeMap<BodyId, Vec2>,
    in_zone: bool,
    entries: usize,
}
impl Playground {
    fn new() -> Self {
        let mut world = PhysicsWorld2D::new(2.0);
        for (p, size) in [
            (Vec2::new(0.0, 6.0), Vec2::new(30.0, 1.0)),
            (Vec2::new(-15.0, 0.0), Vec2::new(1.0, 12.0)),
            (Vec2::new(15.0, 0.0), Vec2::new(1.0, 12.0)),
        ] {
            let mut b = PhysicsBody2D::new(p, Shape2D::box_shape(size));
            b.kind = BodyKind::Static;
            world.insert(b);
        }
        let mut p = PhysicsBody2D::new(
            Vec2::new(-9.0, 4.0),
            Shape2D::box_shape(Vec2::new(0.8, 1.4)),
        );
        p.gravity = Some(Vec2::new(0.0, 22.0));
        p.mass = 3.0;
        let player = world.insert(p);
        for x in [-5.0, -2.0, 1.0] {
            let mut b = PhysicsBody2D::new(Vec2::new(x, 4.5), Shape2D::box_shape(Vec2::splat(1.4)));
            b.gravity = Some(Vec2::new(0.0, 22.0));
            b.friction = 0.25;
            b.drag = 0.2;
            world.insert(b);
        }
        for (x, speed) in [(3.0, 2.0), (7.0, -2.0)] {
            let mut b = PhysicsBody2D::new(Vec2::new(x, -2.0), Shape2D::round(0.6));
            b.gravity = Some(Vec2::new(0.0, 22.0));
            b.restitution = 0.85;
            b.velocity = Vec2::new(speed, 0.0);
            world.insert(b);
        }
        let mut p = PhysicsBody2D::new(
            Vec2::new(-7.0, 2.0),
            Shape2D::box_shape(Vec2::new(4.0, 0.5)),
        );
        p.kind = BodyKind::Kinematic;
        p.velocity.x = 2.0;
        let platform = world.insert(p);
        // A rider starts on the platform to demonstrate carry immediately.
        let mut rider = PhysicsBody2D::new(Vec2::new(-7.0, 1.25), Shape2D::box_shape(Vec2::ONE));
        rider.gravity = Some(Vec2::new(0.0, 22.0));
        world.insert(rider);
        let mut t = PhysicsBody2D::new(
            Vec2::new(10.0, 3.0),
            Shape2D::box_shape(Vec2::new(4.0, 5.0)),
        );
        t.kind = BodyKind::Static;
        t.is_trigger = true;
        let trigger = world.insert(t);
        let previous = world.iter().map(|(id, b)| (id, b.position)).collect();
        Self {
            world,
            player,
            platform,
            trigger,
            events: Events::default(),
            previous,
            in_zone: false,
            entries: 0,
        }
    }
}
impl Game for Playground {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(RESET, KeyboardKey::KEY_R)
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        if ctx.input.pressed(RESET) {
            *self = Self::new();
        }
        self.previous = self.world.iter().map(|(id, b)| (id, b.position)).collect();
        let player = self.world.body_mut(self.player).unwrap();
        player.velocity.x =
            (f32::from(ctx.input.down(RIGHT)) - f32::from(ctx.input.down(LEFT))) * 6.0;
        if ctx.input.pressed(JUMP) && player.grounded {
            player.velocity.y = -11.0;
        }
        let platform = self.world.body_mut(self.platform).unwrap();
        if platform.position.x > 5.0 {
            platform.velocity.x = -2.0;
        }
        if platform.position.x < -7.0 {
            platform.velocity.x = 2.0;
        }
        self.events.clear();
        self.world.step(ctx.tick, &mut self.events);
        for e in self.events.read() {
            if e.trigger == self.trigger && e.other == self.player {
                match e.phase {
                    TriggerPhase::Enter => {
                        self.in_zone = true;
                        self.entries += 1;
                    }
                    TriggerPhase::Stay => self.in_zone = true,
                    TriggerPhase::Exit => self.in_zone = false,
                }
            }
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        frame.clear(Color::new(20, 25, 40, 255));
        let alpha = frame.alpha;
        frame.world_2d(
            Camera2D {
                target: Vec2::ZERO,
                rotation: 0.0,
                view_height: 20.0,
            },
            |canvas| {
                for (id, b) in self.world.iter() {
                    let p = self
                        .previous
                        .get(&id)
                        .copied()
                        .unwrap_or(b.position)
                        .lerp(b.position, alpha);
                    let color = if id == self.trigger {
                        if self.in_zone {
                            Color::new(50, 200, 100, 90)
                        } else {
                            Color::new(60, 120, 200, 90)
                        }
                    } else if id == self.player {
                        Color::WHITE
                    } else if id == self.platform {
                        Color::SKYBLUE
                    } else if b.kind == BodyKind::Static {
                        Color::GRAY
                    } else if matches!(b.shape, Shape2D::Circle { .. }) {
                        Color::ORANGE
                    } else {
                        Color::BEIGE
                    };
                    match b.shape {
                        Shape2D::Box { .. } => canvas.rectangle(b.shape.bounds(p), color),
                        Shape2D::Circle { radius } => canvas.circle(p, radius, color),
                    }
                }
            },
        );
        frame.ui(|ui| {
            ui.text(
                "A/D move and push   Space jump   R reset",
                Vec2::splat(18.0),
                20.0,
                Color::WHITE,
            );
            ui.text(
                &format!(
                    "Trigger entries: {}   In zone: {}",
                    self.entries, self.in_zone
                ),
                Vec2::new(18.0, 48.0),
                18.0,
                Color::SKYBLUE,
            );
        });
    }
}
fn main() -> Result<(), Error> {
    App::new(Config::new("rayengine / Arcade physics"))
        .with_options(RunOptions::from_env()?)
        .run(Playground::new())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playground_runs_fixed_ticks_without_exhausting_the_solver() {
        let mut game = Playground::new();
        for index in 0..1200 {
            let platform = game.world.body_mut(game.platform).unwrap();
            if platform.position.x > 5.0 {
                platform.velocity.x = -2.0;
            }
            if platform.position.x < -7.0 {
                platform.velocity.x = 2.0;
            }
            game.world.body_mut(game.player).unwrap().velocity.x =
                if index < 400 { 6.0 } else { -6.0 };
            game.events.clear();
            let report = game.world.step(
                Tick {
                    index,
                    dt: 1.0 / 120.0,
                },
                &mut game.events,
            );
            assert_eq!(report.dropped_time, 0.0, "tick {index}: {report:?}");
            assert!(!report.unresolved_overlaps, "tick {index}: {report:?}");
            assert!(
                game.world
                    .iter()
                    .all(|(_, b)| b.position.is_finite() && b.velocity.is_finite())
            );
        }
    }
}
