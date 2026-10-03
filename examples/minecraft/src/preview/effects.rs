//! Short-lived block-break debris using the optional particles plugin. Bursts are
//! anchored to global block centers, so render-origin rebasing never moves them.
use crate::gameplay::as_global;
use rayengine::{
    prelude::*,
    raylib::prelude::{RaylibDraw, RaylibDraw3D},
    render::Canvas3D,
};
use rayengine_particles::{Emitter, EmitterConfig};
use rayengine_voxel::{
    glam::{DVec3, Vec4},
    prelude::BlockPos,
};

/// Simultaneous bursts; the oldest is replaced when mining very quickly.
const MAX_BURSTS: usize = 8;
const PER_BURST: usize = 18;

struct Burst {
    anchor: DVec3,
    emitter: Emitter,
}
/// Bounded pool of debris bursts.
#[derive(Default)]
pub(super) struct Debris {
    bursts: Vec<Burst>,
    seed: u64,
}
impl Debris {
    /// Spawn debris tinted like the broken block's item.
    pub fn burst(&mut self, position: BlockPos, [r, g, b]: [u8; 3]) {
        let color = Vec4::new(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        );
        self.seed = self.seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let Ok(mut emitter) = Emitter::new(EmitterConfig {
            capacity: PER_BURST,
            max_spawn: PER_BURST,
            lifetime: [0.35, 0.8],
            position_spread: Vec3::splat(0.32),
            velocity: Vec3::new(0.0, 2.2, 0.0),
            velocity_spread: Vec3::new(1.8, 1.2, 1.8),
            acceleration: Vec3::new(0.0, -14.0, 0.0),
            start_color: color,
            end_color: color * Vec4::new(0.6, 0.6, 0.6, 1.0),
            start_size: 0.14,
            end_size: 0.04,
            seed: self.seed,
            ..Default::default()
        }) else {
            return;
        };
        emitter.burst(PER_BURST);
        if self.bursts.len() == MAX_BURSTS {
            self.bursts.remove(0);
        }
        self.bursts.push(Burst {
            anchor: as_global(position) + DVec3::splat(0.5),
            emitter,
        });
    }
    /// Advance one fixed tick and drop finished bursts.
    pub fn step(&mut self, dt: f32) {
        self.bursts
            .retain_mut(|b| b.emitter.step(dt).is_ok() && !b.emitter.is_empty());
    }
    /// Live debris count, for the debug overlay.
    pub fn len(&self) -> usize {
        self.bursts.iter().map(|b| b.emitter.len()).sum()
    }
    /// Draw as small cubes relative to the current render origin.
    pub fn draw<D: RaylibDraw + RaylibDraw3D>(
        &self,
        canvas: &mut Canvas3D<'_, D>,
        origin: BlockPos,
        alpha: f32,
    ) {
        let origin = as_global(origin);
        for burst in &self.bursts {
            let offset = (burst.anchor - origin).as_vec3();
            for particle in burst.emitter.particles() {
                let look = burst.emitter.appearance(particle, alpha);
                let c = look.color * 255.0;
                canvas.cube(
                    Aabb3::from_center(
                        offset + particle.interpolated_position(alpha),
                        Vec3::splat(look.size),
                    ),
                    Color::new(c.x as u8, c.y as u8, c.z as u8, 255),
                );
            }
        }
    }
}
