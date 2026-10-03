//! Shape overlap queries and independent swept character movement.
//!
//! Characters slide against static boxes one axis at a time. Sweeps prevent
//! tunneling along each axis, but this is not a full continuous collision solver.
//! Initial overlaps are depenetrated in a bounded pass before movement.
//! For body pairs, layers and triggers, see [`crate::physics`].

use glam::{Vec2, Vec3};

macro_rules! aabb {
    ($name:ident, $vector:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name {
            /// Minimum corner.
            pub min: $vector,
            /// Maximum corner.
            pub max: $vector,
        }

        impl $name {
            /// Constructs a box from its center and nonnegative full dimensions.
            /// Panics for negative or nonfinite sizes/centers.
            pub fn from_center(center: $vector, size: $vector) -> Self {
                assert!(center.is_finite() && size.is_finite() && size.min_element() >= 0.0);
                Self {
                    min: center - size * 0.5,
                    max: center + size * 0.5,
                }
            }

            /// Full dimensions.
            pub fn size(&self) -> $vector {
                self.max - self.min
            }

            /// Center point.
            pub fn center(&self) -> $vector {
                (self.min + self.max) * 0.5
            }

            /// Whether a point lies inside or on the box.
            pub fn contains(&self, point: $vector) -> bool {
                point.cmpge(self.min).all() && point.cmple(self.max).all()
            }

            /// Whether boxes overlap with positive volume. Touching is not overlap.
            pub fn intersects(&self, other: &Self) -> bool {
                self.min.cmplt(other.max).all() && self.max.cmpgt(other.min).all()
            }
        }
    };
}

aabb!(
    Aabb2,
    Vec2,
    "Axis-aligned 2D box, also used for UI rectangles."
);
aabb!(Aabb3, Vec3, "Axis-aligned 3D box.");

/// Simple swept 2D character body. Positive Y is down.
#[derive(Clone, Copy, Debug)]
pub struct Body2D {
    /// Center position in world units.
    pub position: Vec2,
    /// Half dimensions of the collision box.
    pub half_size: Vec2,
    /// Velocity in world units per second.
    pub velocity: Vec2,
    /// Whether the last movement touched a solid while moving down.
    pub grounded: bool,
}

impl Body2D {
    /// Creates a stationary body from its center and full dimensions.
    pub fn new(position: Vec2, size: Vec2) -> Self {
        let bounds = Aabb2::from_center(position, size);
        Self {
            position,
            half_size: bounds.size() * 0.5,
            velocity: Vec2::ZERO,
            grounded: false,
        }
    }

    /// Current world-space collision bounds.
    pub fn bounds(&self) -> Aabb2 {
        Aabb2 {
            min: self.position - self.half_size,
            max: self.position + self.half_size,
        }
    }

    /// Integrates velocity and slides along static boxes, resolving X then Y.
    /// Clipped velocity components become zero. `dt` must be finite and nonnegative.
    pub fn move_and_slide(&mut self, dt: f32, solids: &[Aabb2]) {
        assert!(dt.is_finite() && dt >= 0.0);
        let mut position = self.position.to_array();
        let half = self.half_size.to_array();
        crate::physics::depenetrate_box(
            &mut position,
            half,
            solids.iter().map(|s| (s.min.to_array(), s.max.to_array())),
        );
        let mut velocity = self.velocity.to_array();
        self.grounded = false;
        for axis in [0, 1] {
            let step = velocity[axis] * dt;
            let (resolved, hit) = sweep_axis(
                position,
                half,
                axis,
                step,
                solids.iter().map(|s| (s.min.to_array(), s.max.to_array())),
            );
            position[axis] = resolved;
            if hit {
                velocity[axis] = 0.0;
                self.grounded |= axis == 1 && step > 0.0;
            }
        }
        self.position = Vec2::from_array(position);
        self.velocity = Vec2::from_array(velocity);
    }
}

