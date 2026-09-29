//! Display-independent selection, proximity, and conservative camera visibility.
//!
//! [`SpatialIndex2D`] and [`SpatialIndex3D`] own snapshots of caller-supplied
//! IDs and boxes. Rebuild after world bounds change; no ECS or raylib state is
//! read implicitly. Queries borrow the snapshot and never allocate when using
//! visitor methods. A failed rebuild leaves the previous snapshot intact.

mod index;
mod ray;
mod visibility;

pub use index::{SpatialHit2, SpatialHit3, SpatialIndex2D, SpatialIndex3D};
pub use ray::{Ray2, Ray3, RayHit2, RayHit3};
pub use visibility::{Frustum2D, Frustum3D};

use std::fmt;

/// Invalid, nonfinite, or unsupported spatial input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialError {
    /// Ray origin/direction is nonfinite or direction has no usable length.
    InvalidRay,
    /// A maximum query distance must be nonnegative and not NaN.
    InvalidDistance,
    /// One box has nonfinite coordinates or a reversed axis.
    InvalidBounds(usize),
    /// Camera, clipping distances, or viewport geometry is invalid.
    InvalidCamera,
}

impl fmt::Display for SpatialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRay => write!(f, "ray origin/direction is invalid"),
            Self::InvalidDistance => write!(f, "query distance must be nonnegative and not NaN"),
            Self::InvalidBounds(index) => write!(f, "invalid spatial bounds at input {index}"),
            Self::InvalidCamera => write!(f, "camera or clipping geometry is invalid"),
        }
    }
}

impl std::error::Error for SpatialError {}

pub(crate) fn valid_distance(distance: f32) -> Result<(), SpatialError> {
    if distance >= 0.0 {
        Ok(())
    } else {
        Err(SpatialError::InvalidDistance)
    }
}

#[cfg(test)]
mod tests;
