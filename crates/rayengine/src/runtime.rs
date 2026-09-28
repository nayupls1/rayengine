//! Window lifecycle: initialize once, sample input, fixed-update, draw, present.

use crate::{
    assets::{Assets, MeshId, ModelId, SoundId, TextureId, path_string},
    input::Bindings,
    render::{Frame, rect},
};
use rayengine_core::{
    glam::Vec2,
    input::Input,
    mesh::MeshData,
    time::{FixedClock, Tick},
    viewport::{ScaleMode, Viewport},
};
use raylib::prelude::*;
use std::{
    fmt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// SDK setup, asset or I/O failure.
#[derive(Debug)]
pub enum Error {
    /// Invalid game or run configuration.
    Config(String),
    /// Graphics/audio initialization or render target failure.
    Backend(String),
    /// Asset validation/loading failure.
    Asset(String),
    /// Filesystem operation failed.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) | Self::Backend(message) | Self::Asset(message) => {
                f.write_str(message)
            }
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(error) = self {
            Some(error)
        } else {
            None
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Explicit game defaults. Resizable and high-DPI support are always enabled.
#[derive(Clone, Debug)]
pub struct Config {
    /// Window title.
    pub title: String,
    /// Initial logical window dimensions.
    pub window_size: (u32, u32),
    /// Reference content/UI size. Cameras use world units instead.
    pub reference_size: Vec2,
    /// Viewport fit/expand policy.
    pub scale_mode: ScaleMode,
    /// Fixed simulation updates per second, from 1 to 1000.
    pub fixed_hz: u32,
    /// Maximum simulation updates before rendering a frame.
    pub max_catch_up: u32,
    /// Render framerate cap. Zero is uncapped.
    pub target_fps: u32,
    /// Request display synchronization from the graphics driver.
    pub vsync: bool,
    /// Initialize audio. Disabled until a game requests it.
    pub audio: bool,
    /// Background color outside the content viewport.
    pub bar_color: Color,
}

impl Config {
    /// Creates a 960×540 reference view, 1280×720 window, and 120 Hz simulation.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            window_size: (1280, 720),
            reference_size: Vec2::new(960.0, 540.0),
            scale_mode: ScaleMode::Fit,
            fixed_hz: 120,
            max_catch_up: 8,
            target_fps: 120,
            vsync: true,
            audio: false,
            bar_color: Color::new(9, 14, 24, 255),
        }
    }

    /// Validates configuration before invoking the native backend.
    pub fn validate(&self) -> Result<(), Error> {
        if self.title.contains('\0') {
            return Err(Error::Config("window title cannot contain NUL".into()));
        }
        validate_size(self.window_size)?;
        if !self.reference_size.is_finite()
            || self.reference_size.min_element() < 1.0
            || self.reference_size.max_element() > 8192.0
        {
            return Err(Error::Config(
                "reference dimensions must be finite and between 1 and 8192".into(),
            ));
        }
        if !(1..=1000).contains(&self.fixed_hz) || !(1..=1000).contains(&self.max_catch_up) {
            return Err(Error::Config(
                "fixed_hz and max_catch_up must be between 1 and 1000".into(),
            ));
        }
        if self.target_fps > 1000 {
            return Err(Error::Config("target_fps must be 0..=1000".into()));
        }
        Ok(())
    }
}

/// Optional run controls for bounded native rendering smoke checks.
/// This is not the deferred interactive game-testing protocol.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Exit after this many presented frames. `None` means normal interactive play.
    pub frames: Option<u64>,
    /// Save the last presented frame as a PNG.
    pub screenshot: Option<PathBuf>,
    /// Override initial logical window dimensions.
    pub size: Option<(u32, u32)>,
    /// Hide the window; still requires a working native graphics/display context.
    pub hidden: bool,
    /// Disable VSync and the render cap. Useful for rendering measurements.
    pub uncapped: bool,
}

