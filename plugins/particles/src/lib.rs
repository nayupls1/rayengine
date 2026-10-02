#![doc = include_str!("../README.md")]
#![cfg_attr(
    feature = "render",
    doc = "\n\n## Complete composition example\n\n```no_run"
)]
#![cfg_attr(feature = "render", doc = include_str!("../examples/effects.rs"))]
#![cfg_attr(feature = "render", doc = "```")]

use rayengine_core::glam::{Vec3, Vec4};
use std::fmt;

#[cfg(feature = "render")]
pub mod render;

/// Hard admission ceiling for particle storage and spawn work per call.
pub const MAX_PARTICLES: usize = 1_000_000;

/// Independent emitter configuration, copied and validated on construction.
#[derive(Clone, Debug)]
pub struct EmitterConfig {
    /// Maximum live particles; storage is reserved once at construction.
    pub capacity: usize,
    /// Maximum admitted births per step or burst call (no deferred queue).
    pub max_spawn: usize,
    /// Continuous births per second. Zero selects burst-only operation.
    pub rate: f32,
    /// Inclusive minimum/maximum lifetime in seconds, strictly positive.
    pub lifetime: [f32; 2],
    /// World-space center of initial positions (XY for 2D).
    pub position: Vec3,
    /// Independent uniform +/- variation on each initial position component.
    pub position_spread: Vec3,
    /// Initial velocity in world units/second.
    pub velocity: Vec3,
    /// Independent uniform +/- variation on each initial velocity component.
    pub velocity_spread: Vec3,
    /// Constant acceleration in world units/second squared.
    pub acceleration: Vec3,
    /// Straight RGBA at birth, each component in 0..=1.
    pub start_color: Vec4,
    /// Straight RGBA at death; interpolated linearly by normalized age.
    pub end_color: Vec4,
    /// Full square width at birth, in world units.
    pub start_size: f32,
    /// Full square width at death; interpolated linearly by normalized age.
    pub end_size: f32,
    /// Explicit PRNG seed; reset restores it. No global random state is used.
    pub seed: u64,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            capacity: 256,
            max_spawn: 64,
            rate: 0.0,
            lifetime: [1.0, 1.0],
            position: Vec3::ZERO,
            position_spread: Vec3::ZERO,
            velocity: Vec3::ZERO,
            velocity_spread: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            start_color: Vec4::ONE,
            end_color: Vec4::new(1.0, 1.0, 1.0, 0.0),
            start_size: 1.0,
            end_size: 0.0,
            seed: 0,
        }
    }
}

/// Invalid configuration, timestep, or moved emitter origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleError(pub &'static str);
impl fmt::Display for ParticleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ParticleError {}

impl EmitterConfig {
    fn validate(&self) -> Result<(), ParticleError> {
        if self.capacity == 0
            || self.capacity > MAX_PARTICLES
            || self.max_spawn == 0
            || self.max_spawn > MAX_PARTICLES
        {
            return Err(ParticleError(
                "capacity and max_spawn must be in 1..=MAX_PARTICLES",
            ));
        }
        if !self.rate.is_finite() || self.rate < 0.0 {
            return Err(ParticleError("rate must be finite and nonnegative"));
        }
        if !self.lifetime.iter().all(|v| v.is_finite() && *v > 0.0)
            || self.lifetime[0] > self.lifetime[1]
        {
            return Err(ParticleError(
                "lifetime must be finite, positive, and ordered",
            ));
        }
        validate_spread(self.position, self.position_spread)?;
        validate_spread(self.velocity, self.velocity_spread)?;
        if !self.acceleration.is_finite() {
            return Err(ParticleError("acceleration must be finite"));
        }
        for color in [self.start_color, self.end_color] {
            if !color.is_finite() || color.min_element() < 0.0 || color.max_element() > 1.0 {
                return Err(ParticleError("color components must be in 0..=1"));
            }
        }
        if ![self.start_size, self.end_size]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
        {
            return Err(ParticleError("sizes must be finite and nonnegative"));
        }
        Ok(())
    }
}
fn validate_spread(center: Vec3, spread: Vec3) -> Result<(), ParticleError> {
    if !center.is_finite()
        || !spread.is_finite()
        || spread.min_element() < 0.0
        || !(center.abs() + spread).is_finite()
    {
        return Err(ParticleError(
            "initial values and nonnegative spread must have finite bounds",
        ));
    }
    Ok(())
}

/// Read-only particle simulation state, in stable birth order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    position: Vec3,
    previous_position: Vec3,
    velocity: Vec3,
    age: f32,
    previous_age: f32,
    lifetime: f32,
}
impl Particle {
    /// Current world-space position.
    pub fn position(&self) -> Vec3 {
        self.position
    }
    /// Current velocity.
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }
    /// Elapsed lifetime in seconds.
    pub fn age(&self) -> f32 {
        self.age
    }
    /// Sampled total lifetime in seconds.
    pub fn lifetime(&self) -> f32 {
        self.lifetime
    }
    /// Interpolated world-space position; alpha is clamped, NaN selects current.
    pub fn interpolated_position(&self, alpha: f32) -> Vec3 {
        self.previous_position
            .lerp(self.position, alpha_value(alpha))
    }
}
fn alpha_value(alpha: f32) -> f32 {
    if alpha.is_nan() {
        1.0
    } else {
        alpha.clamp(0.0, 1.0)
    }
}

/// Appearance sampled at an interpolated age.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance {
    /// Straight RGBA, each component in 0..=1.
    pub color: Vec4,
    /// Full square width in world units.
    pub size: f32,
}