/// Corresponding 3D character body. Positive Y is up.
#[derive(Clone, Copy, Debug)]
pub struct Body3D {
    /// Center position in world units.
    pub position: Vec3,
    /// Half dimensions of the collision box.
    pub half_size: Vec3,
    /// Velocity in world units per second.
    pub velocity: Vec3,
    /// Whether the last movement touched a solid while moving down.
    pub grounded: bool,
}

impl Body3D {
    /// Creates a stationary body from its center and full dimensions.
    pub fn new(position: Vec3, size: Vec3) -> Self {
        let bounds = Aabb3::from_center(position, size);
        Self {
            position,
            half_size: bounds.size() * 0.5,
            velocity: Vec3::ZERO,
            grounded: false,
        }
    }

    /// Current world-space collision bounds.
    pub fn bounds(&self) -> Aabb3 {
        Aabb3 {
            min: self.position - self.half_size,
            max: self.position + self.half_size,
        }
    }

    /// Integrates velocity and slides along static boxes, resolving X, Z then Y.
    /// Clipped velocity components become zero. `dt` must be finite and nonnegative.
    pub fn move_and_slide(&mut self, dt: f32, solids: &[Aabb3]) {
        assert!(dt.is_finite() && dt >= 0.0);
        let mut position = self.position.to_array();
        let half = self.half_size.to_array();
        crate::physics::depenetrate_box(
            &mut position,
            half,
            solids.iter().map(|s| (s.min.to_array(), s.max.to_array())),
        );
        let mut velocity = self.velocity.to_array();
        self.grounded = false;
        for axis in [0, 2, 1] {
            let step = velocity[axis] * dt;
            let (resolved, hit) = sweep_axis(
                position,
                half,
                axis,
                step,
                solids.iter().map(|s| (s.min.to_array(), s.max.to_array())),
            );
            position[axis] = resolved;
            if hit {
                velocity[axis] = 0.0;
                self.grounded |= axis == 1 && step < 0.0;
            }
        }
        self.position = Vec3::from_array(position);
        self.velocity = Vec3::from_array(velocity);
    }
}

