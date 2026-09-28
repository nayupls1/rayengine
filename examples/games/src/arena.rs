//! Arena: a compact platform fighter with a training opponent and optional AI.

use rayengine::prelude::*;
use rayengine::raylib::prelude::MouseButton;

const CAMERA: Camera2D = Camera2D {
    target: Vec2::new(0.0, -45.0),
    view_height: 540.0,
    rotation: 0.0,
};

/// Move left.
pub const LEFT: Action = Action(0);
/// Move right.
pub const RIGHT: Action = Action(1);
/// Jump, including one airborne jump.
pub const JUMP: Action = Action(2);
/// Strike toward the facing direction.
pub const ATTACK: Action = Action(3);
/// Reset the match.
pub const RESET: Action = Action(4);
/// Toggle the training opponent's simple AI.
pub const TOGGLE_AI: Action = Action(5);
/// Strike left or right toward the mouse's world-space position.
pub const MOUSE_ATTACK: Action = Action(6);

#[derive(Clone, Copy)]
enum Strike {
    Facing,
    Toward(f32),
}

/// Character component, kept separate from rendering and window state.
#[derive(Clone, Copy, Debug)]
pub struct Fighter {
    /// Swept collision and velocity.
    pub body: Body2D,
    /// Previous center for render interpolation.
    pub previous: Vec2,
    /// Accumulated damage, increasing knockback.
    pub damage: f32,
    /// Facing direction, -1 or +1.
    pub facing: f32,
    /// Remaining stun duration.
    pub stun: f32,
    jumps: u8,
    coyote: f32,
    jump_buffer: f32,
    attack: f32,
    cooldown: f32,
    connected: bool,
}

impl Fighter {
    fn new(position: Vec2, facing: f32) -> Self {
        Self {
            body: Body2D::new(position, Vec2::new(34.0, 54.0)),
            previous: position,
            damage: 0.0,
            facing,
            stun: 0.0,
            jumps: 2,
            coyote: 0.0,
            jump_buffer: 0.0,
            attack: 0.0,
            cooldown: 0.0,
            connected: false,
        }
    }

    fn attack_bounds(&self) -> Aabb2 {
        Aabb2::from_center(
            self.body.position + Vec2::new(self.facing * 45.0, -3.0),
            Vec2::new(65.0, 48.0),
        )
    }

    fn active(&self) -> bool {
        self.attack > 0.04 && self.attack < 0.15
    }
}

#[derive(Clone, Copy)]
struct Spark {
    position: Vec2,
    velocity: Vec2,
    life: f32,
}

/// CPU-only game simulation. Tests and benchmarks call the same step as play.
pub struct ArenaSimulation {
    scene: Scene,
    fighters: [Entity; 2],
    solids: Vec<Aabb2>,
    sparks: Vec<Spark>,
    /// Number of knockouts scored by each fighter.
    pub scores: [u32; 2],
    /// Whether the opponent approaches, jumps, and attacks.
    pub enemy_ai: bool,
    time: f32,
}

impl Default for ArenaSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl ArenaSimulation {
    /// Creates the stage and two fighters.
    pub fn new() -> Self {
        let mut scene = Scene::new();
        let a = scene.spawn_2d(
            Transform2D::at(Vec2::new(-160.0, 80.0)),
            (Fighter::new(Vec2::new(-160.0, 80.0), 1.0),),
        );
        let b = scene.spawn_2d(
            Transform2D::at(Vec2::new(160.0, 80.0)),
            (Fighter::new(Vec2::new(160.0, 80.0), -1.0),),
        );
        Self {
            scene,
            fighters: [a, b],
            solids: vec![
                Aabb2::from_center(Vec2::new(0.0, 150.0), Vec2::new(740.0, 50.0)),
                Aabb2::from_center(Vec2::new(-205.0, 30.0), Vec2::new(155.0, 16.0)),
                Aabb2::from_center(Vec2::new(205.0, 30.0), Vec2::new(155.0, 16.0)),
                Aabb2::from_center(Vec2::new(0.0, -78.0), Vec2::new(150.0, 16.0)),
            ],
            sparks: Vec::with_capacity(128),
            scores: [0; 2],
            enemy_ai: false,
            time: 0.0,
        }
    }

