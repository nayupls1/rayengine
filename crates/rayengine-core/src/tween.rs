//! Display-independent tweens, easing, sequences, parallel groups and screen shake.
//!
//! Tweens are plain game-owned values advanced by explicitly supplied time,
//! like [`crate::time::Timer`] and [`crate::sprite::AnimationPlayer`]. Timing uses
//! integer nanoseconds, so loops never drift and even [`Duration::MAX`] steps run
//! in constant time. Nothing allocates while advancing; a [`Tween`] is `Copy`
//! whenever its value is, and groups reuse the storage they were built with.
//!
//! ```
//! use rayengine_core::{events::Events, glam::Vec2, tween::*};
//! use std::time::Duration;
//!
//! const PANEL: TweenId = TweenId(1);
//! let mut slide = Tween::new(Vec2::new(-200.0, 20.0), Vec2::new(16.0, 20.0), Duration::from_millis(400))
//!     .with_ease(Ease::BackOut)
//!     .with_delay(Duration::from_millis(100))
//!     .with_id(PANEL);
//! let mut completed = Events::with_capacity(4);
//! for _ in 0..60 {
//!     if let Some(done) = slide.advance(Duration::from_secs_f32(1.0 / 120.0)) {
//!         completed.send(done);
//!     }
//! }
//! assert_eq!(slide.value(), Vec2::new(16.0, 20.0)); // exact end value
//! assert_eq!(completed.read(), &[TweenCompleted { id: PANEL }]); // reported once
//! ```

pub mod ease;
mod shake;

pub use ease::Ease;
pub use shake::{Shake, ShakeConfig, ShakeError, ShakeSample};

use glam::{Quat, Vec2, Vec3, Vec4};
use std::time::Duration;

/// Values that can be interpolated by a tween.
///
/// `t` is eased progress. It is `0.0` and `1.0` at the endpoints but may lie
/// outside `[0, 1]` for overshooting curves such as [`Ease::BackOut`].
pub trait Tweenable: Copy {
    /// Interpolates (or extrapolates) from `from` toward `to`.
    fn interpolate(from: Self, to: Self, t: f32) -> Self;
}

impl Tweenable for f32 {
    fn interpolate(from: Self, to: Self, t: f32) -> Self {
        from + (to - from) * t
    }
}
impl Tweenable for Vec2 {
    fn interpolate(from: Self, to: Self, t: f32) -> Self {
        from.lerp(to, t)
    }
}
impl Tweenable for Vec3 {
    fn interpolate(from: Self, to: Self, t: f32) -> Self {
        from.lerp(to, t)
    }
}
/// Also suitable for linear RGBA colors in `[0, 1]`.
impl Tweenable for Vec4 {
    fn interpolate(from: Self, to: Self, t: f32) -> Self {
        from.lerp(to, t)
    }
}
/// Spherical interpolation for rotations such as doors and hinges.
impl Tweenable for Quat {
    fn interpolate(from: Self, to: Self, t: f32) -> Self {
        from.slerp(to, t)
    }
}
/// 8-bit RGBA colors (the core's mesh and manifest color layout). Each channel
/// is interpolated independently, rounded and clamped to `0..=255`.
impl Tweenable for [u8; 4] {
    fn interpolate(from: Self, to: Self, t: f32) -> Self {
        std::array::from_fn(|i| {
            let (a, b) = (f32::from(from[i]), f32::from(to[i]));
            (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
        })
    }
}

/// Game-defined tween identity carried by [`TweenCompleted`], like input actions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TweenId(pub u32);

/// Returned exactly once when a tween, sequence or group completes.
/// Send it to a game-owned [`crate::events::Events`] queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TweenCompleted {
    /// Identity of the completed tween or group.
    pub id: TweenId,
}

/// Behavior after one traversal of a tween's duration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TweenMode {
    /// Run from start to end once, then complete.
    #[default]
    Once,
    /// Restart from the start after each cycle.
    Loop,
    /// Alternate direction each cycle: start→end, end→start, ...
    PingPong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Status {
    Active,
    Finished,
    Cancelled,
}

