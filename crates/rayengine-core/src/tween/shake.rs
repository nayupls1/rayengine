//! Trauma-based screen shake for 2D and 3D cameras.

use crate::camera::{Camera2D, Camera3D};
use glam::{Mat2, Quat, Vec2};
use std::{fmt, time::Duration};

/// Shake limits at full trauma. Offsets are in camera-aligned world units, so
/// scale them to the camera's visible height or distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakeConfig {
    /// Maximum horizontal/vertical offset at full trauma, in world units.
    pub max_offset: Vec2,
    /// Maximum roll at full trauma, in radians.
    pub max_roll: f32,
    /// Noise samples per second; higher values shake faster.
    pub frequency: f32,
    /// Trauma removed per second (`1.0` fades full trauma in one second).
    pub decay: f32,
    /// Seed selecting a reproducible shake pattern.
    pub seed: u32,
}

impl Default for ShakeConfig {
    fn default() -> Self {
        Self {
            max_offset: Vec2::splat(8.0),
            max_roll: 0.05,
            frequency: 18.0,
            decay: 1.5,
            seed: 0,
        }
    }
}

/// A shake configuration with a nonfinite or out-of-range field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShakeError(pub &'static str);

impl fmt::Display for ShakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ShakeError {}

/// Displacement to apply to a camera this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShakeSample {
    /// Camera-aligned offset: +X right and +Y screen-down in 2D, +Y up in 3D.
    pub offset: Vec2,
    /// Roll in radians.
    pub roll: f32,
}

/// Trauma-driven camera shake with smooth, seeded, deterministic noise.
///
/// Add trauma (`0..=1`) on impacts. Shake strength is trauma squared, so small
/// hits stay subtle, and trauma decays linearly with explicitly supplied time.
/// Apply the shake to a copy of the camera when drawing; the game keeps its
/// unshaken camera. With zero trauma the camera is returned unchanged.
///
/// ```
/// use rayengine_core::{camera::Camera2D, tween::{Shake, ShakeConfig}};
/// use std::time::Duration;
/// let mut shake = Shake::new(ShakeConfig::default()).unwrap();
/// let camera = Camera2D::default();
/// shake.add_trauma(0.6);
/// shake.advance(Duration::from_millis(16));
/// let shaken = shake.apply_2d(camera);
/// assert_ne!(shaken.target, camera.target);
/// shake.advance(Duration::from_secs(1)); // fully decayed
/// assert_eq!(shake.apply_2d(camera).target, camera.target);
/// ```
#[derive(Clone, Debug)]
pub struct Shake {
    config: ShakeConfig,
    trauma: f32,
    // Noise position kept as an integer lattice index plus a fraction, so
    // running for hours never loses precision.
    lattice: u32,
    phase: f64,
}

impl Shake {
    /// Validates finite, nonnegative limits, decay and a positive frequency.
    pub fn new(config: ShakeConfig) -> Result<Self, ShakeError> {
        Self::validate(&config)?;
        Ok(Self {
            config,
            trauma: 0.0,
            lattice: 0,
            phase: 0.0,
        })
    }

    fn validate(config: &ShakeConfig) -> Result<(), ShakeError> {
        if !config.max_offset.is_finite() || config.max_offset.min_element() < 0.0 {
            return Err(ShakeError("shake offset must be finite and nonnegative"));
        }
        if !config.max_roll.is_finite() || config.max_roll < 0.0 {
            return Err(ShakeError("shake roll must be finite and nonnegative"));
        }
        if !config.frequency.is_finite() || config.frequency <= 0.0 {
            return Err(ShakeError("shake frequency must be finite and positive"));
        }
        if !config.decay.is_finite() || config.decay < 0.0 {
            return Err(ShakeError("shake decay must be finite and nonnegative"));
        }
        Ok(())
    }

    /// Current limits.
    pub fn config(&self) -> &ShakeConfig {
        &self.config
    }
    /// Replaces limits, keeping trauma and noise position.
    pub fn set_config(&mut self, config: ShakeConfig) -> Result<(), ShakeError> {
        Self::validate(&config)?;
        self.config = config;
        Ok(())
    }