impl RunOptions {
    /// Reads `--frames N`, `--screenshot file.png`, `--size WIDTHxHEIGHT`,
    /// `--hidden`, and `--uncapped`. Unknown or incomplete arguments are errors.
    pub fn from_env() -> Result<Self, Error> {
        Self::parse(std::env::args().skip(1))
    }

    /// Parses the same run controls from explicit strings.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, Error> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => {
                    let value = args
                        .next()
                        .ok_or_else(|| Error::Config("--frames needs a positive count".into()))?;
                    options.frames = Some(
                        value
                            .parse::<u64>()
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or_else(|| {
                                Error::Config("--frames needs a positive count".into())
                            })?,
                    );
                }
                "--screenshot" => {
                    options.screenshot = Some(
                        args.next()
                            .ok_or_else(|| Error::Config("--screenshot needs a PNG path".into()))?
                            .into(),
                    )
                }
                "--size" => {
                    let value = args
                        .next()
                        .ok_or_else(|| Error::Config("--size needs WIDTHxHEIGHT".into()))?;
                    let (w, h) = value
                        .split_once('x')
                        .ok_or_else(|| Error::Config("--size needs WIDTHxHEIGHT".into()))?;
                    let size = (
                        w.parse()
                            .map_err(|_| Error::Config("invalid width".into()))?,
                        h.parse()
                            .map_err(|_| Error::Config("invalid height".into()))?,
                    );
                    validate_size(size)?;
                    options.size = Some(size);
                }
                "--hidden" => options.hidden = true,
                "--uncapped" => options.uncapped = true,
                _ => return Err(Error::Config(format!("unknown run option: {arg}"))),
            }
        }
        if let Some(path) = &options.screenshot {
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                return Err(Error::Config("screenshot path must end in .png".into()));
            }
            path_string(path)?;
        }
        Ok(options)
    }
}

/// Counters returned after a game exits.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunReport {
    /// Number of presented frames.
    pub frames: u64,
    /// Number of executed simulation ticks.
    pub ticks: u64,
    /// Total time discarded by bounded catch-up.
    pub dropped_time: Duration,
    /// Wall time spent in the game loop, including presentation throttling.
    pub elapsed: Duration,
}

/// Initialization context; asset loading is explicit and errors are recoverable.
pub struct InitContext<'context, 'audio> {
    /// Native raylib handle for advanced resource creation.
    pub raylib: &'context mut RaylibHandle,
    /// Token identifying raylib's owning thread.
    pub thread: &'context RaylibThread,
    /// Assets owned until this game run finishes.
    pub assets: &'context mut Assets<'audio>,
}

impl InitContext<'_, '_> {
    /// Validates and uploads generated geometry with the default material.
    pub fn mesh(&mut self, data: &MeshData) -> Result<MeshId, Error> {
        self.assets.upload_mesh(self.raylib, self.thread, data)
    }

    /// Replaces generated geometry, preserving its handle and the old mesh on error.
    pub fn replace_mesh(&mut self, id: MeshId, data: &MeshData) -> Result<(), Error> {
        self.assets.replace_mesh(self.thread, id, data)
    }

    /// Loads/caches a texture by canonical path.
    pub fn texture(&mut self, path: impl AsRef<Path>) -> Result<TextureId, Error> {
        self.assets
            .load_texture(self.raylib, self.thread, path.as_ref())
    }
    /// Loads/caches a model by canonical path.
    pub fn model(&mut self, path: impl AsRef<Path>) -> Result<ModelId, Error> {
        self.assets
            .load_model(self.raylib, self.thread, path.as_ref())
    }
    /// Loads/caches a sound. Requires `Config::audio = true`.
    pub fn sound(&mut self, path: impl AsRef<Path>) -> Result<SoundId, Error> {
        self.assets.load_sound(path.as_ref())
    }
}

