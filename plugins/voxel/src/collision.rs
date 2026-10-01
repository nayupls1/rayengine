//! Bounded, origin-relative full-cell collision adapters. No world-wide scans.
use crate::glam::Vec3;
use crate::{BlockPos, CollisionKind, VoxelWorld};
use rayengine_core::{collision::Aabb3, first_person::FirstPersonController};

/// Policy for cells whose chunks are not resident.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissingColliders {
    /// Reject the complete query, allowing the game to pause movement.
    #[default]
    Reject,
    /// Treat unavailable cells as solid boundaries.
    Solid,
    /// Explicitly allow movement through unavailable cells.
    Skip,
}
/// Rejected query. The output vector is empty on any failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColliderError {
    /// Nonfinite/inverted bounds, unsupported budget, grid overflow, or relative
    /// coordinates beyond ±1,048,576 units. Rebase before querying distant cells.
    InvalidQuery,
    /// The complete query would visit more cells than the supplied budget.
    BudgetExceeded,
    /// First unavailable cell under the Reject policy.
    Unloaded(BlockPos),
    /// Output allocation failed.
    Allocation,
}
impl std::fmt::Display for ColliderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "voxel collider query: {self:?}")
    }
}
impl std::error::Error for ColliderError {}
/// Counters for the complete rectangular query, including touching cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColliderReport {
    /// Cells read, bounded by max_cells; independent of world resident count.
    pub visited: usize,
    /// Unavailable cells (under Solid or Skip).
    pub missing: usize,
    /// Returned full-cell boxes.
    pub solids: usize,
}
const RELATIVE_LIMIT: f64 = 1_048_576.0;
/// Converts a grid cell to a full-cell box relative to an integer render/physics
/// origin. Subtraction happens in i64 before conversion; distant origins fail.
pub fn block_bounds(position: BlockPos, origin: BlockPos) -> Result<Aabb3, ColliderError> {
    let delta = [
        i64::from(position.x) - i64::from(origin.x),
        i64::from(position.y) - i64::from(origin.y),
        i64::from(position.z) - i64::from(origin.z),
    ];
    if delta.iter().any(|&v| (v as f64).abs() > RELATIVE_LIMIT) {
        return Err(ColliderError::InvalidQuery);
    }
    let min = Vec3::from_array(delta.map(|v| v as f32));
    Ok(Aabb3 {
        min,
        max: min + Vec3::ONE,
    })
}
/// Conservative broadphase for the next controller step, including a possible
/// jump, acceleration toward either walk/sprint speed, and gravity. This encloses
/// the X/Z/Y swept path, not just its endpoint. dt must be finite/nonnegative.
pub fn controller_bounds(
    controller: &FirstPersonController,
    dt: f32,
) -> Result<Aabb3, ColliderError> {
    if !dt.is_finite() || dt < 0.0 {
        return Err(ColliderError::InvalidQuery);
    }
    let body = &controller.body;
    let config = controller.config();
    let horizontal = config.walk_speed.max(config.sprint_speed);
    let speed = Vec3::new(
        body.velocity.x.abs().max(horizontal),
        body.velocity
            .y
            .abs()
            .max(config.jump_speed)
            .max(config.max_fall_speed)
            + config.gravity * dt,
        body.velocity.z.abs().max(horizontal),
    );
    let extent = body.half_size + speed * dt + Vec3::splat(0.001);
    let bounds = Aabb3 {
        min: body.position - extent,
        max: body.position + extent,
    };
    if !bounds.min.is_finite() || !bounds.max.is_finite() || !bounds.min.cmplt(bounds.max).all() {
        return Err(ColliderError::InvalidQuery);
    }
    Ok(bounds)
}
impl VoxelWorld {
    /// Reuses caller-owned output capacity. Reads only cells intersecting bounds,
    /// including cells touching either boundary. Validates the entire region and
    /// cell budget before traversal. Failure clears output, never returns a partial
    /// collider set. Bounds and returned boxes share the supplied integer origin.
    /// Maximum admitted budget is 1,048,576 cells.
    pub fn collect_colliders(
        &self,
        bounds: Aabb3,
        origin: BlockPos,
        max_cells: usize,
        missing: MissingColliders,
        output: &mut Vec<Aabb3>,
    ) -> Result<ColliderReport, ColliderError> {
        output.clear();
        let result = self.collect_inner(bounds, origin, max_cells, missing, output);
        if result.is_err() {
            output.clear();
        }
        result
    }
    fn collect_inner(
        &self,
        bounds: Aabb3,
        origin: BlockPos,
        max_cells: usize,
        missing: MissingColliders,
        output: &mut Vec<Aabb3>,
    ) -> Result<ColliderReport, ColliderError> {
        if max_cells == 0
            || max_cells > 1_048_576
            || !bounds.min.is_finite()
            || !bounds.max.is_finite()
            || !bounds.min.cmple(bounds.max).all()
            || bounds.min.as_dvec3().abs().max_element() >= RELATIVE_LIMIT
            || bounds.max.as_dvec3().abs().max_element() >= RELATIVE_LIMIT
        {
            return Err(ColliderError::InvalidQuery);
        }
        let offset = [origin.x, origin.y, origin.z];
        let mut lo = [0_i32; 3];
        let mut hi = [0_i32; 3];
        for axis in 0..3 {
            // ceil(min)-1 includes a cell ending exactly at the lower boundary.
            let min = f64::from(offset[axis]) + f64::from(bounds.min[axis]).ceil() - 1.0;
            let max = f64::from(offset[axis]) + f64::from(bounds.max[axis]).floor();
            if min < f64::from(i32::MIN) || max > f64::from(i32::MAX) {
                return Err(ColliderError::InvalidQuery);
            }
            lo[axis] = min as i32;
            hi[axis] = max as i32;
        }
        let mut count = 1_u64;
        for axis in 0..3 {
            count = count
                .checked_mul((i64::from(hi[axis]) - i64::from(lo[axis]) + 1) as u64)
                .ok_or(ColliderError::BudgetExceeded)?;
        }
        if count > max_cells as u64 {
            return Err(ColliderError::BudgetExceeded);
        }
        output
            .try_reserve(count as usize)
            .map_err(|_| ColliderError::Allocation)?;
        let mut report = ColliderReport::default();
        let mut cached_pos = None;
        let mut cached_chunk = None;
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                for x in lo[0]..=hi[0] {
                    let position = BlockPos::new(x, y, z);
                    let (chunk_pos, local) = position.split();
                    if cached_pos != Some(chunk_pos) {
                        cached_pos = Some(chunk_pos);
                        cached_chunk = self.chunk(chunk_pos);
                    }
                    report.visited += 1;
                    let solid = if let Some(chunk) = cached_chunk {
                        self.registry()
                            .get(chunk.get(local))
                            .expect("validated block")
                            .collision
                            == CollisionKind::Solid
                    } else {
                        report.missing += 1;
                        match missing {
                            MissingColliders::Reject => {
                                return Err(ColliderError::Unloaded(position));
                            }
                            MissingColliders::Solid => true,
                            MissingColliders::Skip => false,
                        }
                    };
                    if solid {
                        output.push(block_bounds(position, origin)?);
                    }
                }
            }
        }
        report.solids = output.len();
        Ok(report)
    }
}
#[cfg(test)]
mod tests;
