//! CPU playback timing for imported skeletal animation clips.
//!
//! Imported clips are sequences of evenly sampled keyframes. [`KeyframeRate`]
//! states how many keyframes are sampled per period of simulation time, and a
//! [`KeyframePlayer`] maps explicitly supplied time to a fractional keyframe.
//! The native runtime owns the pose data; this module only decides which pose
//! to sample, so timing, looping and completion can be tested without a GPU.

pub use crate::sprite::PlaybackMode;
use std::{fmt, time::Duration};

/// Invalid skeletal clip timing or model/clip pairing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkeletalError {
    /// A rate must sample at least one keyframe per nonzero period.
    InvalidRate,
    /// A clip must contain at least one keyframe.
    NoKeyframes,
    /// The clip's duration exceeds [`Duration::MAX`].
    DurationOverflow,
    /// The model has no skeleton, so clips cannot pose it.
    NoSkeleton,
    /// The clip was sampled for a skeleton with a different bone count.
    BoneCountMismatch {
        /// Bones in the model's skeleton.
        model: u32,
        /// Bones in each keyframe of the clip.
        clip: u32,
    },
}

impl fmt::Display for SkeletalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRate => f.write_str("keyframe rate needs a nonzero count and period"),
            Self::NoKeyframes => f.write_str("skeletal clip has no keyframes"),
            Self::DurationOverflow => f.write_str("skeletal clip duration exceeds Duration::MAX"),
            Self::NoSkeleton => f.write_str("model has no skeleton to animate"),
            Self::BoneCountMismatch { model, clip } => write!(
                f,
                "skeletal clip has {clip} bones but the model skeleton has {model}"
            ),
        }
    }
}
impl std::error::Error for SkeletalError {}

/// Bone counts are the only skeleton facts every supported format records for
/// both models and clips. Clips must also be authored against the same joint
/// order; that cannot be verified from the native data.
pub fn check_skeleton(model_bones: u32, clip_bones: u32) -> Result<(), SkeletalError> {
    if model_bones == 0 {
        return Err(SkeletalError::NoSkeleton);
    }
    if model_bones != clip_bones {
        return Err(SkeletalError::BoneCountMismatch {
            model: model_bones,
            clip: clip_bones,
        });
    }
    Ok(())
}

/// Exact keyframe sampling rate: `frames` keyframes per `period`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyframeRate {
    frames: u32,
    period: Duration,
}

impl KeyframeRate {
    /// glTF/GLB clips: the native loader resamples every channel at 60 keyframes per second.
    pub const GLTF: Self = Self::per_second(60);
    /// M3D clips: the native loader stores one keyframe per 17 milliseconds.
    pub const M3D: Self = Self {
        frames: 1,
        period: Duration::from_millis(17),
    };

    /// `frames` keyframes per second.
    ///
    /// # Panics
    ///
    /// Panics if `frames` is zero; use [`Self::new`] for unchecked input.
    pub const fn per_second(frames: u32) -> Self {
        assert!(frames > 0, "keyframe rate needs a nonzero count");
        Self {
            frames,
            period: Duration::from_secs(1),
        }
    }
    /// Validates a nonzero keyframe count and period, e.g. IQM's authored rate.
    pub fn new(frames: u32, period: Duration) -> Result<Self, SkeletalError> {
        if frames == 0 || period.is_zero() {
            return Err(SkeletalError::InvalidRate);
        }
        Ok(Self { frames, period })
    }
    /// Keyframes sampled per period.
    pub fn frames(self) -> u32 {
        self.frames
    }
    /// Period containing [`Self::frames`] keyframes.
    pub fn period(self) -> Duration {
        self.period
    }
}

/// Keyframe count and rate of one clip. Its duration spans the first to the
/// last keyframe, so a looping clip should end on a copy of its first pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipTiming {
    keyframes: u32,
    rate: KeyframeRate,
}

impl ClipTiming {
    /// Rejects clips without keyframes or with an unrepresentable duration.
    /// A single keyframe is a static pose of zero duration.
    pub fn new(keyframes: u32, rate: KeyframeRate) -> Result<Self, SkeletalError> {
        if keyframes == 0 {
            return Err(SkeletalError::NoKeyframes);
        }
        let timing = Self { keyframes, rate };
        if timing.span().div_ceil(u128::from(rate.frames)) > Duration::MAX.as_nanos() {
            return Err(SkeletalError::DurationOverflow);
        }
        Ok(timing)
    }
    /// Number of sampled poses.
    pub fn keyframes(self) -> u32 {
        self.keyframes
    }
    /// Sampling rate.
    pub fn rate(self) -> KeyframeRate {
        self.rate
    }
    /// Time from the first to the last keyframe, rounded up to whole nanoseconds
    /// so advancing a one-shot by this duration always completes it.
    pub fn duration(self) -> Duration {
        nanos_duration(self.span().div_ceil(u128::from(self.rate.frames)))
    }
    /// Clip length in scaled units (period nanoseconds per keyframe).
    fn span(self) -> u128 {
        u128::from(self.keyframes - 1) * self.rate.period.as_nanos()
    }
}