/// One fixed update. Gameplay receives actions and timing, never a variable dt.
pub struct Update<'context, 'audio> {
    /// Current fixed tick.
    pub tick: Tick,
    /// Action states. Press/release edges are consumed after this update.
    pub input: &'context Input,
    /// Current viewport, shared with UI and cameras.
    pub viewport: Viewport,
    /// Pointer in UI units, or `None` in bars or while unfocused.
    pub pointer: Option<Vec2>,
    /// Loaded assets, including sound playback.
    pub assets: &'context Assets<'audio>,
    pub(crate) quit: &'context mut bool,
}

impl Update<'_, '_> {
    /// Requests exit after the current update batch and final presentation.
    pub fn quit(&mut self) {
        *self.quit = true;
    }
}

/// Shared game lifecycle for 2D, 3D, or mixed games.
pub trait Game {
    /// Chooses the cursor policy at startup. Capture is suspended on focus loss.
    fn cursor_mode(&self) -> CursorMode {
        CursorMode::Free
    }
    /// Declares actions and physical buttons before entering the loop.
    fn bindings(&self) -> Bindings {
        Bindings::new()
    }
    /// Loads resources once, before the first simulation tick.
    fn init(&mut self, _context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        Ok(())
    }
    /// Advances game state by one fixed tick.
    fn fixed_update(&mut self, context: &mut Update<'_, '_>);
    /// Draws world passes and UI. Interpolate state using `frame.alpha`.
    fn draw(&mut self, frame: &mut Frame<'_, '_>);
}

/// Game-owned cursor policy, applied by the runner while the window is focused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorMode {
    /// Visible cursor that can leave the window.
    #[default]
    Free,
    /// Hidden cursor with unbounded relative motion for first-person look.
    Captured,
}

#[derive(Default)]
struct CursorState {
    focused: bool,
    captured: bool,
}

impl CursorState {
    fn update(&mut self, mode: CursorMode, focused: bool) -> (bool, bool) {
        let capture = focused && mode == CursorMode::Captured;
        let changed = capture != self.captured;
        let accept_motion = focused && self.focused && !changed;
        self.focused = focused;
        self.captured = capture;
        (changed, accept_motion)
    }
}

/// Owns game configuration and executes the prescribed lifecycle.
pub struct App {
    config: Config,
    options: RunOptions,
}

impl App {
    /// Builds a runner. Configuration is validated by [`Self::run`].
    pub fn new(config: Config) -> Self {
        Self {
            config,
            options: RunOptions::default(),
        }
    }
    /// Applies optional bounded-run and screenshot controls.
    pub fn with_options(mut self, options: RunOptions) -> Self {
        self.options = options;
        self
    }

