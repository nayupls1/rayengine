//! Exact snapshot queries over the solver's shared translation geometry.

use super::{BodyId, CollisionFilter, geometry};
use crate::spatial::{Ray2, Ray3};
use glam::{Vec2, Vec3};

/// Eligibility for world queries. Both collision masks must accept each other.
#[derive(Clone, Copy, Debug, Default)]
pub struct QueryFilter<'a> {
    /// Query layers and acceptance mask.
    pub collision: CollisionFilter,
    /// Stable identities to skip, including the casting body's own identity.
    pub excluded: &'a [BodyId],
    /// Whether nonblocking triggers are eligible. Defaults to false.
    pub include_triggers: bool,
}
impl QueryFilter<'_> {
    fn allows(self, id: BodyId, filter: CollisionFilter, trigger: bool) -> bool {
        self.collision.allows(filter)
            && (self.include_triggers || !trigger)
            && !self.excluded.contains(&id)
    }
}

/// Invalid query input. Queries return errors rather than panicking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryError {
    /// Nonfinite center, nonpositive/nonfinite dimensions, or arithmetic overflow
    /// in bounds, relative coordinates or combined shape extents.
    InvalidGeometry,
    /// Nonfinite translation, negative/nonfinite ray limit, or overflowing
    /// translation length/end position. Ray limits must be finite.
    InvalidMotion,
}
impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidGeometry => "invalid collision query geometry",
            Self::InvalidMotion => "invalid collision query motion",
        })
    }
}
impl std::error::Error for QueryError {}

fn validate<const N: usize>(shape: geometry::Shape<N>, center: [f32; N]) -> Result<(), QueryError> {
    let (min, max) = shape.bounds(center);
    if !shape.valid()
        || !center
            .iter()
            .chain(min.iter())
            .chain(max.iter())
            .all(|v| v.is_finite())
    {
        return Err(QueryError::InvalidGeometry);
    }
    Ok(())
}
fn motion<const N: usize>(center: [f32; N], delta: [f32; N]) -> Result<f64, QueryError> {
    let length = delta
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    if !delta.iter().all(|v| v.is_finite())
        || !geometry::add(center, delta).iter().all(|v| v.is_finite())
        || length > f64::from(f32::MAX)
    {
        return Err(QueryError::InvalidMotion);
    }
    Ok(length)
}
fn pair<const N: usize>(
    a: geometry::Shape<N>,
    pa: [f32; N],
    b: geometry::Shape<N>,
    pb: [f32; N],
) -> Result<(), QueryError> {
    validate(b, pb)?;
    if !geometry::sub(pa, pb)
        .iter()
        .chain(geometry::add(a.half(), b.half()).iter())
        .all(|v| v.is_finite())
    {
        return Err(QueryError::InvalidGeometry);
    }
    Ok(())
}
fn cast<const N: usize>(
    a: geometry::Shape<N>,
    pa: [f32; N],
    b: geometry::Shape<N>,
    pb: [f32; N],
    delta: [f64; N],
) -> Option<(f64, [f32; N])> {
    // Point rays use a zero-sized box internally; public shapes stay positive.
    if geometry::overlap(a, pa, b, pb).is_some() {
        return Some((0.0, [0.0; N]));
    }
    geometry::sweep_precise(a, pa, b, pb, delta)
}