/// Something that can be advanced inside a [`Sequence`] or [`Parallel`] group.
///
/// Implemented by [`Tween`], [`Sequence`] and [`Parallel`], so groups nest.
/// Games can implement it for their own timed behaviors.
pub trait Animate {
    /// Advances unless paused, finished or cancelled. Returns the unused part
    /// of `dt` from the call that completes the animation, otherwise `None`.
    fn step(&mut self, dt: Duration) -> Option<Duration>;
    /// Returns to the start and rearms completion, preserving pause state.
    fn rewind(&mut self);
    /// Jumps to the final state and marks it finished, without reporting completion.
    fn skip_to_end(&mut self);
    /// Whether finished or cancelled; a group skips done members.
    fn is_done(&self) -> bool;
}

/// Shared explicit controls with the same meaning for tweens and groups.
macro_rules! controls {
    () => {
        /// Advances by explicitly supplied time. Returns one completion on the call
        /// that reaches the end; paused, finished and cancelled playback do nothing.
        pub fn advance(&mut self, dt: Duration) -> Option<TweenCompleted> {
            self.step(dt).map(|_| TweenCompleted { id: self.id })
        }
        /// Sets the identity reported on completion.
        pub fn with_id(mut self, id: TweenId) -> Self {
            self.id = id;
            self
        }
        /// Identity reported on completion.
        pub fn id(&self) -> TweenId {
            self.id
        }
        /// Stops advancement; time supplied while paused is discarded.
        pub fn pause(&mut self) {
            self.paused = true;
        }
        /// Allows advancement again. Finished or cancelled playback stays so until reset.
        pub fn resume(&mut self) {
            self.paused = false;
        }
        /// Whether explicit time advancement is paused.
        pub fn is_paused(&self) -> bool {
            self.paused
        }
        /// Rewinds to the start and rearms completion, preserving pause state.
        pub fn reset(&mut self) {
            self.rewind();
        }
        /// Stops at the current value without completing; [`Self::reset`] rearms.
        pub fn cancel(&mut self) {
            if self.status == Status::Active {
                self.status = Status::Cancelled;
            }
        }
        /// Jumps to the final value and returns the completion immediately, even
        /// while paused. Returns `None` if already finished or cancelled.
        pub fn finish(&mut self) -> Option<TweenCompleted> {
            if self.status != Status::Active {
                return None;
            }
            self.skip_to_end();
            Some(TweenCompleted { id: self.id })
        }
        /// Whether the end was reached (by advancing or [`Self::finish`]).
        pub fn is_finished(&self) -> bool {
            self.status == Status::Finished
        }
        /// Whether [`Self::cancel`] stopped playback before the end.
        pub fn is_cancelled(&self) -> bool {
            self.status == Status::Cancelled
        }
    };
}

fn from_nanos(nanos: u128) -> Duration {
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

/// Typed interpolation between two values with duration, delay, easing and repetition.
///
/// The delay applies once, before the first cycle. Repeating tweens run until
/// [`Self::with_cycles`] repetitions complete, or forever. Exact cycle boundaries
/// wrap to the next cycle; a finished tween holds its exact final value.
/// [`Self::finish`] on an endless tween ends its current cycle, so a ping-pong
/// leg running backward finishes at the start value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween<T> {
    from: T,
    to: T,
    duration: Duration,
    delay: Duration,
    ease: Ease,
    mode: TweenMode,
    cycles: Option<u32>,
    id: TweenId,
    waited: Duration,
    position: Duration,
    completed: u32,
    reversed: bool,
    paused: bool,
    status: Status,
}

