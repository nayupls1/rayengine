//! Closed, static directional areas. All calculations use double precision
//! internally to avoid overflow/underflow for finite single-precision inputs.

use super::{Aabb2, Circle};
use glam::{DVec2, Vec2};
use std::fmt;

/// Invalid input to a [`Sector2`] constructor or query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorError {
    /// Origin must have finite coordinates.
    InvalidOrigin,
    /// Direction must be finite and nonzero.
    InvalidDirection,
    /// Range must be finite and nonnegative.
    InvalidRange,
    /// Full opening angle must be finite and in `0..=TAU` radians.
    InvalidAngle,
    /// Padded broadphase bounds cannot be represented with finite coordinates.
    InvalidBounds,
    /// A query point must have finite coordinates.
    InvalidPoint,
    /// A target circle must have a finite center and finite, positive radius.
    InvalidCircle,
}

impl fmt::Display for SectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidOrigin => "sector origin must be finite",
            Self::InvalidDirection => "sector direction must be finite and nonzero",
            Self::InvalidRange => "sector range must be finite and nonnegative",
            Self::InvalidAngle => "sector angle must be in 0..=TAU radians",
            Self::InvalidBounds => "sector broadphase bounds must be finite",
            Self::InvalidPoint => "sector query point must be finite",
            Self::InvalidCircle => {
                "target circle must have a finite center and positive finite radius"
            }
        })
    }
}

impl std::error::Error for SectorError {}

/// Validated 2D sector centered on a direction, for attacks and telegraphs.
///
/// The angle is the **full** opening in radians, symmetric about the direction.
/// `0` is a line segment, `PI` is a half disk, and `TAU` is a full disk.
/// Range `0` is just the origin, which belongs to every sector. Arc, radial
/// edges, and touching circles are included. Unlike physics penetration
/// queries, contact does not need positive area.
///
/// Direction follows the supplied coordinate system: rotating `+X` toward
/// `+Y` is counterclockwise in Y-up coordinates and clockwise on a Y-down
/// screen. Vector directions avoid angle-wrap discontinuities.
///
/// This is one static area query, **not** continuous collision detection for a
/// rotating swing. Attack timing, damage, visibility and per-swing deduplication
/// belong to the game.
///
/// # Headless directional hit example
///
/// ```
#[doc = include_str!("../../examples/directional_hit.rs")]
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector2 {
    origin: Vec2,
    direction: DVec2,
    range: f32,
    angle: f32,
    bounds: Aabb2,
}

impl Sector2 {
    /// Creates a sector, normalizing any finite nonzero direction.
    ///
    /// Range must be finite and nonnegative; angle must be finite and in
    /// `0..=std::f32::consts::TAU`. Construction also checks that padded
    /// [`bounds`](Self::bounds) fit in finite `f32` coordinates. Fields are
    /// private so a successfully constructed sector stays valid.
    pub fn new(origin: Vec2, direction: Vec2, range: f32, angle: f32) -> Result<Self, SectorError> {
        if !origin.is_finite() {
            return Err(SectorError::InvalidOrigin);
        }
        if !direction.is_finite() || direction == Vec2::ZERO {
            return Err(SectorError::InvalidDirection);
        }
        if !range.is_finite() || range < 0.0 {
            return Err(SectorError::InvalidRange);
        }
        if !angle.is_finite() || !(0.0..=std::f32::consts::TAU).contains(&angle) {
            return Err(SectorError::InvalidAngle);
        }
        let center = origin.as_dvec2();
        let radius = DVec2::splat(f64::from(range));
        // Pad one representable value beyond outward rounding. The index's
        // positive-area overlap must retain closed contacts and flat sectors.
        let min = Vec2::from_array((center - radius).to_array().map(|v| {
            let rounded = v as f32;
            let outward = if f64::from(rounded) > v {
                rounded.next_down()
            } else {
                rounded
            };
            outward.next_down()
        }));
        let max = Vec2::from_array((center + radius).to_array().map(|v| {
            let rounded = v as f32;
            let outward = if f64::from(rounded) < v {
                rounded.next_up()
            } else {
                rounded
            };
            outward.next_up()
        }));
        if !min.is_finite() || !max.is_finite() {
            return Err(SectorError::InvalidBounds);
        }
        Ok(Self {
            origin,
            direction: direction.as_dvec2().normalize(),
            range,
            angle,
            bounds: Aabb2 { min, max },
        })
    }

