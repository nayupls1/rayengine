//! Bounded 3D grid traversal, separate from gameplay selection/mining policies.
use crate::{BlockDef, BlockId, BlockPos, Face, VoxelError, VoxelWorld};
use rayengine_core::{glam::DVec3, spatial::Ray3};

/// Largest admitted traversal budget; keeps user-provided queries bounded.
pub const MAX_RAY_CELLS: u32 = 1_048_576;

/// Normalized f64 ray. Distances are world units (one block per unit).
/// f64 preserves individual cells throughout the entire i32 coordinate grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridRay {
    origin: DVec3,
    direction: DVec3,
}
impl GridRay {
    /// Validates and normalizes without overflow/underflow, including tiny
    /// directions. The forward starting cell must fit in the i32 grid.
    pub fn new(origin: DVec3, direction: DVec3) -> Result<Self, VoxelError> {
        if !origin.is_finite() || !direction.is_finite() {
            return Err(VoxelError::InvalidRay);
        }
        let scale = direction.abs().max_element();
        if scale == 0.0 {
            return Err(VoxelError::InvalidRay);
        }
        let scaled = direction / scale;
        let ray = Self {
            origin,
            direction: scaled / scaled.length(),
        };
        ray.start()?;
        Ok(ray)
    }
    /// VoxelWorld origin.
    pub fn origin(self) -> DVec3 {
        self.origin
    }
    /// Unit direction.
    pub fn direction(self) -> DVec3 {
        self.direction
    }
    fn start(self) -> Result<(BlockPos, Option<Face>), VoxelError> {
        let mut cell = [0_i32; 3];
        let mut face = None;
        for (axis, component) in cell.iter_mut().enumerate() {
            let origin = self.origin[axis];
            let dir = self.direction[axis];
            let boundary = origin.fract() == 0.0;
            let floor = origin.floor();
            let value = if boundary && dir < 0.0 {
                floor - 1.0
            } else {
                floor
            };
            if value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
                return Err(VoxelError::InvalidRay);
            }
            *component = value as i32;
            if face.is_none() && boundary && dir != 0.0 {
                face = Some(Face::entering(axis, if dir > 0.0 { 1 } else { -1 }));
            }
        }
        Ok((BlockPos::new(cell[0], cell[1], cell[2]), face))
    }

    /// Traverses without allocations; calls the source once per visited cell.
    /// At an integer boundary, the starting cell is the one immediately forward
    /// along each moving axis. Parallel axes use floor (half-open ownership).
    /// Exact floating-point edge/corner ties advance all tied axes together,
    /// skipping zero-length side cells; X/Y/Z order selects the reported face.
    /// Strict interior starts have no entry face/adjacent placement cell.
    /// Distance is inclusive. Missing data and budget exhaustion are distinct
    /// outcomes, never silently reported as an unobstructed miss.
    pub fn cast(
        self,
        options: RaycastOptions,
        mut source: impl FnMut(BlockPos) -> RayCell,
    ) -> Result<Raycast, VoxelError> {
        if !options.max_distance.is_finite()
            || options.max_distance < 0.0
            || options.max_cells == 0
            || options.max_cells > MAX_RAY_CELLS
        {
            return Err(VoxelError::InvalidRayOptions);
        }
        let (mut position, mut face) = self.start()?;
        let step = self.direction.to_array().map(|d| {
            if d > 0.0 {
                1
            } else if d < 0.0 {
                -1
            } else {
                0
            }
        });
        let axes = [position.x, position.y, position.z];
        let mut next = [f64::INFINITY; 3];
        let mut inverse = [f64::INFINITY; 3];
        for axis in 0..3 {
            if step[axis] != 0 {
                let boundary = f64::from(axes[axis]) + if step[axis] > 0 { 1.0 } else { 0.0 };
                next[axis] = (boundary - self.origin[axis]) / self.direction[axis];
                inverse[axis] = 1.0 / self.direction[axis];
            }
        }
        let mut distance = 0.0;
        for visited in 1..=options.max_cells {
            let outcome = match source(position) {
                RayCell::Hit(block) => Some(RaycastOutcome::Hit(VoxelHit {
                    position,
                    block,
                    distance,
                    point: self.origin + self.direction * distance,
                    face,
                    adjacent: face.and_then(|f| position.neighbor(f)),
                })),
                RayCell::Missing if options.missing == MissingPolicy::Stop => {
                    Some(RaycastOutcome::Unloaded { position, distance })
                }
                _ => None,
            };
            if let Some(outcome) = outcome {
                return Ok(Raycast {
                    outcome,
                    visited_cells: visited,
                });
            }
            let nearest = next.into_iter().fold(f64::INFINITY, f64::min);
            if nearest > options.max_distance {
                return Ok(Raycast {
                    outcome: RaycastOutcome::Miss,
                    visited_cells: visited,
                });
            }
            let mut target = [position.x, position.y, position.z];
            face = None;
            for axis in 0..3 {
                if next[axis] == nearest {
                    let Some(value) = target[axis].checked_add(step[axis]) else {
                        return Ok(Raycast {
                            outcome: RaycastOutcome::OutOfBounds { distance: nearest },
                            visited_cells: visited,
                        });
                    };
                    target[axis] = value;
                    if face.is_none() {
                        face = Some(Face::entering(axis, step[axis]));
                    }
                    // Recompute from the original ray to avoid accumulating
                    // rounding error at repeated corner crossings/reach limits.
                    let boundary = f64::from(value) + if step[axis] > 0 { 1.0 } else { 0.0 };
                    next[axis] = (boundary - self.origin[axis]) * inverse[axis];
                }
            }
            position = BlockPos::new(target[0], target[1], target[2]);
            if visited == options.max_cells {
                return Ok(Raycast {
                    outcome: RaycastOutcome::BudgetExhausted {
                        next_position: position,
                        distance: nearest,
                    },
                    visited_cells: visited,
                });
            }
            distance = nearest;
        }
        unreachable!("positive validated traversal budget")
    }
}
impl TryFrom<Ray3> for GridRay {
    type Error = VoxelError;
    /// Converts the engine's f32 camera ray. Precision already lost in f32 is not
    /// recovered; use GridRay::new with f64 positions for large-world queries.
    fn try_from(ray: Ray3) -> Result<Self, Self::Error> {
        Self::new(ray.origin().as_dvec3(), ray.direction().as_dvec3())
    }
}

