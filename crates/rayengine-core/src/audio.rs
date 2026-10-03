//! Display-independent audio buses, settings, and frame-clock gain envelopes.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, time::Duration};

/// A bus in one mixer. Handles must not be shared between mixers or runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BusId(usize);
impl BusId {
    /// Parent gain applied once to every bus.
    pub const MASTER: Self = Self(0);
    /// Default bus for streamed music.
    pub const MUSIC: Self = Self(1);
    /// Default bus for short sound effects.
    pub const SFX: Self = Self(2);
}

/// Invalid audio controls. Invalid operations preserve the previous state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioError(pub &'static str);
impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for AudioError {}

/// Validates a finite gain in the inclusive range 0..=1.
pub fn validate_gain(value: f32) -> Result<(), AudioError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(AudioError("volume must be finite and in 0..=1"))
    }
}

/// Linear amplitude envelope with saturating, integer-duration frame timing.
#[derive(Clone, Debug)]
pub struct GainFade {
    start: f32,
    target: f32,
    elapsed: Duration,
    duration: Duration,
}
impl GainFade {
    /// Creates a fade; zero duration immediately yields the target gain.
    pub fn new(start: f32, target: f32, duration: Duration) -> Result<Self, AudioError> {
        validate_gain(start)?;
        validate_gain(target)?;
        Ok(Self {
            start,
            target,
            elapsed: Duration::ZERO,
            duration,
        })
    }
    /// Current amplitude gain.
    pub fn gain(&self) -> f32 {
        if self.finished() {
            return self.target;
        }
        let fraction = self.elapsed.as_secs_f64() / self.duration.as_secs_f64();
        (f64::from(self.start) + f64::from(self.target - self.start) * fraction) as f32
    }
    /// Advances by wall time without simulation catch-up limits.
    pub fn advance(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta).min(self.duration);
    }
    /// Whether the target has been reached.
    pub fn finished(&self) -> bool {
        self.elapsed >= self.duration
    }
    /// Changes the target without a jump from the current gain.
    pub fn retarget(&mut self, target: f32, duration: Duration) -> Result<(), AudioError> {
        *self = Self::new(self.gain(), target, duration)?;
        Ok(())
    }
}

/// User-controlled bus values suitable for a game-owned versioned save payload.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusSettings {
    /// User volume in 0..=1; retained while muted or ducked.
    pub volume: f32,
    /// Silence this bus without changing its volume.
    pub muted: bool,
}
impl Default for BusSettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}
/// Bus settings keyed by stable names, independent of run-local handles.
/// The game owns the save schema; this type performs no filesystem I/O.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Persistent volume/mute values. Transient ducking is intentionally omitted.
    pub buses: BTreeMap<String, BusSettings>,
}
struct Bus {
    name: String,
    settings: BusSettings,
    duck: GainFade,
}
/// Flat named buses under master; music/sfx are available by default.
/// Muting affects gain only; envelopes and stream clocks continue advancing.
pub struct AudioBuses {
    buses: Vec<Bus>,
}
impl Default for AudioBuses {
    fn default() -> Self {
        let mut buses = Self { buses: Vec::new() };
        for name in ["master", "music", "sfx"] {
            buses.add(name).expect("default bus");
        }
        buses
    }
}
impl AudioBuses {
    /// Resolves an existing name.
    pub fn find(&self, name: &str) -> Option<BusId> {
        self.buses
            .iter()
            .position(|bus| bus.name == name)
            .map(BusId)
    }
    /// Adds a game-defined bus, or returns the existing handle for that name.
    pub fn add(&mut self, name: &str) -> Result<BusId, AudioError> {
        if name.trim().is_empty() {
            return Err(AudioError("bus name must not be empty"));
        }
        if let Some(id) = self.find(name) {
            return Ok(id);
        }
        let id = BusId(self.buses.len());
        self.buses.push(Bus {
            name: name.into(),
            settings: BusSettings::default(),
            duck: GainFade::new(1.0, 1.0, Duration::ZERO)?,
        });
        Ok(id)
    }
    fn bus(&self, id: BusId) -> Result<&Bus, AudioError> {
        self.buses
            .get(id.0)
            .ok_or(AudioError("bus does not belong to this mixer"))
    }
    fn bus_mut(&mut self, id: BusId) -> Result<&mut Bus, AudioError> {
        self.buses
            .get_mut(id.0)
            .ok_or(AudioError("bus does not belong to this mixer"))
    }
    /// Reads persistent controls, independent of ducking and parent gain.
    pub fn settings_for(&self, id: BusId) -> Result<BusSettings, AudioError> {
        Ok(self.bus(id)?.settings)
    }
    /// Sets a user volume; rejects nonfinite/out-of-range values.
    pub fn set_volume(&mut self, id: BusId, volume: f32) -> Result<(), AudioError> {
        validate_gain(volume)?;
        self.bus_mut(id)?.settings.volume = volume;
        Ok(())
    }
    /// Mutes a bus, preserving the stored volume.
    pub fn set_muted(&mut self, id: BusId, muted: bool) -> Result<(), AudioError> {
        self.bus_mut(id)?.settings.muted = muted;
        Ok(())
    }
    /// Fades a transient multiplier, e.g. 0.25 while a pause menu covers play.
    /// This does not alter the persisted user volume.
    pub fn duck(&mut self, id: BusId, gain: f32, duration: Duration) -> Result<(), AudioError> {
        self.bus_mut(id)?.duck.retarget(gain, duration)
    }
    /// Effective bus gain, including master exactly once.
    pub fn gain(&self, id: BusId) -> Result<f32, AudioError> {
        let own = |bus: &Bus| {
            if bus.settings.muted {
                0.0
            } else {
                bus.settings.volume * bus.duck.gain()
            }
        };
        let gain = own(self.bus(id)?);
        Ok(if id == BusId::MASTER {
            gain
        } else {
            gain * own(&self.buses[0])
        })
    }
    /// Advances every duck envelope by one frame's wall time.
    pub fn advance(&mut self, delta: Duration) {
        for bus in &mut self.buses {
            bus.duck.advance(delta);
        }
    }
    /// Captures user values for serialization into the game's save container.
    pub fn settings(&self) -> AudioSettings {
        AudioSettings {
            buses: self
                .buses
                .iter()
                .map(|bus| (bus.name.clone(), bus.settings))
                .collect(),
        }
    }
    /// Atomically applies named settings, creating extra buses when needed.
    /// Unlisted buses and all transient duck envelopes retain their values.
    pub fn apply_settings(&mut self, settings: &AudioSettings) -> Result<(), AudioError> {
        for (name, values) in &settings.buses {
            if name.trim().is_empty() {
                return Err(AudioError("bus name must not be empty"));
            }
            validate_gain(values.volume)?;
        }
        for (name, values) in &settings.buses {
            let id = self.add(name)?;
            self.bus_mut(id)?.settings = *values;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
