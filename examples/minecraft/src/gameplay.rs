//! CPU-only movement and game-owned interaction rules. No GPU or input bindings.
use rayengine_core::{
    collision::Aabb3,
    first_person::{FirstPersonConfig, FirstPersonController, FirstPersonError, FirstPersonInput},
};
use rayengine_voxel::{
    glam::{DVec3, Vec3},
    prelude::*,
};
/// Maximum hand interaction distance from the current simulation eye, in blocks.
pub const REACH: f64 = 5.0;
/// Origin-relative player with reusable local collider storage.
pub struct Player {
    /// Integer origin shared by physics, cameras, and chunk rendering.
    pub origin: BlockPos,
    /// Existing engine controller; game rules own spawning and world queries.
    pub controller: FirstPersonController,
    colliders: Vec<Aabb3>,
}
impl Player {
    /// Creates a 0.6×1.8×0.6 body at validated global feet coordinates.
    pub fn new(feet: DVec3) -> Result<Self, FirstPersonError> {
        let origin = origin_at(feet).ok_or(FirstPersonError("spawn outside block grid"))?;
        let mut center = (feet - as_global(origin)).as_vec3() + Vec3::new(0.0, 0.9, 0.0);
        // Avoid starting microscopically inside a support cell after f32 rounding.
        center.y = center.y.next_up();
        let mut controller = FirstPersonController::new(
            center,
            Vec3::new(0.6, 1.8, 0.6),
            FirstPersonConfig {
                walk_speed: 4.5,
                sprint_speed: 7.0,
                jump_speed: 8.0,
                gravity: 24.0,
                eye_offset: Vec3::new(0.0, 0.72, 0.0),
                ..Default::default()
            },
        )?;
        let bounds = controller.body.bounds();
        let global_min = as_global(origin) + bounds.min.as_dvec3();
        let global_max = as_global(origin) + bounds.max.as_dvec3();
        if global_min.min_element() < f64::from(i32::MIN)
            || global_max.max_element() > f64::from(i32::MAX) + 1.0
        {
            return Err(FirstPersonError("spawn body outside block grid"));
        }
        controller.set_look(0.0, -0.15)?;
        Ok(Self {
            origin,
            controller,
            colliders: Vec::new(),
        })
    }
    /// Global body center, retaining precision through integer-origin addition.
    pub fn position(&self) -> DVec3 {
        as_global(self.origin) + self.controller.body.position.as_dvec3()
    }
    /// Chunk that streaming should prioritize now.
    pub fn focus(&self) -> ChunkPos {
        origin_at(self.position())
            .expect("validated player within grid")
            .split()
            .0
    }
    /// Moves against current resident blocks. An incomplete query applies look
    /// only and returns an error; it never advances gravity through missing data.
    pub fn step(
        &mut self,
        world: &VoxelWorld,
        input: FirstPersonInput,
        dt: f32,
    ) -> Result<ColliderReport, ColliderError> {
        let query = controller_bounds(&self.controller, dt).and_then(|bounds| {
            world.collect_colliders(
                bounds,
                self.origin,
                4096,
                MissingColliders::Reject,
                &mut self.colliders,
            )
        });
        match query {
            Ok(report) => {
                self.controller.step(input, dt, &self.colliders);
                self.rebase();
                Ok(report)
            }
            Err(error) => {
                self.controller.step(input, 0.0, &[]);
                Err(error)
            }
        }
    }
    fn rebase(&mut self) {
        let next = origin_at(self.position()).expect("query kept player within grid");
        let delta = (as_global(self.origin) - as_global(next)).as_vec3();
        if next == self.origin {
            return;
        }
        let half = self.controller.body.half_size;
        self.controller.body.position =
            shift_center(self.controller.body.position, half, delta, &self.colliders);
        self.controller.previous =
            shift_center(self.controller.previous, half, delta, &self.colliders);
        self.origin = next;
    }
    /// Current eye ray (no interpolation), stopped by unloaded terrain.
    pub fn selection(&self, world: &VoxelWorld) -> Result<Raycast, VoxelError> {
        let camera = self.controller.camera(1.0);
        select(
            world,
            as_global(self.origin) + camera.position.as_dvec3(),
            (camera.target - camera.position).as_dvec3(),
        )
    }
}
// A contact resolved in one f32 coordinate system can overlap after translation:
// e.g. 16.7 + 0.3 rounds to 17, but (16.7 - 16) + 0.3 can exceed 1.
// Retain the separating planes from the old bounds when moving the origin.
fn shift_center(position: Vec3, half: Vec3, delta: Vec3, solids: &[Aabb3]) -> Vec3 {
    let old = Aabb3 {
        min: position - half,
        max: position + half,
    };
    let mut center = position + delta;
    for solid in solids {
        let shifted = Aabb3 {
            min: solid.min + delta,
            max: solid.max + delta,
        };
        let bounds = Aabb3 {
            min: center - half,
            max: center + half,
        };
        if !bounds.intersects(&shifted) {
            continue;
        }
        for axis in 0..3 {
            if old.max[axis] <= solid.min[axis] && center[axis] + half[axis] > shifted.min[axis] {
                center[axis] = shifted.min[axis] - half[axis];
                if center[axis] + half[axis] > shifted.min[axis] {
                    center[axis] = center[axis].next_down();
                }
            } else if old.min[axis] >= solid.max[axis]
                && center[axis] - half[axis] < shifted.max[axis]
            {
                center[axis] = shifted.max[axis] + half[axis];
                if center[axis] - half[axis] < shifted.max[axis] {
                    center[axis] = center[axis].next_up();
                }
            }
        }
    }
    center
}
fn origin_at(position: DVec3) -> Option<BlockPos> {
    if !position.is_finite()
        || position.floor().min_element() < f64::from(i32::MIN)
        || position.floor().max_element() > f64::from(i32::MAX)
    {
        return None;
    }
    let p = position.floor().as_ivec3();
    BlockPos::new(p.x, p.y, p.z).split().0.origin().ok()
}
/// Exact global representation of an integer origin.
pub fn as_global(p: BlockPos) -> DVec3 {
    DVec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z))
}
/// Game selection predicate: visible non-air cells, inclusive five-block reach.
/// No target is returned through missing chunks or beyond the cell budget.
pub fn select(world: &VoxelWorld, eye: DVec3, direction: DVec3) -> Result<Raycast, VoxelError> {
    world.raycast(
        GridRay::new(eye, direction)?,
        RaycastOptions {
            max_distance: REACH,
            max_cells: 64,
            missing: MissingPolicy::Stop,
        },
        |id, def| id != BlockId::AIR && def.render != RenderKind::Invisible,
    )
}
/// Placement rule for this demo: loaded air only, a registered non-air block,
/// and no positive-volume overlap with the player's current collision body.
/// Touching the player's feet is allowed. Inventory admission is applied by the survival caller.
pub fn can_place(world: &VoxelWorld, position: BlockPos, block: BlockId, player: &Player) -> bool {
    block != BlockId::AIR
        && world.registry().get(block).is_some()
        && world.block(position) == Some(BlockId::AIR)
        && block_bounds(position, player.origin)
            .is_ok_and(|bounds| !bounds.intersects(&player.controller.body.bounds()))
}
/// Held hand-mining state. Release, target changes, and chunk revisions reset it.
#[derive(Default)]
pub struct Interaction {
    target: Option<(BlockPos, BlockId, ChunkStamp, u32)>,
    elapsed: f32,
    progress: f32,
}
/// One routed interaction request; survival adds inventory admission around this.
#[derive(Clone, Copy, Debug)]
pub struct InteractionInput {
    /// Held mining request.
    pub mining: bool,
    /// Placement edge; takes priority over mining.
    pub place: bool,
    /// Finite nonnegative seconds.
    pub dt: f32,
    /// Registered placement block; AIR makes placement unavailable.
    pub block: BlockId,
}
/// One interaction tick's selected cell and committed edit, if any.
#[derive(Clone, Copy, Debug, Default)]
pub struct InteractionReport {
    /// Reach-limited current target; None for misses or unloaded terrain.
    pub selected: Option<VoxelHit>,
    /// Successful mutation; collision sees it immediately, rendering remeshes asynchronously.
    pub edit: Option<BlockEdit>,
    /// Fraction of hand-mining duration completed, in `0..=1`.
    pub progress: f32,
}
impl Interaction {
    /// Apply game rules after movement. Hand mining takes hardness seconds;
    /// unbreakable blocks never progress. Right-click places an unlimited supplied
    /// block and takes priority over mining. This low-level helper ignores item
    /// counts; [`crate::survival::Survival::interact`] adds tools, drops and inventory.
    pub fn step(
        &mut self,
        world: &mut VoxelWorld,
        player: &Player,
        mining: bool,
        place: bool,
        dt: f32,
        block: BlockId,
    ) -> Result<InteractionReport, VoxelError> {
        self.apply(
            world,
            player,
            InteractionInput {
                mining,
                place,
                dt,
                block,
            },
            |_| 1.0,
        )
    }
    /// Apply a tool speed policy; changing speed resets the mining timer as does
    /// changing the cell/revision. World edits still use the same loaded/body checks.
    pub fn apply(
        &mut self,
        world: &mut VoxelWorld,
        player: &Player,
        input: InteractionInput,
        speed: impl Fn(BlockId) -> f32,
    ) -> Result<InteractionReport, VoxelError> {
        let InteractionInput {
            mining,
            place,
            dt,
            block,
        } = input;
        assert!(dt.is_finite() && dt >= 0.0);
        let selected = match player.selection(world)?.outcome {
            RaycastOutcome::Hit(hit) => Some(hit),
            _ => None,
        };
        let mut report = InteractionReport {
            selected,
            ..Default::default()
        };
        if !mining || place || selected.is_none() {
            self.reset();
        }
        let Some(hit) = selected else {
            return Ok(report);
        };
        if place {
            if let Some(position) = hit
                .adjacent
                .filter(|&pos| can_place(world, pos, block, player))
            {
                report.edit = world.set_block(position, block)?;
            }
        } else if mining {
            let stamp = world
                .stamp(hit.position.split().0)
                .expect("resident ray hit");
            let speed = speed(hit.block);
            assert!(speed.is_finite() && speed > 0.0);
            let target = (hit.position, hit.block, stamp, speed.to_bits());
            if self.target != Some(target) {
                self.reset();
                self.target = Some(target);
            }
            if let Some(hardness) = world
                .registry()
                .get(hit.block)
                .expect("registered hit")
                .hardness
            {
                self.elapsed += dt * speed;
                self.progress = if hardness == 0.0 {
                    1.0
                } else {
                    (self.elapsed / hardness).min(1.0)
                };
                report.progress = self.progress;
                if self.progress >= 1.0 {
                    report.edit = world.set_block(hit.position, BlockId::AIR)?;
                    self.reset();
                }
            }
        }
        Ok(report)
    }
    /// Cancel a held mining gesture when UI, tool selection, or respawn changes.
    pub fn reset(&mut self) {
        self.target = None;
        self.elapsed = 0.0;
        self.progress = 0.0;
    }
}
#[cfg(test)]
mod tests;
