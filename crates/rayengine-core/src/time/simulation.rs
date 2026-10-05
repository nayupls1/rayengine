//! A game-owned, scaled timeline independent of input and presentation.

use super::{FixedClock, FramePlan};
use std::{error::Error, fmt, time::Duration};

const SPEED_UNITS: u128 = 1_000_000;

/// A speed was nonfinite or outside `0..=1000`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSimulationSpeed;

impl fmt::Display for InvalidSimulationSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("simulation speed must be finite and between 0 and 1000")
    }
}

impl Error for InvalidSimulationSpeed {}

/// Optional CPU simulation timeline with pause and speed controls.
///
/// Advance this clock with **unscaled** elapsed time. Speed changes the number
/// of requested ticks, never their duration. Input, menus and presentation use
/// their own unscaled update cadence. Each call requests at most `max_steps`;
/// excess whole simulation ticks are discarded and reported in
/// [`FramePlan::dropped`]. Discarded time does not advance tick indices or
/// [`Self::elapsed`]. Fractional ticks survive overload, pause and speed changes.
/// There is no retained whole-tick backlog.
///
/// Speed is rounded to the nearest millionth, in `0..=1000`. Scaling uses integer
/// arithmetic and carries subnanosecond fractions across calls, so splitting
/// elapsed time does not introduce rounding drift. Extremely large scaled
/// durations saturate at [`Duration::MAX`], as does the underlying accumulator.
/// Dropped time reports only the representable part in that case.
///
/// ```
/// use rayengine_core::time::SimulationClock;
/// use std::time::Duration;
/// let mut simulation = SimulationClock::new(100, 8);
/// simulation.set_speed(4.0).unwrap();
/// let plan = simulation.advance(Duration::from_millis(10));
/// assert_eq!(plan.steps, 4);
/// for tick in plan.ticks(simulation.step()) {
///     assert_eq!(tick.dt, 0.01); // unchanged by fast-forward
/// }
/// simulation.set_paused(true);
/// assert_eq!(simulation.advance(Duration::from_secs(60)).steps, 0);
/// ```
#[derive(Debug)]
pub struct SimulationClock {
    fixed: FixedClock,
    speed_units: u32,
    subnanoseconds: u128,
    paused: bool,
}

impl SimulationClock {
    /// Creates a running, normal-speed timeline. Panics unless `hz` is in
    /// `1..=1000` and `max_steps` is positive, just like [`FixedClock::new`].
    pub fn new(hz: u32, max_steps: u32) -> Self {
        Self {
            fixed: FixedClock::new(hz, max_steps),
            speed_units: SPEED_UNITS as u32,
            subnanoseconds: 0,
            paused: false,
        }
    }

    /// Fixed simulation timestep, rounded to the nearest nanosecond.
    pub fn step(&self) -> Duration {
        self.fixed.step()
    }

    /// Number of requested simulation ticks, excluding discarded work.
    /// Saturates at `u64::MAX`, just like [`FixedClock::ticks`].
    pub fn ticks(&self) -> u64 {
        self.fixed.ticks()
    }

    /// Requested simulation time (`ticks * step`), excluding fractional and
    /// discarded time. The caller must execute every requested tick.
    pub fn elapsed(&self) -> Duration {
        duration_from_nanos(self.step().as_nanos() * u128::from(self.ticks()))
    }

    /// Configured speed after rounding to the nearest millionth.
    pub fn speed(&self) -> f64 {
        f64::from(self.speed_units) / SPEED_UNITS as f64
    }

    /// Changes speed without changing the fixed timestep or pending fractions.
    /// Zero stops progress without changing the explicit pause state. Positive
    /// speeds below `0.0000005` round to zero. Invalid values leave state intact.
    pub fn set_speed(&mut self, speed: f64) -> Result<(), InvalidSimulationSpeed> {
        if !speed.is_finite() || !(0.0..=1000.0).contains(&speed) {
            return Err(InvalidSimulationSpeed);
        }
        self.speed_units = (speed * SPEED_UNITS as f64).round() as u32;
        Ok(())
    }

    /// Whether this timeline is explicitly paused (independent of speed).
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pauses/resumes without banking time or removing pending fractions.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Simulation interpolation fraction. It freezes while paused or at speed
    /// zero. Use this for simulation state rather than the host clock's alpha.
    pub fn alpha(&self) -> f32 {
        self.fixed.alpha()
    }

    /// Scales unscaled elapsed time and returns bounded simulation work.
    /// Calling while paused discards that call's elapsed time without catch-up
    /// on resume. This method does not sample or consume input; sample commands
    /// once outside the tick loop and take pending edges once inside it.
    pub fn advance(&mut self, elapsed: Duration) -> FramePlan {
        let scaled = if self.paused || self.speed_units == 0 {
            Duration::ZERO
        } else {
            // Even Duration::MAX * the maximum speed units fits in u128.
            let units = elapsed.as_nanos() * u128::from(self.speed_units) + self.subnanoseconds;
            self.subnanoseconds = units % SPEED_UNITS;
            duration_from_nanos(units / SPEED_UNITS)
        };
        self.fixed.advance(scaled)
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let nanos = nanos.min(Duration::MAX.as_nanos());
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

#[cfg(test)]
mod tests;
