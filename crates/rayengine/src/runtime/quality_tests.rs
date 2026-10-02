use super::*;
use rayengine_core::quality::AntiAliasing;

#[test]
fn quality_fails_before_window_initialization() {
    let mut config = Config::new("invalid quality");
    config.render_quality.render_scale = f32::NAN;
    assert!(matches!(config.validate(), Err(Error::Config(_))));
    config.render_quality.render_scale = 2.0;
    config.scale_mode = ScaleMode::IntegerFit;
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("IntegerFit")
    );
    config.scale_mode = ScaleMode::Fit;
    config.window_size = (8192, 8192);
    config.reference_size = Vec2::ONE;
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("dimensions")
    );
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_quality_runner_resize_letterbox_screenshot_and_error_teardown() {
    struct Probe;
    impl Game for Probe {
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            frame.clear(Color::WHITE);
            let logical = frame.viewport.logical_size;
            let point = logical * 0.5;
            let screen = frame.viewport.ui_to_screen(point);
            assert!(
                frame
                    .viewport
                    .screen_to_ui(screen)
                    .unwrap()
                    .abs_diff_eq(point, 0.001)
            );
            frame.ui(|ui| {
                assert_eq!(ui.logical_size, logical);
                ui.rectangle(
                    rayengine_core::collision::Aabb2 {
                        min: Vec2::ZERO,
                        max: logical,
                    },
                    Color::GREEN,
                );
            });
            if frame.index == 0 {
                frame.raylib.set_window_size(240, 320);
            }
            if frame.index >= 2 {
                assert!(frame.viewport.origin.y > 80.0);
                assert!(frame.viewport.screen_to_ui(Vec2::ZERO).is_none());
            }
        }
    }
    let directory = std::env::temp_dir().join(format!("rayengine-quality-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (i, quality) in [
        RenderQuality::default(),
        RenderQuality {
            anti_aliasing: AntiAliasing::Fxaa,
            ..Default::default()
        },
        RenderQuality {
            render_scale: 2.0,
            ..Default::default()
        },
        RenderQuality {
            render_scale: 2.0,
            anti_aliasing: AntiAliasing::Fxaa,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut config = Config::new("quality resize");
        config.window_size = (320, 180);
        config.reference_size = Vec2::new(320.0, 180.0);
        config.render_quality = quality;
        // Give native resize events time to settle before framebuffer readback.
        config.vsync = false;
        config.target_fps = 60;
        let path = directory.join(format!("{i}.png"));
        let report = App::new(config)
            .with_options(RunOptions {
                frames: Some(6),
                hidden: true,
                uncapped: false,
                screenshot: Some(path.clone()),
                diagnostics: Some(DiagnosticsConfig::new("quality-resize.v1")),
                ..Default::default()
            })
            .run(Probe)
            .unwrap();
        let metrics = report.diagnostics.unwrap();
        assert_eq!(metrics.settings.output_size, (240, 135));
        assert_eq!(
            metrics.settings.render_size,
            (
                (240.0 * quality.render_scale) as u32,
                (135.0 * quality.render_scale) as u32
            )
        );
        let screenshot = Image::load_image(path.to_str().unwrap()).unwrap();
        assert_eq!((screenshot.width, screenshot.height), (240, 320));
        assert_eq!(screenshot.get_color(120, 160), Color::GREEN);
        assert_eq!(screenshot.get_color(0, 0), Color::new(9, 14, 24, 255));
    }
    struct Oversize;
    impl Game for Oversize {
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            frame.raylib.set_window_size(5000, 100);
        }
    }
    let mut config = Config::new("resize failure");
    config.window_size = (320, 180);
    config.scale_mode = ScaleMode::Expand;
    config.render_quality.render_scale = 2.0;
    let error = App::new(config)
        .with_options(RunOptions {
            frames: Some(4),
            hidden: true,
            uncapped: true,
            ..Default::default()
        })
        .run(Oversize)
        .unwrap_err();
    assert!(error.to_string().contains("dimensions"));
    // A new run after the failed allocation exercises clean context/resource teardown.
    App::new(Config::new("after failure"))
        .with_options(RunOptions {
            frames: Some(1),
            hidden: true,
            uncapped: true,
            ..Default::default()
        })
        .run(Probe)
        .unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn checked_quality_manifest_profiles_and_rust_override_precedence() {
    let source = include_str!("../../examples/render_quality.toml");
    let directory =
        std::env::temp_dir().join(format!("rayengine-quality-manifest-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("rayengine.toml");
    std::fs::write(&path, source).unwrap();
    let manifest = crate::manifest::ProjectManifest::load(&path).unwrap();
    for (name, scale, aa, mode) in [
        ("native", 1.0, AntiAliasing::None, ScaleMode::Fit),
        ("fxaa", 1.0, AntiAliasing::Fxaa, ScaleMode::Fit),
        ("2x", 2.0, AntiAliasing::None, ScaleMode::Fit),
        ("2x-fxaa", 2.0, AntiAliasing::Fxaa, ScaleMode::Fit),
        ("pixel", 1.0, AntiAliasing::None, ScaleMode::IntegerFit),
    ] {
        let project = manifest.resolve(Some(name)).unwrap();
        let config = Config::new("profiles").with_project(&project).unwrap();
        assert_eq!(
            config.render_quality,
            RenderQuality {
                render_scale: scale,
                anti_aliasing: aa
            }
        );
        assert_eq!(config.scale_mode, mode);
    }
    std::fs::write(
        &path,
        "schema_version = 1\n[render]\nanti_aliasing = 'fxaa'",
    )
    .unwrap();
    let partial = crate::manifest::ProjectManifest::load(&path)
        .unwrap()
        .resolve(None)
        .unwrap();
    let mut rust = Config::new("rust");
    rust.render_quality.render_scale = 2.0;
    let config = rust.with_project(&partial).unwrap();
    assert_eq!(
        config.render_quality.render_scale, 2.0,
        "undeclared render scale retains Rust value"
    );
    assert_eq!(config.render_quality.anti_aliasing, AntiAliasing::Fxaa);
    let mut rust = Config::new("pixel");
    rust.scale_mode = ScaleMode::IntegerFit;
    assert!(
        rust.with_project(&partial)
            .unwrap_err()
            .to_string()
            .contains("IntegerFit")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
