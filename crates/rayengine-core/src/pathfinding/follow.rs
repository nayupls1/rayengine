use super::GridLayout;
use crate::collision::{Body2D, Body3D};
use glam::{UVec2, Vec2};

/// Steers a character through world-space waypoints.
///
/// The follower only chooses velocities; collision stays with
/// [`Body2D::move_and_slide`] or [`Body3D::move_and_slide`]. An intermediate
/// waypoint is reached within `arrive_radius` or one step (`speed * dt`),
/// whichever is larger, or once the character has moved beyond it along the
/// segment leading to it while staying within that tolerance of the
/// segment's line, so overshooting never turns back. The first waypoint is skipped when the character is
/// already ahead of it toward the second, avoiding a step back to its
/// starting cell's center. The final waypoint is finished only inside
/// `arrive_radius`, and velocity is limited so the character stops on it.
///
/// ```
/// use rayengine_core::collision::Body2D;
/// use rayengine_core::glam::{UVec2, Vec2};
/// use rayengine_core::pathfinding::{GridLayout, PathFollower};
///
/// let layout = GridLayout::new(Vec2::ZERO, 1.0);
/// let mut follower = PathFollower::new(0.05);
/// follower.set_cells(&layout, &[UVec2::new(0, 0), UVec2::new(3, 0)]);
/// let mut body = Body2D::new(Vec2::splat(0.5), Vec2::splat(0.6));
/// for _ in 0..120 {
///     follower.steer_body_2d(&mut body, 4.0, 1.0 / 60.0);
///     body.move_and_slide(1.0 / 60.0, &[]);
/// }
/// assert!(follower.is_finished());
/// assert!((body.position - Vec2::new(3.5, 0.5)).length() < 0.05);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct PathFollower {
    waypoints: Vec<Vec2>,
    next: usize,
    arrive_radius: f32,
}

impl PathFollower {
    /// Creates a finished follower. Panics unless `arrive_radius` is finite
    /// and positive.
    pub fn new(arrive_radius: f32) -> Self {
        assert!(arrive_radius.is_finite() && arrive_radius > 0.0);
        Self {
            waypoints: Vec::new(),
            next: 0,
            arrive_radius,
        }
    }

    /// Follows the centers of `cells`, reusing waypoint capacity.
    pub fn set_cells(&mut self, layout: &GridLayout, cells: &[UVec2]) {
        self.set_waypoints(cells.iter().map(|&cell| layout.cell_center(cell)));
    }

    /// Follows world-space points, reusing waypoint capacity. Panics for a
    /// nonfinite point.
    pub fn set_waypoints(&mut self, points: impl IntoIterator<Item = Vec2>) {
        self.waypoints.clear();
        self.waypoints.extend(points);
        assert!(self.waypoints.iter().all(|point| point.is_finite()));
        self.next = 0;
    }

    /// Stops following and forgets the waypoints.
    pub fn clear(&mut self) {
        self.waypoints.clear();
        self.next = 0;
    }

    /// Whether every waypoint has been reached (or none were given).
    pub fn is_finished(&self) -> bool {
        self.next >= self.waypoints.len()
    }

    /// All waypoints of the current path.
    pub fn waypoints(&self) -> &[Vec2] {
        &self.waypoints
    }

    /// Waypoints not reached yet, starting with the current target.
    pub fn remaining(&self) -> &[Vec2] {
        &self.waypoints[self.next.min(self.waypoints.len())..]
    }

    /// Current target waypoint.
    pub fn next_waypoint(&self) -> Option<Vec2> {
        self.remaining().first().copied()
    }

    /// Velocity toward the current waypoint at `speed` units per second.
    ///
    /// Advances past reached waypoints first, returns zero once finished, and
    /// slows down so one step of `dt` seconds does not overshoot the final
    /// waypoint. Panics for a nonfinite position, a negative or nonfinite
    /// speed, or a negative or nonfinite `dt`.
    pub fn velocity(&mut self, position: Vec2, speed: f32, dt: f32) -> Vec2 {
        assert!(position.is_finite() && speed.is_finite() && speed >= 0.0);
        assert!(dt.is_finite() && dt >= 0.0);
        let last = self.waypoints.len().saturating_sub(1);
        // One step can jump over the arrival radius, so intermediate
        // waypoints accept anything within a step; the speed limit below
        // lets the final waypoint keep the strict radius.
        let tolerance = self.arrive_radius.max(speed * dt);
        while let Some(&target) = self.waypoints.get(self.next) {
            let offset = position - target;
            let radius = if self.next == last {
                self.arrive_radius
            } else {
                tolerance
            };
            let reached = offset.length() <= radius;
            // An intermediate waypoint is also passed once the character is
            // beyond it along the incoming segment and within the tolerance
            // of that segment's line, so overshoots do not turn back but a
            // character pushed aside still rounds the corner. The first
            // waypoint (usually the start cell's center) is skipped once the
            // character is ahead of it. The final one must be reached.
            let passed = match self.next.checked_sub(1) {
                _ if self.next == last => false,
                Some(previous) => {
                    let direction = (target - self.waypoints[previous]).normalize_or_zero();
                    direction.dot(offset) > 0.0 && direction.perp_dot(offset).abs() <= tolerance
                }
                None => (self.waypoints[1] - target).dot(offset) > 0.0,
            };
            if !(reached || passed) {
                break;
            }
            self.next += 1;
        }
        let Some(target) = self.next_waypoint() else {
            return Vec2::ZERO;
        };
        let offset = target - position;
        let distance = offset.length();
        let mut speed = speed;
        if self.next + 1 == self.waypoints.len() && dt > 0.0 {
            speed = speed.min(distance / dt);
        }
        offset / distance * speed
    }

    /// Sets a top-down body's velocity toward the path; see
    /// [`velocity`](Self::velocity).
    pub fn steer_body_2d(&mut self, body: &mut Body2D, speed: f32, dt: f32) {
        body.velocity = self.velocity(body.position, speed, dt);
    }

    /// Sets a 3D body's horizontal velocity, reading waypoints as world
    /// `(x, z)`. Vertical velocity (gravity, jumps) is kept.
    pub fn steer_body_3d(&mut self, body: &mut Body3D, speed: f32, dt: f32) {
        let velocity = self.velocity(Vec2::new(body.position.x, body.position.z), speed, dt);
        body.velocity.x = velocity.x;
        body.velocity.z = velocity.y;
    }
}
