//! Camera-space bounds for culling without a renderer or graphics context.

use super::SpatialError;
use crate::{
    camera::{Camera2D, Camera3D},
    collision::{Aabb2, Aabb3},
    viewport::Viewport,
};
use glam::{Mat2, Vec2, Vec3, Vec4};

/// Visible 2D camera rectangle, including rotation and fitted/expanded aspect.
#[derive(Clone, Copy, Debug)]
pub struct Frustum2D {
    center: Vec2,
    right: Vec2,
    down: Vec2,
    half: Vec2,
}

impl Frustum2D {
    /// Captures the current camera/viewport. Touching boxes count as visible.
    pub fn from_camera(camera: &Camera2D, viewport: &Viewport) -> Result<Self, SpatialError> {
        let aspect = viewport.aspect();
        let half = Vec2::new(camera.view_height * aspect * 0.5, camera.view_height * 0.5);
        if !camera.target.is_finite()
            || !camera.rotation.is_finite()
            || !camera.view_height.is_finite()
            || camera.view_height <= 0.0
            || !aspect.is_finite()
            || aspect <= 0.0
            || !half.is_finite()
        {
            return Err(SpatialError::InvalidCamera);
        }
        let rotation = Mat2::from_angle(-camera.rotation);
        Ok(Self {
            center: camera.target,
            right: rotation * Vec2::X,
            down: rotation * Vec2::Y,
            half,
        })
    }

    /// Exact oriented-rectangle versus AABB visibility test, inclusive of edges.
    pub fn intersects(&self, bounds: Aabb2) -> bool {
        if !bounds.min.is_finite() || !bounds.max.is_finite() || !bounds.min.cmple(bounds.max).all()
        {
            return false;
        }
        let center = bounds.min * 0.5 + bounds.max * 0.5;
        let extent = (bounds.max - bounds.min) * 0.5;
        let delta = center - self.center;
        let axes = [Vec2::X, Vec2::Y, self.right, self.down];
        axes.into_iter().all(|axis| {
            let box_radius = axis.x.abs() * extent.x + axis.y.abs() * extent.y;
            let view_radius =
                axis.dot(self.right).abs() * self.half.x + axis.dot(self.down).abs() * self.half.y;
            axis.dot(delta).abs() <= box_radius + view_radius
        })
    }
}

/// Six-plane 3D camera frustum. Box results are conservative near corners.
#[derive(Clone, Copy, Debug)]
pub struct Frustum3D {
    planes: [Vec4; 6],
}

impl Frustum3D {
    /// Captures a perspective camera with explicit near/far distances.
    ///
    /// The clipping convention matches [`Camera3D::projection`]. Touching
    /// planes count as visible. The camera must have a valid forward/up basis.
    pub fn from_camera(
        camera: &Camera3D,
        viewport: &Viewport,
        near: f32,
        far: f32,
    ) -> Result<Self, SpatialError> {
        let aspect = viewport.aspect();
        let forward = camera.target - camera.position;
        if !camera.position.is_finite()
            || !camera.target.is_finite()
            || !camera.up.is_finite()
            || !forward.is_finite()
            || forward.length_squared() <= 0.0
            || !forward.length_squared().is_finite()
            || camera.up.cross(forward).length_squared() <= 0.0
            || !camera.up.cross(forward).length_squared().is_finite()
            || !camera.vertical_fov.is_finite()
            || !(0.0..180.0).contains(&camera.vertical_fov)
            || !aspect.is_finite()
            || aspect <= 0.0
            || !near.is_finite()
            || !far.is_finite()
            || near <= 0.0
            || far <= near
        {
            return Err(SpatialError::InvalidCamera);
        }
        let clip = camera.projection(viewport, near, far) * camera.view_matrix();
        if !clip.is_finite() {
            return Err(SpatialError::InvalidCamera);
        }
        let columns = clip.to_cols_array_2d();
        let row = |i| Vec4::new(columns[0][i], columns[1][i], columns[2][i], columns[3][i]);
        let planes = [
            row(3) + row(0),
            row(3) - row(0),
            row(3) + row(1),
            row(3) - row(1),
            row(3) + row(2),
            row(3) - row(2),
        ];
        Ok(Self { planes })
    }

    /// Conservative AABB visibility: never culls a box touching the frustum.
    /// May report a box near multiple clipped corners as visible.
    pub fn intersects(&self, bounds: Aabb3) -> bool {
        if !bounds.min.is_finite() || !bounds.max.is_finite() || !bounds.min.cmple(bounds.max).all()
        {
            return false;
        }
        self.planes.iter().all(|plane| {
            let positive = Vec3::new(
                if plane.x >= 0.0 {
                    bounds.max.x
                } else {
                    bounds.min.x
                },
                if plane.y >= 0.0 {
                    bounds.max.y
                } else {
                    bounds.min.y
                },
                if plane.z >= 0.0 {
                    bounds.max.z
                } else {
                    bounds.min.z
                },
            );
            plane.truncate().dot(positive) + plane.w >= 0.0
        })
    }
}
