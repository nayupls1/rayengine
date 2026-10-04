//! Generated looping music, scene crossfades, pause ducking, and saved settings.
use rayengine::{
    prelude::*,
    save::{self, SaveLimits, SaveOptions},
};
use std::{path::PathBuf, rc::Rc, time::Duration};
#[path = "../examples/support/audio_wave.rs"]
mod audio_wave;

const SCENE: Action = Action(0);
const PAUSE: Action = Action(1);
const EFFECT: Action = Action(2);
const DOWN: Action = Action(3);
const UP: Action = Action(4);
const MUTE: Action = Action(5);
const SAVE: Action = Action(6);
const SCHEMA: u32 = 1;
fn controls() -> Bindings {
    Bindings::new()
        .bind(SCENE, KeyboardKey::KEY_ENTER)
        .bind(PAUSE, KeyboardKey::KEY_P)
        .bind(EFFECT, KeyboardKey::KEY_SPACE)
        .bind(DOWN, KeyboardKey::KEY_LEFT)
        .bind(UP, KeyboardKey::KEY_RIGHT)
        .bind(MUTE, KeyboardKey::KEY_M)
        .bind(SAVE, KeyboardKey::KEY_S)
}
struct Shared {
    tracks: [MusicId; 2],
    click: SoundId,
    save_path: PathBuf,
}
struct SceneState {
    shared: Rc<Shared>,
    scene: usize,
}
impl State for SceneState {
    fn enter(
        &mut self,
        ctx: &mut InitContext<'_, '_>,
        _: &mut StateResources,
    ) -> Result<(), Error> {
        // Run-owned shared tracks survive outgoing scene exit, allowing overlap.
        ctx.assets.audio().crossfade(
            self.shared.tracks[self.scene],
            MusicOptions::default(),
            Duration::from_secs(1),
        )
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        if ctx.input.pressed(PAUSE) {
            assert!(
                commands
                    .request(Transition::Push(Box::new(Settings {
                        shared: self.shared.clone(),
                        message: String::new()
                    })))
                    .is_ok()
            );
        } else if ctx.input.pressed(SCENE) {
            assert!(
                commands
                    .request(Transition::Replace(Box::new(SceneState {
                        shared: self.shared.clone(),
                        scene: 1 - self.scene
                    })))
                    .is_ok()
            );
        }
        if ctx.input.pressed(EFFECT) {
            ctx.assets
                .play_sound(
                    self.shared.click,
                    SoundOptions {
                        volume: 0.6,
                        pitch: if self.scene == 0 { 1.0 } else { 1.5 },
                        pan: if self.scene == 0 { -0.6 } else { 0.6 },
                        ..SoundOptions::default()
                    },
                )
                .expect("valid sound controls");
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>, _: &mut StateCommands) {
        frame.clear(if self.scene == 0 {
            Color::new(18, 29, 48, 255)
        } else {
            Color::new(40, 24, 54, 255)
        });
        let status = frame
            .assets
            .audio()
            .music_status(self.shared.tracks[self.scene])
            .unwrap();
        let label = format!(
            "Scene {}  |  track fade: {:.2}  |  cursor: {:.1}s",
            self.scene + 1,
            status.gain,
            status.seconds
        );
        frame.ui(|ui| {
            ui.text("AUDIO MIXER", Vec2::new(40.0, 60.0), 36.0, Color::WHITE);
            ui.text(&label, Vec2::new(40.0, 125.0), 22.0, Color::SKYBLUE);
            ui.text(
                "Enter: crossfade scene   Space: panned effect",
                Vec2::new(40.0, 180.0),
                22.0,
                Color::WHITE,
            );
            ui.text(
                "P: pause / audio settings",
                Vec2::new(40.0, 220.0),
                22.0,
                Color::WHITE,
            );
        });
    }
}
struct Settings {
    shared: Rc<Shared>,
    message: String,
}
impl State for Settings {
    fn policy(&self) -> StatePolicy {
        StatePolicy {
            draw_below: true,
            ..StatePolicy::default()
        }
    }
    fn enter(
        &mut self,
        ctx: &mut InitContext<'_, '_>,
        _: &mut StateResources,
    ) -> Result<(), Error> {
        ctx.assets
            .audio()
            .buses_mut()
            .duck(BusId::MUSIC, 0.25, Duration::from_millis(250))?;
        Ok(())
    }
    fn exit(&mut self, ctx: &mut InitContext<'_, '_>) {
        ctx.assets
            .audio()
            .buses_mut()
            .duck(BusId::MUSIC, 1.0, Duration::from_millis(250))
            .expect("valid bus");
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        if ctx.input.pressed(PAUSE) {
            assert!(commands.request(Transition::Pop).is_ok());
        }
        let mut audio = ctx.assets.audio();
        let current = audio.buses().settings_for(BusId::MUSIC).unwrap();
        if ctx.input.pressed(DOWN) || ctx.input.pressed(UP) {
            let step = if ctx.input.pressed(UP) { 0.1 } else { -0.1 };
            audio
                .buses_mut()
                .set_volume(BusId::MUSIC, (current.volume + step).clamp(0.0, 1.0))
                .unwrap();
        }
        if ctx.input.pressed(MUTE) {
            audio
                .buses_mut()
                .set_muted(BusId::MUSIC, !current.muted)
                .unwrap();
        }
        if ctx.input.pressed(SAVE) {
            self.message = match save::save_with(
                &self.shared.save_path,
                SCHEMA,
                &audio.buses().settings(),
                SaveOptions::default(),
                serde_json::to_vec,
            ) {
                Ok(()) => format!("Saved {}", self.shared.save_path.display()),
                Err(error) => format!("Save failed: {error}"),
            };
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>, _: &mut StateCommands) {
        let size = frame.viewport.logical_size;
        let values = frame
            .assets
            .audio()
            .buses()
            .settings_for(BusId::MUSIC)
            .unwrap();
        let label = format!(
            "Music volume: {:.0}%  |  muted: {}",
            values.volume * 100.0,
            values.muted
        );
        frame.ui(|ui| {
            ui.rectangle(
                Aabb2 {
                    min: Vec2::ZERO,
                    max: size,
                },
                Color::new(0, 0, 0, 210),
            );
            ui.text(
                "PAUSED / AUDIO SETTINGS",
                Vec2::new(40.0, 70.0),
                30.0,
                Color::WHITE,
            );
            ui.text(&label, Vec2::new(40.0, 130.0), 24.0, Color::SKYBLUE);
            ui.text(
                "Left/Right: volume   M: mute   S: save   P: resume",
                Vec2::new(40.0, 185.0),
                20.0,
                Color::WHITE,
            );
            ui.text(
                "Music and fades continue while simulation is paused.",
                Vec2::new(40.0, 230.0),
                20.0,
                Color::WHITE,
            );
            ui.text(&self.message, Vec2::new(40.0, 280.0), 18.0, Color::WHITE);
        });
    }
}
struct Demo {
    files: PathBuf,
    stack: Option<StateStack>,
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        controls()
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let tracks = [
            ctx.music(self.files.join("calm.wav"))?,
            ctx.music(self.files.join("bright.wav"))?,
        ];
        let click = ctx.sound(self.files.join("click.wav"))?;
        ctx.assets.audio().set_sound_limit(click, Some(4))?;
        let save_path = PathBuf::from("audio-settings.raysave");
        // Only a missing save uses defaults. Corrupt/future saves remain intact.
        match save::load(&save_path, SaveLimits::default()) {
            Ok(container) => {
                container
                    .require_schema(SCHEMA)
                    .map_err(|e| Error::Asset(e.to_string()))?;
                let settings: AudioSettings = serde_json::from_slice(&container.payload)
                    .map_err(|e| Error::Asset(e.to_string()))?;
                ctx.assets.audio().buses_mut().apply_settings(&settings)?;
            }
            Err(save::SaveError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::Asset(error.to_string())),
        }
        self.stack = Some(StateStack::new(
            SceneState {
                shared: Rc::new(Shared {
                    tracks,
                    click,
                    save_path,
                }),
                scene: 0,
            },
            controls(),
        ));
        self.stack.as_mut().unwrap().init(ctx)
    }
    fn boundary(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        if let Some(stack) = &mut self.stack {
            stack.boundary(ctx)?;
            if let Some(error) = stack.take_error() {
                return Err(error);
            }
        }
        Ok(())
    }
    fn shutdown(&mut self, ctx: &mut InitContext<'_, '_>) {
        if let Some(stack) = &mut self.stack {
            stack.shutdown(ctx);
        }
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.stack.as_mut().unwrap().fixed_update(ctx);
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        self.stack.as_mut().unwrap().draw(frame);
    }
}
fn main() -> Result<(), Error> {
    let generated = audio_wave::GeneratedAudio::new()?;
    let mut config = Config::new("rayengine / Audio");
    config.audio = true;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Demo {
            files: generated.directory.clone(),
            stack: None,
        })?;
    Ok(())
}
