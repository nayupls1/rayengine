//! Rebuildable, owned bounding-volume hierarchies for read-heavy queries.

use super::{
    Frustum2D, Frustum3D, Ray2, Ray3, RayHit2, RayHit3, SpatialError, ray::slab, valid_distance,
};
use crate::collision::{Aabb2, Aabb3};
use glam::{Vec2, Vec3};
use std::cmp::Ordering;

const LEAF_CAPACITY: usize = 8;

#[derive(Clone, Copy)]
struct Bounds<const N: usize> {
    min: [f32; N],
    max: [f32; N],
}

impl<const N: usize> Bounds<N> {
    fn valid(self) -> bool {
        (0..N).all(|axis| {
            self.min[axis].is_finite()
                && self.max[axis].is_finite()
                && self.min[axis] <= self.max[axis]
        })
    }

    fn union(mut self, other: Self) -> Self {
        for axis in 0..N {
            self.min[axis] = self.min[axis].min(other.min[axis]);
            self.max[axis] = self.max[axis].max(other.max[axis]);
        }
        self
    }

    fn overlaps(self, other: Self) -> bool {
        (0..N).all(|axis| self.min[axis] < other.max[axis] && self.max[axis] > other.min[axis])
    }

    fn center(self, axis: usize) -> f32 {
        self.min[axis] * 0.5 + self.max[axis] * 0.5
    }
}

#[derive(Clone, Copy)]
struct Entry<T, const N: usize> {
    id: T,
    bounds: Bounds<N>,
    rank: usize,
}

enum Children {
    Leaf { start: usize, end: usize },
    Branch { left: usize, right: usize },
}

struct Node<const N: usize> {
    bounds: Bounds<N>,
    children: Children,
}

struct Bvh<T, const N: usize> {
    entries: Vec<Entry<T, N>>,
    nodes: Vec<Node<N>>,
    staging_entries: Vec<Entry<T, N>>,
    staging_nodes: Vec<Node<N>>,
}

impl<T: Copy, const N: usize> Bvh<T, N> {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            nodes: Vec::new(),
            staging_entries: Vec::new(),
            staging_nodes: Vec::new(),
        }
    }

    fn rebuild(
        &mut self,
        source: impl IntoIterator<Item = (T, Bounds<N>)>,
    ) -> Result<usize, SpatialError> {
        self.staging_entries.clear();
        for (rank, (id, bounds)) in source.into_iter().enumerate() {
            if !bounds.valid() {
                return Err(SpatialError::InvalidBounds(rank));
            }
            self.staging_entries.push(Entry { id, bounds, rank });
        }
        self.staging_nodes.clear();
        if !self.staging_entries.is_empty() {
            build(&mut self.staging_entries, &mut self.staging_nodes, 0);
        }
        std::mem::swap(&mut self.entries, &mut self.staging_entries);
        std::mem::swap(&mut self.nodes, &mut self.staging_nodes);
        Ok(self.entries.len())
    }

    fn visit(&self, intersects: impl Fn(Bounds<N>) -> bool, mut emit: impl FnMut(T)) {
        if !self.nodes.is_empty() {
            self.visit_node(0, &intersects, &mut emit);
        }
    }

    fn visit_node(
        &self,
        index: usize,
        intersects: &impl Fn(Bounds<N>) -> bool,
        emit: &mut impl FnMut(T),
    ) {
        let node = &self.nodes[index];
        if !intersects(node.bounds) {
            return;
        }
        match node.children {
            Children::Leaf { start, end } => {
                for entry in &self.entries[start..end] {
                    if intersects(entry.bounds) {
                        emit(entry.id);
                    }
                }
            }
            Children::Branch { left, right } => {
                self.visit_node(left, intersects, emit);
                self.visit_node(right, intersects, emit);
            }
        }
    }

    fn nearest(
        &self,
        origin: [f32; N],
        direction: [f32; N],
        max_distance: f32,
    ) -> Option<(T, f32, [f32; N])> {
        let mut best = None;
        if !self.nodes.is_empty() {
            self.nearest_node(0, origin, direction, max_distance, &mut best);
        }
        best.map(|(id, distance, normal, _)| (id, distance, normal))
    }

    fn nearest_node(
        &self,
        index: usize,
        origin: [f32; N],
        direction: [f32; N],
        max_distance: f32,
        best: &mut Option<(T, f32, [f32; N], usize)>,
    ) {
        let node = &self.nodes[index];
        let limit = best.as_ref().map_or(max_distance, |hit| hit.1);
        if slab(origin, direction, node.bounds.min, node.bounds.max, limit).is_none() {
            return;
        }
        match node.children {
            Children::Leaf { start, end } => {
                for entry in &self.entries[start..end] {
                    let limit = best.as_ref().map_or(max_distance, |hit| hit.1);
                    if let Some((distance, normal)) =
                        slab(origin, direction, entry.bounds.min, entry.bounds.max, limit)
                        && best.as_ref().is_none_or(|hit| {
                            distance < hit.1 || (distance == hit.1 && entry.rank < hit.3)
                        })
                    {
                        *best = Some((entry.id, distance, normal, entry.rank));
                    }
                }
            }
            Children::Branch { left, right } => {
                let left_distance = self.node_distance(left, origin, direction, limit);
                let right_distance = self.node_distance(right, origin, direction, limit);
                // Visit the closer child first to tighten the search limit.
                match (left_distance, right_distance) {
                    (Some(a), Some(b)) => {
                        let (first, second) = if a <= b { (left, right) } else { (right, left) };
                        self.nearest_node(first, origin, direction, max_distance, best);
                        self.nearest_node(second, origin, direction, max_distance, best);
                    }
                    (Some(_), None) => {
                        self.nearest_node(left, origin, direction, max_distance, best)
                    }
                    (None, Some(_)) => {
                        self.nearest_node(right, origin, direction, max_distance, best)
                    }
                    (None, None) => {}
                }
            }
        }
    }

    fn node_distance(
        &self,
        index: usize,
        origin: [f32; N],
        direction: [f32; N],
        limit: f32,
    ) -> Option<f32> {
        let bounds = self.nodes[index].bounds;
        slab(origin, direction, bounds.min, bounds.max, limit).map(|hit| hit.0)
    }
}

