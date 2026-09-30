#![doc = include_str!("../README.md")]
#![doc = "\n\n```no_run"]
#![doc = include_str!("../examples/composition.rs")]
#![doc = "```"]

use rayengine::{assets::Assets, prelude::*};
use std::f32::consts::TAU;

/// Shared game-owned state used by beacon instances.
///
/// The game can add any components to the scene and set its own camera.
#[derive(Default)]
pub struct BeaconWorld {
    /// Game-owned ECS and transforms; the plugin touches only its own entity.
    pub scene: Scene,
    /// Camera chosen by the game, shared by all beacon draw calls.
    pub camera: Camera3D,
}

/// A spinning generated mesh with one exclusively owned mesh and scene entity.
///
/// Initialization may occur again only after [`Plugin::unload`]. An initialized
/// instance must keep using the same world and run's assets. Dropping the plugin
/// leaves its entity and SDK mesh in those owners; unload explicitly when
/// detaching before the world/run ends. A failed init does not spawn an entity.
/// Deleting the entity externally makes update/draw no-ops until unload/reinit.
/// This type is intentionally not `Clone`: its asset/entity ownership is unique.
pub struct Beacon {
    position: Vec3,
    tint: Color,
    radians_per_second: f32,
    angle: f32,
    previous_angle: f32,
    entity: Option<Entity>,
    mesh: Option<MeshId>,
}

impl Beacon {
    /// Configures a beacon; rejects non-finite positions or angular speed.
    /// Negative speed spins in the opposite direction. Configuration allocates
    /// no resources; initialization uploads geometry on the render thread.
    pub fn new(position: Vec3, tint: Color, radians_per_second: f32) -> Result<Self, Error> {
        if !position.is_finite() || !radians_per_second.is_finite() {
            return Err(Error::Config(
                "beacon position and speed must be finite".into(),
            ));
        }
        Ok(Self {
            position,
            tint,
            radians_per_second,
            angle: 0.0,
            previous_angle: 0.0,
            entity: None,
            mesh: None,
        })
    }

    /// Entity created by initialization, or `None` before init/after unload.
    pub fn entity(&self) -> Option<Entity> {
        self.entity
    }

    /// Uploaded mesh owned by this instance, or `None` before init/after unload.
    pub fn mesh(&self) -> Option<MeshId> {
        self.mesh
    }

    /// CPU-only simulation, also usable in display-independent tests.
    /// A zero timestep collapses interpolation history without advancing motion.
    /// Panics unless `dt` is finite and nonnegative.
    pub fn step(&mut self, world: &mut BeaconWorld, dt: f32) {
        assert!(dt.is_finite() && dt >= 0.0, "invalid beacon timestep");
        let Some(entity) = self.entity else { return };
        let Ok(mut transform) = world.scene.world.get::<&mut Transform3D>(entity) else {
            return;
        };
        self.previous_angle = self.angle;
        // Reduce in f64 so every finite f32 configuration/timestep stays finite.
        let delta = (f64::from(self.radians_per_second) * f64::from(dt)) % f64::from(TAU);
        self.angle += delta as f32;
        transform.rotation = Quat::from_rotation_y(self.angle);
        // Keep a continuous interval for interpolation across the wrap boundary.
        if self.angle >= TAU {
            self.angle -= TAU;
            self.previous_angle -= TAU;
        } else if self.angle < 0.0 {
            self.angle += TAU;
            self.previous_angle += TAU;
        }
    }
}

impl Plugin<BeaconWorld> for Beacon {
    fn init(
        &mut self,
        world: &mut BeaconWorld,
        context: &mut InitContext<'_, '_>,
    ) -> Result<(), Error> {
        if self.entity.is_some() || self.mesh.is_some() {
            return Err(Error::Config(
                "unload a beacon before initializing it again".into(),
            ));
        }
        let mesh = context.mesh(&geometry())?;
        self.entity = Some(world.scene.spawn_3d(Transform3D::at(self.position), ()));
        self.mesh = Some(mesh);
        Ok(())
    }

    fn fixed_update(&mut self, world: &mut BeaconWorld, context: &mut Update<'_, '_>) {
        self.step(world, context.tick.dt);
    }

    fn draw(&mut self, world: &BeaconWorld, frame: &mut Frame<'_, '_>) {
        let (Some(entity), Some(mesh)) = (self.entity, self.mesh) else {
            return;
        };
        let Ok(transform) = world.scene.world.get::<&Transform3D>(entity) else {
            return;
        };
        let mut interpolated = *transform;
        interpolated.rotation = Quat::from_rotation_y(
            self.previous_angle + (self.angle - self.previous_angle) * frame.alpha,
        );
        frame.world_3d(world.camera, |canvas| {
            canvas.mesh(mesh, interpolated, self.tint);
        });
    }

    fn unload(&mut self, world: &mut BeaconWorld, assets: &mut Assets<'_>) {
        if let Some(entity) = self.entity.take() {
            let _ = world.scene.despawn(entity);
        }
        if let Some(mesh) = self.mesh.take() {
            assets.unload_mesh(mesh);
        }
        self.angle = 0.0;
        self.previous_angle = 0.0;
    }
}

fn geometry() -> MeshData {
    // Counterclockwise faces of a small octahedron, no file assets required.
    let top = Vec3::Y;
    let bottom = Vec3::NEG_Y;
    let ring = [Vec3::X, Vec3::NEG_Z, Vec3::NEG_X, Vec3::Z];
    let mut positions = Vec::with_capacity(24);
    for i in 0..4 {
        let a = ring[i];
        let b = ring[(i + 1) % 4];
        positions.extend([top, a, b, bottom, b, a]);
    }
    MeshData::new(positions)
}

#[cfg(test)]
mod tests;
