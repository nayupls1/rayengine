//! Predictable arcade physics with continuous translation and no rotation.
//!
//! Worlds are game-owned and stepped explicitly from the fixed update. The
//! legacy [`crate::collision::Body2D`] and [`crate::collision::Body3D`] remain
//! independent. See [`crate::physics::guide`] for setup and solver limits.

mod geometry;
mod grid;
mod solver;

use crate::collision::{Aabb2, Aabb3};
use crate::events::Events;
use crate::time::Tick;
use geometry::Shape;
use glam::{Vec2, Vec3};
use std::collections::{BTreeMap, BTreeSet};

/// Checked usage guide, also available without the rendering SDK.
#[doc = include_str!("../docs/physics.md")]
pub mod guide {}

/// Stable, never-reused world collider identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId(pub u64);

/// Motion authority. Only dynamic bodies receive forces and collision response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Immovable solid; velocity is ignored.
    Static,
    /// Game-driven velocity; pushes dynamics and carries riders, ignoring solids.
    Kinematic,
    /// Integrated and separated using inverse mass.
    Dynamic,
}

/// Symmetric collision filtering, shared by bodies, static solids and triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionFilter {
    /// Bitset of layers this collider belongs to.
    pub layers: u32,
    /// Bitset of layers this collider accepts.
    pub mask: u32,
}
impl Default for CollisionFilter {
    fn default() -> Self {
        Self {
            layers: 1,
            mask: u32::MAX,
        }
    }
}
impl CollisionFilter {
    /// Both masks must accept the other collider's layers.
    pub fn allows(self, other: Self) -> bool {
        self.mask & other.layers != 0 && other.mask & self.layers != 0
    }
}

/// Trigger transition at a fixed tick boundary. A swept pass emits enter/exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerPhase {
    /// Pair began overlapping, or crossed the trigger during this tick.
    Enter,
    /// Pair remained overlapping at consecutive boundaries.
    Stay,
    /// Pair stopped overlapping, including removal or filter changes.
    Exit,
}
/// Typed trigger event. Queues append until the game clears or drains them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriggerEvent {
    /// Trigger identity.
    pub trigger: BodyId,
    /// Other collider identity (including another trigger).
    pub other: BodyId,
    /// Transition kind.
    pub phase: TriggerPhase,
}

/// Bounded solver work and any unconsumed simulation time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepReport {
    /// Number of continuous solid contacts and overlap repairs processed.
    pub contacts: usize,
    /// Time discarded if the 256-contact budget was exhausted.
    pub dropped_time: f32,
    /// Whether the bounded depenetration pass left any solid overlaps.
    pub unresolved_overlaps: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Contact<const N: usize> {
    normal: [f32; N],
    depth: f32,
}

