//! Opt-in streamed music and one-shot mixing. See [`crate::guides::audio`].
//!
//! The runtime owns all native resources and advances this mixer once per frame,
//! including while minimized. State-stack policies never implicitly stop audio.

use crate::{
    Error,
    assets::{Assets, SoundId, asset_path, path_string},
};
use rayengine_core::audio::validate_gain;
pub use rayengine_core::audio::{
    AudioBuses, AudioError, AudioSettings, BusId, BusSettings, GainFade,
};
use raylib::prelude::{Music, RaylibAudio, Sound, Wave};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

impl From<AudioError> for Error {
    fn from(error: AudioError) -> Self {
        Self::Asset(error.to_string())
    }
}

/// Cached, run-local music stream handle, separate from a short [`SoundId`].
/// Unloading permanently invalidates it; handles must not be shared between runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MusicId(usize);

/// Controls for one one-shot instance. Independent instances can overlap.
#[derive(Clone, Copy, Debug)]
pub struct SoundOptions {
    /// Destination bus; defaults to sfx.
    pub bus: BusId,
    /// Finite per-instance volume in 0..=1.
    pub volume: f32,
    /// Playback-rate multiplier in 0.25..=4.0 (also changes duration).
    pub pitch: f32,
    /// Stereo pan: -1 left, 0 center, 1 right.
    pub pan: f32,
}
impl Default for SoundOptions {
    fn default() -> Self {
        Self {
            bus: BusId::SFX,
            volume: 1.0,
            pitch: 1.0,
            pan: 0.0,
        }
    }
}
impl SoundOptions {
    fn validate(&self, buses: &AudioBuses) -> Result<(), Error> {
        validate_gain(self.volume)?;
        buses.gain(self.bus)?;
        if !self.pitch.is_finite() || !(0.25..=4.0).contains(&self.pitch) {
            return Err(Error::Asset("pitch must be finite and in 0.25..=4".into()));
        }
        if !self.pan.is_finite() || !(-1.0..=1.0).contains(&self.pan) {
            return Err(Error::Asset("pan must be finite and in -1..=1".into()));
        }
        Ok(())
    }
    fn apply(&self, sound: &Sound<'_>, buses: &AudioBuses) {
        sound.set_volume(self.volume * buses.gain(self.bus).expect("validated bus"));
        sound.set_pitch(self.pitch);
        sound.set_pan(self.pan);
    }
}

/// Stream playback controls. Each cached handle represents one playback cursor.
#[derive(Clone, Copy, Debug)]
pub struct MusicOptions {
    /// Destination bus; defaults to music.
    pub bus: BusId,
    /// Finite per-track volume in 0..=1.
    pub volume: f32,
    /// Whether the stream repeats at end of file.
    pub looping: bool,
    /// Initial fade duration when starting a stopped stream.
    pub fade_in: Duration,
}
impl Default for MusicOptions {
    fn default() -> Self {
        Self {
            bus: BusId::MUSIC,
            volume: 1.0,
            looping: true,
            fade_in: Duration::ZERO,
        }
    }
}
impl MusicOptions {
    fn validate(&self, buses: &AudioBuses) -> Result<(), Error> {
        validate_gain(self.volume)?;
        buses.gain(self.bus)?;
        Ok(())
    }
}

/// Snapshot of a live stream's playback cursor and fade (before bus gain).
#[derive(Clone, Copy, Debug)]
pub struct MusicStatus {
    /// Whether playback is active, including explicitly paused streams.
    pub active: bool,
    /// Whether explicitly paused. Paused fades continue on the frame clock.
    pub paused: bool,
    /// Current linear envelope gain in 0..=1, before track/bus volume.
    pub gain: f32,
    /// Playback cursor in seconds reported by raylib.
    pub seconds: f32,
}
struct Track<'audio> {
    stream: Music<'audio>,
    options: MusicOptions,
    fade: GainFade,
    active: bool,
    paused: bool,
    stop_after_fade: bool,
}
impl Track<'_> {
    fn sync_volume(&self, buses: &AudioBuses) {
        self.stream.set_volume(
            self.options.volume
                * self.fade.gain()
                * buses.gain(self.options.bus).expect("validated bus"),
        );
    }
    fn stop(&mut self) {
        self.stream.stop_stream();
        self.active = false;
        self.paused = false;
        self.stop_after_fade = false;
        self.fade = GainFade::new(0.0, 0.0, Duration::ZERO).expect("zero gain");
    }
    fn start(&mut self, options: MusicOptions, duration: Duration, buses: &AudioBuses) {
        let start = if self.active { self.fade.gain() } else { 0.0 };
        self.options = options;
        self.stream.set_looping(options.looping);
        self.fade = GainFade::new(start, 1.0, duration).expect("unit gains");
        self.stop_after_fade = false;
        self.sync_volume(buses);
        if !self.active {
            self.stream.play_stream();
        } else if self.paused {
            self.stream.resume_stream();
        }
        self.paused = false;
        self.active = true;
    }
}
struct Voice<'audio> {
    sound: Sound<'audio>,
    options: SoundOptions,
}
pub(crate) struct SoundPool<'audio> {
    wave: Wave<'audio>,
    voices: Vec<Voice<'audio>>,
    original: SoundOptions,
    limit: Option<usize>,
}
impl<'audio> SoundPool<'audio> {
    pub(crate) fn new(wave: Wave<'audio>) -> Self {
        Self {
            wave,
            voices: Vec::new(),
            original: SoundOptions::default(),
            limit: None,
        }
    }
}

