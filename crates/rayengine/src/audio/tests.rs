use super::*;
#[test]
fn audio_disabled_rejects_loading_and_stale_playback() {
    let mut assets = Assets::new(None);
    assert!(assets.load_music(Path::new("missing.wav")).is_err());
    assert!(assets.load_sound(Path::new("missing.wav")).is_err());
    assert!(
        assets
            .audio()
            .play_music(MusicId(0), MusicOptions::default())
            .is_err()
    );
    assert!(
        !assets
            .play_sound(SoundId(0), SoundOptions::default())
            .unwrap()
    );
    assert_eq!(assets.resource_counts().music_streams, 0);
}
#[test]
fn one_shot_controls_reject_invalid_numbers() {
    let buses = AudioBuses::default();
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(
            SoundOptions {
                volume: value,
                ..SoundOptions::default()
            }
            .validate(&buses)
            .is_err()
        );
    }
    for value in [f32::NAN, f32::INFINITY, 0.0, 0.1, 4.1] {
        assert!(
            SoundOptions {
                pitch: value,
                ..SoundOptions::default()
            }
            .validate(&buses)
            .is_err()
        );
    }
    for value in [f32::NAN, f32::INFINITY, -1.1, 1.1] {
        assert!(
            SoundOptions {
                pan: value,
                ..SoundOptions::default()
            }
            .validate(&buses)
            .is_err()
        );
    }
    for value in [-1.0, 0.0, 1.0] {
        SoundOptions {
            pan: value,
            ..SoundOptions::default()
        }
        .validate(&buses)
        .unwrap();
    }
}

