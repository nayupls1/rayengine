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
        assert_eq!(assets.resource_counts().sound_instances, 1);
        assert!(assets.play(click)); // legacy restart remains at two concurrent voices
        assets.audio().set_sound_limit(click, Some(0)).unwrap();
        assert!(!assets.play(click));
        assets.audio().set_sound_limit(click, None).unwrap();
        assets.sound(click).unwrap().stop();
        assert!(assets.play_sound(click, options).unwrap());
        assets
            .audio()
            .buses_mut()
            .set_muted(BusId::MASTER, true)
            .unwrap();
        assets.update_audio(Duration::ZERO);
        let audio = assets.audio();
        let original = audio.sounds[click.0].as_ref().unwrap().original;
        assert_eq!(original.pan, -0.7);
        assert_eq!(original.pitch, 0.25);
        assert_eq!(
            original.volume * audio.buses.gain(original.bus).unwrap(),
            0.0
        );
        drop(audio);
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
fn native_audio_frame_clock_without_simulation_ticks() {
    use crate::{App, Config, Game, InitContext, RunOptions, Update, render::Frame};
    struct Probe {
        directory: PathBuf,
        track: Option<MusicId>,
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
            panic!("no fixed ticks expected");
        }
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            frame.clear(raylib::prelude::Color::BLACK);
            if frame.index >= 5 {
                assert_eq!(
                    frame
                        .assets
                        .audio()
                        .music_status(self.track.unwrap())
                        .unwrap()
                        .gain,
                    1.0
                );
            }
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
        })
        .unwrap();
    assert_eq!(report.ticks, 0);
    assert_eq!(report.frames, 10);
}