impl<T: Tweenable> Tween<T> {
    /// Linear one-shot tween without delay. A zero duration completes on its
    /// first advance, including a zero-length one.
    pub fn new(from: T, to: T, duration: Duration) -> Self {
        Self {
            from,
            to,
            duration,
            delay: Duration::ZERO,
            ease: Ease::Linear,
            mode: TweenMode::Once,
            cycles: None,
            id: TweenId::default(),
            waited: Duration::ZERO,
            position: Duration::ZERO,
            completed: 0,
            reversed: false,
            paused: false,
            status: Status::Active,
        }
    }
    /// Waits this long, holding the start value, before the first cycle.
    /// A tween already past its delay keeps running; the new delay applies after reset.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        let started = self.waited >= self.delay
            && (!self.position.is_zero() || self.completed > 0 || self.status != Status::Active);
        self.delay = delay;
        if started {
            self.waited = delay;
        }
        self
    }
    /// Selects the easing curve applied to each cycle.
    pub fn with_ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }
    /// Selects one-shot, looping or ping-pong playback.
    /// Panics if a repeating mode is selected with a zero duration.
    pub fn with_mode(mut self, mode: TweenMode) -> Self {
        assert!(
            mode == TweenMode::Once || !self.duration.is_zero(),
            "repeating tween duration must be nonzero"
        );
        self.mode = mode;
        // Only ping-pong legs run backward; other modes continue forward.
        self.reversed &= mode == TweenMode::PingPong;
        self
    }
    /// Completes a repeating tween after this many cycles; a ping-pong cycle is
    /// one leg, so `2` goes there and back. Ignored by [`TweenMode::Once`].
    /// Panics for zero.
    pub fn with_cycles(mut self, cycles: u32) -> Self {
        assert!(cycles > 0, "tween cycle count must be positive");
        self.cycles = Some(cycles);
        self
    }

    /// Start value.
    pub fn from(&self) -> T {
        self.from
    }
    /// End value of a forward cycle.
    pub fn to(&self) -> T {
        self.to
    }
    /// Duration of one cycle.
    pub fn duration(&self) -> Duration {
        self.duration
    }
    /// Delay before the first cycle.
    pub fn delay(&self) -> Duration {
        self.delay
    }
    /// Easing curve.
    pub fn ease(&self) -> Ease {
        self.ease
    }
    /// Repetition behavior.
    pub fn mode(&self) -> TweenMode {
        self.mode
    }
    /// Completed cycles; saturates for endless repetition.
    pub fn completed_cycles(&self) -> u32 {
        self.completed
    }

    /// Un-eased progress through the current cycle, in `[0, 1]`; zero during the delay.
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return if self.status == Status::Finished {
                1.0
            } else {
                0.0
            };
        }
        (self.position.as_nanos() as f64 / self.duration.as_nanos() as f64) as f32
    }

    /// Current value. Reading never advances playback. The endpoints are returned
    /// exactly, so a finished tween reports precisely its final value.
    pub fn value(&self) -> T {
        let t = self.progress();
        let eased = self.ease.apply(if self.reversed { 1.0 - t } else { t });
        if eased == 0.0 {
            self.from
        } else if eased == 1.0 {
            self.to
        } else {
            T::interpolate(self.from, self.to, eased)
        }
    }

    /// Restarts toward a new end value from the current value, preserving
    /// duration, easing, mode, identity and pause state. The delay is skipped, so
    /// an interrupted UI slide turns around immediately instead of stalling.
    pub fn retarget(&mut self, to: T) {
        self.from = self.value();
        self.to = to;
        self.rewind();
        self.waited = self.delay;
    }

    controls!();
}

impl<T: Tweenable> Animate for Tween<T> {
    fn step(&mut self, dt: Duration) -> Option<Duration> {
        if self.paused || self.status != Status::Active {
            return None;
        }
        let mut left = dt;
        if self.waited < self.delay {
            let wait = self.delay - self.waited;
            if left < wait {
                self.waited += left;
                return None;
            }
            left -= wait;
            self.waited = self.delay;
        }
        // Even two Duration::MAX values fit in u128 nanoseconds.
        let period = self.duration.as_nanos();
        let next = self.position.as_nanos() + left.as_nanos();
        if self.mode == TweenMode::Once {
            if next < period {
                self.position = from_nanos(next);
                return None;
            }
            self.skip_to_end();
            return Some(from_nanos(next - period));
        }
        // Repeating modes always have a nonzero period.
        let crossed = next / period;
        if let Some(cycles) = self.cycles {
            // A limit set after more cycles already ran finishes on this step.
            let remaining = u128::from(cycles.saturating_sub(self.completed));
            if crossed >= remaining {
                self.skip_to_end();
                return Some(from_nanos((next - remaining * period).min(left.as_nanos())));
            }
        }
        self.completed = (u128::from(self.completed) + crossed).min(u128::from(u32::MAX)) as u32;
        if self.mode == TweenMode::PingPong && crossed % 2 == 1 {
            self.reversed = !self.reversed;
        }
        self.position = from_nanos(next % period);
        None
    }

    fn rewind(&mut self) {
        self.waited = Duration::ZERO;
        self.position = Duration::ZERO;
        self.completed = 0;
        self.reversed = false;
        self.status = Status::Active;
    }

    fn skip_to_end(&mut self) {
        self.waited = self.delay;
        self.position = self.duration;
        (self.completed, self.reversed) = match (self.mode, self.cycles) {
            (TweenMode::Once, _) => (1, false),
            // The last ping-pong leg runs backward when it is an even-numbered leg.
            (mode, Some(cycles)) => (cycles, mode == TweenMode::PingPong && cycles % 2 == 0),
            // Endless repetition finishes its current cycle, keeping its direction.
            (_, None) => (self.completed.saturating_add(1), self.reversed),
        };
        self.status = Status::Finished;
    }

