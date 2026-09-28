//! Meadow: a small open exploration area with jumping, pickups and checkpoints.

use rayengine::prelude::*;
use rayengine::raylib::prelude::RaylibDraw3D;

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
/// Orbit the camera left.
pub const ORBIT_LEFT: Action = Action(6);
/// Orbit the camera right.
pub const ORBIT_RIGHT: Action = Action(7);
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
    /// Camera orbit angle in radians.
    pub yaw: f32,
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

    /// Advances gameplay using the same systems as interactive play.
    pub fn step(&mut self, input: &Input, dt: f32) {
        self.time += dt;
        self.yaw += input.axis(ORBIT_LEFT, ORBIT_RIGHT) * dt * 1.8;
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
            axis.x * cos + axis.y * sin,
            0.0,
            -axis.x * sin + axis.y * cos,
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

/// Playable third-person presentation over [`MeadowSimulation`].
pub struct Meadow {
    simulation: MeadowSimulation,
    camera: Camera3D,
    previous_camera: Camera3D,
    status: String,
    shown_collected: usize,
}

impl Default for Meadow {
    fn default() -> Self {
        let simulation = MeadowSimulation::new();
        let target = simulation.explorer().body.position;
        let camera = Camera3D {
            position: target + Vec3::new(0.0, 9.0, 16.0),
            target,
            vertical_fov: 55.0,
            up: Vec3::Y,
        };
        Self {
            status: format!("0 / {} orbs", simulation.total_orbs()),
            simulation,
            camera,
            previous_camera: camera,
            shown_collected: 0,
        }
    }
}

impl Game for Meadow {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(FORWARD, KeyboardKey::KEY_W)
            .bind(BACK, KeyboardKey::KEY_S)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(SPRINT, KeyboardKey::KEY_LEFT_SHIFT)
            .bind(ORBIT_LEFT, KeyboardKey::KEY_Q)
            .bind(ORBIT_RIGHT, KeyboardKey::KEY_E)
            .bind(RESET, KeyboardKey::KEY_R)
    }

    fn fixed_update(&mut self, context: &mut Update<'_, '_>) {
        self.simulation.step(context.input, context.tick.dt);
        self.previous_camera = self.camera;
        let target = self.simulation.explorer().body.position + Vec3::Y * 0.7;
        let (sin, cos) = self.simulation.yaw.sin_cos();
        let desired = target + Vec3::new(sin * 16.0, 9.0, cos * 16.0);
        let blend = 1.0 - (-8.0 * context.tick.dt).exp();
        self.camera.position = self.camera.position.lerp(desired, blend);
        self.camera.target = self.camera.target.lerp(target, blend);
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
        let mut camera = self.camera;
        camera.position = self
            .previous_camera
            .position
            .lerp(camera.position, frame.alpha);
        camera.target = self.previous_camera.target.lerp(camera.target, frame.alpha);
        let explorer = self.simulation.explorer();
        let position = explorer.previous.lerp(explorer.body.position, frame.alpha);
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
            // Geometric character, with a ground shadow and eyes.
            canvas.cube(
                Aabb3::from_center(
                    Vec3::new(position.x, 0.03, position.z),
                    Vec3::new(1.0, 0.03, 0.7),
                ),
                Color::new(92, 137, 94, 255),
            );
            canvas.cube(
                Aabb3::from_center(position - Vec3::Y * 0.15, Vec3::new(0.7, 1.1, 0.55)),
                Color::new(224, 132, 108, 255),
            );
            canvas.cube(
                Aabb3::from_center(position + Vec3::Y * 0.6, Vec3::splat(0.7)),
                cream,
            );
            for x in [-0.16, 0.16] {
                canvas.cube(
                    Aabb3::from_center(
                        position + Vec3::new(x, 0.67, 0.36),
                        Vec3::new(0.09, 0.09, 0.04),
                    ),
                    dark,
                );
            }
        });
        frame.ui(|ui| {
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
                "WASD move   SPACE jump   SHIFT sprint   Q/E orbit   R checkpoint",
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