fn build<T: Copy, const N: usize>(
    entries: &mut [Entry<T, N>],
    nodes: &mut Vec<Node<N>>,
    start: usize,
) -> usize {
    let mut bounds = entries[0].bounds;
    let mut min_center = [f32::INFINITY; N];
    let mut max_center = [f32::NEG_INFINITY; N];
    for entry in entries.iter() {
        bounds = bounds.union(entry.bounds);
        for axis in 0..N {
            let center = entry.bounds.center(axis);
            min_center[axis] = min_center[axis].min(center);
            max_center[axis] = max_center[axis].max(center);
        }
    }
    let index = nodes.len();
    nodes.push(Node {
        bounds,
        children: Children::Leaf {
            start,
            end: start + entries.len(),
        },
    });
    if entries.len() > LEAF_CAPACITY {
        let axis = (0..N)
            .max_by(|&a, &b| {
                (max_center[a] - min_center[a])
                    .partial_cmp(&(max_center[b] - min_center[b]))
                    .unwrap_or(Ordering::Equal)
            })
            .unwrap_or(0);
        let middle = entries.len() / 2;
        entries.select_nth_unstable_by(middle, |a, b| {
            a.bounds
                .center(axis)
                .total_cmp(&b.bounds.center(axis))
                .then(a.rank.cmp(&b.rank))
        });
        let (left_entries, right_entries) = entries.split_at_mut(middle);
        let left = build(left_entries, nodes, start);
        let right = build(right_entries, nodes, start + middle);
        nodes[index].children = Children::Branch { left, right };
    }
    index
}

/// Nearest indexed 2D box contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialHit2<T> {
    /// Caller-supplied collider ID.
    pub id: T,
    /// Ray/box contact.
    pub hit: RayHit2,
}

/// Nearest indexed 3D box contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialHit3<T> {
    /// Caller-supplied collider ID.
    pub id: T,
    /// Ray/box contact.
    pub hit: RayHit3,
}