macro_rules! queries {
    ($n:literal, $v:ident, $ray:ident, $shape:ident, $world:ident, $hit:ident, $world_hit:ident, $overlap:ident, $penetration:ident) => {
        /// Earliest entering contact along a finite translation.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $hit {
            /// Fraction of the requested translation, in `[0, 1]`, rounded to f32.
            /// World ordering uses the unrounded fraction.
            pub fraction: f32,
            /// Distance traveled by the cast's reference center, in world units.
            /// Rounded to f32 independently of position and fraction.
            pub distance: f32,
            /// Reference center at impact (ray point for raycasts). For shape
            /// casts this is the moving center, not a point on either surface.
            pub position: $v,
            /// Unit normal from target toward caster. Initial positive overlaps
            /// use zero because there is no unique entering surface.
            pub normal: $v,
        }
        /// Exact world cast contact and the eligible collider's stable identity.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $world_hit {
            /// Hit collider.
            pub body: BodyId,
            /// Translation contact.
            pub hit: $hit,
        }
        /// Positive overlap with a world collider.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $overlap {
            /// Overlapped collider.
            pub body: BodyId,
            /// Translation that separates the query shape from this collider.
            pub penetration: super::$penetration,
        }
        impl super::$shape {
            /// Casts this shape's center along `translation` against a fixed target.
            /// Initial positive overlaps return zero travel and zero normal.
            /// Touching hits only while entering; tangencies and parallel grazing
            /// miss, including grazing within f64 discriminant roundoff.
            /// Zero travel reports only positive overlap. Invalid inputs
            /// return an error. See [`super::guide`] for snapshot semantics.
            pub fn cast(
                self,
                center: $v,
                translation: $v,
                target: Self,
                target_center: $v,
            ) -> Result<Option<$hit>, QueryError> {
                Ok(self
                    .cast_precise(center, translation, target, target_center)?
                    .map(|(_, hit)| hit))
            }
            fn cast_precise(
                self,
                center: $v,
                translation: $v,
                target: Self,
                target_center: $v,
            ) -> Result<Option<(f64, $hit)>, QueryError> {
                validate(self.internal(), center.to_array())?;
                let length = motion(center.to_array(), translation.to_array())?;
                pair(
                    self.internal(),
                    center.to_array(),
                    target.internal(),
                    target_center.to_array(),
                )?;
                Ok(cast(
                    self.internal(),
                    center.to_array(),
                    target.internal(),
                    target_center.to_array(),
                    translation.to_array().map(f64::from),
                )
                .map(|(fraction, normal)| {
                    (
                        fraction,
                        $hit {
                            fraction: fraction as f32,
                            distance: (length * fraction) as f32,
                            position: $v::from_array(std::array::from_fn(|i| {
                                (f64::from(center[i]) + f64::from(translation[i]) * fraction) as f32
                            })),
                            normal: $v::from_array(normal),
                        },
                    )
                }))
            }
            /// Casts a point ray against this exact shape at `center`.
            /// Uses the same entering/tangency rules as [`Self::cast`]. A ray
            /// strictly inside reports zero travel/normal, including zero limit.
            /// `max_distance` must be finite and nonnegative.
            pub fn raycast(
                self,
                center: $v,
                ray: $ray,
                max_distance: f32,
            ) -> Result<Option<$hit>, QueryError> {
                Ok(self
                    .raycast_precise(center, ray, max_distance)?
                    .map(|(_, hit)| hit))
            }
            fn raycast_precise(
                self,
                center: $v,
                ray: $ray,
                max_distance: f32,
            ) -> Result<Option<(f64, $hit)>, QueryError> {
                if !max_distance.is_finite() || max_distance < 0.0 {
                    return Err(QueryError::InvalidMotion);
                }
                let delta = ray.direction() * max_distance;
                motion(ray.origin().to_array(), delta.to_array())?;
                let point = geometry::Shape::Box([0.0; $n]);
                pair(
                    point,
                    ray.origin().to_array(),
                    self.internal(),
                    center.to_array(),
                )?;
                Ok(cast(
                    point,
                    ray.origin().to_array(),
                    self.internal(),
                    center.to_array(),
                    ray.direction()
                        .to_array()
                        .map(|v| f64::from(v) * f64::from(max_distance)),
                )
                .map(|(fraction, normal)| {
                    (
                        fraction,
                        $hit {
                            fraction: fraction as f32,
                            distance: (f64::from(max_distance) * fraction) as f32,
                            position: $v::from_array(std::array::from_fn(|i| {
                                (f64::from(ray.origin()[i])
                                    + f64::from(ray.direction()[i])
                                        * f64::from(max_distance)
                                        * fraction) as f32
                            })),
                            normal: $v::from_array(normal),
                        },
                    )
                }))
            }
        }
        impl super::$world {
            /// Earliest eligible snapshot hit, scanning current body geometry.
            /// Equal unrounded fractions choose the lowest [`BodyId`]. Target velocities
            /// are ignored. Includes edits made through `body_mut` immediately.
            /// Invalid query or eligible body geometry returns an error.
            pub fn cast_shape(
                &self,
                shape: super::$shape,
                center: $v,
                translation: $v,
                filter: QueryFilter<'_>,
            ) -> Result<Option<$world_hit>, QueryError> {
                validate(shape.internal(), center.to_array())?;
                motion(center.to_array(), translation.to_array())?;
                let mut nearest: Option<(f64, $world_hit)> = None;
                for (id, body) in self.iter() {
                    if !filter.allows(id, body.filter, body.is_trigger) {
                        continue;
                    }
                    if let Some((fraction, hit)) =
                        shape.cast_precise(center, translation, body.shape, body.position)?
                    {
                        if nearest.is_none_or(|(old_fraction, _)| fraction < old_fraction) {
                            nearest = Some((fraction, $world_hit { body: id, hit }));
                        }
                    }
                }
                Ok(nearest.map(|(_, hit)| hit))
            }
            /// Earliest eligible exact point ray hit. Same finite limits,
            /// initial-overlap and entering rules as the standalone shape query.
            /// Equal unrounded fractions choose the lowest [`BodyId`].
            pub fn raycast(
                &self,
                ray: $ray,
                max_distance: f32,
                filter: QueryFilter<'_>,
            ) -> Result<Option<$world_hit>, QueryError> {
                if !max_distance.is_finite() || max_distance < 0.0 {
                    return Err(QueryError::InvalidMotion);
                }
                motion(
                    ray.origin().to_array(),
                    (ray.direction() * max_distance).to_array(),
                )?;
                let mut nearest: Option<(f64, $world_hit)> = None;
                for (id, body) in self.iter() {
                    if !filter.allows(id, body.filter, body.is_trigger) {
                        continue;
                    }
                    if let Some((fraction, hit)) =
                        body.shape
                            .raycast_precise(body.position, ray, max_distance)?
                    {
                        if nearest.is_none_or(|(old_fraction, _)| fraction < old_fraction) {
                            nearest = Some((fraction, $world_hit { body: id, hit }));
                        }
                    }
                }
                Ok(nearest.map(|(_, hit)| hit))
            }
            /// Visits exact positive overlaps in ascending [`BodyId`] order.
            /// Touching is excluded. Scans current geometry without an index or
            /// hit-buffer allocation. Validation completes before any callbacks,
            /// so invalid eligible geometry returns an error without partial visits.
            pub fn visit_overlaps(
                &self,
                shape: super::$shape,
                center: $v,
                filter: QueryFilter<'_>,
                mut visitor: impl FnMut($overlap),
            ) -> Result<(), QueryError> {
                validate(shape.internal(), center.to_array())?;
                for (id, body) in self.iter() {
                    if filter.allows(id, body.filter, body.is_trigger) {
                        pair(
                            shape.internal(),
                            center.to_array(),
                            body.shape.internal(),
                            body.position.to_array(),
                        )?;
                    }
                }
                for (id, body) in self.iter() {
                    if !filter.allows(id, body.filter, body.is_trigger) {
                        continue;
                    }
                    if let Some(penetration) = shape.overlap(center, body.shape, body.position) {
                        visitor($overlap {
                            body: id,
                            penetration,
                        });
                    }
                }
                Ok(())
            }
        }
    };
}
queries!(
    2,
    Vec2,
    Ray2,
    Shape2D,
    PhysicsWorld2D,
    CastHit2D,
    WorldCastHit2D,
    OverlapHit2D,
    Penetration2D
);
queries!(
    3,
    Vec3,
    Ray3,
    Shape3D,
    PhysicsWorld3D,
    CastHit3D,
    WorldCastHit3D,
    OverlapHit3D,
    Penetration3D
);

#[cfg(test)]
mod tests;
