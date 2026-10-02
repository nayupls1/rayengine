//! Validated sprite regions and CPU-only animation with explicit simulation time.

use glam::Vec2;
use std::{fmt, sync::Arc, time::Duration};

/// Invalid sprite geometry or animation definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteError {
    /// Region dimensions are zero or their endpoints overflow `u32`.
    InvalidRegion,
    /// Clip name is empty or contains only whitespace.
    EmptyName,
    /// Clip has no frames.
    EmptyClip,
    /// Frame duration is zero.
    ZeroDuration,
    /// Sum of frame durations exceeds [`Duration::MAX`].
    DurationOverflow,
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRegion => "sprite region must have nonzero dimensions and u32 endpoints",
            Self::EmptyName => "animation clip must have a nonblank name",
            Self::EmptyClip => "animation clip must contain at least one frame",
            Self::ZeroDuration => "animation frame duration must be nonzero",
            Self::DurationOverflow => "animation clip duration exceeds Duration::MAX",
        })
    }
}
impl std::error::Error for SpriteError {}

/// A positive, axis-aligned source rectangle in top-left texture pixel coordinates.
/// Bounds against an actual texture are checked at drawing time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl SpriteRegion {
    /// Rejects zero dimensions and overflowing endpoints. Flips belong to
    /// [`SpriteTransform`], so source dimensions are always positive.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, SpriteError> {
        if width == 0
            || height == 0
            || x.checked_add(width).is_none()
            || y.checked_add(height).is_none()
        {
            return Err(SpriteError::InvalidRegion);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
    /// Left edge in texture pixels.
    pub fn x(self) -> u32 {
        self.x
    }
    /// Top edge in texture pixels.
    pub fn y(self) -> u32 {
        self.y
    }
    /// Width in texture pixels.
    pub fn width(self) -> u32 {
        self.width
    }
    /// Height in texture pixels.
    pub fn height(self) -> u32 {
        self.height
    }
    /// Whether the entire region is inside these texture dimensions.
    pub fn fits(self, width: u32, height: u32) -> bool {
        self.x + self.width <= width && self.y + self.height <= height
    }
}

/// Sprite destination geometry in world units, with positive size and explicit pivot.
/// Flips change texture sampling; they do not move the pivot or geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteTransform {
    /// World position at which the local origin is placed.
    pub position: Vec2,
    /// Destination width and height in world units, both strictly positive.
    pub size: Vec2,
    /// Pivot offset from the unrotated destination's top-left, in world units.
    /// It may be outside the destination. Use `size * 0.5` for a centered pivot.
    pub origin: Vec2,
    /// Clockwise rotation in radians, matching [`crate::camera::Camera2D`].
    pub rotation: f32,
    /// Reverse horizontal source sampling.
    pub flip_x: bool,
    /// Reverse vertical source sampling.
    pub flip_y: bool,
}

impl Default for SpriteTransform {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            size: Vec2::ONE,
            origin: Vec2::ZERO,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
        }
    }
}

impl SpriteTransform {
    /// Rejects nonfinite coordinates, nonpositive size, and rotations whose
    /// conversion to the backend's degrees would overflow.
    pub fn is_valid(self) -> bool {
        self.position.is_finite()
            && self.size.is_finite()
            && self.size.min_element() > 0.0
            && self.origin.is_finite()
            && self.rotation.is_finite()
            && self.rotation.to_degrees().is_finite()
    }
}

/// One selected sprite-sheet region and its simulation duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteFrame {
    /// Region selected while this frame is current.
    pub region: SpriteRegion,
    /// Time spent on this frame; must be nonzero when constructing a clip.
    pub duration: Duration,
}

/// Behavior at the end of a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    /// Wrap to the first frame; never emit a completion event.
    Loop,
    /// Hold the last frame and emit one completion event per play/reset.
    Once,
}

/// Immutable named clip. Share with [`Arc`] between independent players.
#[derive(Debug)]
pub struct AnimationClip {
    name: String,
    frames: Vec<SpriteFrame>,
    ends: Vec<Duration>,
    duration: Duration,
    mode: PlaybackMode,
}

