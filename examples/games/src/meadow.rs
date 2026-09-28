//! Meadow: a small open exploration area with jumping, pickups and checkpoints.

use rayengine::prelude::*;
use rayengine::raylib::prelude::RaylibDraw3D;

const MOUSE_SENSITIVITY: f32 = 0.0025;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
const EYE_HEIGHT: f32 = 0.7;

/// Strafe left relative to the camera.
pub const LEFT: Action = Action(0);
/// Strafe right relative to the camera.
pub const RIGHT: Action = Action(1);
/// Walk forward relative to the camera.
pub const FORWARD: Action = Action(2);
/// Walk backward relative to the camera.
pub const BACK: Action = Action(3);
/// Jump, with a short input buffer and coyote window.
pub const JUMP: Action = Action(4);
/// Sprint.
pub const SPRINT: Action = Action(5);
/// Turn left, as a keyboard alternative to mouse look.
pub const TURN_LEFT: Action = Action(6);
/// Turn right, as a keyboard alternative to mouse look.
pub const TURN_RIGHT: Action = Action(7);
/// Return to the latest checkpoint.
pub const RESET: Action = Action(8);

/// Player component used by both fixed simulation and rendering.
#[derive(Clone, Copy, Debug)]
pub struct Explorer {
    /// Swept character body, using positive Y up.
    pub body: Body3D,
    /// Previous center for interpolation.
    pub previous: Vec3,
    coyote: f32,
    jump_buffer: f32,
}

#[derive(Clone, Copy)]
struct Tree {
    position: Vec3,
    height: f32,
}

#[derive(Clone, Copy)]
struct Orb {
    position: Vec3,
    checkpoint: Vec3,
    collected: bool,
}

/// Display-independent simulation for the 3D exploration game.
pub struct MeadowSimulation {
    scene: Scene,
    player: Entity,
    solids: Vec<Aabb3>,
    platforms: Vec<Aabb3>,
    trees: Vec<Tree>,
    orbs: Vec<Orb>,
    checkpoint: Vec3,
    /// Horizontal view angle in radians; positive turns right.
    pub yaw: f32,
    /// Vertical view angle in radians; positive looks up.
    pub pitch: f32,
    /// Number of collected orbs.
    pub collected: usize,
    time: f32,
}

impl Default for MeadowSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl MeadowSimulation {
    /// Creates a geometric world with a climbable course and collectible orbs.
    pub fn new() -> Self {
        let mut scene = Scene::new();
        let spawn = Vec3::new(0.0, 1.0, 8.0);
        let player = scene.spawn_3d(
            Transform3D::at(spawn),
            (Explorer {
                body: Body3D::new(spawn, Vec3::new(0.8, 1.8, 0.8)),
                previous: spawn,
                coyote: 0.0,
                jump_buffer: 0.0,
            },),
        );
        let ground = Aabb3::from_center(Vec3::new(0.0, -2.0, 0.0), Vec3::new(140.0, 4.0, 140.0));
        let route = [
            Vec3::new(0.0, 0.7, -1.0),
            Vec3::new(3.5, 1.6, -4.0),
            Vec3::new(7.0, 2.5, -7.0),
            Vec3::new(7.0, 3.4, -11.5),
            Vec3::new(3.5, 4.3, -14.5),
            Vec3::new(0.0, 5.2, -17.5),
            Vec3::new(-3.5, 6.1, -20.5),
            Vec3::new(-3.5, 7.0, -25.0),
            Vec3::new(0.0, 7.9, -28.0),
            Vec3::new(3.5, 8.8, -31.0),
        ];
        let platforms: Vec<_> = route
            .iter()
            .map(|p| Aabb3::from_center(Vec3::new(p.x, p.y * 0.5, p.z), Vec3::new(3.2, p.y, 3.2)))
            .collect();
        let mut orbs: Vec<_> = route
            .iter()
            .map(|p| Orb {
                position: *p + Vec3::Y * 1.35,
                checkpoint: *p + Vec3::Y * 0.91,
                collected: false,
            })
            .collect();
        for p in [
            Vec3::new(14.0, 0.0, 8.0),
            Vec3::new(-15.0, 0.0, -5.0),
            Vec3::new(18.0, 0.0, -23.0),
        ] {
            orbs.push(Orb {
                position: p + Vec3::Y * 1.35,
                checkpoint: p + Vec3::Y * 0.91,
                collected: false,
            });
        }
        let mut solids = vec![ground];
        solids.extend_from_slice(&platforms);
        let mut trees = Vec::with_capacity(80);
        // Deterministic placement with no external RNG or asset requirement.
        for i in 0..90 {
            let x = ((i * 37 + 11) % 113) as f32 - 56.0;
            let z = ((i * 53 + 29) % 107) as f32 - 53.0;
            let p = Vec3::new(x, 0.0, z);
            if x.abs() < 12.0 && (-38.0..18.0).contains(&z) {
                continue;
            }
            if orbs
                .iter()
                .any(|orb| (orb.position - p).with_y(0.0).length_squared() < 16.0)
            {
                continue;
            }
            let height = 3.8 + (i % 5) as f32 * 0.6;
            trees.push(Tree {
                position: p,
                height,
            });
            solids.push(Aabb3::from_center(
                p + Vec3::Y * 1.0,
                Vec3::new(0.7, 2.0, 0.7),
            ));
        }
        Self {
            scene,
            player,
            solids,
            platforms,
            trees,
            orbs,
            checkpoint: spawn,
            yaw: 0.0,
            pitch: 0.0,
            collected: 0,
            time: 0.0,
        }
    }