macro_rules! dimension {
    ($n:literal, $vec:ident, $aabb:ident, $shape:ident, $round_variant:ident, $body:ident, $world:ident, $grid:ident, $penetration:ident) => {
        /// Local centered collision geometry, with no rotation.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum $shape {
            /// Axis-aligned box with strictly positive half dimensions.
            Box {
                /// Half dimensions in world units.
                half_size: $vec,
            },
            /// Round shape with a strictly positive radius.
            $round_variant {
                /// Radius in world units.
                radius: f32,
            },
        }
        impl $shape {
            /// Box from full dimensions. Panics for nonfinite/nonpositive sizes.
            pub fn box_shape(size: $vec) -> Self {
                let shape = Self::Box {
                    half_size: size * 0.5,
                };
                assert!(shape.internal().valid());
                shape
            }
            /// Round shape. Panics for nonfinite/nonpositive radii.
            pub fn round(radius: f32) -> Self {
                assert!(radius.is_finite() && radius > 0.0);
                Self::$round_variant { radius }
            }
            fn internal(self) -> Shape<$n> {
                match self {
                    Self::Box { half_size } => Shape::Box(half_size.to_array()),
                    Self::$round_variant { radius } => Shape::Round(radius),
                }
            }
            /// World bounds for a finite center. Panics for invalid geometry.
            pub fn bounds(self, center: $vec) -> $aabb {
                assert!(center.is_finite() && self.internal().valid());
                let (min, max) = self.internal().bounds(center.to_array());
                $aabb {
                    min: $vec::from_array(min),
                    max: $vec::from_array(max),
                }
            }
            /// Positive penetration. Normal moves `self` away from `other`.
            /// Coincident centers use the positive X direction as a stable tie.
            pub fn overlap(
                self,
                center: $vec,
                other: Self,
                other_center: $vec,
            ) -> Option<$penetration> {
                assert!(
                    center.is_finite()
                        && other_center.is_finite()
                        && self.internal().valid()
                        && other.internal().valid()
                );
                geometry::overlap(
                    self.internal(),
                    center.to_array(),
                    other.internal(),
                    other_center.to_array(),
                )
                .map(|c| $penetration {
                    normal: $vec::from_array(c.normal),
                    depth: c.depth,
                })
            }
        }
        /// Minimum translation needed to remove a positive overlap.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $penetration {
            /// Unit direction from the second shape toward the first.
            pub normal: $vec,
            /// Translation distance along the normal.
            pub depth: f32,
        }
        /// World collider, including solids and nonblocking trigger volumes.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $body {
            /// World-space center.
            pub position: $vec,
            /// Local collision geometry.
            pub shape: $shape,
            /// Intrinsic velocity, excluding moving-platform carry.
            pub velocity: $vec,
            /// Static, kinematic or dynamic motion authority.
            pub kind: BodyKind,
            /// Layer membership and symmetric acceptance mask.
            pub filter: CollisionFilter,
            /// Nonblocking volume that generates typed trigger events.
            pub is_trigger: bool,
            /// Positive mass; affects dynamic/dynamic push-apart and impulses.
            pub mass: f32,
            /// Optional per-body acceleration, in world units per second squared.
            pub gravity: Option<$vec>,
            /// Bounce coefficient in `[0, 1]`. Pair response uses the maximum.
            pub restitution: f32,
            /// Contact friction in `[0, 1]`; scales the normal impulse budget.
            pub friction: f32,
            /// Nonnegative drag per second, applied as `velocity / (1 + drag * dt)`.
            pub drag: f32,
            /// Optional nonnegative intrinsic speed cap, applied after forces.
            /// Collision impulses may exceed it until the next tick.
            pub max_speed: Option<f32>,
            /// Contact with a solid below (Y down in 2D, Y up in 3D).
            pub grounded: bool,
        }
        impl $body {
            /// Stationary dynamic collider with mass one and no automatic gravity.
            pub fn new(position: $vec, shape: $shape) -> Self {
                assert!(position.is_finite() && shape.internal().valid());
                Self {
                    position,
                    shape,
                    velocity: $vec::ZERO,
                    kind: BodyKind::Dynamic,
                    filter: CollisionFilter::default(),
                    is_trigger: false,
                    mass: 1.0,
                    gravity: None,
                    restitution: 0.0,
                    friction: 0.0,
                    drag: 0.0,
                    max_speed: None,
                    grounded: false,
                }
            }
            /// Current conservative world bounds.
            pub fn bounds(&self) -> $aabb {
                self.shape.bounds(self.position)
            }
        }
        /// Reusable uniform-grid broadphase with stable, sorted query results.
        /// Touching bounds are candidates; callers perform their own narrowphase.
        #[derive(Debug)]
        pub struct $grid {
            inner: grid::Grid<$n>,
        }
        impl $grid {
            /// Creates a grid with a finite positive cell width.
            pub fn new(cell_size: f32) -> Self {
                Self {
                    inner: grid::Grid::new(cell_size),
                }
            }
            /// Inserts or replaces caller-owned identity and bounds.
            /// Panics for nonfinite/reversed bounds.
            pub fn insert(&mut self, id: u64, bounds: $aabb) {
                self.inner
                    .insert(id, bounds.min.to_array(), bounds.max.to_array());
            }
            /// Removes an identity, if present.
            pub fn remove(&mut self, id: u64) {
                self.inner.remove(id);
            }
            /// Removes all entries.
            pub fn clear(&mut self) {
                self.inner.clear();
            }
            /// Sorted unique IDs whose bounds touch or overlap the query.
            /// Huge objects and queries use a bounded-storage linear fallback.
            pub fn query(&self, bounds: $aabb) -> Vec<u64> {
                self.inner
                    .query(bounds.min.to_array(), bounds.max.to_array())
            }
        }
        /// Small deterministic arcade world. Own it in game state and call
        /// `step(ctx.tick, &mut events)` exactly once per fixed update.
        #[derive(Debug)]
        pub struct $world {
            bodies: BTreeMap<BodyId, $body>,
            next_id: u64,
            overlaps: BTreeSet<(BodyId, BodyId)>,
            grid: grid::Grid<$n>,
        }
        impl $world {
            /// Creates an empty world. Cell size should approximate typical bodies.
            pub fn new(cell_size: f32) -> Self {
                Self {
                    bodies: BTreeMap::new(),
                    next_id: 0,
                    overlaps: BTreeSet::new(),
                    grid: grid::Grid::new(cell_size),
                }
            }
            /// Inserts a body and returns an identity that is never reused.
            pub fn insert(&mut self, body: $body) -> BodyId {
                let id = BodyId(self.next_id);
                self.next_id = self
                    .next_id
                    .checked_add(1)
                    .expect("physics identity exhausted");
                self.bodies.insert(id, body);
                id
            }
            /// Removes a collider. Active trigger pairs exit on the next step.
            pub fn remove(&mut self, id: BodyId) -> Option<$body> {
                self.bodies.remove(&id)
            }
            /// Reads a collider by stable identity.
            pub fn body(&self, id: BodyId) -> Option<&$body> {
                self.bodies.get(&id)
            }
            /// Edits a collider. Inputs are validated at the next step.
            pub fn body_mut(&mut self, id: BodyId) -> Option<&mut $body> {
                self.bodies.get_mut(&id)
            }
            /// All colliders in stable identity order.
            pub fn iter(&self) -> impl Iterator<Item = (BodyId, &$body)> {
                self.bodies.iter().map(|(id, b)| (*id, b))
            }
            /// Advances one fixed tick; appends trigger transitions to `events`.
            /// Panics for nonfinite/invalid body properties or negative/nonfinite dt.
            /// Kinematic bodies follow velocity even through static geometry.
            pub fn step(&mut self, tick: Tick, events: &mut Events<TriggerEvent>) -> StepReport {
                let mut nodes: Vec<_> = self
                    .bodies
                    .iter()
                    .map(|(id, b)| solver::Node {
                        id: *id,
                        position: b.position.to_array(),
                        shape: b.shape.internal(),
                        velocity: b.velocity.to_array(),
                        kind: b.kind,
                        filter: b.filter,
                        trigger: b.is_trigger,
                        mass: b.mass,
                        gravity: b.gravity.map(|g| g.to_array()),
                        restitution: b.restitution,
                        friction: b.friction,
                        drag: b.drag,
                        max_speed: b.max_speed,
                        grounded: false,
                        carry: [0.0; $n],
                    })
                    .collect();
                let report = solver::step(
                    &mut nodes,
                    tick.dt,
                    &mut self.grid,
                    &mut self.overlaps,
                    events,
                );
                for node in nodes {
                    let b = self.bodies.get_mut(&node.id).unwrap();
                    b.position = $vec::from_array(node.position);
                    b.velocity = $vec::from_array(node.velocity);
                    b.grounded = node.grounded;
                }
                report
            }
        }
    };
}
dimension!(
    2,
    Vec2,
    Aabb2,
    Shape2D,
    Circle,
    PhysicsBody2D,
    PhysicsWorld2D,
    UniformGrid2D,
    Penetration2D
);
dimension!(
    3,
    Vec3,
    Aabb3,
    Shape3D,
    Sphere,
    PhysicsBody3D,
    PhysicsWorld3D,
    UniformGrid3D,
    Penetration3D
);

#[cfg(test)]
mod tests;

// Used by the independent swept character API before its axis sweeps.
pub(crate) fn depenetrate_box<const N: usize>(
    position: &mut [f32; N],
    half: [f32; N],
    solids: impl Iterator<Item = ([f32; N], [f32; N])> + Clone,
) {
    for _ in 0..16 {
        let mut changed = false;
        for (min, max) in solids.clone() {
            let center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
            let extent = std::array::from_fn(|i| (max[i] - min[i]) * 0.5);
            if let Some(contact) =
                geometry::overlap(Shape::Box(half), *position, Shape::Box(extent), center)
            {
                *position = geometry::add(
                    *position,
                    geometry::scale(contact.normal, contact.depth + 1e-5),
                );
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}