    /// World-space origin, included for all angles and ranges.
    pub fn origin(self) -> Vec2 {
        self.origin
    }

    /// Normalized center direction (rounded to single precision).
    pub fn direction(self) -> Vec2 {
        self.direction.as_vec2()
    }

    /// Maximum distance from the origin in world units.
    pub fn range(self) -> f32 {
        self.range
    }

    /// Full opening angle in radians.
    pub fn angle(self) -> f32 {
        self.angle
    }

    /// Conservative world-space AABB for broadphase candidate selection.
    ///
    /// Encloses the full range disk, so narrow sectors may return many false
    /// positives. Bounds are padded outward by at least one representable step
    /// to retain tangencies with [`crate::spatial::SpatialIndex2D::overlapping`],
    /// whose box test requires positive area. Index the target's **whole circle
    /// bounds**, then test each candidate with [`intersects_circle`](Self::intersects_circle).
    pub fn bounds(self) -> Aabb2 {
        self.bounds
    }

    /// Whether a finite point is inside or on this sector, including its origin.
    pub fn contains(self, point: Vec2) -> Result<bool, SectorError> {
        if !point.is_finite() {
            return Err(SectorError::InvalidPoint);
        }
        let local = self.local(point);
        Ok(local.length_squared() <= f64::from(self.range).powi(2)
            && self.contains_direction(local))
    }

    /// Exact static sector-versus-circle intersection, including tangency.
    ///
    /// Tests the circle's radius against the nearest point in the sector, not
    /// just its center. Handles wide sectors (over `PI`), radial edges, arc
    /// endpoints, circles enclosing the origin, and zero range/angle. No arc
    /// tessellation or angular inflation is used. Floating-point rounding
    /// applies at boundaries; no gameplay tolerance is added.
    ///
    /// Returns an error for nonfinite centers or nonpositive/nonfinite radii,
    /// including invalid circles made by modifying [`Circle`]'s public fields.
    pub fn intersects_circle(self, circle: &Circle) -> Result<bool, SectorError> {
        if !circle.center.is_finite() || !circle.radius.is_finite() || circle.radius <= 0.0 {
            return Err(SectorError::InvalidCircle);
        }
        let local = self.local(circle.center);
        let range = f64::from(self.range);
        let distance_squared = if self.contains_direction(local) {
            // Radial projection is in the sector. Inside centers have distance
            // zero; outside centers are nearest to the curved arc.
            (local.length() - range).max(0.0).powi(2)
        } else {
            // Outside the angular interval, the closest point lies on one of
            // the radial segments (including the origin and arc endpoints).
            let (sin, cos) = (f64::from(self.angle) * 0.5).sin_cos();
            [DVec2::new(cos, sin), DVec2::new(cos, -sin)]
                .into_iter()
                .map(|edge| {
                    let nearest = edge * local.dot(edge).clamp(0.0, range);
                    local.distance_squared(nearest)
                })
                .fold(f64::INFINITY, f64::min)
        };
        Ok(distance_squared <= f64::from(circle.radius).powi(2))
    }

    fn local(self, point: Vec2) -> DVec2 {
        let offset = point.as_dvec2() - self.origin.as_dvec2();
        DVec2::new(self.direction.dot(offset), self.direction.perp_dot(offset))
    }

    fn contains_direction(self, local: DVec2) -> bool {
        local == DVec2::ZERO
            || self.angle == std::f32::consts::TAU
            || local.y.atan2(local.x).abs() <= f64::from(self.angle) * 0.5
    }
}

#[cfg(test)]
mod tests;
