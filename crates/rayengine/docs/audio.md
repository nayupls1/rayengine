# Streamed music and audio buses

Set `Config::audio = true` to request an audio device. Audio stays disabled by
default; games without it perform no native audio initialization. Loading sounds
or music without opting in returns an error. Device initialization failures are
reported by `App::run`.

Load short effects with `InitContext::sound` and streamed tracks with
`InitContext::music`. Both cache by canonical path and return separate typed
handles. Music supports formats enabled in the bundled raylib, including WAV and
OGG. A track streams decoded chunks instead of loading the entire PCM into RAM.
One cached `MusicId` has one playback cursor. Loading the same path does not
create an independent playback cursor.

```no_run
use rayengine::prelude::*;
use std::time::Duration;
fn start(ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
    let theme = ctx.music("assets/theme.ogg")?;
    let next = ctx.music("assets/next.ogg")?;
    let click = ctx.sound("assets/click.wav")?;
    ctx.assets.audio().play_music(theme, MusicOptions {
        fade_in: Duration::from_secs(1), ..MusicOptions::default()
    })?;
    // On a scene change; options.fade_in is replaced by the crossfade duration.
    ctx.assets.audio().crossfade(next, MusicOptions::default(), Duration::from_secs(2))?;
    ctx.assets.audio().set_sound_limit(click, Some(4))?;
    ctx.assets.play_sound(click, SoundOptions {
        volume: 0.7, pitch: 1.2, pan: -0.5, ..SoundOptions::default()
    })?;
    Ok(())
}
```

`Assets::audio()` returns a short-lived mutable mixer guard. Drop it before
calling other audio operations on assets; those borrow the same mixer. Music
operations return errors for unloaded handles. `play_sound` returns `Ok(false)`
for an unloaded sound or a reached cap. Invalid live controls return errors
without changing playback. Unloading releases streams/voices and permanently
invalidates their handles. A later load receives a new handle.

`master`, `music`, and `sfx` buses exist by default. Use `buses_mut().add("dialogue")`
to create extra flat buses under master. Track/instance volume is multiplied by
its bus gain and master gain exactly once. All volume and duck gains must be
finite and in `0..=1`. Mute retains the user's volume and leaves playback clocks
running. Bus changes affect active audio on the next runtime frame; new playback
uses the latest bus controls immediately.

`SoundOptions` controls each overlapping instance independently. Pitch is a
finite playback-rate multiplier in `0.25..=4.0` and changes duration too. Pan is
`-1` left, `0` center, `1` right. The original sound voice and overlapping voices
all count toward `set_sound_limit`; `None` allows unlimited instances and `Some(0)`
rejects new plays. Lowering a cap lets existing instances finish. Mixer-managed native
voice buffers are created lazily from cached PCM and reused until sound unload;
no files are read during playback. `Assets::play` retains its restart-the-original
voice behavior and preserves native volume/pitch/pan set through `Assets::sound`.
That original legacy voice bypasses bus mixing; use `play_sound` to adopt mixer
controls. Direct `Assets::sound(...).play()` also bypasses concurrency admission.
Raw and legacy playback never have their controls overwritten by frame updates,
even when managed instances of the same sound are playing. Their active original
voice still counts toward caps when admitting new legacy/managed playback.

The runtime updates streamed buffers and envelopes once per main-loop iteration,
using unbounded wall time independently of `FixedClock` catch-up and simulation
ticks. Audio-enabled games keep native event polling nonblocking while minimized, so
stream buffers and fades continue updating (simulation and drawing pause).
Minimized polling is throttled to avoid a busy loop.
Games should not advance the mixer again in fixed updates or draw callbacks.
Fades interpolate amplitude linearly. Zero duration switches immediately and
interrupted fades start from the current envelope gain. `crossfade` fades all
other active tracks to zero and stops/rewinds them at completion. An active target
keeps its cursor; a stopped target starts from the beginning. Changing the looping
mode through `play_music` or `crossfade` restarts the target's cursor while retaining
its current envelope gain. `fade_out` also
stops/rewinds at completion. Explicit `pause_music` freezes the cursor; envelopes
continue, so an outgoing paused stream still stops when its fade completes.
`resume_music` resumes at the current envelope gain. Nonlooping tracks become
inactive at their natural end.

State-stack policies do not implicitly pause, stop, or unload shared audio. A
pause overlay can duck music in `enter` and restore its transient gain in `exit`:

```no_run
use rayengine::prelude::*;
use std::time::Duration;
fn duck(ctx: &mut InitContext<'_, '_>, paused: bool) -> Result<(), Error> {
    ctx.assets.audio().buses_mut().duck(BusId::MUSIC,
        if paused { 0.25 } else { 1.0 }, Duration::from_millis(250))?;
    Ok(())
}
```

Ducking does not change the user's stored bus volume, even when a settings menu
changes that volume while paused. A game owns policy for nested overlays and
restores the appropriate duck target on exit. Shared tracks must stay run-owned
when crossfading across scene replacement, because outgoing state resources
unload immediately. Use `StateResources::own_music` only for exclusive streams
that should stop as soon as their state exits. Native music, effects, pooled
voices, and cached waveforms all release before the audio device closes, also on
initialization failure or normal/error shutdown.

`AudioSettings` captures named bus volume/mute values using serde. Embed it in
your game-defined payload in the existing versioned, checksummed save container:

```no_run
use rayengine::{prelude::*, save::{self, SaveOptions, SaveLimits}};
fn persist(ctx: &mut InitContext<'_, '_>) -> Result<(), Box<dyn std::error::Error>> {
    let settings = ctx.assets.audio().buses().settings();
    save::save_with("settings.raysave", 1, &settings, SaveOptions::default(), serde_json::to_vec)?;
    let loaded = save::load("settings.raysave", SaveLimits::default())?;
    loaded.require_schema(1)?;
    let settings: AudioSettings = serde_json::from_slice(&loaded.payload)?;
    ctx.assets.audio().buses_mut().apply_settings(&settings)?;
    Ok(())
}
```

The game chooses schema versions and migration/recovery policy. Applying settings
validates the entire snapshot before edits, creates extra named buses, retains
unlisted buses, and leaves transient duck envelopes untouched. Bus handles are
run-local; save names rather than handles. Ducking, current tracks, and fade
progress are not persisted.

Run `cargo run -p rayengine --example audio`. It generates original WAV loops and
an effect in a temporary directory, deleted after the run. Enter crossfades
scenes, Space plays a panned/pitched effect, and P opens a modal pause/settings
state. The menu's Left/Right and M controls adjust music volume/mute while the
track and fades keep running. S explicitly saves `audio-settings.raysave` in the
working directory; a subsequent run loads it. Corrupt/future saves are preserved
and reported as errors. No external assets are downloaded. An audio output
device and desktop display are required. The full example below is checked by
rustdoc.

The CI probe `scripts/native_audio_smoke.sh` requires PulseAudio and uses a private,
clocked silent sink under an existing display (or `xvfb-run -a`). It does not
require speakers or modify the desktop audio server, and includes the legacy
pitch/duration regression. To run that regression against your normal audio device,
run `RAYENGINE_AUDIO_REALTIME=1 cargo test -p rayengine native_audio -- --ignored --test-threads=1`.
