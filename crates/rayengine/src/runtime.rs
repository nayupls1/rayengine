//! Window lifecycle: initialize once, sample input, fixed-update, draw, present.

use crate::{
    assets::{Assets, MaterialId, MeshId, ModelId, ShaderId, SoundId, TextureId, path_string},
    diagnostics::{DiagnosticsConfig, DiagnosticsReport, DrawCounters, RunSettings},
    input::{Bindings, SamplingState},
    material::{MaterialDesc, UniformId, UniformValue},
    render::{Frame, rect},
};
use rayengine_core::{
    glam::Vec2,
    input::Input,
    mesh::MeshData,
    quality::RenderQuality,
    time::{FixedClock, Tick},
    ui::{UiActions, UiInput},
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
    /// Offscreen world quality; default is native resolution without an edge filter.
    pub render_quality: RenderQuality,
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
    /// Native window-exit key. Defaults to Escape; `None` lets game UI bind
    /// Escape itself. The window close button and `Update::quit` still work.
    pub exit_key: Option<KeyboardKey>,
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
            render_quality: RenderQuality::default(),
            fixed_hz: 120,
            max_catch_up: 8,
            target_fps: 120,
            vsync: true,
            audio: false,
            exit_key: Some(KeyboardKey::KEY_ESCAPE),
            bar_color: Color::new(9, 14, 24, 255),
        }
    }

    /// Applies explicitly declared project/profile fields over these Rust defaults.
    /// Missing fields preserve the caller's values. RunOptions overrides apply last.
    pub fn with_project(
        mut self,
        project: &crate::manifest::ResolvedManifest,
    ) -> Result<Self, Error> {
        let settings = &project.settings;
        macro_rules! apply {
            ($section:literal, $field:ident, $value:expr) => {
                if project.is_declared($section, stringify!($field)) {
                    self.$field = $value;
                }
            };
        }
        apply!("window", title, settings.window.title.clone());
        if project.is_declared("window", "size") {
            self.window_size = (settings.window.size[0], settings.window.size[1]);
        }
        apply!(
            "render",
            reference_size,
            Vec2::from_array(settings.render.reference_size)
        );
        apply!(
            "render",
            scale_mode,
            match settings.render.scale_mode {
                crate::manifest::ScaleMode::Fit => ScaleMode::Fit,
                crate::manifest::ScaleMode::Expand => ScaleMode::Expand,
                crate::manifest::ScaleMode::IntegerFit => ScaleMode::IntegerFit,
            }
        );
        if project.is_declared("render", "render_scale") {
            self.render_quality.render_scale = settings.render.render_scale;
        }
        if project.is_declared("render", "anti_aliasing") {
            self.render_quality.anti_aliasing = settings.render.anti_aliasing;
        }
        apply!("render", target_fps, settings.render.target_fps);
        apply!("render", vsync, settings.render.vsync);
        let [r, g, b, a] = settings.render.bar_color;
        apply!("render", bar_color, Color::new(r, g, b, a));
        apply!("runtime", fixed_hz, settings.runtime.fixed_hz);
        apply!("runtime", max_catch_up, settings.runtime.max_catch_up);
        apply!("runtime", audio, settings.runtime.audio);
        self.validate()?;
        Ok(self)
    }

    /// Loads a project directory, Cargo manifest, or explicit rayengine.toml.
    /// Absent optional manifests preserve the existing manifest-free workflow.
    pub fn with_optional_project(
        self,
        path: impl AsRef<Path>,
        profile: Option<&str>,
    ) -> Result<Self, Error> {
        let manifest = crate::manifest::ProjectManifest::load_optional(path)
            .map_err(|e| Error::Config(e.to_string()))?;
        match manifest {
            Some(manifest) => self.with_project(
                &manifest
                    .resolve(profile)
                    .map_err(|e| Error::Config(e.to_string()))?,
            ),
            None if profile.is_some() => {
                Err(Error::Config("a profile requires rayengine.toml".into()))
            }
            None => Ok(self),
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
        let view = Viewport::new(
            Vec2::new(self.window_size.0 as f32, self.window_size.1 as f32),
            self.reference_size,
            self.scale_mode,
        )
        .expect("validated dimensions");
        self.render_quality
            .plan(&view, Vec2::ONE, self.scale_mode)
            .map_err(|e| Error::Config(e.to_string()))?;
        Ok(())
    }
}