impl AnimationClip {
    /// Validates a nonblank name, at least one frame, positive durations and
    /// a representable total duration. Texture bounds are checked when drawn.
    pub fn new(
        name: impl Into<String>,
        frames: Vec<SpriteFrame>,
        mode: PlaybackMode,
    ) -> Result<Self, SpriteError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(SpriteError::EmptyName);
        }
        if frames.is_empty() {
            return Err(SpriteError::EmptyClip);
        }
        let mut duration = Duration::ZERO;
        let mut ends = Vec::with_capacity(frames.len());
        for frame in &frames {
            if frame.duration.is_zero() {
                return Err(SpriteError::ZeroDuration);
            }
            duration = duration
                .checked_add(frame.duration)
                .ok_or(SpriteError::DurationOverflow)?;
            ends.push(duration);
        }
        Ok(Self {
            name,
            frames,
            ends,
            duration,
            mode,
        })
    }
    /// Game-defined clip name, preserved verbatim.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Ordered frames and their individual durations.
    pub fn frames(&self) -> &[SpriteFrame] {
        &self.frames
    }
    /// Total time for one traversal.
    pub fn duration(&self) -> Duration {
        self.duration
    }
    /// Behavior at the clip's end.
    pub fn mode(&self) -> PlaybackMode {
        self.mode
    }
}

/// A one-shot completion returned exactly once by [`AnimationPlayer::advance`].
/// Can be sent to [`crate::events::Events`]; retains the completed clip even
/// if the player immediately switches to another clip.
#[derive(Clone, Debug)]
pub struct AnimationCompleted {
    /// Immutable clip that reached its end.
    pub clip: Arc<AnimationClip>,
}

/// CPU playback cursor. Drawing/reading a frame never changes playback.
/// Advancement uses integer nanoseconds and binary frame lookup, so large
/// time steps do not iterate over every crossed frame or loop.
#[derive(Clone, Debug)]
pub struct AnimationPlayer {
    clip: Arc<AnimationClip>,
    elapsed: Duration,
    paused: bool,
    finished: bool,
}

impl AnimationPlayer {
    /// Starts at the first frame, unpaused. Accepts an owned clip or shared `Arc`.
    pub fn new(clip: impl Into<Arc<AnimationClip>>) -> Self {
        Self {
            clip: clip.into(),
            elapsed: Duration::ZERO,
            paused: false,
            finished: false,
        }
    }
    /// Switches/restarts a clip, clearing pause and completion state.
    pub fn play(&mut self, clip: impl Into<Arc<AnimationClip>>) {
        *self = Self::new(clip);
    }
    /// Rewinds this clip and rearms its completion event, preserving pause state.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.finished = false;
    }
    /// Stops advancement without changing the current frame.
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
    /// Current immutable clip.
    pub fn clip(&self) -> &Arc<AnimationClip> {
        &self.clip
    }
    /// Time within this traversal, clamped to duration for a finished one-shot.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
    /// Current frame index. Exact boundaries select the next frame; a completed
    /// one-shot holds the last frame for drawing.
    pub fn frame_index(&self) -> usize {
        self.clip
            .ends
            .partition_point(|end| *end <= self.elapsed)
            .min(self.clip.frames.len() - 1)
    }
    /// Current frame, without advancing simulation.
    pub fn frame(&self) -> &SpriteFrame {
        &self.clip.frames[self.frame_index()]
    }
    /// Advances by explicitly supplied simulation time. Zero time, paused or
    /// completed playback does nothing. Looping wraps including exact boundaries;
    /// a one-shot discards excess time and returns one completion on its first end.
    /// No allocation occurs, including at completion (the clip's `Arc` is cloned).
    pub fn advance(&mut self, dt: Duration) -> Option<AnimationCompleted> {
        if self.paused || self.finished || dt.is_zero() {
            return None;
        }
        // Even two Duration::MAX values fit in u128 nanoseconds.
        let next = self.elapsed.as_nanos() + dt.as_nanos();
        let duration = self.clip.duration.as_nanos();
        let next = match self.clip.mode {
            PlaybackMode::Loop => next % duration,
            PlaybackMode::Once if next >= duration => {
                self.elapsed = self.clip.duration;
                self.finished = true;
                return Some(AnimationCompleted {
                    clip: Arc::clone(&self.clip),
                });
            }
            PlaybackMode::Once => next,
        };
        self.elapsed = Duration::new((next / 1_000_000_000) as u64, (next % 1_000_000_000) as u32);
        None
    }
}

#[cfg(test)]
mod tests;
