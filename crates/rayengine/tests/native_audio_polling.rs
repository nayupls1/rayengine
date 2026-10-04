//! Visible audio runner flags in a fresh process. Raylib retains window flags
//! across runs, so hidden-window unit tests must not precede this probe.

#[test]
#[ignore = "requires an audio output device and display; run with --ignored --test-threads=1"]
fn native_audio_visible_runner_keeps_event_polling_available() {
    use rayengine::{App, Config, Error, Game, InitContext, RunOptions, Update, render::Frame};
    struct Probe {
        audio: bool,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            let state = ctx.raylib.get_window_state();
            assert!(
                !state.window_hidden(),
                "hidden windows would bypass the regression"
            );
            assert_eq!(
                state.window_always_run(),
                self.audio,
                "audio needs nonblocking minimized polling; audio-disabled defaults remain unchanged"
            );
            // Exit before presentation: this inspects the actual native window flags
            // without relying on a window manager to iconify/restore test windows.
            Err(Error::Asset("visible audio polling probe complete".into()))
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {
            unreachable!();
        }
        fn draw(&mut self, _: &mut Frame<'_, '_>) {
            unreachable!();
        }
    }
    for audio in [false, true, false] {
        let mut config = Config::new("native visible audio polling");
        config.audio = audio;
        config.window_size = (64, 64);
        let result = App::new(config)
            .with_options(RunOptions {
                hidden: false,
                frames: Some(1),
                ..RunOptions::default()
            })
            .run(Probe { audio });
        assert!(
            matches!(result, Err(Error::Asset(message)) if message == "visible audio polling probe complete")
        );
    }
}