/// Bounded simulation owned and explicitly ticked by the game.
///
/// New emitters start emitting. Stop disables continuous emission and bursts;
/// existing particles still age. Reset clears particles, fractional emission,
/// and RNG history, and resumes emission. Dropping removes all CPU state.
pub struct Emitter {
    config: EmitterConfig,
    particles: Vec<Particle>,
    emitting: bool,
    remainder: f64,
    random: u64,
}
impl Emitter {
    /// Validates configuration and reserves fixed-capacity storage fallibly.
    pub fn new(config: EmitterConfig) -> Result<Self, ParticleError> {
        config.validate()?;
        let mut particles = Vec::new();
        particles
            .try_reserve_exact(config.capacity)
            .map_err(|_| ParticleError("particle storage allocation failed"))?;
        let random = config.seed;
        Ok(Self {
            config,
            particles,
            emitting: true,
            remainder: 0.0,
            random,
        })
    }
    /// Validated configuration; changing the origin uses [`Self::set_position`].
    pub fn config(&self) -> &EmitterConfig {
        &self.config
    }
    /// Live particles, ordered oldest to newest without allocating a snapshot.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }
    /// Current number of live particles.
    pub fn len(&self) -> usize {
        self.particles.len()
    }
    /// Whether no particles are alive.
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }
    /// Whether continuous emission and bursts are enabled.
    pub fn is_emitting(&self) -> bool {
        self.emitting
    }
    /// Stops new births; live particles continue to age on subsequent steps.
    pub fn stop(&mut self) {
        self.emitting = false;
    }
    /// Resumes births, keeping live particles and fractional emission history.
    pub fn start(&mut self) {
        self.emitting = true;
    }
    /// Clears particles and restores seed/phase, then resumes births.
    pub fn reset(&mut self) {
        self.particles.clear();
        self.remainder = 0.0;
        self.random = self.config.seed;
        self.emitting = true;
    }
    /// Moves future births only. Failure leaves the origin unchanged.
    pub fn set_position(&mut self, position: Vec3) -> Result<(), ParticleError> {
        validate_spread(position, self.config.position_spread)?;
        self.config.position = position;
        Ok(())
    }
    /// Requests immediate births. Returns the number admitted. Excess is dropped
    /// at the capacity/max_spawn limit, with no queue or RNG work for rejected births.
    pub fn burst(&mut self, count: usize) -> usize {
        if !self.emitting {
            return 0;
        }
        let count = count
            .min(self.config.max_spawn)
            .min(self.config.capacity - self.len());
        for _ in 0..count {
            let position = self.config.position + self.variation() * self.config.position_spread;
            let velocity = self.config.velocity + self.variation() * self.config.velocity_spread;
            let t = self.unit();
            let lifetime = (f64::from(self.config.lifetime[0]) * (1.0 - f64::from(t))
                + f64::from(self.config.lifetime[1]) * f64::from(t))
                as f32;
            self.particles.push(Particle {
                position,
                previous_position: position,
                velocity,
                age: 0.0,
                previous_age: 0.0,
                lifetime,
            });
        }
        count
    }
    /// Advances exactly the supplied fixed-tick seconds, then admits continuous
    /// births at tick end. Returns admitted births. Excess whole births are dropped;
    /// only the fractional part carries forward. Zero collapses interpolation
    /// history without emitting. Invalid dt leaves all state unchanged.
    /// Numerically overflowing motion retires that particle instead of storing NaN/Inf.
    pub fn step(&mut self, dt: f32) -> Result<usize, ParticleError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(ParticleError("dt must be finite and nonnegative"));
        }
        let delta = f64::from(dt);
        let acceleration = self.config.acceleration.as_dvec3();
        self.particles.retain_mut(|p| {
            p.previous_position = p.position;
            p.previous_age = p.age;
            let age = f64::from(p.age) + delta;
            if age >= f64::from(p.lifetime) {
                return false;
            }
            p.age = age as f32;
            let velocity = p.velocity.as_dvec3();
            p.position =
                (p.position.as_dvec3() + velocity * delta + acceleration * (0.5 * delta * delta))
                    .as_vec3();
            p.velocity = (velocity + acceleration * delta).as_vec3();
            p.position.is_finite() && p.velocity.is_finite()
        });
        if dt == 0.0 || !self.emitting {
            return Ok(0);
        }
        let desired = self.remainder + f64::from(self.config.rate) * delta;
        self.remainder = desired.fract();
        Ok(self.burst(desired.floor().min(self.config.max_spawn as f64) as usize))
    }
    /// Samples linear size/color evolution for one particle. Use a particle from
    /// this emitter so the appearance uses the matching configuration.
    pub fn appearance(&self, particle: &Particle, alpha: f32) -> Appearance {
        let age =
            particle.previous_age * (1.0 - alpha_value(alpha)) + particle.age * alpha_value(alpha);
        let t = (age / particle.lifetime).clamp(0.0, 1.0);
        Appearance {
            color: self.config.start_color.lerp(self.config.end_color, t),
            size: (f64::from(self.config.start_size) * (1.0 - f64::from(t))
                + f64::from(self.config.end_size) * f64::from(t)) as f32,
        }
    }
    fn unit(&mut self) -> f32 {
        // SplitMix64, including seed zero; upper 24 bits give [0, 1).
        self.random = self.random.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.random;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 40) as f32 / 16_777_216.0
    }
    fn variation(&mut self) -> Vec3 {
        Vec3::new(self.unit(), self.unit(), self.unit()) * 2.0 - Vec3::ONE
    }
}

#[cfg(test)]
mod tests;