/// Run-owned audio controls, accessible through [`Assets::audio`].
/// Bus edits apply to already playing voices on the next frame. New playback
/// uses current bus values immediately. All methods run on the owning thread.
pub struct AudioMixer<'audio> {
    buses: AudioBuses,
    tracks: Vec<Option<Track<'audio>>>,
    music_paths: HashMap<PathBuf, MusicId>,
    pub(crate) sounds: Vec<Option<SoundPool<'audio>>>,
    device: Option<&'audio RaylibAudio>,
}
impl<'audio> AudioMixer<'audio> {
    pub(crate) fn new(device: Option<&'audio RaylibAudio>) -> Self {
        Self {
            buses: AudioBuses::default(),
            tracks: Vec::new(),
            music_paths: HashMap::new(),
            sounds: Vec::new(),
            device,
        }
    }
    /// Reads named buses and persistent settings.
    pub fn buses(&self) -> &AudioBuses {
        &self.buses
    }
    /// Changes named bus volumes/mutes, transient ducking, or saved settings.
    pub fn buses_mut(&mut self) -> &mut AudioBuses {
        &mut self.buses
    }
    pub(crate) fn load_music(&mut self, path: &Path) -> Result<MusicId, Error> {
        let device = self
            .device
            .ok_or_else(|| Error::Asset("enable Config::audio before loading music".into()))?;
        let path = asset_path(path)?;
        if let Some(&id) = self.music_paths.get(&path) {
            return Ok(id);
        }
        let stream = device
            .new_music(path_string(&path)?)
            .map_err(|error| Error::Asset(format!("{}: {error}", path.display())))?;
        let id = MusicId(self.tracks.len());
        self.tracks.push(Some(Track {
            stream,
            options: MusicOptions::default(),
            fade: GainFade::new(0.0, 0.0, Duration::ZERO)?,
            active: false,
            paused: false,
            stop_after_fade: false,
        }));
        self.music_paths.insert(path, id);
        Ok(id)
    }
    fn track_mut(&mut self, id: MusicId) -> Result<&mut Track<'audio>, Error> {
        self.tracks
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("music handle is unloaded".into()))
    }
    /// Starts/fades in a stream. An active handle retains its cursor and current
    /// gain, retargeting toward full gain; an explicitly paused handle resumes.
    pub fn play_music(&mut self, id: MusicId, options: MusicOptions) -> Result<(), Error> {
        options.validate(&self.buses)?;
        let buses = &self.buses;
        let track = self
            .tracks
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("music handle is unloaded".into()))?;
        track.start(options, options.fade_in, buses);
        Ok(())
    }
    /// Fades all other active streams out and this stream in over `duration`.
    /// Interrupted fades begin at current gains; zero duration switches immediately.
    /// Validates the target/options before changing any outgoing stream.
    /// `options.fade_in` is replaced by `duration` for this operation.
    pub fn crossfade(
        &mut self,
        id: MusicId,
        options: MusicOptions,
        duration: Duration,
    ) -> Result<(), Error> {
        options.validate(&self.buses)?;
        self.track_mut(id)?;
        for (index, track) in self.tracks.iter_mut().enumerate() {
            let Some(track) = track else {
                continue;
            };
            if index == id.0 {
                track.start(options, duration, &self.buses);
            } else if track.active {
                track.fade.retarget(0.0, duration)?;
                track.stop_after_fade = true;
                if duration.is_zero() {
                    track.stop();
                }
            }
        }
        Ok(())
    }
    /// Fades a stream to silence and stops/rewinds it at completion.
    pub fn fade_out(&mut self, id: MusicId, duration: Duration) -> Result<(), Error> {
        let track = self.track_mut(id)?;
        if track.active {
            track.fade.retarget(0.0, duration)?;
            track.stop_after_fade = true;
            if duration.is_zero() {
                track.stop();
            }
        }
        Ok(())
    }
    /// Immediately stops and rewinds a stream.
    pub fn stop_music(&mut self, id: MusicId) -> Result<(), Error> {
        self.track_mut(id)?.stop();
        Ok(())
    }
    /// Pauses the playback cursor. Frame-clock fades continue while paused.
    pub fn pause_music(&mut self, id: MusicId) -> Result<(), Error> {
        let track = self.track_mut(id)?;
        if track.active && !track.paused {
            track.stream.pause_stream();
            track.paused = true;
        }
        Ok(())
    }
    /// Resumes an explicitly paused stream at its current fade gain.
    pub fn resume_music(&mut self, id: MusicId) -> Result<(), Error> {
        let track = self.track_mut(id)?;
        if track.active && track.paused {
            track.stream.resume_stream();
            track.paused = false;
        }
        Ok(())
    }
    /// Returns None for an unloaded handle.
    pub fn music_status(&self, id: MusicId) -> Option<MusicStatus> {
        let track = self.tracks.get(id.0)?.as_ref()?;
        Some(MusicStatus {
            active: track.active,
            paused: track.paused,
            gain: track.fade.gain(),
            seconds: track.stream.get_time_played(),
        })
    }
    /// Stops and releases the stream immediately; future reloads get a new ID.
    pub fn unload_music(&mut self, id: MusicId) -> bool {
        let removed = self.tracks.get_mut(id.0).and_then(Option::take).is_some();
        self.music_paths.retain(|_, handle| *handle != id);
        removed
    }
    /// Sets a per-sound concurrency cap. None permits unlimited instances.
    /// Zero rejects all new playback; reducing a cap does not stop existing voices.
    /// Completed native voice buffers are reused until the sound unloads.
    pub fn set_sound_limit(&mut self, id: SoundId, limit: Option<usize>) -> Result<(), Error> {
        self.sounds
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("sound handle is unloaded".into()))?
            .limit = limit;
        Ok(())
    }
    pub(crate) fn play_sound(
        &mut self,
        id: SoundId,
        source: &Sound<'audio>,
        options: SoundOptions,
        restart: bool,
    ) -> Result<bool, Error> {
        options.validate(&self.buses)?;
        let pool = self
            .sounds
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("sound handle is unloaded".into()))?;
        let playing = usize::from(source.is_playing())
            + pool.voices.iter().filter(|v| v.sound.is_playing()).count();
        if pool.limit.is_some_and(|limit| playing >= limit)
            && !(restart && source.is_playing() && pool.limit != Some(0))
        {
            return Ok(false);
        }
        if restart || !source.is_playing() {
            options.apply(source, &self.buses);
            pool.original = options;
            source.play();
        } else {
            let slot = pool.voices.iter().position(|v| !v.sound.is_playing());
            let voice = if let Some(slot) = slot {
                &mut pool.voices[slot]
            } else {
                let sound = self
                    .device
                    .expect("loaded sound requires audio")
                    .new_sound_from_wave(&pool.wave)
                    .map_err(|error| Error::Asset(error.to_string()))?;
                pool.voices.push(Voice { sound, options });
                pool.voices.last_mut().expect("pushed voice")
            };
            voice.options = options;
            options.apply(&voice.sound, &self.buses);
            voice.sound.play();
        }
        Ok(true)
    }
    pub(crate) fn update(&mut self, delta: Duration, sources: &[Option<Sound<'audio>>]) {
        self.buses.advance(delta);
        for track in self.tracks.iter_mut().flatten() {
            if !track.active {
                continue;
            }
            track.fade.advance(delta);
            if track.stop_after_fade && track.fade.finished() {
                track.stop();
                continue;
            }
            track.sync_volume(&self.buses);
            if !track.paused {
                track.stream.update_stream();
                if !track.stream.is_stream_playing() {
                    track.stop();
                }
            }
        }
        for (source, pool) in sources.iter().zip(&self.sounds) {
            if let (Some(source), Some(pool)) = (source, pool) {
                source.set_volume(
                    pool.original.volume
                        * self.buses.gain(pool.original.bus).expect("validated bus"),
                );
                for voice in &pool.voices {
                    voice.sound.set_volume(
                        voice.options.volume
                            * self.buses.gain(voice.options.bus).expect("validated bus"),
                    );
                }
            }
        }
    }
    pub(crate) fn resource_counts(&self) -> (u64, u64) {
        (
            self.tracks.iter().flatten().count() as u64,
            self.sounds
                .iter()
                .flatten()
                .map(|pool| pool.voices.len() as u64)
                .sum(),
        )
    }
}

