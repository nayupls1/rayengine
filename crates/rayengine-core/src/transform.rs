//! Local and global transforms for typed 2D and 3D entities.

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
use hecs::Entity;

/// Optional scene parent. Prefer [`crate::scene::Scene::set_parent`] to validate edits.
#[derive(Clone, Copy, Debug)]
pub struct Parent(pub Entity);

/// A 2D local transform. Positive Y points down; rotation is in radians.
#[derive(Clone, Copy, Debug)]
pub struct Transform2D {
    /// Local position.
    pub position: Vec2,
    /// Local rotation in radians.
    pub rotation: f32,
    /// Local scale.
    pub scale: Vec2,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
        }
    }
}

impl Transform2D {
    /// Translation with identity rotation and scale.
    pub fn at(position: Vec2) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }

    /// Local affine matrix.
    pub fn matrix(self) -> Mat3 {
        Mat3::from_scale_angle_translation(self.scale, self.rotation, self.position)
    }
}

/// A 3D local transform. Positive Y points up, using right-handed coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Transform3D {
    /// Local position.
    pub position: Vec3,
    /// Local rotation.
    pub rotation: Quat,
    /// Local scale.
    pub scale: Vec3,
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform3D {
    /// Translation with identity rotation and scale.
    pub fn at(position: Vec3) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }

    /// Local affine matrix.
    pub fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

/// Computed 2D transform including every scene ancestor.
#[derive(Clone, Copy, Debug)]
pub struct GlobalTransform2D(pub Mat3);

impl Default for GlobalTransform2D {
    fn default() -> Self {
        Self(Mat3::IDENTITY)
    }
}

/// Computed 3D transform including every scene ancestor.
#[derive(Clone, Copy, Debug)]
pub struct GlobalTransform3D(pub Mat4);

impl Default for GlobalTransform3D {
    fn default() -> Self {
        Self(Mat4::IDENTITY)
    }
}
