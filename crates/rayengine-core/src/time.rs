//! Fixed simulation timing with bounded catch-up and render interpolation.

use std::time::Duration;

/// Information for exactly one fixed simulation update.
#[derive(Clone, Copy, Debug)]
pub struct Tick {
    /// Zero-based simulation tick index.
    pub index: u64,
    /// Duration of this tick in seconds, independent of render framerate.
    pub dt: f32,
}

/// Updates to execute before drawing a frame.
#[derive(Clone, Copy, Debug)]
pub struct FramePlan {
    /// Number of fixed updates to execute.
    pub steps: u32,
    /// Index of the first tick in this plan.
    pub first_tick: u64,
    /// Fraction between the previous and current simulation state, in `[0, 1)`.
    pub alpha: f32,
    /// Whole simulation time discarded to bound catch-up work.
    pub dropped: Duration,
}

/// Accumulator clock that never requests an unbounded number of updates.
///
/// ```
/// use std::time::Duration;
/// use rayengine_core::time::FixedClock;
/// let mut clock = FixedClock::new(60, 8);
/// let frame = clock.advance(Duration::from_millis(20));
/// assert_eq!(frame.steps, 1);
/// assert!(frame.alpha > 0.0 && frame.alpha < 1.0);
/// ```
#[derive(Debug)]
pub struct FixedClock {
    step: Duration,
    accumulator: Duration,
    max_steps: u32,
    tick: u64,
}

impl FixedClock {
    /// Creates a clock. Panics for zero rates, zero limits, or rates above 1000 Hz.
    pub fn new(hz: u32, max_steps: u32) -> Self {
        assert!((1..=1000).contains(&hz), "fixed rate must be 1..=1000 Hz");
        assert!(max_steps > 0, "catch-up limit must be positive");
        Self {
            step: Duration::from_secs_f64(1.0 / f64::from(hz)),
            accumulator: Duration::ZERO,
            max_steps,
            tick: 0,
        }
    }

    /// Fixed timestep duration, rounded to the nearest nanosecond.
    pub fn step(&self) -> Duration {
        self.step
    }

    /// Total number of requested simulation ticks.
    pub fn ticks(&self) -> u64 {
        self.tick
    }

    /// Adds elapsed wall time and returns the bounded work for this render frame.
    /// Excess whole ticks are dropped, while the fractional remainder is kept.
    pub fn advance(&mut self, elapsed: Duration) -> FramePlan {
        self.accumulator = self.accumulator.saturating_add(elapsed);
        let available = self.accumulator.as_nanos() / self.step.as_nanos();
        let steps = available.min(u128::from(self.max_steps)) as u32;
        let remainder = self.accumulator.as_nanos() % self.step.as_nanos();
        let fraction = Duration::from_nanos(remainder as u64);
        let dropped = self
            .accumulator
            .saturating_sub(self.step * steps + fraction);
        self.accumulator = fraction;
        let first_tick = self.tick;
        self.tick = self.tick.saturating_add(u64::from(steps));
        FramePlan {
            steps,
            first_tick,
            // f32 rounding must not turn a fraction just below one into one.
            alpha: (fraction.as_secs_f64() / self.step.as_secs_f64())
                .min(f64::from(f32::from_bits(1.0f32.to_bits() - 1))) as f32,
            dropped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_partition_does_not_change_simulation_ticks() {
        let mut whole = FixedClock::new(60, 1000);
        let mut pieces = FixedClock::new(60, 1000);
        whole.advance(Duration::from_secs(2));
        for _ in 0..200 {
            pieces.advance(Duration::from_millis(10));
        }
        assert_eq!(whole.ticks(), pieces.ticks());
        assert_eq!(whole.accumulator, pieces.accumulator);
    }

    #[test]
    fn catch_up_is_bounded_without_losing_fraction() {
        let mut clock = FixedClock::new(100, 4);
        let frame = clock.advance(Duration::from_millis(105));
        assert_eq!(frame.steps, 4);
        assert_eq!(frame.dropped, Duration::from_millis(60));
        assert!((frame.alpha - 0.5).abs() < 0.00001);
        let next = clock.advance(Duration::from_millis(5));
        assert_eq!(next.first_tick, 4);
        assert_eq!(next.steps, 1);
    }

    #[test]
    fn zero_elapsed_never_invents_an_update() {
        let mut clock = FixedClock::new(60, 8);
        assert_eq!(clock.advance(Duration::ZERO).steps, 0);
    }
}