    /// Copies the player (0) or opponent (1) component. Panics for another index.
    pub fn fighter(&self, index: usize) -> Fighter {
        *self
            .scene
            .world
            .get::<&Fighter>(self.fighters[index])
            .expect("fighter exists")
    }

    /// Advances the full gameplay simulation with already sampled actions.
    pub fn step(&mut self, input: &Input, dt: f32) {
        self.step_with_pointer(input, None, dt);
    }

    /// Advances gameplay with an optional world-space mouse position.
    /// Mouse strikes require a valid pointer; keyboard strikes use current facing.
    pub fn step_with_pointer(&mut self, input: &Input, pointer: Option<Vec2>, dt: f32) {
        if input.pressed(RESET) {
            *self = Self::new();
            return;
        }
        if input.pressed(TOGGLE_AI) {
            self.enemy_ai = !self.enemy_ai;
        }
        self.time += dt;
        let player = self.fighter(0);
        let enemy = self.fighter(1);
        let separation = player.body.position.x - enemy.body.position.x;
        let enemy_move = if self.enemy_ai && separation.abs() > 65.0 {
            separation.signum() * 0.7
        } else {
            0.0
        };
        let enemy_jump = self.enemy_ai
            && enemy.body.grounded
            && (player.body.position.y < enemy.body.position.y - 55.0
                || enemy.body.position.x.abs() > 325.0);
        let mouse_strike = pointer
            .filter(|point| input.pressed(MOUSE_ATTACK) && point.is_finite())
            .map(|point| Strike::Toward(point.x));
        let controls = [
            (
                input.axis(LEFT, RIGHT),
                input.pressed(JUMP),
                mouse_strike.or_else(|| input.pressed(ATTACK).then_some(Strike::Facing)),
            ),
            (
                enemy_move,
                enemy_jump,
                (self.enemy_ai && separation.abs() < 100.0).then_some(Strike::Facing),
            ),
        ];
        for (index, &(axis, jump, attack)) in controls.iter().enumerate() {
            let mut fighter = self
                .scene
                .world
                .get::<&mut Fighter>(self.fighters[index])
                .expect("fighter exists");
            advance_fighter(&mut fighter, axis, jump, attack, dt, &self.solids);
        }
        for (attacker, defender) in [(0, 1), (1, 0)] {
            let strike = self.fighter(attacker);
            let target = self.fighter(defender);
            if strike.active()
                && !strike.connected
                && strike.attack_bounds().intersects(&target.body.bounds())
            {
                self.scene
                    .world
                    .get::<&mut Fighter>(self.fighters[attacker])
                    .expect("fighter")
                    .connected = true;
                let mut target = self
                    .scene
                    .world
                    .get::<&mut Fighter>(self.fighters[defender])
                    .expect("fighter");
                target.damage += 12.0;
                target.body.velocity = Vec2::new(
                    strike.facing * (340.0 + target.damage * 4.0),
                    -220.0 - target.damage * 1.8,
                );
                target.stun = 0.32;
                target.body.grounded = false;
                for i in 0..12 {
                    let angle = i as f32 * std::f32::consts::TAU / 12.0;
                    if self.sparks.len() < 128 {
                        self.sparks.push(Spark {
                            position: target.body.position,
                            velocity: Vec2::new(angle.cos(), angle.sin()) * 220.0,
                            life: 0.3,
                        });
                    }
                }
            }
        }
        for index in 0..2 {
            let fighter = self.fighter(index);
            if fighter.body.position.x.abs() > 620.0
                || fighter.body.position.y > 420.0
                || fighter.body.position.y < -520.0
            {
                self.scores[1 - index] += 1;
                let position = Vec2::new(if index == 0 { -160.0 } else { 160.0 }, -170.0);
                *self
                    .scene
                    .world
                    .get::<&mut Fighter>(self.fighters[index])
                    .expect("fighter") =
                    Fighter::new(position, if index == 0 { 1.0 } else { -1.0 });
            }
        }
        for (transform, fighter) in self
            .scene
            .world
            .query::<(&mut Transform2D, &Fighter)>()
            .iter()
        {
            transform.position = fighter.body.position;
        }
        for spark in &mut self.sparks {
            spark.position += spark.velocity * dt;
            spark.velocity.y += 500.0 * dt;
            spark.life -= dt;
        }
        self.sparks.retain(|s| s.life > 0.0);
    }
}