fn sweep_axis<const N: usize>(
    position: [f32; N],
    half: [f32; N],
    axis: usize,
    step: f32,
    solids: impl Iterator<Item = ([f32; N], [f32; N])>,
) -> (f32, bool) {
    if step == 0.0 {
        return (position[axis], false);
    }
    let mut allowed = step;
    let mut boundary = 0.0;
    let mut hit = false;
    for (min, max) in solids {
        let overlaps_other_axes = (0..N)
            .filter(|&i| i != axis)
            .all(|i| position[i] + half[i] > min[i] && position[i] - half[i] < max[i]);
        if !overlaps_other_axes {
            continue;
        }
        let low = position[axis] - half[axis];
        let high = position[axis] + half[axis];
        if step > 0.0 && high <= min[axis] && high + allowed >= min[axis] {
            allowed = (min[axis] - high).max(0.0);
            boundary = min[axis];
            hit = true;
        } else if step < 0.0 && low >= max[axis] && low + allowed <= max[axis] {
            allowed = (max[axis] - low).min(0.0);
            boundary = max[axis];
            hit = true;
        }
    }
    if !hit {
        return (position[axis] + step, false);
    }
    // Resolve only the nearest contact after scanning the boxes. Adding a
    // clipped displacement can round the body inside and lose the next sweep.
    let resolved = if step > 0.0 {
        let center = boundary - half[axis];
        if center + half[axis] > boundary {
            center.next_down()
        } else {
            center
        }
    } else {
        let center = boundary + half[axis];
        if center - half[axis] < boundary {
            center.next_up()
        } else {
            center
        }
    };
    (resolved, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_boxes_are_not_overlapping() {
        let a = Aabb2::from_center(Vec2::ZERO, Vec2::splat(2.0));
        let b = Aabb2::from_center(Vec2::new(2.0, 0.0), Vec2::splat(2.0));
        assert!(!a.intersects(&b));
        assert!(a.contains(Vec2::ONE));
    }

    #[test]
    fn fast_2d_fall_cannot_tunnel_through_floor() {
        let floor = Aabb2::from_center(Vec2::new(0.0, 10.0), Vec2::new(100.0, 1.0));
        let mut body = Body2D::new(Vec2::ZERO, Vec2::splat(2.0));
        body.velocity = Vec2::new(0.0, 10_000.0);
        body.move_and_slide(1.0, &[floor]);
        assert_eq!(body.position.y, 8.5);
        assert_eq!(body.velocity.y, 0.0);
        assert!(body.grounded);
    }

    #[test]
    fn wall_contact_preserves_tangential_motion() {
        let wall = Aabb2::from_center(Vec2::new(10.0, 0.0), Vec2::new(1.0, 100.0));
        let mut body = Body2D::new(Vec2::ZERO, Vec2::splat(2.0));
        body.velocity = Vec2::new(100.0, 2.0);
        body.move_and_slide(1.0, &[wall]);
        assert_eq!(body.position, Vec2::new(8.5, 2.0));
        assert_eq!(body.velocity, Vec2::new(0.0, 2.0));
    }

    #[test]
    fn three_dimensions_use_y_up_and_stop_at_nearest_solid() {
        let low = Aabb3::from_center(Vec3::new(0.0, -10.0, 0.0), Vec3::new(100.0, 1.0, 100.0));
        let high = Aabb3::from_center(Vec3::ZERO, Vec3::new(100.0, 1.0, 100.0));
        let mut body = Body3D::new(Vec3::new(0.0, 10.0, 0.0), Vec3::splat(2.0));
        body.velocity.y = -10_000.0;
        body.move_and_slide(1.0, &[low, high]);
        assert_eq!(body.position.y, 1.5);
        assert!(body.grounded);
        assert_eq!(body.velocity.y, 0.0);
    }

    #[test]
    fn upward_collision_does_not_ground_player() {
        let ceiling = Aabb3::from_center(Vec3::new(0.0, 5.0, 0.0), Vec3::splat(2.0));
        let mut body = Body3D::new(Vec3::ZERO, Vec3::splat(2.0));
        body.velocity.y = 100.0;
        body.move_and_slide(0.1, &[ceiling]);
        assert_eq!(body.position.y, 3.0);
        assert!(!body.grounded);
    }

    #[test]
    fn fractional_contacts_remain_outside_solids_in_2d() {
        for coordinate in [-8.8, -0.7, 0.7, 1.6, 3.4, 8.8] {
            for axis in 0..2 {
                let mut center = Vec2::ZERO;
                center[axis] = coordinate;
                let solid = Aabb2::from_center(center, Vec2::splat(0.7));
                for direction in [-1.0, 1.0] {
                    let mut body = Body2D::new(Vec2::ZERO, Vec2::new(0.8, 1.8));
                    body.position[axis] = if direction < 0.0 {
                        solid.max[axis] + body.half_size[axis] + 0.137
                    } else {
                        solid.min[axis] - body.half_size[axis] - 0.137
                    };
                    let mut contacts = 0;
                    for _ in 0..120 {
                        body.velocity[axis] = direction * 3.25;
                        body.move_and_slide(1.0 / 120.0, &[solid]);
                        assert!(!body.bounds().intersects(&solid), "{body:?}, {solid:?}");
                        if body.velocity[axis] == 0.0 {
                            contacts += 1;
                            assert_eq!(body.grounded, axis == 1 && direction > 0.0);
                        }
                    }
                    assert!(contacts > 100, "lost contact: {body:?}, {solid:?}");
                }
            }
        }
    }

    #[test]
    fn fractional_contacts_remain_outside_solids_in_3d() {
        for coordinate in [-8.8, -0.7, 0.7, 1.6, 3.4, 8.8] {
            for axis in 0..3 {
                let mut center = Vec3::ZERO;
                center[axis] = coordinate;
                let solid = Aabb3::from_center(center, Vec3::splat(0.7));
                for direction in [-1.0, 1.0] {
                    let mut body = Body3D::new(Vec3::ZERO, Vec3::new(0.8, 1.8, 0.8));
                    body.position[axis] = if direction < 0.0 {
                        solid.max[axis] + body.half_size[axis] + 0.137
                    } else {
                        solid.min[axis] - body.half_size[axis] - 0.137
                    };
                    let mut contacts = 0;
                    for _ in 0..120 {
                        body.velocity[axis] = direction * 3.25;
                        body.move_and_slide(1.0 / 120.0, &[solid]);
                        assert!(!body.bounds().intersects(&solid), "{body:?}, {solid:?}");
                        if body.velocity[axis] == 0.0 {
                            contacts += 1;
                            assert_eq!(body.grounded, axis == 1 && direction < 0.0);
                        }
                    }
                    assert!(contacts > 100, "lost contact: {body:?}, {solid:?}");
                }
            }
        }
    }
}

/// Penetration queries shared with the arcade physics world.
pub use crate::physics::{Penetration2D, Penetration3D};

/// World-space circle for standalone overlap queries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    /// Center in world units.
    pub center: Vec2,
    /// Positive radius.
    pub radius: f32,
}
impl Circle {
    /// Creates a circle. Panics for a nonfinite center or invalid radius.
    pub fn new(center: Vec2, radius: f32) -> Self {
        assert!(center.is_finite());
        crate::physics::Shape2D::round(radius);
        Self { center, radius }
    }
    /// Positive overlap; normal moves this circle away from the other.
    pub fn overlap(&self, other: &Self) -> Option<Penetration2D> {
        crate::physics::Shape2D::round(self.radius).overlap(
            self.center,
            crate::physics::Shape2D::round(other.radius),
            other.center,
        )
    }
    /// Positive overlap against a box, including a center inside the box.
    pub fn overlap_box(&self, other: &Aabb2) -> Option<Penetration2D> {
        crate::physics::Shape2D::round(self.radius).overlap(
            self.center,
            crate::physics::Shape2D::box_shape(other.size()),
            other.center(),
        )
    }
}
/// World-space sphere for standalone overlap queries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    /// Center in world units.
    pub center: Vec3,
    /// Positive radius.
    pub radius: f32,
}
impl Sphere {
    /// Creates a sphere. Panics for a nonfinite center or invalid radius.
    pub fn new(center: Vec3, radius: f32) -> Self {
        assert!(center.is_finite());
        crate::physics::Shape3D::round(radius);
        Self { center, radius }
    }
    /// Positive overlap; normal moves this sphere away from the other.
    pub fn overlap(&self, other: &Self) -> Option<Penetration3D> {
        crate::physics::Shape3D::round(self.radius).overlap(
            self.center,
            crate::physics::Shape3D::round(other.radius),
            other.center,
        )
    }
    /// Positive overlap against a box, including a center inside the box.
    pub fn overlap_box(&self, other: &Aabb3) -> Option<Penetration3D> {
        crate::physics::Shape3D::round(self.radius).overlap(
            self.center,
            crate::physics::Shape3D::box_shape(other.size()),
            other.center(),
        )
    }
}
impl Aabb2 {
    /// Positive overlap; normal moves this box away from the other box.
    pub fn overlap(&self, other: &Self) -> Option<Penetration2D> {
        crate::physics::Shape2D::box_shape(self.size()).overlap(
            self.center(),
            crate::physics::Shape2D::box_shape(other.size()),
            other.center(),
        )
    }
    /// Positive overlap; normal moves this box away from the circle.
    pub fn overlap_circle(&self, other: &Circle) -> Option<Penetration2D> {
        other.overlap_box(self).map(|c| Penetration2D {
            normal: -c.normal,
            depth: c.depth,
        })
    }
}
impl Aabb3 {
    /// Positive overlap; normal moves this box away from the other box.
    pub fn overlap(&self, other: &Self) -> Option<Penetration3D> {
        crate::physics::Shape3D::box_shape(self.size()).overlap(
            self.center(),
            crate::physics::Shape3D::box_shape(other.size()),
            other.center(),
        )
    }
    /// Positive overlap; normal moves this box away from the sphere.
    pub fn overlap_sphere(&self, other: &Sphere) -> Option<Penetration3D> {
        other.overlap_box(self).map(|c| Penetration3D {
            normal: -c.normal,
            depth: c.depth,
        })
    }
}
