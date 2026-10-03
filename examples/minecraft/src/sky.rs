//! CPU-only day/night cycle shared by the voxel shader, lit pickups and the sky.
use rayengine_core::glam::Vec3;

/// Real seconds for one full day and night.
pub const DAY_SECONDS: f32 = 600.0;
/// New sessions begin shortly after sunrise.
pub const MORNING: f32 = 0.06;

/// Fraction of the current day: 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    time: f32,
    day: u32,
}
impl Default for Sky {
    fn default() -> Self {
        Self {
            time: MORNING,
            day: 1,
        }
    }
}
/// Resolved lighting for one frame. Directions are world-space ray directions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyLight {
    /// Direction sun- or moonlight travels; never zero.
    pub direction: Vec3,
    /// Directional irradiance from the sun or moon.
    pub color: Vec3,
    /// Uniform sky irradiance.
    pub ambient: Vec3,
    /// Clear and fog color, linear 0..=1.
    pub sky: Vec3,
    /// Unit vector toward the sun's disc (the moon is opposite).
    pub sun: Vec3,
    /// 0 at night, 1 in full daylight.
    pub daylight: f32,
}
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
impl Sky {
    /// A specific time of day in `0..1`; out-of-range values wrap.
    pub fn at(time: f32) -> Self {
        Self {
            time: time.rem_euclid(1.0),
            day: 1,
        }
    }
    /// Fraction of the current day in `0..1`.
    pub fn time(&self) -> f32 {
        self.time
    }
    /// One-based day counter.
    pub fn day(&self) -> u32 {
        self.day
    }
    /// Advance by simulation seconds. Non-finite or negative steps are ignored.
    pub fn advance(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let next = self.time + dt / DAY_SECONDS;
        self.day = self.day.saturating_add(next.floor() as u32);
        self.time = next.rem_euclid(1.0);
    }
    /// Clock reading as hours and minutes, with sunrise at 06:00.
    pub fn clock(&self) -> (u32, u32) {
        let minutes = ((self.time * 24.0 + 6.0) * 60.0) as u32 % (24 * 60);
        (minutes / 60, minutes % 60)
    }
    /// Sun position, light colors and sky tint for the current time.
    pub fn light(&self) -> SkyLight {
        let angle = self.time * std::f32::consts::TAU;
        // A slight southern tilt keeps east/west faces from sharing one value.
        let sun = Vec3::new(angle.cos(), angle.sin(), 0.35).normalize();
        let height = sun.y;
        let daylight = smoothstep(-0.12, 0.22, height);
        // Low sun is warm; overhead sun is near white.
        let warm = 1.0 - smoothstep(0.0, 0.45, height.abs());
        let sun_color = Vec3::new(1.0, 0.95, 0.86).lerp(Vec3::new(1.0, 0.62, 0.36), warm);
        let moon_color = Vec3::new(0.14, 0.16, 0.26);
        let (direction, color) = if height > -0.05 {
            (-sun, sun_color * 0.62 * smoothstep(-0.05, 0.12, height))
        } else {
            (sun, moon_color * smoothstep(0.05, 0.25, -height))
        };
        let night_sky = Vec3::new(0.03, 0.04, 0.10);
        let day_sky = Vec3::new(0.47, 0.67, 0.86);
        let dusk_sky = Vec3::new(0.86, 0.50, 0.32);
        let dusk = (1.0 - smoothstep(0.0, 0.4, height.abs())) * smoothstep(-0.25, 0.0, height);
        let sky = night_sky
            .lerp(day_sky, daylight)
            .lerp(dusk_sky, dusk * 0.75);
        let ambient = Vec3::new(0.14, 0.15, 0.24).lerp(Vec3::new(0.52, 0.54, 0.58), daylight);
        SkyLight {
            direction,
            color,
            ambient,
            sky,
            sun,
            daylight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_wrap_and_count_without_losing_precision() {
        let mut sky = Sky::default();
        assert_eq!(sky.day(), 1);
        sky.advance(DAY_SECONDS);
        assert_eq!(sky.day(), 2);
        assert!((sky.time() - MORNING).abs() < 1e-5);
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            sky.advance(dt);
        }
        assert_eq!(sky.day(), 2);
        assert_eq!(Sky::at(0.25).clock(), (12, 0));
        assert_eq!(Sky::at(0.75).clock(), (0, 0));
        assert_eq!(Sky::at(-0.75).time(), 0.25);
    }

    #[test]
    fn noon_is_bright_midnight_is_dark_and_lights_stay_valid() {
        let noon = Sky::at(0.25).light();
        let midnight = Sky::at(0.75).light();
        assert!(noon.daylight > 0.99 && midnight.daylight < 0.01);
        assert!(noon.ambient.x > midnight.ambient.x * 3.0);
        assert!(noon.sky.z > midnight.sky.z);
        assert!(noon.direction.y < 0.0, "sunlight travels downward at noon");
        assert!(
            midnight.direction.y < 0.0,
            "moonlight travels downward at midnight"
        );
        for i in 0..400 {
            let light = Sky::at(i as f32 / 400.0).light();
            assert!(light.direction.is_finite() && light.direction.length() > 0.5);
            for c in [light.color, light.ambient, light.sky] {
                assert!(c.is_finite() && c.min_element() >= 0.0 && c.max_element() <= 1.0);
            }
        }
    }
}
