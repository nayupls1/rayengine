use super::GridLayout;
use crate::collision::{Body2D, Body3D};
use glam::{UVec2, Vec2};

/// Steers a character through world-space waypoints.
///
/// The follower only chooses velocities; collision stays with
/// [`Body2D::move_and_slide`] or [`Body3D::move_and_slide`]. A waypoint is
/// reached within `arrive_radius`. Each step is shortened so it ends on the
/// current waypoint instead of jumping past it, keeping the character close
/// to the path's segments, which [`smooth_path`](super::smooth_path) checked
/// for clearance. Collision moves along each axis in turn, so keep steps
/// (`speed * dt`) well under a cell. Any waypoint but the last also counts once the character is
/// beyond it along its segment and within `arrive_radius` of that segment's
/// line: the incoming segment, or the outgoing one for the first waypoint, so
/// a character on the path ahead of its starting cell's center does not step
/// back. A character pushed farther aside returns to the waypoint first.
///
/// ```
/// use rayengine_core::collision::Body2D;
/// use rayengine_core::glam::{UVec2, Vec2};
/// use rayengine_core::pathfinding::{GridLayout, PathFollower};
///
/// let layout = GridLayout::new(Vec2::ZERO, Vec2::ONE);
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
    /// slows down so one step of `dt` seconds does not overshoot the current
    /// waypoint. Panics for a nonfinite position, a negative or nonfinite
    /// speed, or a negative or nonfinite `dt`.
    pub fn velocity(&mut self, position: Vec2, speed: f32, dt: f32) -> Vec2 {
        assert!(position.is_finite() && speed.is_finite() && speed >= 0.0);
        assert!(dt.is_finite() && dt >= 0.0);
        let last = self.waypoints.len().saturating_sub(1);
        while let Some(&target) = self.waypoints.get(self.next) {
            let offset = position - target;
            let reached = offset.length() <= self.arrive_radius;
            let segment = match self.next {
                next if next == last => Vec2::ZERO,
                0 => self.waypoints[1] - target,
                next => target - self.waypoints[next - 1],
            };
            let direction = segment.normalize_or_zero();
            let passed = direction.dot(offset) > 0.0
                && direction.perp_dot(offset).abs() <= self.arrive_radius;
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
        let speed = if dt > 0.0 {
            speed.min(distance / dt)
        } else {
            speed
        };
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