/// Owned 2D collider snapshot with a balanced AABB hierarchy.
///
/// Rebuild explicitly after moving colliders. Queries see the last successful
/// snapshot. IDs need only be `Copy`; the index does not read or mutate ECS
/// components. Duplicate IDs are accepted and reported as supplied. Rebuild
/// validates all boxes before committing, reuses staging capacity, and needs
/// temporary room for both the old and new snapshots.
pub struct SpatialIndex2D<T: Copy>(Bvh<T, 2>);

/// Owned 3D collider snapshot with the same lifecycle as [`SpatialIndex2D`].
pub struct SpatialIndex3D<T: Copy>(Bvh<T, 3>);

macro_rules! index_impl {
    ($index:ident, $ray:ident, $hit:ident, $ray_hit:ident, $box:ident, $vec:ident, $frustum:ident, $n:expr) => {
        impl<T: Copy> Default for $index<T> {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<T: Copy> $index<T> {
            /// Creates an empty index without allocating.
            pub fn new() -> Self {
                Self(Bvh::new())
            }

            /// Number of indexed colliders in the last successful snapshot.
            pub fn len(&self) -> usize {
                self.0.entries.len()
            }

            /// Whether the snapshot is empty.
            pub fn is_empty(&self) -> bool {
                self.0.entries.is_empty()
            }

            /// Copies `(id, bounds)` pairs and atomically replaces the snapshot.
            /// Invalid input returns its original position and preserves old data.
            /// Build cost is O(n log n); query cost depends on overlap distribution.
            pub fn rebuild(
                &mut self,
                source: impl IntoIterator<Item = (T, $box)>,
            ) -> Result<usize, SpatialError> {
                self.0.rebuild(source.into_iter().map(|(id, bounds)| {
                    (
                        id,
                        Bounds {
                            min: bounds.min.to_array(),
                            max: bounds.max.to_array(),
                        },
                    )
                }))
            }

            /// Nearest ray contact within an inclusive world-unit distance.
            /// Exact-distance ties use original input order. Infinity is allowed.
            pub fn nearest(
                &self,
                ray: $ray,
                max_distance: f32,
            ) -> Result<Option<$hit<T>>, SpatialError> {
                valid_distance(max_distance)?;
                Ok(self
                    .0
                    .nearest(
                        ray.origin.to_array(),
                        ray.direction.to_array(),
                        max_distance,
                    )
                    .map(|(id, distance, normal)| $hit {
                        id,
                        hit: $ray_hit {
                            distance,
                            point: ray.at(distance),
                            normal: <$vec>::from_array(normal),
                        },
                    }))
            }

            /// Visits boxes with positive-volume overlap. Touching alone is excluded.
            /// Visitor order follows the hierarchy, not input order. No allocation.
            pub fn visit_overlapping(
                &self,
                area: $box,
                emit: impl FnMut(T),
            ) -> Result<(), SpatialError> {
                let bounds = Bounds {
                    min: area.min.to_array(),
                    max: area.max.to_array(),
                };
                if !bounds.valid() {
                    return Err(SpatialError::InvalidBounds(0));
                }
                self.0.visit(|candidate| candidate.overlaps(bounds), emit);
                Ok(())
            }

            /// Collects positive-volume overlaps for convenience.
            pub fn overlapping(&self, area: $box) -> Result<Vec<T>, SpatialError> {
                let mut ids = Vec::new();
                self.visit_overlapping(area, |id| ids.push(id))?;
                Ok(ids)
            }

            /// Visits boxes that may be visible in this camera. No allocation.
            pub fn visit_visible(&self, view: &$frustum, emit: impl FnMut(T)) {
                self.0.visit(
                    |bounds| {
                        view.intersects($box {
                            min: <$vec>::from_array(bounds.min),
                            max: <$vec>::from_array(bounds.max),
                        })
                    },
                    emit,
                );
            }

            /// Collects potentially visible boxes for convenience.
            pub fn visible(&self, view: &$frustum) -> Vec<T> {
                let mut ids = Vec::new();
                self.visit_visible(view, |id| ids.push(id));
                ids
            }
        }
    };
}

index_impl!(
    SpatialIndex2D,
    Ray2,
    SpatialHit2,
    RayHit2,
    Aabb2,
    Vec2,
    Frustum2D,
    2
);
index_impl!(
    SpatialIndex3D,
    Ray3,
    SpatialHit3,
    RayHit3,
    Aabb3,
    Vec3,
    Frustum3D,
    3
);