/// Optional run controls for bounded native rendering smoke checks.
/// This is not the deferred interactive game-testing protocol.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Opt-in timings, submission counters and resource sampling. Disabled by default.
    pub diagnostics: Option<DiagnosticsConfig>,
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
    /// `--hidden`, `--uncapped`, `--diagnostics report.json`, and `--workload ID`.
    /// Either diagnostic flag enables collection. Unknown/incomplete arguments fail.
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
                "--diagnostics" => {
                    let path = args
                        .next()
                        .ok_or_else(|| Error::Config("--diagnostics needs a JSON path".into()))?;
                    options
                        .diagnostics
                        .get_or_insert_with(|| DiagnosticsConfig::new("unspecified"))
                        .output = Some(path.into());
                }
                "--workload" => {
                    let id = args
                        .next()
                        .ok_or_else(|| Error::Config("--workload needs a stable ID".into()))?;
                    options
                        .diagnostics
                        .get_or_insert_with(|| DiagnosticsConfig::new("unspecified"))
                        .workload = id;
                }
                "--hidden" => options.hidden = true,
                "--uncapped" => options.uncapped = true,
                _ => return Err(Error::Config(format!("unknown run option: {arg}"))),
            }
        }
        if let Some(diagnostics) = &options.diagnostics {
            diagnostics.validate()?;
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
#[derive(Clone, Debug, Default)]
pub struct RunReport {
    /// Optional constant-space performance summary; None when disabled.
    pub diagnostics: Option<DiagnosticsReport>,
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
    /// Loads/caches an outline font by canonical path and normalized options.
    pub fn font(
        &mut self,
        path: impl AsRef<Path>,
        options: crate::fonts::FontOptions,
    ) -> Result<crate::fonts::FontId, Error> {
        self.assets.load_font(self.thread, path.as_ref(), options)
    }

    /// Loads the selected project's resolved named fonts on the owning render thread.
    /// On error, previously loaded fonts remain cached for reuse or unloading.
    pub fn fonts(
        &mut self,
        project: &crate::manifest::ResolvedManifest,
    ) -> Result<std::collections::BTreeMap<String, crate::fonts::FontId>, Error> {
        project
            .settings
            .fonts
            .iter()
            .map(|(name, declaration)| {
                crate::fonts::FontOptions::try_from(declaration)
                    .and_then(|options| self.font(&declaration.path, options))
                    .map(|id| (name.clone(), id))
                    .map_err(|e| {
                        Error::Asset(format!(
                            "font '{name}' ({}): {e}",
                            declaration.path.display()
                        ))
                    })
            })
            .collect()
    }
    /// Creates an owned material description with borrowed resource dependencies.
    pub fn material(&mut self, desc: MaterialDesc) -> Result<MaterialId, Error> {
        self.assets.create_material(self.raylib, self.thread, desc)
    }
    /// Compiles custom GLSL; None uses raylib's standard mesh vertex shader.
    pub fn shader_from_source(
        &mut self,
        vertex: Option<&str>,
        fragment: &str,
    ) -> Result<ShaderId, Error> {
        self.assets
            .shader_source(self.raylib, self.thread, vertex, fragment)
    }
    /// Loads/caches shader files. None uses the standard mesh vertex shader.
    pub fn shader(
        &mut self,
        vertex: Option<&Path>,
        fragment: impl AsRef<Path>,
    ) -> Result<ShaderId, Error> {
        self.assets
            .load_shader(self.raylib, self.thread, vertex, fragment.as_ref())
    }
    /// Registers a cached binding after checking the active GLSL type.
    pub fn uniform(
        &mut self,
        shader: ShaderId,
        name: &str,
        initial: UniformValue,
    ) -> Result<UniformId, Error> {
        self.assets.uniform(shader, name, initial)
    }

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
    /// Uploads a CPU image into a new run-owned texture without filesystem I/O.
    /// Does not cache by image identity. The image may be dropped immediately
    /// afterward; explicit unloading permanently invalidates the returned handle.
    pub fn texture_from_image(
        &mut self,
        image: &raylib::prelude::Image,
    ) -> Result<TextureId, Error> {
        self.assets
            .upload_texture_image(self.raylib, self.thread, image)
    }
    /// Loads/caches a model by canonical path.
    pub fn model(&mut self, path: impl AsRef<Path>) -> Result<ModelId, Error> {
        self.assets
            .load_model(self.raylib, self.thread, path.as_ref())
    }
    /// Loads/caches every skeletal clip in a glTF/GLB, IQM or M3D file by
    /// canonical path and rate. The rate states how the backend sampled the
    /// file: [`KeyframeRate::GLTF`](crate::core::skeletal::KeyframeRate::GLTF),
    /// [`KeyframeRate::M3D`](crate::core::skeletal::KeyframeRate::M3D), or the
    /// authored rate for IQM. Fails if the file has no clips or if any clip is
    /// invalid (no keyframes, missing pose rows or nonfinite transforms).
    pub fn model_animations(
        &mut self,
        path: impl AsRef<Path>,
        rate: crate::core::skeletal::KeyframeRate,
    ) -> Result<crate::assets::ModelAnimationsId, Error> {
        self.assets
            .load_model_animations(self.raylib, self.thread, path.as_ref(), rate)
    }
    /// Loads/caches a streamed music track. Requires `Config::audio = true`.
    pub fn music(&mut self, path: impl AsRef<Path>) -> Result<crate::audio::MusicId, Error> {
        self.assets.load_music(path.as_ref())
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
    /// Live controls. Changes are reconciled after this tick; affected values
    /// become neutral until the next render-frame sample. Unchanged inputs and
    /// pending edges are preserved. Invalid edits return errors without changes.
    /// Assigning an invalid set directly makes [`App::run`] return a configuration
    /// error after this callback, before sampling the new set.
    pub bindings: &'context mut Bindings,
    /// Current viewport, shared with UI and cameras.
    pub viewport: Viewport,
    /// Pointer in UI units, or `None` in bars, while captured, or while unfocused.
    pub pointer: Option<Vec2>,
    /// Whether the window is focused and can accept UI input.
    pub window_focused: bool,
    /// Loaded assets, including sound playback.
    pub assets: &'context Assets<'audio>,
    pub(crate) quit: &'context mut bool,
}

impl Update<'_, '_> {
    /// Samples UI actions using this tick's mapped pointer and window focus.
    pub fn ui_input(&self, actions: UiActions) -> UiInput {
        UiInput::from_actions(self.input, self.pointer, actions, self.window_focused)
    }
    /// Requests exit after the current update batch and final presentation.
    pub fn quit(&mut self) {
        *self.quit = true;
    }
}

/// Shared game lifecycle for 2D, 3D, or mixed games.
pub trait Game {
    /// Chooses the current cursor policy. Re-read before input sampling and after
    /// fixed updates, so opening/closing menus can release/recapture the cursor.
    /// Capture is suspended on focus loss; transition motion is discarded.
    fn cursor_mode(&self) -> CursorMode {
        CursorMode::Free
    }
    /// Declares initial button and analog bindings before window creation.
    /// Change live controls through [`Update::bindings`].
    fn bindings(&self) -> Bindings {
        Bindings::new()
    }
    /// Loads resources once, before the first simulation tick.
    fn init(&mut self, _context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        Ok(())
    }
    /// Applies deferred lifecycle work after each fixed update and each draw,
    /// outside render passes. Asset creation and teardown are safe here.
    /// Errors stop the run; `shutdown` still runs while the backend is alive.
    fn boundary(&mut self, _context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        Ok(())
    }
    /// Teardown while assets, audio, and graphics are alive. Called once after
    /// initialization is attempted, on both successful and error returns.
    /// Must tolerate partially completed initialization.
    fn shutdown(&mut self, _context: &mut InitContext<'_, '_>) {}
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
    discard_motion: bool,
}

impl CursorState {
    fn sync(&mut self, mode: CursorMode, focused: bool) -> bool {
        let capture = focused && mode == CursorMode::Captured;
        let changed = capture != self.captured;
        self.discard_motion |= changed || focused != self.focused;
        self.focused = focused;
        self.captured = capture;
        changed
    }

    fn take_motion(&mut self) -> bool {
        let discard = std::mem::take(&mut self.discard_motion);
        self.focused && !discard
    }

    #[cfg(test)]
    fn update(&mut self, mode: CursorMode, focused: bool) -> (bool, bool) {
        let changed = self.sync(mode, focused);
        (changed, self.take_motion())
    }
}

fn sync_cursor(
    raylib: &mut RaylibHandle,
    state: &mut CursorState,
    mode: CursorMode,
    focused: bool,
) {
    if state.sync(mode, focused) {
        if state.captured {
            raylib.disable_cursor();
        } else {
            raylib.enable_cursor();
        }
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
        let mut bindings = game.bindings();
        bindings.validate()?;
        if let Some(diagnostics) = &options.diagnostics {
            diagnostics.validate()?;
        }
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
            builder.hidden();
        }
        if options.hidden || config.audio {
            // GLFW otherwise waits for events while minimized, starving streamed
            // audio. The loop below still pauses simulation and throttles polling.
            builder.always_run();
        }
        let (mut raylib, thread) = builder.build();
        if !options.hidden && !config.audio {
            // raylib retains flags across windows. Do not carry audio's polling
            // policy into a subsequent audio-disabled run in the same process.
            raylib.clear_window_state(WindowState::default().set_window_always_run(true));
        }
        raylib.set_exit_key(config.exit_key);
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
        let mut previous_bindings = bindings.clone();
        let mut sampling = SamplingState::default();
        let (actions, axes) = bindings.capacities();
        let mut input = Input::with_capacities(actions, axes);
        let initialized = game.init(&mut InitContext {
            raylib: &mut raylib,
            thread: &thread,
            assets: &mut assets,
        });
        let result = initialized.and_then(|()| {
            let mut cursor = CursorState::default();
            let mut fxaa = crate::quality::shader(&mut raylib, &thread, config.render_quality)?;
            let mut targets: Option<crate::quality::QualityTargets> = None;
            let mut post_targets: Option<crate::post_processing::PostTargets> = None;
            let mut clock = FixedClock::new(config.fixed_hz, config.max_catch_up);
            let start = Instant::now();
            let mut previous_frame = start;
            let mut report = RunReport {
                diagnostics: options.diagnostics.as_ref().map(|diagnostics| {
                    DiagnosticsReport::new(
                        diagnostics,
                        RunSettings {
                            backend: if cfg!(target_os = "windows") {
                                "glfw-win32"
                            } else if cfg!(target_os = "macos") {
                                "glfw-cocoa"
                            } else if cfg!(all(unix, not(target_vendor = "apple")))
                                && cfg!(feature = "wayland")
                            {
                                "glfw-x11+wayland"
                            } else if cfg!(all(unix, not(target_vendor = "apple"))) {
                                "glfw-x11"
                            } else {
                                "glfw"
                            },
                            os: std::env::consts::OS,
                            arch: std::env::consts::ARCH,
                            sdk_version: env!("CARGO_PKG_VERSION"),
                            graphics: crate::quality::graphics_info(&thread),
                            window_size: config.window_size,
                            reference_size: config.reference_size.to_array(),
                            scale_mode: match config.scale_mode {
                                ScaleMode::Fit => "fit",
                                ScaleMode::Expand => "expand",
                                ScaleMode::IntegerFit => "integer-fit",
                            },
                            fixed_hz: config.fixed_hz,
                            max_catch_up: config.max_catch_up,
                            target_fps: if options.uncapped {
                                0
                            } else {
                                config.target_fps
                            },
                            vsync: config.vsync && !options.uncapped,
                            render_size: (0, 0),
                            render_scale: config.render_quality.render_scale,
                            anti_aliasing: match config.render_quality.anti_aliasing {
                                rayengine_core::quality::AntiAliasing::None => "none",
                                rayengine_core::quality::AntiAliasing::Fxaa => "fxaa",
                            },
                            output_size: (0, 0),
                            render_target_bytes: 0,
                        },
                        assets.resource_counts(),
                    )
                }),
                ..RunReport::default()
            };
            let mut quit = false;
            let mut last_view = None;
            while !quit
                && !raylib.window_should_close()
                && options.frames.is_none_or(|limit| report.frames < limit)
            {
                let now = Instant::now();
                let elapsed = now.duration_since(previous_frame);
                previous_frame = now;
                // Audio follows wall time, including minimized iterations, independently
                // of the fixed clock and state-stack update propagation.
                assets.update_audio(elapsed);
                let window = Vec2::new(
                    raylib.get_screen_width() as f32,
                    raylib.get_screen_height() as f32,
                );
                let view = Viewport::new(window, config.reference_size, config.scale_mode);
                if raylib.is_window_minimized() || view.is_none() {
                    sync_cursor(&mut raylib, &mut cursor, game.cursor_mode(), false);
                    input.release_all();
                    sampling = SamplingState::default();
                    // Keep backend event polling alive, but pause simulation while minimized.
                    raylib
                        .begin_drawing(&thread)
                        .clear_background(config.bar_color);
                    std::thread::sleep(Duration::from_millis(16));
                    continue;
                }
                let view = view.expect("non-minimized viewport");
                let window_focused = raylib.is_window_focused();
                sync_cursor(&mut raylib, &mut cursor, game.cursor_mode(), window_focused);
                bindings.sample(&raylib, &mut input, &mut sampling);
                if cursor.take_motion() {
                    let delta = raylib.get_mouse_delta();
                    input.add_pointer_delta(Vec2::new(delta.x, delta.y));
                }
                if window_focused {
                    let wheel = raylib.get_mouse_wheel_move_v();
                    input.add_scroll_delta(Vec2::new(wheel.x, wheel.y));
                }
                let mouse = raylib.get_mouse_position();
                let plan = clock.advance(elapsed);
                report.dropped_time += plan.dropped;
                if let Some(metrics) = &mut report.diagnostics {
                    metrics.dropped_ns = metrics
                        .dropped_ns
                        .saturating_add(plan.dropped.as_nanos().min(u128::from(u64::MAX)) as u64);
                }
                for step in 0..plan.steps {
                    let update_start = report.diagnostics.as_ref().map(|_| Instant::now());
                    game.fixed_update(&mut Update {
                        tick: Tick {
                            index: plan.first_tick + u64::from(step),
                            dt: clock.step().as_secs_f32(),
                        },
                        input: &input,
                        bindings: &mut bindings,
                        viewport: view,
                        pointer: (window_focused && !cursor.captured)
                            .then(|| view.screen_to_ui(Vec2::new(mouse.x, mouse.y)))
                            .flatten(),
                        window_focused,
                        assets: &assets,
                        quit: &mut quit,
                    });
                    if let Some(metrics) = &mut report.diagnostics {
                        metrics
                            .update
                            .record(update_start.expect("diagnostics enabled").elapsed());
                        metrics.updates = metrics.updates.saturating_add(1);
                    }
                    game.boundary(&mut InitContext {
                        raylib: &mut raylib,
                        thread: &thread,
                        assets: &mut assets,
                    })?;
                    input.consume_edges();
                    bindings.reconcile(&mut previous_bindings, &mut input)?;
                    sync_cursor(&mut raylib, &mut cursor, game.cursor_mode(), window_focused);
                    report.ticks += 1;
                    if quit {
                        break;
                    }
                }
                let render_start = report.diagnostics.as_ref().map(|_| Instant::now());
                let dpi = Vec2::new(
                    raylib.get_render_width() as f32,
                    raylib.get_render_height() as f32,
                ) / window;
                let mut target_plan = config
                    .render_quality
                    .plan(&view, dpi, config.scale_mode)
                    .map_err(|e| Error::Config(e.to_string()))?;
                let chain = assets.post_processing().clone();
                assets.validate_post_processing(&chain)?;
                let effects = !chain.materials.is_empty();
                if effects && !target_plan.separate_ui {
                    target_plan.separate_ui = true;
                    target_plan.target_bytes +=
                        16 * u64::from(target_plan.output.0) * u64::from(target_plan.output.1);
                }
                let post_bytes = if effects {
                    crate::post_processing::PostTargets::bytes(target_plan.output)
                } else {
                    0
                };
                assets.target_reservation = target_plan.target_bytes + post_bytes;
                let all_bytes =
                    assets.targets.planned_bytes(&view, dpi)? + assets.target_reservation;
                if all_bytes > RenderQuality::MAX_TARGET_BYTES {
                    return Err(Error::Config(format!(
                        "render targets need {all_bytes} bytes; limit is {}",
                        RenderQuality::MAX_TARGET_BYTES
                    )));
                }
                if !effects
                    || post_targets
                        .as_ref()
                        .is_some_and(|t| t.size != target_plan.output)
                {
                    drop(post_targets.take());
                }
                assets.targets.release_resized(&view, dpi)?;
                if targets
                    .as_ref()
                    .is_none_or(|targets| targets.plan != target_plan)
                {
                    // Drop old targets before allocating to keep the checked memory bound.
                    // Any failure exits cleanly while the graphics context remains alive.
                    drop(targets.take());
                    targets = Some(crate::quality::QualityTargets::new(
                        &mut raylib,
                        &thread,
                        target_plan,
                        config.scale_mode == ScaleMode::IntegerFit,
                    )?);
                }
                assets
                    .targets
                    .sync(&mut raylib, &thread, &view, dpi, assets.target_reservation)?;
                if effects && post_targets.is_none() {
                    post_targets = Some(crate::post_processing::PostTargets::new(
                        &mut raylib,
                        &thread,
                        target_plan.output,
                        config.scale_mode == ScaleMode::IntegerFit,
                    )?);
                }
                let targets = targets.as_mut().expect("created targets");
                let size = target_plan.world;
                let draws = {
                    let mut frame = Frame {
                        counters: report.diagnostics.as_ref().map(|_| DrawCounters::default()),
                        raylib: &mut raylib,
                        thread: &thread,
                        target: &mut targets.world,
                        ui_target: targets.ui.as_mut(),
                        assets: &mut assets,
                        viewport: view,
                        alpha: plan.alpha,
                        index: report.frames,
                        delta: elapsed,
                    };
                    frame.clear(Color::BLACK);
                    game.draw(&mut frame);
                    frame.draw_counters()
                };
                assets.validate_post_processing(&chain)?;
                if effects {
                    targets.resolve_with_ui(
                        &mut raylib,
                        &thread,
                        fxaa.as_mut(),
                        chain.ui == crate::post_processing::UiPlacement::BeforeEffects,
                    );
                    post_targets.as_mut().expect("effect targets").apply(
                        &mut raylib,
                        &thread,
                        targets.presented(),
                        targets.ui.as_ref(),
                        &chain,
                        &mut assets,
                    )?;
                } else {
                    targets.resolve(&mut raylib, &thread, fxaa.as_mut());
                }
                game.boundary(&mut InitContext {
                    raylib: &mut raylib,
                    thread: &thread,
                    assets: &mut assets,
                })?;
                let presented = post_targets
                    .as_ref()
                    .map(|t| t.presented())
                    .unwrap_or_else(|| targets.presented());
                if let Some(metrics) = &mut report.diagnostics {
                    metrics
                        .render
                        .record(render_start.expect("diagnostics enabled").elapsed());
                }
                let present_start = report.diagnostics.as_ref().map(|_| Instant::now());
                present_frame(
                    &mut raylib,
                    &thread,
                    presented,
                    view,
                    config.bar_color,
                    false,
                );
                last_view = Some(view);
                if let Some(metrics) = &mut report.diagnostics {
                    metrics
                        .present
                        .record(present_start.expect("diagnostics enabled").elapsed());
                    metrics.settings.render_size = size;
                    metrics.settings.output_size = target_plan.output;
                    metrics.settings.render_target_bytes =
                        assets.target_reservation + assets.render_target_usage().1;
                    metrics.record_frame(draws.unwrap_or_default(), assets.resource_counts());
                    metrics.frame.record(now.elapsed());
                }
                report.frames += 1;
            }
            if let Some(path) = &options.screenshot {
                // Re-submit the last resolved image after final event polling. This
                // avoids reading a stale back buffer or a pending-resize drawable.
                let image = if let Some((targets, view)) = targets.as_ref().zip(last_view) {
                    let source = post_targets
                        .as_ref()
                        .map(|t| t.presented())
                        .unwrap_or_else(|| targets.presented());
                    present_frame(&mut raylib, &thread, source, view, config.bar_color, true)
                        .expect("requested capture")
                } else {
                    raylib.load_image_from_screen(&thread)
                };
                let png = image
                    .export_image_to_memory(".png")
                    .map_err(|e| Error::Backend(e.to_string()))?;
                std::fs::write(path, &*png)?;
            }
            report.elapsed = start.elapsed();
            if let Some((path, metrics)) = options
                .diagnostics
                .as_ref()
                .and_then(|d| d.output.as_ref())
                .zip(report.diagnostics.as_ref())
            {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent)?;
                }
                let file = std::fs::File::create(path)?;
                let mut writer = std::io::BufWriter::new(file);
                metrics
                    .write_json(&mut writer)
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                std::io::Write::flush(&mut writer)?;
            }
            Ok(report)
        });
        game.shutdown(&mut InitContext {
            raylib: &mut raylib,
            thread: &thread,
            assets: &mut assets,
        });
        result
    }
}

