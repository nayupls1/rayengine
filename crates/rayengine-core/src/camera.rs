//! Corresponding cameras with a stable visible height or vertical field of view.

use crate::viewport::Viewport;
use glam::{Mat2, Mat4, Vec2, Vec3};

/// Camera expressed in world units, independent of display pixel dimensions.
#[derive(Clone, Copy, Debug)]
pub struct Camera2D {
    /// World point at the center of the viewport.
    pub target: Vec2,
    /// Rotation in radians.
    pub rotation: f32,
    /// Visible world height.
    pub view_height: f32,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            target: Vec2::ZERO,
            rotation: 0.0,
            view_height: 540.0,
        }
    }
}

impl Camera2D {
    /// Converts world coordinates to UI units using the current viewport.
    pub fn world_to_ui(&self, world: Vec2, view: &Viewport) -> Vec2 {
        view.logical_size * 0.5
            + Mat2::from_angle(self.rotation)
                * (world - self.target)
                * (view.logical_size.y / self.view_height)
    }

    /// Converts a logical screen pointer to world coordinates, rejecting bars.
    pub fn screen_to_world(&self, screen: Vec2, view: &Viewport) -> Option<Vec2> {
        view.screen_to_ui(screen).map(|ui| {
            self.target
                + Mat2::from_angle(-self.rotation)
                    * ((ui - view.logical_size * 0.5) * (self.view_height / view.logical_size.y))
        })
    }
}

/// Perspective 3D camera; resizing never changes its vertical FOV.
#[derive(Clone, Copy, Debug)]
pub struct Camera3D {
    /// Eye position.
    pub position: Vec3,
    /// World point the camera looks toward.
    pub target: Vec3,
    /// Up direction, normally positive Y.
    pub up: Vec3,
    /// Vertical field of view in degrees.
    pub vertical_fov: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            position: Vec3::new(8.0, 6.0, 8.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            vertical_fov: 55.0,
        }
    }
}

impl Camera3D {
    /// Right-handed world-to-view matrix.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// OpenGL perspective matrix for the viewport and explicit clipping distances.
    pub fn projection(&self, view: &Viewport, near: f32, far: f32) -> Mat4 {
        Mat4::perspective_rh_gl(self.vertical_fov.to_radians(), view.aspect(), near, far)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewport::ScaleMode;

    #[test]
    fn resized_rotated_camera_preserves_world_pointer_round_trip() {
        let camera = Camera2D {
            target: Vec2::new(120.0, -50.0),
            rotation: 0.7,
            view_height: 20.0,
        };
        let point = Vec2::new(122.0, -48.0);
        for size in [Vec2::new(1920.0, 1080.0), Vec2::new(900.0, 1400.0)] {
            let view = Viewport::new(size, Vec2::new(960.0, 540.0), ScaleMode::Fit).unwrap();
            let screen = view.ui_to_screen(camera.world_to_ui(point, &view));
            assert!(
                camera
                    .screen_to_world(screen, &view)
                    .unwrap()
                    .abs_diff_eq(point, 0.0001)
            );
        }
    }

    #[test]
    fn fitted_3d_projection_is_identical_across_aspects_and_dpi() {
        let a = Viewport::new(
            Vec2::new(960.0, 540.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Fit,
        )
        .unwrap();
        let b = Viewport::new(
            Vec2::new(800.0, 1200.0),
            Vec2::new(960.0, 540.0),
            ScaleMode::Fit,
        )
        .unwrap();
        let camera = Camera3D::default();
        assert!(
            camera
                .projection(&a, 0.1, 1000.0)
                .abs_diff_eq(camera.projection(&b, 0.1, 1000.0), 0.00001)
        );
    }
}