    /// Adds trauma, saturating at one. Nonfinite amounts are ignored; negative
    /// amounts reduce trauma.
    pub fn add_trauma(&mut self, amount: f32) {
        if amount.is_finite() {
            self.trauma = (self.trauma + amount).clamp(0.0, 1.0);
        }
    }
    /// Sets trauma, clamped to `0..=1`. Nonfinite values are ignored.
    pub fn set_trauma(&mut self, trauma: f32) {
        if trauma.is_finite() {
            self.trauma = trauma.clamp(0.0, 1.0);
        }
    }
    /// Current trauma in `0..=1`.
    pub fn trauma(&self) -> f32 {
        self.trauma
    }
    /// Shake strength, `trauma²`.
    pub fn intensity(&self) -> f32 {
        self.trauma * self.trauma
    }

    /// Decays trauma and moves along the noise by explicitly supplied time.
    pub fn advance(&mut self, dt: Duration) {
        let seconds = dt.as_secs_f64();
        self.trauma =
            (f64::from(self.trauma) - f64::from(self.config.decay) * seconds).max(0.0) as f32;
        let phase = self.phase + seconds * f64::from(self.config.frequency);
        let whole = phase.floor();
        self.phase = phase - whole;
        // Lattice indices wrap; hashed noise stays continuous across the wrap.
        self.lattice = self
            .lattice
            .wrapping_add(whole.rem_euclid(4_294_967_296.0) as u32);
    }

    /// Current displacement; exactly zero without trauma.
    pub fn sample(&self) -> ShakeSample {
        let intensity = self.intensity();
        if intensity == 0.0 {
            return ShakeSample::default();
        }
        ShakeSample {
            offset: Vec2::new(self.noise(0), self.noise(1)) * self.config.max_offset * intensity,
            roll: self.noise(2) * self.config.max_roll * intensity,
        }
    }

    /// Shaken copy of a 2D camera. The offset follows the camera's screen axes.
    pub fn apply_2d(&self, camera: Camera2D) -> Camera2D {
        let sample = self.sample();
        if sample == ShakeSample::default() {
            return camera;
        }
        Camera2D {
            target: camera.target + Mat2::from_angle(-camera.rotation) * sample.offset,
            rotation: camera.rotation + sample.roll,
            ..camera
        }
    }

    /// Shaken copy of a 3D camera: the eye and target move together along the
    /// view's right/up axes and `up` rolls around the view direction.
    /// A degenerate camera (eye at target, or `up` parallel to the view) is returned unchanged.
    pub fn apply_3d(&self, camera: Camera3D) -> Camera3D {
        let sample = self.sample();
        let forward = (camera.target - camera.position).normalize_or_zero();
        let right = forward.cross(camera.up).normalize_or_zero();
        if sample == ShakeSample::default() || right == glam::Vec3::ZERO {
            return camera;
        }
        let up = right.cross(forward);
        let offset = right * sample.offset.x + up * sample.offset.y;
        Camera3D {
            position: camera.position + offset,
            target: camera.target + offset,
            up: Quat::from_axis_angle(forward, sample.roll) * camera.up,
            ..camera
        }
    }

    /// Smooth value noise in `[-1, 1]` for one independent channel.
    fn noise(&self, channel: u32) -> f32 {
        let a = self.hash(self.lattice, channel);
        let b = self.hash(self.lattice.wrapping_add(1), channel);
        let t = self.phase as f32;
        let t = t * t * (3.0 - 2.0 * t);
        a + (b - a) * t
    }

    fn hash(&self, index: u32, channel: u32) -> f32 {
        // lowbias32 integer hash mixed with seed and channel.
        let mut x = index ^ self.config.seed.rotate_left(16) ^ channel.wrapping_mul(0x9E37_79B9);
        x ^= x >> 16;
        x = x.wrapping_mul(0x7FEB_352D);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846C_A68B);
        x ^= x >> 16;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lattice_wraps_without_a_discontinuity() {
        let mut shake = Shake::new(ShakeConfig {
            frequency: 1.0,
            decay: 0.0,
            ..ShakeConfig::default()
        })
        .unwrap();
        shake.set_trauma(1.0);
        shake.lattice = u32::MAX;
        shake.phase = 0.999_999;
        let before = shake.sample();
        shake.advance(Duration::from_micros(2));
        assert_eq!(shake.lattice, 0);
        assert!((shake.sample().offset - before.offset).length() < 1e-3);
        shake.advance(Duration::MAX);
        assert!(shake.phase >= 0.0 && shake.phase < 1.0);
        assert!(shake.sample().offset.is_finite());
    }
}