impl<'audio> Assets<'audio> {
    /// Audio controls available during init, fixed updates, and draw callbacks.
    /// Keep this guard short-lived: calling another audio asset operation while
    /// holding it would borrow the same mixer twice.
    pub fn audio(&self) -> std::cell::RefMut<'_, AudioMixer<'audio>> {
        self.mixer.borrow_mut()
    }
    /// Plays an independent one-shot with volume/pitch/pan and bus routing.
    /// Returns Ok(false) for stale handles or when the configured limit is reached.
    /// The first overlapping play allocates a native buffer from cached PCM;
    /// later plays reuse completed buffers. No files are read during playback.
    pub fn play_sound(&self, id: SoundId, options: SoundOptions) -> Result<bool, Error> {
        let Some(source) = self.sound(id) else {
            return Ok(false);
        };
        self.mixer
            .borrow_mut()
            .play_sound(id, source, options, false)
    }
    /// Unloads a streamed track. It remains invalid for the rest of this run.
    pub fn unload_music(&mut self, id: MusicId) -> bool {
        self.mixer.get_mut().unload_music(id)
    }
    pub(crate) fn load_music(&mut self, path: &Path) -> Result<MusicId, Error> {
        self.mixer.get_mut().load_music(path)
    }
    pub(crate) fn update_audio(&mut self, delta: Duration) {
        self.mixer.get_mut().update(delta, &self.sounds);
    }
}

#[cfg(test)]
mod tests;
