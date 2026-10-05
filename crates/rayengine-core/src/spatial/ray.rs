//! Normalized rays and exact slab contacts with inclusive boundaries.

use super::{SpatialError, valid_distance};
use crate::collision::{Aabb2, Aabb3};
use glam::{Vec2, Vec3};

/// A 2D ray with a unit-length direction; distances are world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray2 {
    /// Ray origin in world coordinates.
    pub(crate) origin: Vec2,
    /// Unit-length direction.
    pub(crate) direction: Vec2,
}

/// A 3D ray with a unit-length direction; distances are world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray3 {
    /// Ray origin in world coordinates.
    pub(crate) origin: Vec3,
    /// Unit-length direction.
    pub(crate) direction: Vec3,
}

/// Contact on a 2D box. The normal points outward; strict inside starts have zero normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit2 {
    /// World-unit distance along the normalized ray.
    pub distance: f32,
    /// World-space contact point.
    pub point: Vec2,
    /// Outward box-face normal, or zero for a strict inside start.
    pub normal: Vec2,
}

/// Contact on a 3D box. The normal points outward; strict inside starts have zero normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit3 {
    /// World-unit distance along the normalized ray.
    pub distance: f32,
    /// World-space contact point.
    pub point: Vec3,
    /// Outward box-face normal, or zero for a strict inside start.
    pub normal: Vec3,
}

fn unit<const N: usize>(origin: [f32; N], direction: [f32; N]) -> Result<[f32; N], SpatialError> {
    if !origin.iter().chain(direction.iter()).all(|v| v.is_finite()) {
        return Err(SpatialError::InvalidRay);
    }
    let scale = direction
        .iter()
        .fold(0.0_f32, |largest, v| largest.max(v.abs()));
    if scale == 0.0 {
        return Err(SpatialError::InvalidRay);
    }
    // Scaling avoids overflow for large finite vectors and underflow for tiny
    // nonzero vectors before normalization.
    let scaled = direction.map(|v| v / scale);
    let squared: f32 = scaled.iter().map(|v| v * v).sum();
    let length = squared.sqrt();
    Ok(scaled.map(|v| v / length))
}

/// Slab intersection. Axis order breaks exact corner ties deterministically.
pub(crate) fn slab<const N: usize>(
    origin: [f32; N],
    direction: [f32; N],
    min: [f32; N],
    max: [f32; N],
    limit: f32,
) -> Option<(f32, [f32; N])> {
    let mut enter = 0.0_f32;
    let mut exit = limit;
    let mut normal = [0.0; N];
    for axis in 0..N {
        let d = direction[axis];
        if d == 0.0 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let (near, far, side) = if d > 0.0 {
            (
                (min[axis] - origin[axis]) / d,
                (max[axis] - origin[axis]) / d,
                -1.0,
            )
        } else {
            (
                (max[axis] - origin[axis]) / d,
                (min[axis] - origin[axis]) / d,
                1.0,
            )
        };
        if near >= 0.0 && near >= enter {
            enter = near;
            normal = [0.0; N];
            normal[axis] = side;
        }
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    if enter.is_finite() && exit >= 0.0 {
        Some((enter, normal))
    } else {
        None
    }
}

macro_rules! ray_impl {
    ($ray:ident, $hit:ident, $vec:ident, $box:ident, $n:expr) => {
        impl $ray {
            /// Ray origin in world coordinates.
            pub fn origin(self) -> $vec {
                self.origin
            }

            /// Unit-length ray direction.
            pub fn direction(self) -> $vec {
                self.direction
            }

            /// Constructs a ray, normalizing its finite, nonzero direction.
            pub fn new(origin: $vec, direction: $vec) -> Result<Self, SpatialError> {
                let direction =
                    <$vec>::from_array(unit::<$n>(origin.to_array(), direction.to_array())?);
                Ok(Self { origin, direction })
            }

            /// World-space position at a distance along the ray.
            pub fn at(self, distance: f32) -> $vec {
                self.origin + self.direction * distance
            }

            /// First box contact, inclusive of faces and `max_distance`.
            ///
            /// Strict inside starts return distance zero and a zero normal.
            /// A boundary start reports zero distance; an outward start uses
            /// zero normal, while an inward start reports that face's normal.
            /// `f32::INFINITY` is accepted for an unbounded search.
            pub fn cast(
                self,
                bounds: $box,
                max_distance: f32,
            ) -> Result<Option<$hit>, SpatialError> {
                valid_distance(max_distance)?;
                if !bounds.min.is_finite()
                    || !bounds.max.is_finite()
                    || !bounds.min.cmple(bounds.max).all()
                {
                    return Err(SpatialError::InvalidBounds(0));
                }
                Ok(slab::<$n>(
                    self.origin.to_array(),
                    self.direction.to_array(),
                    bounds.min.to_array(),
                    bounds.max.to_array(),
                    max_distance,
                )
                .map(|(distance, normal)| $hit {
                    distance,
                    point: self.at(distance),
                    normal: <$vec>::from_array(normal),
                }))
            }
        }
    };
}

ray_impl!(Ray2, RayHit2, Vec2, Aabb2, 2);
ray_impl!(Ray3, RayHit3, Vec3, Aabb3, 3);

impl Ray3 {
    /// Intersects an infinite plane in front of this ray (including its origin).
    /// Returns `None` for invalid plane inputs, a parallel ray, an intersection
    /// behind the origin, or a result outside finite f32 world coordinates.
    /// The plane normal need not be normalized. An in-plane ray has no unique hit.
    pub fn intersect_plane(self, point: Vec3, normal: Vec3) -> Option<Vec3> {
        if !point.is_finite() {
            return None;
        }
        let normal = Self::new(Vec3::ZERO, normal).ok()?.direction().as_dvec3();
        let origin = self.origin.as_dvec3();
        let direction = self.direction.as_dvec3();
        let denominator = normal.dot(direction);
        if denominator == 0.0 {
            return None;
        }
        let distance = normal.dot(point.as_dvec3() - origin) / denominator;
        if !distance.is_finite() || distance < 0.0 {
            return None;
        }
        let hit = (origin + direction * distance).as_vec3();
        hit.is_finite().then_some(hit)
    }
}