fn advance_fighter(
    fighter: &mut Fighter,
    axis: f32,
    jump: bool,
    attack: Option<Strike>,
    dt: f32,
    solids: &[Aabb2],
) {
    fighter.previous = fighter.body.position;
    fighter.stun = (fighter.stun - dt).max(0.0);
    fighter.cooldown = (fighter.cooldown - dt).max(0.0);
    fighter.attack = (fighter.attack - dt).max(0.0);
    fighter.jump_buffer = (fighter.jump_buffer - dt).max(0.0);
    if fighter.body.grounded {
        fighter.jumps = 2;
        fighter.coyote = 0.1;
    } else {
        fighter.coyote = (fighter.coyote - dt).max(0.0);
    }
    if jump {
        fighter.jump_buffer = 0.12;
    }
    if fighter.stun <= 0.0 {
        let acceleration = if fighter.body.grounded {
            2600.0
        } else {
            1400.0
        };
        fighter.body.velocity.x =
            approach(fighter.body.velocity.x, axis * 300.0, acceleration * dt);
        if axis != 0.0 && fighter.attack == 0.0 {
            fighter.facing = axis.signum();
        }
        if fighter.jump_buffer > 0.0 && (fighter.jumps > 0 || fighter.coyote > 0.0) {
            // Walking off an edge consumes the grounded jump after coyote time.
            if !fighter.body.grounded && fighter.coyote == 0.0 && fighter.jumps == 2 {
                fighter.jumps = 1;
            }
            fighter.body.velocity.y = -620.0;
            fighter.jumps = fighter.jumps.saturating_sub(1);
            fighter.body.grounded = false;
            fighter.coyote = 0.0;
            fighter.jump_buffer = 0.0;
        }
        if let Some(strike) = attack
            && fighter.cooldown == 0.0
        {
            if let Strike::Toward(x) = strike {
                if x < fighter.body.position.x {
                    fighter.facing = -1.0;
                } else if x > fighter.body.position.x {
                    fighter.facing = 1.0;
                }
            }
            fighter.attack = 0.18;
            fighter.cooldown = 0.35;
            fighter.connected = false;
        }
    }
    fighter.body.velocity.y = (fighter.body.velocity.y + 1600.0 * dt).min(1000.0);
    // The main stage is solid; upper platforms are one-way so jumps pass through.
    // Stack storage keeps this game-specific collision policy allocation-free.
    let mut active = [solids[0]; 4];
    let mut count = 1;
    for &platform in &solids[1..] {
        if fighter.body.velocity.y >= 0.0 && fighter.body.bounds().max.y <= platform.min.y + 0.001 {
            active[count] = platform;
            count += 1;
        }
    }
    fighter.body.move_and_slide(dt, &active[..count]);
}

fn approach(value: f32, target: f32, amount: f32) -> f32 {
    value + (target - value).clamp(-amount, amount)
}

fn shadow_position(feet: Vec2, solids: &[Aabb2]) -> Option<Vec2> {
    solids
        .iter()
        .filter(|solid| {
            feet.x >= solid.min.x && feet.x <= solid.max.x && feet.y <= solid.min.y + 0.001
        })
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .map(|solid| Vec2::new(feet.x, solid.min.y - 2.0))
}

fn pointer_to_world(pointer: Option<Vec2>, view: &Viewport) -> Option<Vec2> {
    pointer.and_then(|ui| CAMERA.screen_to_world(view.ui_to_screen(ui), view))
}