    fn is_done(&self) -> bool {
        self.status != Status::Active
    }
}

/// Fixed-size storage of group members, indexed without allocation.
///
/// Implemented for arrays and `Vec`s of one [`Animate`] type (allocated once by
/// the game) and for tuples of up to eight different [`Animate`] types.
pub trait Tracks {
    /// Number of members.
    fn len(&self) -> usize;
    /// Whether there are no members.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Member at `index`, which must be less than [`Self::len`].
    fn track(&self, index: usize) -> &dyn Animate;
    /// Mutable member at `index`, which must be less than [`Self::len`].
    fn track_mut(&mut self, index: usize) -> &mut dyn Animate;
}

impl<A: Animate, const N: usize> Tracks for [A; N] {
    fn len(&self) -> usize {
        N
    }
    fn track(&self, index: usize) -> &dyn Animate {
        &self[index]
    }
    fn track_mut(&mut self, index: usize) -> &mut dyn Animate {
        &mut self[index]
    }
}

impl<A: Animate> Tracks for Vec<A> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }
    fn track(&self, index: usize) -> &dyn Animate {
        &self[index]
    }
    fn track_mut(&mut self, index: usize) -> &mut dyn Animate {
        &mut self[index]
    }
}

macro_rules! tuple_tracks {
    ($len:literal: $($name:ident $index:tt),+) => {
        impl<$($name: Animate),+> Tracks for ($($name,)+) {
            fn len(&self) -> usize {
                $len
            }
            fn track(&self, index: usize) -> &dyn Animate {
                match index {
                    $($index => &self.$index,)+
                    _ => panic!("track index {index} out of range for {} tracks", $len),
                }
            }
            fn track_mut(&mut self, index: usize) -> &mut dyn Animate {
                match index {
                    $($index => &mut self.$index,)+
                    _ => panic!("track index {index} out of range for {} tracks", $len),
                }
            }
        }
    };
}
tuple_tracks!(1: A 0);
tuple_tracks!(2: A 0, B 1);
tuple_tracks!(3: A 0, B 1, C 2);
tuple_tracks!(4: A 0, B 1, C 2, D 3);
tuple_tracks!(5: A 0, B 1, C 2, D 3, E 4);
tuple_tracks!(6: A 0, B 1, C 2, D 3, E 4, F 5);
tuple_tracks!(7: A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple_tracks!(8: A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);

/// Runs members one after another. Time left over when a member completes
/// carries into the next, so frame/tick partitioning never shifts later members.
///
/// An empty sequence completes on its first advance. A paused member holds
/// the sequence; cancelled members are skipped. An endlessly repeating member
/// never yields to later members. Control members through the sequence.
///
/// ```
/// use rayengine_core::tween::*;
/// use std::time::Duration;
/// let ms = Duration::from_millis;
/// // Flash red, then fade back to white.
/// let mut flash = Sequence::new([
///     Tween::new([255, 255, 255, 255], [255, 40, 40, 255], ms(50)),
///     Tween::new([255, 40, 40, 255], [255, 255, 255, 255], ms(200)).with_ease(Ease::QuadOut),
/// ]);
/// assert!(flash.advance(ms(100)).is_none());
/// assert_eq!(flash.current(), 1);
/// assert!(flash.advance(ms(150)).is_some());
/// assert_eq!(flash.value(), Some([255, 255, 255, 255]));
/// ```
#[derive(Clone, Debug)]
pub struct Sequence<S> {
    tracks: S,
    current: usize,
    id: TweenId,
    paused: bool,
    status: Status,
}

impl<S: Tracks> Sequence<S> {
    /// Starts at the first member, unpaused, with the default identity.
    pub fn new(tracks: S) -> Self {
        Self {
            tracks,
            current: 0,
            id: TweenId::default(),
            paused: false,
            status: Status::Active,
        }
    }
    /// Members, for reading their values.
    pub fn tracks(&self) -> &S {
        &self.tracks
    }
    /// Members, for adjusting them in place. Changes to a member the group has
    /// already finished with take effect only after resetting the group.
    pub fn tracks_mut(&mut self) -> &mut S {
        &mut self.tracks
    }
    /// Index of the running member; equals the member count once finished.
    pub fn current(&self) -> usize {
        self.current
    }

    controls!();
}

impl<S: Tracks> Sequence<S> {
    /// Value of the running (or, once finished, last) member of a sequence of
    /// same-typed tweens. `None` only for an empty sequence.
    pub fn value<T: Tweenable>(&self) -> Option<T>
    where
        S: AsRef<[Tween<T>]>,
    {
        let tweens = self.tracks.as_ref();
        tweens
            .get(self.current.min(tweens.len().saturating_sub(1)))
            .map(Tween::value)
    }
}

impl<S: Tracks> Animate for Sequence<S> {
    fn step(&mut self, dt: Duration) -> Option<Duration> {
        if self.paused || self.status != Status::Active {
            return None;
        }
        let mut left = dt;
        while self.current < self.tracks.len() {
            let track = self.tracks.track_mut(self.current);
            if !track.is_done() {
                left = track.step(left)?;
            }
            self.current += 1;
        }
        self.status = Status::Finished;
        Some(left)
    }

    fn rewind(&mut self) {
        for index in 0..self.tracks.len() {
            self.tracks.track_mut(index).rewind();
        }
        self.current = 0;
        self.status = Status::Active;
    }

    fn skip_to_end(&mut self) {
        for index in 0..self.tracks.len() {
            self.tracks.track_mut(index).skip_to_end();
        }
        self.current = self.tracks.len();
        self.status = Status::Finished;
    }

    fn is_done(&self) -> bool {
        self.status != Status::Active
    }
}

/// Runs members simultaneously and completes when every member is done.
///
/// An empty group completes on its first advance. Paused members hold the
/// group open; cancelled members count as done. Read member values through
/// [`Self::tracks`].
///
/// ```
/// use rayengine_core::{glam::Vec2, tween::*};
/// use std::time::Duration;
/// let ms = Duration::from_millis;
/// let mut pickup = Parallel::new((
///     Tween::new(Vec2::ZERO, Vec2::new(0.0, -12.0), ms(300)).with_ease(Ease::QuadOut),
///     Tween::new(1.0_f32, 0.0, ms(500)).with_delay(ms(100)),
/// ))
/// .with_id(TweenId(7));
/// assert!(pickup.advance(ms(300)).is_none());
/// assert_eq!(pickup.tracks().0.value(), Vec2::new(0.0, -12.0));
/// assert_eq!(pickup.advance(ms(300)), Some(TweenCompleted { id: TweenId(7) }));
/// ```
#[derive(Clone, Debug)]
pub struct Parallel<S> {
    tracks: S,
    id: TweenId,
    paused: bool,
    status: Status,
}

impl<S: Tracks> Parallel<S> {
    /// Starts every member together, unpaused, with the default identity.
    pub fn new(tracks: S) -> Self {
        Self {
            tracks,
            id: TweenId::default(),
            paused: false,
            status: Status::Active,
        }
    }
    /// Members, for reading their values.
    pub fn tracks(&self) -> &S {
        &self.tracks
    }
    /// Members, for adjusting them in place. Changes to a member the group has
    /// already finished with take effect only after resetting the group.
    pub fn tracks_mut(&mut self) -> &mut S {
        &mut self.tracks
    }

    controls!();
}

impl<S: Tracks> Animate for Parallel<S> {
    fn step(&mut self, dt: Duration) -> Option<Duration> {
        if self.paused || self.status != Status::Active {
            return None;
        }
        // The group ends with its longest member: the smallest leftover.
        let mut left = dt;
        let mut done = true;
        for index in 0..self.tracks.len() {
            let track = self.tracks.track_mut(index);
            if track.is_done() {
                continue;
            }
            match track.step(dt) {
                Some(rest) => left = left.min(rest),
                None => done &= track.is_done(),
            }
        }
        if !done {
            return None;
        }
        self.status = Status::Finished;
        Some(left)
    }

    fn rewind(&mut self) {
        for index in 0..self.tracks.len() {
            self.tracks.track_mut(index).rewind();
        }
        self.status = Status::Active;
    }

    fn skip_to_end(&mut self) {
        for index in 0..self.tracks.len() {
            self.tracks.track_mut(index).skip_to_end();
        }
        self.status = Status::Finished;
    }

    fn is_done(&self) -> bool {
        self.status != Status::Active
    }
}

#[cfg(test)]
mod tests;