fn present_frame(
    rl: &mut RaylibHandle,
    thread: &RaylibThread,
    source: &RenderTexture2D,
    view: Viewport,
    bars: Color,
    capture: bool,
) -> Option<Image> {
    let mut draw = rl.begin_drawing(thread);
    draw.clear_background(bars);
    let mut presented = draw.begin_blend_mode(BlendMode::BLEND_ALPHA_PREMULTIPLY);
    presented.draw_texture_pro(
        source.texture(),
        Rectangle::new(
            0.0,
            0.0,
            source.texture().width as f32,
            -(source.texture().height as f32),
        ),
        rect(rayengine_core::collision::Aabb2 {
            min: view.origin,
            max: view.origin + view.size,
        }),
        Vector2::zero(),
        0.0,
        Color::WHITE,
    );
    // EndBlendMode flushes the blit before readback and EndDrawing swaps buffers.
    drop(presented);
    capture.then(|| draw.load_image_from_screen(thread))
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
    fn manifest_examples_and_runtime_defaults_agree() {
        let path =
            std::env::temp_dir().join(format!("rayengine-runtime-manifest-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let documentation = include_str!("../docs/project_manifest.md");
        for block in documentation.split("```toml\n").skip(1) {
            let source = block.split("```").next().unwrap();
            std::fs::write(path.join("rayengine.toml"), source).unwrap();
            let manifest =
                crate::manifest::ProjectManifest::load(path.join("rayengine.toml")).unwrap();
            let project = manifest.resolve(None).unwrap();
            let config = Config::new("rayengine game")
                .with_project(&project)
                .unwrap();
            assert_eq!(
                config.window_size,
                (
                    project.settings.window.size[0],
                    project.settings.window.size[1]
                )
            );
            assert_eq!(
                config.reference_size.to_array(),
                project.settings.render.reference_size
            );
            assert_eq!(config.target_fps, project.settings.render.target_fps);
            assert_eq!(config.vsync, project.settings.render.vsync);
            assert_eq!(config.fixed_hz, project.settings.runtime.fixed_hz);
            assert_eq!(config.max_catch_up, project.settings.runtime.max_catch_up);
            assert_eq!(config.audio, project.settings.runtime.audio);
            for profile in manifest.profiles() {
                manifest.resolve(Some(profile)).unwrap();
            }
        }
        std::fs::write(path.join("rayengine.toml"), "schema_version = 1\n[window]\nsize = [640, 480]\n[profiles.dev.render]\ntarget_fps = 30").unwrap();
        let mut rust = Config::new("Custom title");
        rust.fixed_hz = 60;
        rust.audio = true;
        let config = rust.with_optional_project(&path, Some("dev")).unwrap();
        assert_eq!(config.title, "Custom title");
        assert_eq!(config.window_size, (640, 480));
        assert_eq!(config.target_fps, 30);
        assert_eq!(config.fixed_hz, 60);
        assert!(config.audio);
        assert!(
            Config::new("Test")
                .with_optional_project(&path, Some("missing"))
                .is_err()
        );
        std::fs::remove_file(path.join("rayengine.toml")).unwrap();
        assert_eq!(
            Config::new("No manifest")
                .with_optional_project(&path, None)
                .unwrap()
                .title,
            "No manifest"
        );
        assert!(
            Config::new("No manifest")
                .with_optional_project(&path, Some("dev"))
                .is_err()
        );
        std::fs::remove_dir(path).unwrap();
    }

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
    fn menu_transitions_discard_motion_even_when_changed_between_samples() {
        let mut state = CursorState::default();
        assert!(state.sync(CursorMode::Captured, true));
        assert!(!state.take_motion());
        assert!(state.take_motion());
        assert!(state.sync(CursorMode::Free, true));
        assert!(!state.captured);
        assert!(state.sync(CursorMode::Captured, true));
        assert!(!state.take_motion()); // Open/close between samples still discarded.
        assert!(state.take_motion());
        assert!(state.sync(CursorMode::Captured, false));
        assert!(!state.take_motion());
        assert!(state.sync(CursorMode::Captured, true));
        assert!(!state.take_motion());
        assert!(state.take_motion());
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
    fn invalid_initial_bindings_fail_before_native_window_creation() {
        struct InvalidBindings;
        impl Game for InvalidBindings {
            fn bindings(&self) -> Bindings {
                Bindings::new().bind(
                    rayengine_core::input::Action(0),
                    crate::input::Button::Gamepad {
                        device: -1,
                        button: GamepadButton::GAMEPAD_BUTTON_LEFT_THUMB,
                    },
                )
            }
            fn fixed_update(&mut self, _: &mut Update<'_, '_>) {
                panic!("must reject before updating");
            }
            fn draw(&mut self, _: &mut Frame<'_, '_>) {
                panic!("must reject before drawing");
            }
        }
        assert!(matches!(
            App::new(Config::new("invalid bindings")).run(InvalidBindings),
            Err(Error::Config(_))
        ));
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
                let image = Image::gen_image_color(8, 8, Color::GREEN);
                let generated = context.texture_from_image(&image)?;
                assert_eq!(context.assets.texture(generated).unwrap().width, 8);
                context.assets.unload_texture(generated);
                let replacement = context.texture_from_image(&image)?;
                assert_ne!(replacement, generated);
                assert!(context.assets.texture(generated).is_none());
                context.assets.unload_texture(fresh);
                // The CPU image is dropped after init; the uploaded texture survives.
                self.texture = Some(replacement);
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

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod diagnostics_tests;

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod sprite_tests;

#[cfg(test)]
mod quality_tests;