#[test]
fn managed_controls_dispatch_volume_pitch_and_pan() {
    #[derive(Default)]
    struct Observed(std::cell::RefCell<Vec<(&'static str, f32)>>);
    impl SoundControls for Observed {
        fn set_volume(&self, value: f32) {
            self.0.borrow_mut().push(("volume", value));
        }
        fn set_pitch(&self, value: f32) {
            self.0.borrow_mut().push(("pitch", value));
        }
        fn set_pan(&self, value: f32) {
            self.0.borrow_mut().push(("pan", value));
        }
    }
    let observed = Observed::default();
    let mut buses = AudioBuses::default();
    buses.set_volume(BusId::MASTER, 0.5).unwrap();
    buses.set_volume(BusId::SFX, 0.4).unwrap();
    let options = SoundOptions {
        volume: 0.5,
        pitch: 1.5,
        pan: -0.75,
        ..SoundOptions::default()
    };
    options.apply(&observed, &buses);
    assert_eq!(
        *observed.0.borrow(),
        [("volume", 0.1), ("pitch", 1.5), ("pan", -0.75)]
    );
    observed.0.borrow_mut().clear();
    buses.set_muted(BusId::MASTER, true).unwrap();
    SoundOptions::default().apply(&observed, &buses);
    assert_eq!(
        *observed.0.borrow(),
        [("volume", 0.0), ("pitch", 1.0), ("pan", 0.0)]
    );
}

#[path = "../../examples/support/audio_wave.rs"]
mod audio_wave;

#[test]
#[ignore = "requires an audio output device; run with --ignored --test-threads=1"]
fn native_audio_streams_voices_caching_limits_and_cleanup() {
    let files = audio_wave::GeneratedAudio::new().unwrap();
    let device = RaylibAudio::init_audio_device().unwrap();
    {
        let mut assets = Assets::new(Some(&device));
        let calm = assets
            .load_music(&files.directory.join("calm.wav"))
            .unwrap();
        let bright = assets
            .load_music(&files.directory.join("bright.wav"))
            .unwrap();
        assert_eq!(
            calm,
            assets
                .load_music(&files.directory.join("./calm.wav"))
                .unwrap()
        );
        let click = assets
            .load_sound(&files.directory.join("click.wav"))
            .unwrap();
        assert_eq!(
            click,
            assets
                .load_sound(&files.directory.join("./click.wav"))
                .unwrap()
        );
        assets
            .audio()
            .play_music(
                calm,
                MusicOptions {
                    fade_in: Duration::from_secs(1),
                    ..MusicOptions::default()
                },
            )
            .unwrap();
        assets.update_audio(Duration::from_millis(500));
        assert_eq!(assets.audio().music_status(calm).unwrap().gain, 0.5);
        assets
            .audio()
            .crossfade(bright, MusicOptions::default(), Duration::from_secs(1))
            .unwrap();
        assets.update_audio(Duration::from_millis(500));
        assert_eq!(assets.audio().music_status(calm).unwrap().gain, 0.25);
        assert_eq!(assets.audio().music_status(bright).unwrap().gain, 0.5);
        // Invalid targets/options must not disturb ongoing crossfades.
        assert!(
            assets
                .audio()
                .crossfade(MusicId(999), MusicOptions::default(), Duration::ZERO)
                .is_err()
        );
        assert!(
            assets
                .audio()
                .crossfade(
                    calm,
                    MusicOptions {
                        volume: f32::NAN,
                        ..MusicOptions::default()
                    },
                    Duration::ZERO
                )
                .is_err()
        );
        assert_eq!(assets.audio().music_status(bright).unwrap().gain, 0.5);
        // Reverse halfway through: neither stream's current gain jumps.
        assets
            .audio()
            .crossfade(calm, MusicOptions::default(), Duration::from_secs(1))
            .unwrap();
        assert_eq!(assets.audio().music_status(calm).unwrap().gain, 0.25);
        assets.audio().pause_music(bright).unwrap();
        assets.update_audio(Duration::from_secs(1));
        assert!(!assets.audio().music_status(bright).unwrap().active);
        assert_eq!(assets.audio().music_status(calm).unwrap().gain, 1.0);
        assets.audio().pause_music(calm).unwrap();
        assert!(assets.audio().music_status(calm).unwrap().paused);
        assets.audio().resume_music(calm).unwrap();
        assert!(!assets.audio().music_status(calm).unwrap().paused);
        assets
            .audio()
            .crossfade(bright, MusicOptions::default(), Duration::ZERO)
            .unwrap();
        assert!(!assets.audio().music_status(calm).unwrap().active);
        assert_eq!(assets.audio().music_status(bright).unwrap().gain, 1.0);
        assets.audio().fade_out(bright, Duration::ZERO).unwrap();
        assert!(!assets.audio().music_status(bright).unwrap().active);
        // Check routing, independent voice metadata, caps, and pooled allocation.
        assets.audio().set_sound_limit(click, Some(2)).unwrap();
        let dialogue = assets.audio().buses_mut().add("dialogue").unwrap();
        assets
            .audio()
            .buses_mut()
            .set_volume(BusId::MASTER, 0.5)
            .unwrap();
        assets
            .audio()
            .buses_mut()
            .set_volume(dialogue, 0.4)
            .unwrap();
        let options = SoundOptions {
            bus: dialogue,
            volume: 0.5,
            pitch: 0.25,
            pan: -0.7,
        };
        assert!(assets.play_sound(click, options).unwrap());
        assert!(
            assets
                .play_sound(
                    click,
                    SoundOptions {
                        pan: 0.7,
                        ..options
                    }
                )
                .unwrap()
        );
        assert!(!assets.play_sound(click, options).unwrap());
        assert_eq!(assets.resource_counts().sound_instances, 2);
        assert!(!assets.play(click)); // no room for an additional legacy voice
        assets.audio().sounds[click.0].as_ref().unwrap().voices[0]
            .sound
            .stop();
        assert!(assets.play(click));
        assert!(assets.play(click)); // restart does not add a concurrent voice
        assets.audio().set_sound_limit(click, Some(0)).unwrap();
        assert!(!assets.play(click));
        assets.audio().set_sound_limit(click, None).unwrap();
        // Reuse a finished/stopped buffer, preserving independent voice options.
        let count = assets.resource_counts().sound_instances;
        assert!(assets.play_sound(click, options).unwrap());
        assert_eq!(assets.resource_counts().sound_instances, count);
        assets
            .audio()
            .buses_mut()
            .set_muted(BusId::MASTER, true)
            .unwrap();
        assets.update_audio(Duration::ZERO);
        let audio = assets.audio();
        let managed = &audio.sounds[click.0].as_ref().unwrap().voices[0].options;
        assert_eq!(managed.pan, -0.7);
        assert_eq!(managed.pitch, 0.25);
        assert_eq!(managed.volume * audio.buses.gain(managed.bus).unwrap(), 0.0);
        drop(audio);
        // Original raw controls survive both legacy play and mixer frame updates.
        // Mixer-managed buffers remain separate from the public original voice.
        let source = assets.sound(click).unwrap();
        source.set_volume(0.2);
        source.set_pitch(0.25);
        source.set_pan(-0.5);
        let count = assets.resource_counts().sound_instances;
        assert!(assets.play(click));
        assets.update_audio(Duration::from_millis(350));
        assert_eq!(assets.resource_counts().sound_instances, count);
        // Enable this duration regression when the output has a real-time sample
        // clock (hardware or the clocked silent sink used by the CI smoke script).
        if std::env::var_os("RAYENGINE_AUDIO_REALTIME").is_some() {
            std::thread::sleep(Duration::from_millis(350));
            assert!(assets.sound(click).unwrap().is_playing());
        }
        assets.sound(click).unwrap().play();
        assets.update_audio(Duration::from_millis(350));
        assert_eq!(assets.resource_counts().sound_instances, count);
        if std::env::var_os("RAYENGINE_AUDIO_REALTIME").is_some() {
            std::thread::sleep(Duration::from_millis(350));
            assert!(assets.sound(click).unwrap().is_playing());
        }
        // A naturally completed managed instance can be reused too.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while assets.audio().sounds[click.0]
            .as_ref()
            .unwrap()
            .voices
            .iter()
            .any(|voice| voice.sound.is_playing())
        {
            assert!(
                std::time::Instant::now() < deadline,
                "managed voices did not finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let count = assets.resource_counts().sound_instances;
        assert!(assets.play_sound(click, options).unwrap());
        assert_eq!(assets.resource_counts().sound_instances, count);
        let short = assets
            .load_music(&files.directory.join("click.wav"))
            .unwrap();
        assets
            .audio()
            .play_music(
                short,
                MusicOptions {
                    looping: false,
                    ..MusicOptions::default()
                },
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while assets.audio().music_status(short).unwrap().active {
            assert!(
                std::time::Instant::now() < deadline,
                "nonlooping stream did not finish"
            );
            assets.update_audio(Duration::from_millis(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        // Disabling looping after a full loop must still reach a natural end.
        // raylib's accumulated decoded-frame counter needs a restart here.
        assets
            .audio()
            .play_music(
                short,
                MusicOptions {
                    fade_in: Duration::from_secs(2),
                    ..MusicOptions::default()
                },
            )
            .unwrap();
        for _ in 0..100 {
            assets.update_audio(Duration::from_millis(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        assets.audio().pause_music(short).unwrap();
        let gain = assets.audio().music_status(short).unwrap().gain;
        assets
            .audio()
            .play_music(
                short,
                MusicOptions {
                    looping: false,
                    fade_in: Duration::from_secs(1),
                    ..MusicOptions::default()
                },
            )
            .unwrap();
        assert_eq!(assets.audio().music_status(short).unwrap().gain, gain);
        assert!(!assets.audio().music_status(short).unwrap().paused);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while assets.audio().music_status(short).unwrap().active {
            assert!(
                std::time::Instant::now() < deadline,
                "stream did not finish after disabling looping"
            );
            assets.update_audio(Duration::from_millis(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        // Stop must rewind the native buffer even when playback was paused.
        assets
            .audio()
            .play_music(short, MusicOptions::default())
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while assets.audio().music_status(short).unwrap().seconds < 0.08 {
            assert!(
                std::time::Instant::now() < deadline,
                "cursor did not advance"
            );
            assets.update_audio(Duration::from_millis(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        assets.audio().pause_music(short).unwrap();
        assets.audio().stop_music(short).unwrap();
        let stopped = assets.audio().music_status(short).unwrap();
        assert!(!stopped.active && !stopped.paused);
        assert_eq!(stopped.seconds, 0.0);
        assert!(assets.unload_music(calm));
        assert!(!assets.unload_music(calm));
        assert!(assets.audio().music_status(calm).is_none());
        let reloaded = assets
            .load_music(&files.directory.join("calm.wav"))
            .unwrap();
        assert_ne!(calm, reloaded);
        // Stop/free overlapping buffers on sound unload, then invalidate the ID.
        assets.unload_sound(click);
        assert_eq!(assets.resource_counts().sound_instances, 0);
        assert!(!assets.play_sound(click, options).unwrap());
        let new_click = assets
            .load_sound(&files.directory.join("click.wav"))
            .unwrap();
        assert_ne!(click, new_click);
        // Active resources also release on drop before the enclosing device.
        assets
            .audio()
            .play_music(reloaded, MusicOptions::default())
            .unwrap();
        assert!(assets.play(new_click));
    }
    assert!(device.is_audio_device_ready());
}

#[test]
#[ignore = "requires an audio output device and display; run with --ignored --test-threads=1"]
fn native_audio_frame_clock_independent_of_simulation_ticks() {
    use crate::{App, Config, Game, InitContext, RunOptions, Update, render::Frame};
    struct Probe {
        directory: PathBuf,
        track: Option<MusicId>,
        elapsed: Duration,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            let track = ctx.music(self.directory.join("calm.wav"))?;
            ctx.assets.audio().play_music(
                track,
                MusicOptions {
                    fade_in: Duration::from_millis(20),
                    ..MusicOptions::default()
                },
            )?;
            self.track = Some(track);
            Ok(())
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {
            // A slow scheduler/GPU may eventually produce ticks. They do not
            // advance the audio clock; this game intentionally does nothing here.
        }
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            frame.clear(raylib::prelude::Color::BLACK);
            self.elapsed = self.elapsed.saturating_add(frame.delta);
            let expected = (self.elapsed.as_secs_f64() / 0.02).min(1.0) as f32;
            let actual = frame
                .assets
                .audio()
                .music_status(self.track.unwrap())
                .unwrap()
                .gain;
            assert!(
                (actual - expected).abs() < 1e-6,
                "frame {}: gain {actual}, expected {expected}",
                frame.index
            );
        }
        fn shutdown(&mut self, ctx: &mut InitContext<'_, '_>) {
            assert!(
                ctx.assets
                    .audio()
                    .music_status(self.track.unwrap())
                    .unwrap()
                    .active
            );
            assert!(ctx.assets.unload_music(self.track.unwrap()));
        }
    }
    let files = audio_wave::GeneratedAudio::new().unwrap();
    let mut config = Config::new("native audio frame clock");
    config.audio = true;
    config.fixed_hz = 1;
    config.target_fps = 120;
    config.vsync = false;
    let report = App::new(config)
        .with_options(RunOptions {
            hidden: true,
            frames: Some(10),
            ..RunOptions::default()
        })
        .run(Probe {
            directory: files.directory.clone(),
            track: None,
            elapsed: Duration::ZERO,
        })
        .unwrap();
    assert_eq!(report.frames, 10);
}