/// Source response for one cell. Query policy selects hit blocks independently
/// of whether they collide or render; unloaded data remains explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RayCell {
    /// Loaded cell that this query does not select.
    Empty,
    /// Selected cell carrying its block ID.
    Hit(BlockId),
    /// Cell data is unavailable.
    Missing,
}
/// Query handling of unavailable data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissingPolicy {
    /// Stop and report the first unavailable cell (default).
    #[default]
    Stop,
    /// Explicitly skip unavailable cells, without declaring them loaded air.
    Skip,
}
/// Finite distance and hard cell budget for a query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaycastOptions {
    /// Inclusive maximum world-unit distance, finite and nonnegative.
    pub max_distance: f64,
    /// Maximum visited cells, 1..=MAX_RAY_CELLS.
    pub max_cells: u32,
    /// How unavailable data should be handled.
    pub missing: MissingPolicy,
}
impl Default for RaycastOptions {
    fn default() -> Self {
        Self {
            max_distance: 8.0,
            max_cells: 256,
            missing: MissingPolicy::Stop,
        }
    }
}
/// Contact with one selected cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelHit {
    /// Selected world cell.
    pub position: BlockPos,
    /// Source's selected block ID.
    pub block: BlockId,
    /// Nonnegative world-unit distance.
    pub distance: f64,
    /// VoxelWorld-space contact point.
    pub point: DVec3,
    /// Outward entry face, or None for an interior start.
    pub face: Option<Face>,
    /// Face-adjacent placement cell, or None for an interior start/grid edge.
    pub adjacent: Option<BlockPos>,
}
/// Distinguishes selection, a completed miss, and incomplete/limited queries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RaycastOutcome {
    /// A selected cell.
    Hit(VoxelHit),
    /// All traversed cells through the requested distance were unselected.
    Miss,
    /// Query stopped at unavailable data.
    Unloaded {
        /// First unavailable cell.
        position: BlockPos,
        /// Distance entering it.
        distance: f64,
    },
    /// Cell budget exhausted before reaching the distance limit.
    BudgetExhausted {
        /// First cell not queried.
        next_position: BlockPos,
        /// Distance entering the next cell.
        distance: f64,
    },
    /// Forward traversal left the representable i32 cell grid.
    OutOfBounds {
        /// Distance to the grid's boundary.
        distance: f64,
    },
}
/// Query outcome plus the number of source calls (including a hit/missing cell).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Raycast {
    /// Completed or limited traversal outcome.
    pub outcome: RaycastOutcome,
    /// Source calls performed, bounded by options.max_cells.
    pub visited_cells: u32,
}
impl VoxelWorld {
    /// Queries resident storage with a game-selected block predicate.
    /// Predicate is called only for loaded, registered blocks (including air).
    pub fn raycast(
        &self,
        ray: GridRay,
        options: RaycastOptions,
        mut select: impl FnMut(BlockId, &BlockDef) -> bool,
    ) -> Result<Raycast, VoxelError> {
        // Immutable world access makes a resident borrow valid for this cast.
        // Resolve the hash entry only when traversal crosses a chunk boundary,
        // including caching absent chunks for explicit MissingPolicy::Skip.
        let mut cached_position = None;
        let mut cached_chunk = None;
        ray.cast(options, |pos| {
            let (chunk_pos, local) = pos.split();
            if cached_position != Some(chunk_pos) {
                cached_position = Some(chunk_pos);
                cached_chunk = self.chunk(chunk_pos);
            }
            let Some(chunk) = cached_chunk else {
                return RayCell::Missing;
            };
            let id = chunk.get(local);
            if select(id, self.registry().get(id).expect("validated resident ID")) {
                RayCell::Hit(id)
            } else {
                RayCell::Empty
            }
        })
    }
}

#[cfg(test)]
mod tests;
