//! Corresponding cameras with a stable visible height or vertical field of view.

use crate::{spatial::Ray3, viewport::Viewport};
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
    /// Constructs a world-space ray from a logical window pointer, rejecting
    /// letterbox bars and invalid/degenerate camera or viewport parameters.
    /// The origin is the eye; the direction is normalized. DPI is already handled
    /// by the viewport contract, so do not scale the pointer to framebuffer pixels.
    pub fn screen_ray(&self, screen: Vec2, view: &Viewport) -> Option<Ray3> {
        let ui = view.screen_to_ui(screen)?;
        if !view.origin.is_finite()
            || !view.size.is_finite()
            || view.size.min_element() <= 0.0
            || !view.logical_size.is_finite()
            || view.logical_size.min_element() <= 0.0
            || !view.scale.is_finite()
            || view.scale <= 0.0
            || !self.vertical_fov.is_finite()
            || self.vertical_fov <= 0.0
            || self.vertical_fov >= 180.0
        {
            return None;
        }
        let forward = Ray3::new(self.position, self.target - self.position)
            .ok()?
            .direction();
        let up = Ray3::new(Vec3::ZERO, self.up).ok()?.direction();
        let right = Ray3::new(Vec3::ZERO, forward.cross(up)).ok()?.direction();
        let up = right.cross(forward);
        let ndc = ui / view.logical_size * 2.0 - Vec2::ONE;
        let height = (self.vertical_fov.to_radians() * 0.5).tan();
        Ray3::new(
            self.position,
            forward + right * (ndc.x * height * view.aspect()) - up * (ndc.y * height),
        )
        .ok()
    }

    /// Picks a plane using [`screen_ray`](Self::screen_ray). Returns `None` for
    /// bars, invalid inputs, parallel rays, or intersections behind the camera.
    /// `normal` may have any finite nonzero magnitude.
    pub fn screen_to_plane(
        &self,
        screen: Vec2,
        view: &Viewport,
        point: Vec3,
        normal: Vec3,
    ) -> Option<Vec3> {
        self.screen_ray(screen, view)?
            .intersect_plane(point, normal)
    }

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
    #[test]
    fn screen_rays_match_projection_in_fitted_and_expanded_viewports() {
        let camera = Camera3D::default();
        for mode in [ScaleMode::Fit, ScaleMode::Expand] {
            for window in [Vec2::new(1600.0, 900.0), Vec2::new(800.0, 1200.0)] {
                let view = Viewport::new(window, Vec2::new(960.0, 540.0), mode).unwrap();
                for fraction in [Vec2::splat(0.5), Vec2::new(0.1, 0.2), Vec2::new(0.8, 0.7)] {
                    let screen = view.ui_to_screen(view.logical_size * fraction);
                    let ray = camera.screen_ray(screen, &view).unwrap();
                    assert_eq!(ray.origin(), camera.position);
                    assert!((ray.direction().length() - 1.0).abs() < 0.00001);
                    let clip = camera.projection(&view, 0.1, 100.0)
                        * camera.view_matrix()
                        * ray.at(10.0).extend(1.0);
                    let ndc = clip.truncate() / clip.w;
                    assert!(
                        Vec2::new(ndc.x, -ndc.y).abs_diff_eq(fraction * 2.0 - Vec2::ONE, 0.0001)
                    );
                }
                let center = view.origin + view.size * 0.5;
                assert!(
                    camera
                        .screen_to_plane(center, &view, Vec3::ZERO, Vec3::Y)
                        .unwrap()
                        .abs_diff_eq(Vec3::ZERO, 0.00001)
                );
                assert!(camera.screen_ray(view.origin - Vec2::ONE, &view).is_none());
                assert!(camera.screen_ray(view.origin + view.size, &view).is_none());
            }
        }
    }

    #[test]
    fn picking_rejects_invalid_cameras_and_planes() {
        let view = Viewport::new(Vec2::splat(100.0), Vec2::splat(100.0), ScaleMode::Fit).unwrap();
        for camera in [
            Camera3D {
                target: Camera3D::default().position,
                ..Camera3D::default()
            },
            Camera3D {
                up: Vec3::ZERO,
                ..Camera3D::default()
            },
            Camera3D {
                position: Vec3::ZERO,
                target: Vec3::Y,
                up: Vec3::Y,
                ..Camera3D::default()
            },
            Camera3D {
                vertical_fov: 180.0,
                ..Camera3D::default()
            },
            Camera3D {
                vertical_fov: f32::NAN,
                ..Camera3D::default()
            },
        ] {
            assert!(camera.screen_ray(Vec2::splat(50.0), &view).is_none());
        }
        for invalid_view in [
            Viewport {
                size: Vec2::splat(f32::NAN),
                ..view
            },
            Viewport {
                logical_size: Vec2::ZERO,
                ..view
            },
            Viewport { scale: 0.0, ..view },
        ] {
            assert!(
                Camera3D::default()
                    .screen_ray(Vec2::splat(50.0), &invalid_view)
                    .is_none()
            );
        }
        let ray = Ray3::new(Vec3::Y, -Vec3::Y).unwrap();
        assert_eq!(
            ray.intersect_plane(Vec3::ZERO, Vec3::Y * 3.0),
            Some(Vec3::ZERO)
        );
        assert_eq!(ray.intersect_plane(Vec3::Y, Vec3::Y), Some(Vec3::Y));
        assert!(ray.intersect_plane(Vec3::ZERO, Vec3::X).is_none());
        assert!(ray.intersect_plane(Vec3::Y * 2.0, Vec3::Y).is_none());
        assert!(ray.intersect_plane(Vec3::ZERO, Vec3::ZERO).is_none());
        assert!(
            ray.intersect_plane(Vec3::splat(f32::NAN), Vec3::Y)
                .is_none()
        );
    }
}