/// A one-shot completion returned exactly once by [`KeyframePlayer::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipCompleted<C> {
    /// Clip handle that reached its end.
    pub clip: C,
}

/// CPU playback cursor for one character instance and one clip handle `C`.
///
/// Advancement uses exact integer arithmetic, so large time steps wrap loops in
/// constant time without drift. Reading the current keyframe never advances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyframePlayer<C> {
    clip: C,
    timing: ClipTiming,
    mode: PlaybackMode,
    /// Elapsed nanoseconds multiplied by the rate's frame count. Always within
    /// the clip span, which is below `u32::MAX` times `Duration::MAX` nanoseconds.
    position: u128,
    paused: bool,
    finished: bool,
}

impl<C: Copy> KeyframePlayer<C> {
    /// Starts at the first keyframe, unpaused.
    pub fn new(clip: C, timing: ClipTiming, mode: PlaybackMode) -> Self {
        Self {
            clip,
            timing,
            mode,
            position: 0,
            paused: false,
            finished: false,
        }
    }
    /// Switches/restarts a clip, clearing pause and completion state.
    pub fn play(&mut self, clip: C, timing: ClipTiming, mode: PlaybackMode) {
        *self = Self::new(clip, timing, mode);
    }
    /// Rewinds this clip and rearms its completion event, preserving pause state.
    pub fn reset(&mut self) {
        self.position = 0;
        self.finished = false;
    }
    /// Stops advancement without changing the current pose.
    pub fn pause(&mut self) {
        self.paused = true;
    }
    /// Allows advancement again. A completed one-shot remains completed until reset/play.
    pub fn resume(&mut self) {
        self.paused = false;
    }
    /// Whether explicit time advancement is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    /// Whether a one-shot has reached its end.
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    /// Current clip handle.
    pub fn clip(&self) -> C {
        self.clip
    }
    /// Current clip timing.
    pub fn timing(&self) -> ClipTiming {
        self.timing
    }
    /// Behavior at the clip's end.
    pub fn mode(&self) -> PlaybackMode {
        self.mode
    }
    /// Time within this traversal, rounded down to whole nanoseconds. A finished
    /// one-shot reports its exact duration.
    pub fn elapsed(&self) -> Duration {
        if self.finished {
            return self.timing.duration();
        }
        nanos_duration(self.position / u128::from(self.timing.rate.frames))
    }
    /// Fractional keyframe in `0.0..=keyframes - 1`. Loops interpolate toward
    /// the next keyframe and wrap to zero at the last; a completed one-shot
    /// holds exactly the last keyframe.
    pub fn keyframe(&self) -> f32 {
        let period = self.timing.rate.period.as_nanos();
        let whole = self.position / period;
        let fraction = (self.position % period) as f64 / period as f64;
        // Rounding can only reach the next whole keyframe, never beyond the last.
        ((whole as f64 + fraction) as f32).min((self.timing.keyframes - 1) as f32)
    }
    /// Advances by explicitly supplied simulation time. Zero time, paused or
    /// completed playback does nothing. Looping wraps including exact boundaries;
    /// a one-shot discards excess time and returns one completion on its first end.
    pub fn advance(&mut self, dt: Duration) -> Option<ClipCompleted<C>> {
        if self.paused || self.finished || dt.is_zero() {
            return None;
        }
        // dt ≤ Duration::MAX (< 2^94 ns) times frames (< 2^32) cannot overflow u128.
        let next = self.position + dt.as_nanos() * u128::from(self.timing.rate.frames);
        let span = self.timing.span();
        match self.mode {
            PlaybackMode::Once if next >= span => {
                self.position = span;
                self.finished = true;
                Some(ClipCompleted { clip: self.clip })
            }
            PlaybackMode::Loop if span == 0 => None,
            PlaybackMode::Loop => {
                self.position = next % span;
                None
            }
            PlaybackMode::Once => {
                self.position = next;
                None
            }
        }
    }
}

fn nanos_duration(nanos: u128) -> Duration {
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

#[cfg(test)]
mod tests;