    /// Opens raylib and runs the game on this thread.
    ///
    /// Native window creation can panic if the display or graphics driver is
    /// unavailable, following raylib-rs behavior. CPU-only tests use `rayengine-core`.
    /// Only one raylib window may exist in a process at a time.
    pub fn run(self, game: impl Game) -> Result<RunReport, Error> {
        let Self {
            mut config,
            options,
        } = self;
        if let Some(size) = options.size {
            config.window_size = size;
        }
        config.validate()?;
        if options.frames == Some(0) {
            return Err(Error::Config("frame limit must be positive".into()));
        }
        if let Some(path) = &options.screenshot {
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                return Err(Error::Config("screenshot path must end in .png".into()));
            }
            path_string(path)?;
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut builder = raylib::init();
        builder
            .size(config.window_size.0 as i32, config.window_size.1 as i32)
            .title(&config.title)
            .resizable()
            .highdpi()
            .log_level(TraceLogLevel::LOG_WARNING);
        if config.vsync && !options.uncapped {
            builder.vsync();
        }
        if options.hidden {
            builder.hidden().always_run();
        }
        let (mut raylib, thread) = builder.build();
        raylib.set_target_fps(if options.uncapped {
            0
        } else {
            config.target_fps
        });
        let audio = if config.audio {
            Some(RaylibAudio::init_audio_device().map_err(|e| Error::Backend(e.to_string()))?)
        } else {
            None
        };
        let mut assets = Assets::new(audio.as_ref());
        // This binding drops the game before assets, audio, and the window even
        // on early returns. Game-owned native resources also remain context-safe.
        let mut game = game;
        let bindings = game.bindings();
        let mut input = Input::with_capacity(bindings.capacity());
        game.init(&mut InitContext {
            raylib: &mut raylib,
            thread: &thread,
            assets: &mut assets,
        })?;
        let cursor_mode = game.cursor_mode();
        let mut cursor = CursorState::default();
        let mut target: Option<RenderTexture2D> = None;
        let mut target_size = (0, 0);
        let mut clock = FixedClock::new(config.fixed_hz, config.max_catch_up);
        let start = Instant::now();
        let mut previous_frame = start;
        let mut report = RunReport::default();
        let mut quit = false;
        while !quit
            && !raylib.window_should_close()
            && options.frames.is_none_or(|limit| report.frames < limit)
        {
            let now = Instant::now();
            let elapsed = now.duration_since(previous_frame);
            previous_frame = now;
            let window = Vec2::new(
                raylib.get_screen_width() as f32,
                raylib.get_screen_height() as f32,
            );
            let view = Viewport::new(window, config.reference_size, config.scale_mode);
            if raylib.is_window_minimized() || view.is_none() {
                if cursor.update(cursor_mode, false).0 {
                    raylib.enable_cursor();
                }
                input.release_all();
                // Keep backend event polling alive, but pause simulation while minimized.
                raylib
                    .begin_drawing(&thread)
                    .clear_background(config.bar_color);
                std::thread::sleep(Duration::from_millis(16));
                continue;
            }
            let view = view.expect("non-minimized viewport");
            let (cursor_changed, accept_motion) =
                cursor.update(cursor_mode, raylib.is_window_focused());
            if cursor_changed {
                if cursor.captured {
                    raylib.disable_cursor();
                } else {
                    raylib.enable_cursor();
                }
            }
            bindings.sample(&raylib, &mut input);
            if accept_motion {
                let delta = raylib.get_mouse_delta();
                input.add_pointer_delta(Vec2::new(delta.x, delta.y));
            }
            let mouse = raylib.get_mouse_position();
            let pointer = raylib
                .is_window_focused()
                .then(|| view.screen_to_ui(Vec2::new(mouse.x, mouse.y)))
                .flatten();
            let plan = clock.advance(elapsed);
            report.dropped_time += plan.dropped;
            for step in 0..plan.steps {
                game.fixed_update(&mut Update {
                    tick: Tick {
                        index: plan.first_tick + u64::from(step),
                        dt: clock.step().as_secs_f32(),
                    },
                    input: &input,
                    viewport: view,
                    pointer,
                    assets: &assets,
                    quit: &mut quit,
                });
                input.consume_edges();
                report.ticks += 1;
                if quit {
                    break;
                }
            }
            let dpi = Vec2::new(
                raylib.get_render_width() as f32,
                raylib.get_render_height() as f32,
            ) / window;
            let size = if config.scale_mode == ScaleMode::IntegerFit {
                (
                    config.reference_size.x.round() as u32,
                    config.reference_size.y.round() as u32,
                )
            } else {
                view.render_size(dpi)
            };
            if size != target_size {
                // Resize only when required; the old target drops while GL is alive.
                let new_target = raylib
                    .load_render_texture(&thread, size.0, size.1)
                    .map_err(|e| Error::Backend(e.to_string()))?;
                new_target.texture().set_texture_filter(
                    &thread,
                    if config.scale_mode == ScaleMode::IntegerFit {
                        TextureFilter::TEXTURE_FILTER_POINT
                    } else {
                        TextureFilter::TEXTURE_FILTER_BILINEAR
                    },
                );
                target = Some(new_target);
                target_size = size;
            }
            let target = target.as_mut().expect("created target");
            let mut frame = Frame {
                raylib: &mut raylib,
                thread: &thread,
                target,
                assets: &mut assets,
                viewport: view,
                alpha: plan.alpha,
                index: report.frames,
            };
            frame.clear(Color::BLACK);
            game.draw(&mut frame);
            {
                let mut draw = raylib.begin_drawing(&thread);
                draw.clear_background(config.bar_color);
                draw.draw_texture_pro(
                    target.texture(),
                    Rectangle::new(0.0, 0.0, size.0 as f32, -(size.1 as f32)),
                    rect(rayengine_core::collision::Aabb2 {
                        min: view.origin,
                        max: view.origin + view.size,
                    }),
                    Vector2::zero(),
                    0.0,
                    Color::WHITE,
                );
            }
            report.frames += 1;
        }
        if let Some(path) = &options.screenshot {
            let image = raylib.load_image_from_screen(&thread);
            let png = image
                .export_image_to_memory(".png")
                .map_err(|e| Error::Backend(e.to_string()))?;
            std::fs::write(path, &*png)?;
        }
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

fn validate_size(size: (u32, u32)) -> Result<(), Error> {
    if (1..=8192).contains(&size.0) && (1..=8192).contains(&size.1) {
        Ok(())
    } else {
        Err(Error::Config(
            "window dimensions must be between 1 and 8192".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_releases_on_focus_loss_and_ignores_motion_at_focus_transitions() {
        let mut state = CursorState::default();
        assert_eq!(state.update(CursorMode::Captured, false), (false, false));
        assert_eq!(state.update(CursorMode::Captured, true), (true, false));
        assert!(state.captured);
        assert_eq!(state.update(CursorMode::Captured, true), (false, true));
        assert_eq!(state.update(CursorMode::Captured, false), (true, false));
        assert!(!state.captured);
        assert_eq!(state.update(CursorMode::Captured, true), (true, false));
        assert_eq!(state.update(CursorMode::Captured, true), (false, true));
        let mut free = CursorState::default();
        assert_eq!(free.update(CursorMode::Free, true), (false, false));
        assert_eq!(free.update(CursorMode::Free, true), (false, true));
        assert!(!free.captured);
    }

    #[test]
    fn bad_configuration_is_rejected_before_opening_window() {
        let mut config = Config::new("test");
        config.fixed_hz = 0;
        assert!(config.validate().is_err());
        config.fixed_hz = 120;
        config.reference_size.x = f32::NAN;
        assert!(config.validate().is_err());
        config.reference_size.x = 960.0;
        config.title = "bad\0title".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn run_arguments_are_strict_and_bounded() {
        let parse = |args: &[&str]| RunOptions::parse(args.iter().map(|s| s.to_string()));
        let options = parse(&[
            "--frames",
            "10",
            "--size",
            "800x600",
            "--screenshot",
            "frame.png",
            "--hidden",
        ])
        .unwrap();
        assert_eq!(options.frames, Some(10));
        assert_eq!(options.size, Some((800, 600)));
        assert!(options.hidden);
        for args in [
            &["--frames", "0"][..],
            &["--frames"],
            &["--size", "0x2"],
            &["--screenshot", "out.jpg"],
            &["--wat"],
        ] {
            assert!(parse(args).is_err());
        }
    }

    #[test]
    #[ignore = "requires a native display and OpenGL context; scripts/native_smoke.sh"]
    fn native_render_smoke() {
        use rayengine_core::{
            camera::{Camera2D as EngineCamera2D, Camera3D as EngineCamera3D},
            collision::{Aabb2, Aabb3},
            glam::Vec3,
        };
        let directory =
            std::env::temp_dir().join(format!("rayengine-render-smoke-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let png = Image::gen_image_color(8, 8, Color::GREEN)
            .export_image_to_memory(".png")
            .unwrap();
        std::fs::write(directory.join("green.png"), &*png).unwrap();
        std::fs::write(
            directory.join("triangle.obj"),
            "v -1 0 0\nv 1 0 0\nv 0 2 0\nf 1 2 3\n",
        )
        .unwrap();

        struct Probe {
            directory: PathBuf,
            texture: Option<TextureId>,
            model: Option<ModelId>,
        }
        impl Game for Probe {
            fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
                let path = self.directory.join("green.png");
                let first = context.texture(&path)?;
                assert_eq!(first, context.texture(&path)?);
                context.assets.unload_texture(first);
                let fresh = context.texture(&path)?;
                assert_ne!(fresh, first);
                assert!(context.assets.texture(first).is_none());
                self.texture = Some(fresh);
                let path = self.directory.join("triangle.obj");
                let model = context.model(&path)?;
                assert_eq!(model, context.model(&path)?);
                context.assets.unload_model(model);
                let fresh_model = context.model(&path)?;
                assert_ne!(fresh_model, model);
                assert!(context.assets.model(model).is_none());
                self.model = Some(fresh_model);
                assert!(context.texture(self.directory.join("missing.png")).is_err());
                assert!(context.sound(self.directory.join("missing.wav")).is_err());
                Ok(())
            }
            fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
            fn draw(&mut self, frame: &mut Frame<'_, '_>) {
                frame.clear(Color::WHITE);
                assert!((frame.viewport.aspect() - 16.0 / 9.0).abs() < 0.0001);
                if frame.index == 0 {
                    frame.world_2d(EngineCamera2D::default(), |canvas| {
                        assert!(canvas.texture(
                            self.texture.unwrap(),
                            Aabb2::from_center(Vec2::ZERO, Vec2::splat(100.0)),
                            Color::WHITE
                        ));
                    });
                } else {
                    frame.world_3d(EngineCamera3D::default(), |canvas| {
                        canvas.cube(
                            Aabb3::from_center(Vec3::ZERO, Vec3::splat(2.0)),
                            Color::BLUE,
                        );
                        assert!(canvas.model(
                            self.model.unwrap(),
                            Vec3::new(3.0, 0.0, 0.0),
                            1.0,
                            Color::RED
                        ));
                    });
                }
                frame.ui(|ui| {
                    ui.rectangle(
                        Aabb2 {
                            min: Vec2::splat(10.0),
                            max: Vec2::splat(60.0),
                        },
                        Color::RED,
                    )
                });
                let mut image = frame.target.texture().load_image().unwrap();
                image.flip_vertical();
                let center = image.get_color(image.width / 2, image.height / 2);
                if frame.index == 0 {
                    assert_eq!(center, Color::GREEN, "2D texture probe");
                } else {
                    assert!(
                        center.b > center.r.saturating_add(40),
                        "3D cube probe: {center:?}"
                    );
                }
                let ui_pixel = image.get_color(
                    (image.width as f32 * 30.0 / frame.viewport.logical_size.x) as i32,
                    (image.height as f32 * 30.0 / frame.viewport.logical_size.y) as i32,
                );
                assert!(
                    ui_pixel.r > 180 && ui_pixel.g < 100,
                    "scaled UI probe: {ui_pixel:?}"
                );
                if frame.index == 0 {
                    frame.raylib.set_window_size(800, 1000);
                }
                if frame.index == 2 {
                    assert_eq!(frame.raylib.get_screen_width(), 800);
                    assert_eq!(frame.raylib.get_screen_height(), 1000);
                    assert!(frame.viewport.origin.y > 200.0);
                }
            }
        }
        let mut config = Config::new("rayengine native render probe");
        config.window_size = (960, 540);
        config.vsync = false;
        let screenshot = directory.join("absolute-path.png");
        let report = App::new(config)
            .with_options(RunOptions {
                frames: Some(3),
                hidden: true,
                screenshot: Some(screenshot.clone()),
                ..RunOptions::default()
            })
            .run(Probe {
                directory: directory.clone(),
                texture: None,
                model: None,
            })
            .unwrap();
        assert_eq!(report.frames, 3);
        assert!(
            std::fs::read(&screenshot)
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