/// Playable presentation over [`ArenaSimulation`].
pub struct Arena {
    simulation: ArenaSimulation,
    hud: [String; 2],
    hud_values: [(u32, u32); 2],
}

impl Default for Arena {
    fn default() -> Self {
        Self {
            simulation: ArenaSimulation::new(),
            hud: ["0%    KO 0".into(), "0%    KO 0".into()],
            hud_values: [(0, 0); 2],
        }
    }
}

impl Game for Arena {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(LEFT, KeyboardKey::KEY_LEFT)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(RIGHT, KeyboardKey::KEY_RIGHT)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(JUMP, KeyboardKey::KEY_W)
            .bind(ATTACK, KeyboardKey::KEY_J)
            .bind(MOUSE_ATTACK, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(RESET, KeyboardKey::KEY_R)
            .bind(TOGGLE_AI, KeyboardKey::KEY_T)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.simulation.step_with_pointer(
            context.input,
            pointer_to_world(context.pointer, &context.viewport),
            context.tick.dt,
        );
        for i in 0..2 {
            let values = (
                self.simulation.fighter(i).damage as u32,
                self.simulation.scores[i],
            );
            if values != self.hud_values[i] {
                self.hud[i] = format!("{}%    KO {}", values.0, values.1);
                self.hud_values[i] = values;
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let navy = Color::new(18, 29, 48, 255);
        let muted = Color::new(135, 161, 187, 255);
        let cyan = Color::new(88, 221, 223, 255);
        let coral = Color::new(255, 131, 116, 255);
        frame.clear(navy);
        let alpha = frame.alpha;
        frame.world_2d(CAMERA, |canvas| {
            for x in -12..=12 {
                let x = x as f32 * 50.0;
                canvas.line(
                    Vec2::new(x, -400.0),
                    Vec2::new(x, 400.0),
                    1.0,
                    Color::new(25, 40, 62, 255),
                );
            }
            for y in -8..=8 {
                let y = y as f32 * 50.0;
                canvas.line(
                    Vec2::new(-700.0, y),
                    Vec2::new(700.0, y),
                    1.0,
                    Color::new(25, 40, 62, 255),
                );
            }
            canvas.circle(Vec2::new(330.0, -190.0), 70.0, Color::new(25, 45, 65, 255));
            for (i, &solid) in self.simulation.solids.iter().enumerate() {
                canvas.rectangle(
                    solid,
                    if i == 0 {
                        Color::new(43, 60, 79, 255)
                    } else {
                        Color::new(56, 78, 99, 255)
                    },
                );
                canvas.rectangle(
                    Aabb2 {
                        min: solid.min,
                        max: Vec2::new(solid.max.x, solid.min.y + 4.0),
                    },
                    cyan,
                );
                if i == 0 {
                    canvas.rectangle(
                        Aabb2::from_center(Vec2::new(-305.0, 205.0), Vec2::new(22.0, 60.0)),
                        Color::new(33, 48, 68, 255),
                    );
                    canvas.rectangle(
                        Aabb2::from_center(Vec2::new(305.0, 205.0), Vec2::new(22.0, 60.0)),
                        Color::new(33, 48, 68, 255),
                    );
                }
            }
            for i in 0..2 {
                let fighter = self.simulation.fighter(i);
                let position = fighter.previous.lerp(fighter.body.position, alpha);
                let color = if fighter.stun > 0.15 {
                    Color::WHITE
                } else if i == 0 {
                    cyan
                } else {
                    coral
                };
                // Shadow, feet, torso, face: all primitive art.
                let feet = position + Vec2::Y * fighter.body.half_size.y;
                if let Some(shadow) = shadow_position(feet, &self.simulation.solids) {
                    canvas.circle(shadow, 16.0, Color::new(13, 23, 38, 255));
                }
                canvas.rectangle(
                    Aabb2::from_center(position + Vec2::new(-10.0, 23.0), Vec2::new(11.0, 9.0)),
                    color,
                );
                canvas.rectangle(
                    Aabb2::from_center(position + Vec2::new(10.0, 23.0), Vec2::new(11.0, 9.0)),
                    color,
                );
                canvas.rectangle(
                    Aabb2::from_center(position + Vec2::new(0.0, 5.0), Vec2::new(34.0, 33.0)),
                    color,
                );
                canvas.circle(position + Vec2::new(0.0, -15.0), 16.0, color);
                canvas.rectangle(
                    Aabb2::from_center(
                        position + Vec2::new(fighter.facing * 7.0, -17.0),
                        Vec2::new(12.0, 5.0),
                    ),
                    navy,
                );
                if fighter.attack > 0.0 {
                    canvas.line(
                        position + Vec2::new(fighter.facing * 16.0, 0.0),
                        position + Vec2::new(fighter.facing * 58.0, -5.0),
                        11.0,
                        color,
                    );
                    canvas.circle(
                        position + Vec2::new(fighter.facing * 60.0, -5.0),
                        10.0,
                        Color::new(255, 222, 139, 255),
                    );
                }
            }
            for spark in &self.simulation.sparks {
                canvas.circle(
                    spark.position,
                    3.0 * (spark.life / 0.3),
                    Color::new(255, 222, 139, 255),
                );
            }
        });
        frame.ui(|ui| {
            ui.text(
                "RAYENGINE  /  2D EXAMPLE",
                Vec2::new(26.0, 20.0),
                13.0,
                muted,
            );
            ui.text("ARENA", Vec2::new(24.0, 40.0), 36.0, Color::WHITE);
            ui.text(
                "A/D move   W/SPACE double jump   CLICK/J strike   T toggle AI   R reset",
                Vec2::new(26.0, 86.0),
                14.0,
                muted,
            );
            let panel = UiRect::top_left(
                Vec2::new(24.0, ui.logical_size.y - 79.0),
                Vec2::new(240.0, 58.0),
            )
            .resolve(ui.logical_size);
            ui.rectangle(panel, Color::new(29, 45, 64, 255));
            ui.rectangle(
                Aabb2 {
                    min: panel.min,
                    max: panel.min + Vec2::new(4.0, panel.size().y),
                },
                cyan,
            );
            ui.text("YOU", panel.min + Vec2::new(14.0, 8.0), 12.0, cyan);
            ui.text(
                &self.hud[0],
                panel.min + Vec2::new(14.0, 27.0),
                22.0,
                Color::WHITE,
            );
            let panel = UiRect::bottom_right(Vec2::new(-24.0, -21.0), Vec2::new(240.0, 58.0))
                .resolve(ui.logical_size);
            ui.rectangle(panel, Color::new(29, 45, 64, 255));
            ui.rectangle(
                Aabb2 {
                    min: panel.min,
                    max: panel.min + Vec2::new(4.0, panel.size().y),
                },
                coral,
            );
            ui.text(
                if self.simulation.enemy_ai {
                    "OPPONENT / AI"
                } else {
                    "TRAINING DUMMY"
                },
                panel.min + Vec2::new(14.0, 8.0),
                12.0,
                coral,
            );
            ui.text(
                &self.hud[1],
                panel.min + Vec2::new(14.0, 27.0),
                22.0,
                Color::WHITE,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1.0 / 120.0;

    fn settle(sim: &mut ArenaSimulation) {
        for _ in 0..180 {
            sim.step(&Input::default(), DT);
        }
    }

    #[test]
    fn mouse_strikes_aim_left_or_right_and_keep_direction_while_moving() {
        for window in [Vec2::new(1280.0, 720.0), Vec2::new(800.0, 1000.0)] {
            let view = Viewport::new(window, Vec2::new(960.0, 540.0), ScaleMode::Fit).unwrap();
            for direction in [-1.0, 1.0] {
                let mut sim = ArenaSimulation::new();
                settle(&mut sim);
                for (index, x) in [0.0, direction * 55.0].into_iter().enumerate() {
                    let mut fighter = sim
                        .scene
                        .world
                        .get::<&mut Fighter>(sim.fighters[index])
                        .unwrap();
                    fighter.body.position.x = x;
                    fighter.facing = -direction;
                }
                // Click far above/below the opponent: only horizontal position
                // should affect aim, even with portrait letterboxing.
                let click = Vec2::new(direction * 100.0, direction * 150.0);
                let screen = view.ui_to_screen(CAMERA.world_to_ui(click, &view));
                let pointer = pointer_to_world(view.screen_to_ui(screen), &view);
                let mut input = Input::default();
                input.set(MOUSE_ATTACK, true);
                input.set(if direction < 0.0 { RIGHT } else { LEFT }, true);
                for _ in 0..20 {
                    sim.step_with_pointer(&input, pointer, DT);
                    input.consume_edges();
                    assert_eq!(sim.fighter(0).facing, direction);
                }
                assert_eq!(sim.fighter(1).damage, 12.0);
                assert_eq!(sim.fighter(1).body.velocity.x.signum(), direction);
            }
        }
    }

    #[test]
    fn mouse_clicks_outside_content_do_not_attack_and_holding_does_not_repeat() {
        let view = Viewport::new(
            Vec2::new(800.0, 1000.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Fit,
        )
        .unwrap();
        let mut sim = ArenaSimulation::new();
        settle(&mut sim);
        let mut input = Input::default();
        input.set(MOUSE_ATTACK, true);
        let pointer = pointer_to_world(view.screen_to_ui(Vec2::new(400.0, 10.0)), &view);
        assert!(pointer.is_none());
        sim.step_with_pointer(&input, pointer, DT);
        assert_eq!(sim.fighter(0).attack, 0.0);
        sim.step_with_pointer(&input, Some(Vec2::new(f32::NAN, 0.0)), DT);
        assert_eq!(sim.fighter(0).attack, 0.0);
        sim.step_with_pointer(&input, Some(Vec2::new(100.0, 0.0)), DT);
        assert!(sim.fighter(0).attack > 0.0);
        input.consume_edges();
        for _ in 0..120 {
            sim.step_with_pointer(&input, Some(Vec2::new(-100.0, 0.0)), DT);
        }
        assert_eq!(sim.fighter(0).attack, 0.0);
        assert_eq!(sim.fighter(0).cooldown, 0.0);
    }

    #[test]
    fn shadows_follow_the_surface_of_every_landing_platform() {
        let mut sim = ArenaSimulation::new();
        for solid in sim.solids.clone() {
            {
                let mut fighter = sim
                    .scene
                    .world
                    .get::<&mut Fighter>(sim.fighters[0])
                    .unwrap();
                fighter.body.position = Vec2::new(
                    solid.center().x,
                    solid.min.y - fighter.body.half_size.y - 1.0,
                );
                fighter.body.velocity = Vec2::ZERO;
            }
            settle(&mut sim);
            let fighter = sim.fighter(0);
            assert!(fighter.body.grounded);
            for alpha in [0.0, 0.5, 1.0] {
                let position = fighter.previous.lerp(fighter.body.position, alpha);
                let feet = position + Vec2::Y * fighter.body.half_size.y;
                assert_eq!(
                    shadow_position(feet, &sim.solids),
                    Some(Vec2::new(position.x, solid.min.y - 2.0))
                );
            }
        }
    }

    #[test]
    fn airborne_shadow_uses_nearest_surface_below_and_disappears_offstage() {
        let sim = ArenaSimulation::new();
        let platform = sim.solids[1];
        let x = platform.center().x;
        assert_eq!(
            shadow_position(Vec2::new(x, platform.min.y - 50.0), &sim.solids),
            Some(Vec2::new(x, platform.min.y - 2.0))
        );
        assert_eq!(
            shadow_position(Vec2::new(x, platform.max.y + 1.0), &sim.solids),
            Some(Vec2::new(x, sim.solids[0].min.y - 2.0))
        );
        assert_eq!(shadow_position(Vec2::new(500.0, 0.0), &sim.solids), None);
        assert_eq!(shadow_position(Vec2::new(0.0, 200.0), &sim.solids), None);
    }

    #[test]
    #[ignore = "requires a native display and OpenGL context; scripts/native_smoke.sh"]
    fn native_gameplay_shadow_is_on_upper_platform() {
        let mut arena = Arena::default();
        {
            let sim = &mut arena.simulation;
            let platform = sim.solids[1];
            let mut fighter = sim
                .scene
                .world
                .get::<&mut Fighter>(sim.fighters[0])
                .unwrap();
            fighter.body.position = Vec2::new(
                platform.center().x,
                platform.min.y - fighter.body.half_size.y - 1.0,
            );
        }
        settle(&mut arena.simulation);
        let image = crate::render_tests::screenshot(arena, "arena-platform-shadow.png");
        // At the reference resolution, these pixels lie between the player's
        // feet on the left platform, and directly below on the main floor.
        let shadow = Color::new(13, 23, 38, 255);
        assert_eq!(image.get_color(275, 335), shadow, "upper platform shadow");
        assert_ne!(image.get_color(275, 438), shadow, "stale ground shadow");
    }

    #[test]
    fn fighter_lands_and_has_two_airborne_jumps() {
        let mut sim = ArenaSimulation::new();
        settle(&mut sim);
        assert!(sim.fighter(0).body.grounded);
        let mut input = Input::default();
        input.set(JUMP, true);
        sim.step(&input, DT);
        assert!(sim.fighter(0).body.velocity.y < 0.0);
        input.consume_edges();
        input.set(JUMP, false);
        input.consume_edges();
        for _ in 0..30 {
            sim.step(&input, DT);
        }
        input.set(JUMP, true);
        sim.step(&input, DT);
        assert_eq!(sim.fighter(0).jumps, 0);
        assert!(sim.fighter(0).body.velocity.y < -500.0);
        input.consume_edges();
        input.set(JUMP, false);
        input.consume_edges();
        input.set(JUMP, true);
        let velocity = sim.fighter(0).body.velocity.y;
        sim.step(&input, DT);
        assert!(sim.fighter(0).body.velocity.y > velocity);
    }

    #[test]
    fn one_strike_deals_damage_once_and_knocks_back() {
        let mut sim = ArenaSimulation::new();
        settle(&mut sim);
        sim.scene
            .world
            .get::<&mut Fighter>(sim.fighters[0])
            .unwrap()
            .body
            .position
            .x = 100.0;
        let mut input = Input::default();
        input.set(ATTACK, true);
        for _ in 0..20 {
            sim.step(&input, DT);
            input.consume_edges();
        }
        assert_eq!(sim.fighter(1).damage, 12.0);
        assert!(sim.fighter(1).body.position.x > 160.0);
    }

    #[test]
    fn blast_zone_awards_knockout_and_respawns() {
        let mut sim = ArenaSimulation::new();
        sim.scene
            .world
            .get::<&mut Fighter>(sim.fighters[1])
            .unwrap()
            .body
            .position
            .y = 600.0;
        sim.step(&Input::default(), DT);
        assert_eq!(sim.scores, [1, 0]);
        assert_eq!(sim.fighter(1).damage, 0.0);
        assert!(sim.fighter(1).body.position.y < 0.0);
    }

    #[test]
    fn scripted_actions_repeat_exactly_within_one_build() {
        let mut a = ArenaSimulation::new();
        let mut b = ArenaSimulation::new();
        let mut input = Input::default();
        for tick in 0..1200 {
            input.set(RIGHT, tick % 240 < 120);
            input.set(LEFT, tick % 240 >= 120);
            input.set(JUMP, tick % 100 == 0);
            input.set(ATTACK, tick % 60 == 0);
            a.step(&input, DT);
            b.step(&input, DT);
            input.consume_edges();
        }
        assert_eq!(a.fighter(0).body.position, b.fighter(0).body.position);
        assert_eq!(a.scores, b.scores);
    }
}