    /// Copies the current player component.
    pub fn explorer(&self) -> Explorer {
        *self
            .scene
            .world
            .get::<&Explorer>(self.player)
            .expect("player exists")
    }

    /// Total number of collectible orbs.
    pub fn total_orbs(&self) -> usize {
        self.orbs.len()
    }

    /// Eye-level first-person camera, interpolating position between fixed ticks.
    /// View angles use the latest input without adding interpolation delay.
    pub fn camera(&self, alpha: f32) -> Camera3D {
        let explorer = self.explorer();
        let position = explorer.previous.lerp(explorer.body.position, alpha) + Vec3::Y * EYE_HEIGHT;
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let direction = Vec3::new(sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch);
        Camera3D {
            position,
            target: position + direction,
            vertical_fov: 75.0,
            up: Vec3::Y,
        }
    }

    /// Advances gameplay using the same systems as interactive play.
    pub fn step(&mut self, input: &Input, dt: f32) {
        self.time += dt;
        let mouse = input.pointer_delta();
        let turn = mouse.x * MOUSE_SENSITIVITY + input.axis(TURN_LEFT, TURN_RIGHT) * dt * 1.8;
        if turn != 0.0 {
            self.yaw = (self.yaw + turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
        }
        self.pitch = (self.pitch - mouse.y * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
        let mut player = self
            .scene
            .world
            .get::<&mut Explorer>(self.player)
            .expect("player exists");
        player.previous = player.body.position;
        if input.pressed(RESET) || player.body.position.y < -15.0 {
            player.body.position = self.checkpoint;
            player.previous = self.checkpoint;
            player.body.velocity = Vec3::ZERO;
            player.body.grounded = false;
            player.jump_buffer = 0.0;
            player.coyote = 0.0;
        }
        let axis =
            Vec2::new(input.axis(LEFT, RIGHT), input.axis(FORWARD, BACK)).clamp_length_max(1.0);
        let (sin, cos) = self.yaw.sin_cos();
        let movement = Vec3::new(
            axis.x * cos - axis.y * sin,
            0.0,
            axis.x * sin + axis.y * cos,
        );
        let speed = if input.down(SPRINT) { 10.0 } else { 6.0 };
        let blend = 1.0 - (-18.0 * dt).exp();
        player.body.velocity.x += (movement.x * speed - player.body.velocity.x) * blend;
        player.body.velocity.z += (movement.z * speed - player.body.velocity.z) * blend;
        player.coyote = if player.body.grounded {
            0.1
        } else {
            (player.coyote - dt).max(0.0)
        };
        player.jump_buffer = if input.pressed(JUMP) {
            0.12
        } else {
            (player.jump_buffer - dt).max(0.0)
        };
        if player.coyote > 0.0 && player.jump_buffer > 0.0 {
            player.body.velocity.y = 11.0;
            player.body.grounded = false;
            player.coyote = 0.0;
            player.jump_buffer = 0.0;
        }
        player.body.velocity.y = (player.body.velocity.y - 26.0 * dt).max(-40.0);
        player.body.move_and_slide(dt, &self.solids);
        let position = player.body.position;
        drop(player);
        self.scene
            .world
            .get::<&mut Transform3D>(self.player)
            .expect("transform exists")
            .position = position;
        for orb in &mut self.orbs {
            if !orb.collected && position.distance_squared(orb.position) < 1.3 * 1.3 {
                orb.collected = true;
                self.collected += 1;
                self.checkpoint = orb.checkpoint;
            }
        }
    }
}

/// Playable first-person presentation over [`MeadowSimulation`].
pub struct Meadow {
    simulation: MeadowSimulation,
    status: String,
    shown_collected: usize,
}

impl Default for Meadow {
    fn default() -> Self {
        let simulation = MeadowSimulation::new();
        Self {
            status: format!("0 / {} orbs", simulation.total_orbs()),
            simulation,
            shown_collected: 0,
        }
    }
}

impl Game for Meadow {
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
            .bind(TURN_LEFT, KeyboardKey::KEY_Q)
            .bind(TURN_RIGHT, KeyboardKey::KEY_E)
            .bind(RESET, KeyboardKey::KEY_R)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.simulation.step(context.input, context.tick.dt);
        if self.shown_collected != self.simulation.collected {
            self.shown_collected = self.simulation.collected;
            self.status = format!(
                "{} / {} orbs",
                self.shown_collected,
                self.simulation.total_orbs()
            );
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let sky = Color::new(208, 231, 237, 255);
        let grass = Color::new(126, 169, 114, 255);
        let cream = Color::new(240, 233, 208, 255);
        let dark = Color::new(37, 58, 60, 255);
        frame.clear(sky);
        let camera = self.simulation.camera(frame.alpha);
        let position = camera.position;
        frame.world_3d(camera, |canvas| {
            canvas.cube(self.simulation.solids[0], grass);
            // Two trails cross the meadow and lead toward the platforming course.
            canvas.cube(
                Aabb3::from_center(Vec3::new(0.0, 0.015, -20.0), Vec3::new(2.2, 0.03, 90.0)),
                Color::new(182, 178, 133, 255),
            );
            canvas.cube(
                Aabb3::from_center(Vec3::new(0.0, 0.025, 8.0), Vec3::new(65.0, 0.03, 2.2)),
                Color::new(182, 178, 133, 255),
            );
            for (i, &platform) in self.simulation.platforms.iter().enumerate() {
                canvas.cube(platform, Color::new(174, 157, 138, 255));
                canvas.cube(
                    Aabb3::from_center(
                        Vec3::new(
                            platform.center().x,
                            platform.max.y + 0.035,
                            platform.center().z,
                        ),
                        Vec3::new(3.25, 0.07, 3.25),
                    ),
                    if i == self.simulation.platforms.len() - 1 {
                        Color::new(236, 190, 94, 255)
                    } else {
                        cream
                    },
                );
            }
            for tree in &self.simulation.trees {
                if (tree.position - position).length_squared() > 55.0 * 55.0 {
                    continue;
                }
                canvas.cube(
                    Aabb3::from_center(tree.position + Vec3::Y * 1.3, Vec3::new(0.6, 2.6, 0.6)),
                    Color::new(116, 91, 70, 255),
                );
                // Low-poly foliage through the explicit raylib escape hatch.
                let p = tree.position + Vec3::Y * 1.8;
                canvas.raw.draw_cylinder(
                    rayengine::raylib::prelude::Vector3::new(p.x, p.y, p.z),
                    0.0,
                    2.0,
                    tree.height - 1.8,
                    7,
                    Color::new(66, 119, 92, 255),
                );
                let p = tree.position + Vec3::Y * (tree.height * 0.48);
                canvas.raw.draw_cylinder(
                    rayengine::raylib::prelude::Vector3::new(p.x, p.y, p.z),
                    0.0,
                    1.6,
                    tree.height * 0.6,
                    7,
                    Color::new(84, 143, 101, 255),
                );
            }
            for (i, orb) in self.simulation.orbs.iter().enumerate() {
                if orb.collected {
                    continue;
                }
                let hover = (self.simulation.time * 2.0 + i as f32).sin() * 0.12;
                canvas.sphere(
                    orb.position + Vec3::Y * hover,
                    0.3,
                    Color::new(255, 205, 95, 255),
                );
                canvas.sphere(orb.position + Vec3::Y * (hover + 0.12), 0.1, cream);
            }
            // A little flag marks the end of the climbing route.
            let goal = self.simulation.platforms.last().expect("course exists");
            let pole = Vec3::new(goal.center().x + 0.9, goal.max.y + 1.8, goal.center().z);
            canvas.cube(Aabb3::from_center(pole, Vec3::new(0.1, 3.6, 0.1)), dark);
            canvas.cube(
                Aabb3::from_center(pole + Vec3::new(0.5, 1.1, 0.0), Vec3::new(1.0, 0.65, 0.07)),
                Color::new(232, 126, 106, 255),
            );
        });
        frame.ui(|ui| {
            ui.circle(ui.logical_size * 0.5, 3.0, dark);
            ui.circle(ui.logical_size * 0.5, 1.5, cream);
            ui.rectangle(
                Aabb2::from_center(Vec2::new(242.0, 71.0), Vec2::new(440.0, 102.0)),
                Color::new(37, 58, 60, 230),
            );
            ui.text(
                "RAYENGINE  /  3D EXAMPLE",
                Vec2::new(36.0, 30.0),
                13.0,
                Color::new(173, 204, 191, 255),
            );
            ui.text("MEADOW", Vec2::new(34.0, 49.0), 34.0, cream);
            ui.text(
                "Explore the trails. Jump the stones. Find the golden orbs.",
                Vec2::new(36.0, 95.0),
                12.0,
                cream,
            );
            let badge = UiRect::bottom_right(Vec2::splat(-22.0), Vec2::new(185.0, 58.0))
                .resolve(ui.logical_size);
            ui.rectangle(badge, Color::new(37, 58, 60, 235));
            ui.text(
                &self.status,
                badge.min + Vec2::new(15.0, 9.0),
                20.0,
                Color::new(255, 205, 95, 255),
            );
            ui.text(
                "Pickups save your checkpoint",
                badge.min + Vec2::new(15.0, 36.0),
                10.0,
                cream,
            );
            let controls = UiRect::top_left(
                Vec2::new(22.0, ui.logical_size.y - 57.0),
                Vec2::new(610.0, 35.0),
            )
            .resolve(ui.logical_size);
            ui.rectangle(controls, Color::new(37, 58, 60, 235));
            ui.text(
                "WASD move   MOUSE look   SPACE jump   SHIFT sprint   R checkpoint",
                controls.min + Vec2::new(13.0, 11.0),
                13.0,
                cream,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1.0 / 120.0;
    fn settle(sim: &mut MeadowSimulation) {
        for _ in 0..120 {
            sim.step(&Input::default(), DT);
        }
    }

    #[test]
    fn camera_is_at_eye_level_and_looks_in_the_mouse_direction() {
        let mut sim = MeadowSimulation::new();
        settle(&mut sim);
        let mut input = Input::default();
        input.add_pointer_delta(Vec2::new(100.0, -80.0));
        sim.step(&input, DT);
        let camera = sim.camera(1.0);
        let body = sim.explorer().body;
        assert!(
            camera
                .position
                .abs_diff_eq(body.position + Vec3::Y * EYE_HEIGHT, 0.00001)
        );
        assert!(body.bounds().contains(camera.position));
        let direction = camera.target - camera.position;
        assert!(direction.x > 0.0 && direction.y > 0.0 && direction.z < 0.0);
        assert!((direction.length() - 1.0).abs() < 0.00001);
        assert_eq!(camera.vertical_fov, 75.0);
        assert_eq!(Meadow::default().cursor_mode(), CursorMode::Captured);
    }

    #[test]
    fn mouse_look_is_independent_of_dt_and_pitch_never_flips_the_camera() {
        let mut input = Input::default();
        input.add_pointer_delta(Vec2::new(100.0, 50.0));
        let mut fast = MeadowSimulation::new();
        let mut slow = MeadowSimulation::new();
        fast.step(&input, 1.0 / 240.0);
        slow.step(&input, 1.0 / 30.0);
        assert_eq!(fast.yaw, slow.yaw);
        assert_eq!(fast.pitch, slow.pitch);
        input.consume_edges();
        let angles = (fast.yaw, fast.pitch);
        fast.step(&input, DT);
        assert_eq!((fast.yaw, fast.pitch), angles);
        for delta in [Vec2::splat(100_000.0), Vec2::splat(-100_000.0)] {
            input.add_pointer_delta(delta);
            fast.step(&input, DT);
            input.consume_edges();
            assert!(fast.pitch.abs() <= MAX_PITCH);
            assert!(fast.yaw.abs() <= std::f32::consts::PI);
            assert!(fast.camera(1.0).view_matrix().is_finite());
        }
    }

    #[test]
    fn movement_follows_yaw_and_stays_horizontal_when_looking_up_or_down() {
        let mut sim = MeadowSimulation::new();
        settle(&mut sim);
        sim.yaw = std::f32::consts::FRAC_PI_2;
        sim.pitch = MAX_PITCH;
        let mut input = Input::default();
        input.set(FORWARD, true);
        for _ in 0..60 {
            sim.step(&input, DT);
        }
        let body = sim.explorer().body;
        assert!(body.velocity.x > 5.9);
        assert!(body.velocity.z.abs() < 0.001);
        assert!(body.grounded);
        assert_eq!(body.velocity.y, 0.0);
    }

    #[test]
    fn camera_interpolates_movement_and_resets_without_a_chase_delay() {
        let mut sim = MeadowSimulation::new();
        settle(&mut sim);
        let mut input = Input::default();
        input.set(JUMP, true);
        sim.step(&input, DT);
        let explorer = sim.explorer();
        for alpha in [0.0, 0.5, 1.0] {
            let expected =
                explorer.previous.lerp(explorer.body.position, alpha) + Vec3::Y * EYE_HEIGHT;
            assert!(sim.camera(alpha).position.abs_diff_eq(expected, 0.00001));
        }
        input.consume_edges();
        input.set(RESET, true);
        sim.step(&input, DT);
        assert!(
            sim.camera(0.0)
                .position
                .abs_diff_eq(sim.checkpoint + Vec3::Y * EYE_HEIGHT, 0.00001)
        );
    }

    #[test]
    fn jump_lands_on_ground_and_does_not_repeat_while_held() {
        let mut sim = MeadowSimulation::new();
        settle(&mut sim);
        assert!(sim.explorer().body.grounded);
        let mut input = Input::default();
        input.set(JUMP, true);
        sim.step(&input, DT);
        input.consume_edges();
        assert!(sim.explorer().body.velocity.y > 0.0);
        for _ in 0..240 {
            sim.step(&input, DT);
        }
        assert!(sim.explorer().body.grounded);
        assert!((sim.explorer().body.position.y - 0.9).abs() < 0.0001);
    }

    #[test]
    fn diagonal_movement_is_normalized() {
        let mut straight = MeadowSimulation::new();
        let mut diagonal = MeadowSimulation::new();
        let mut a = Input::default();
        a.set(RIGHT, true);
        let mut b = Input::default();
        b.set(RIGHT, true);
        b.set(BACK, true);
        for _ in 0..100 {
            straight.step(&a, DT);
            diagonal.step(&b, DT);
        }
        let x = straight.explorer().body.velocity.with_y(0.0).length();
        let diagonal_speed = diagonal.explorer().body.velocity.with_y(0.0).length();
        assert!((x - diagonal_speed).abs() < 0.001);
    }

    #[test]
    fn walking_into_first_stone_stops_at_its_front_face() {
        let mut sim = MeadowSimulation::new();
        settle(&mut sim);
        let stone = sim.platforms[0];
        let mut input = Input::default();
        input.set(FORWARD, true);
        for _ in 0..600 {
            sim.step(&input, DT);
            let body = sim.explorer().body;
            assert!(
                body.bounds().min.z >= stone.max.z,
                "walked inside the stone: {body:?}, stone: {stone:?}"
            );
        }
    }

    #[test]
    fn landing_on_every_stone_keeps_the_body_above_its_top() {
        let mut sim = MeadowSimulation::new();
        for stone in sim.platforms.clone() {
            {
                let mut explorer = sim.scene.world.get::<&mut Explorer>(sim.player).unwrap();
                explorer.body.position = stone.center().with_y(stone.max.y + 3.0);
                explorer.body.velocity = Vec3::ZERO;
                explorer.body.grounded = false;
            }
            settle(&mut sim);
            let body = sim.explorer().body;
            assert!(body.grounded, "did not land on {stone:?}: {body:?}");
            assert!(
                body.bounds().min.y >= stone.max.y,
                "fell inside the stone: {body:?}, stone: {stone:?}"
            );
            assert!((body.bounds().min.y - stone.max.y).abs() < 0.00001);
        }
    }

    #[test]
    #[ignore = "requires a native display and OpenGL context; scripts/native_smoke.sh"]
    fn native_gameplay_first_person_view_stays_above_first_stone() {
        let mut meadow = Meadow::default();
        {
            let sim = &mut meadow.simulation;
            let stone = sim.platforms[0];
            let mut explorer = sim.scene.world.get::<&mut Explorer>(sim.player).unwrap();
            explorer.body.position = stone.center().with_y(stone.max.y + 3.0);
        }
        settle(&mut meadow.simulation);
        let body = meadow.simulation.explorer().body;
        assert!(body.grounded);
        assert!(body.bounds().min.y >= meadow.simulation.platforms[0].max.y);
        meadow.simulation.pitch = -0.9;
        let camera = meadow.simulation.camera(1.0);
        assert_eq!(camera.position.x, body.position.x);
        assert_eq!(camera.position.z, body.position.z);
        let view = Viewport::new(
            Vec2::new(960.0, 540.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Fit,
        )
        .unwrap();
        // Looking down from the player's eyes must show the stone's top face.
        let stone = meadow.simulation.platforms[0];
        let point = Vec3::new(
            stone.center().x + 0.6,
            stone.max.y + 0.07,
            stone.center().z - 0.5,
        );
        let clip =
            camera.projection(&view, 0.01, 1000.0) * camera.view_matrix() * point.extend(1.0);
        let pixel =
            Vec2::new(clip.x / clip.w + 1.0, 1.0 - clip.y / clip.w) * view.logical_size * 0.5;
        let image = crate::render_tests::screenshot(meadow, "meadow-first-person.png");
        assert_eq!(
            image.get_color(pixel.x.round() as i32, pixel.y.round() as i32),
            Color::new(240, 233, 208, 255),
            "first-person camera should see the stone's top from above"
        );
    }

    #[test]
    fn pickup_is_collected_once_and_reset_uses_checkpoint() {
        let mut sim = MeadowSimulation::new();
        sim.scene
            .world
            .get::<&mut Explorer>(sim.player)
            .unwrap()
            .body
            .position = sim.orbs[0].position;
        sim.step(&Input::default(), DT);
        assert_eq!(sim.collected, 1);
        sim.step(&Input::default(), DT);
        assert_eq!(sim.collected, 1);
        sim.scene
            .world
            .get::<&mut Explorer>(sim.player)
            .unwrap()
            .body
            .position = Vec3::new(50.0, -20.0, 50.0);
        sim.step(&Input::default(), DT);
        assert!(sim.explorer().body.position.distance(sim.checkpoint) < 0.01);
    }
}
