use super::*;
use crate::{
    assets::Assets,
    render::{Frame, UiButtonStyle},
};
use rayengine_core::{
    ui::{UiId, UiInput, UiRegion, UiState},
    viewport::{ScaleMode, Viewport},
};

fn ink(image: &Image, region: Aabb2) -> Option<Aabb2> {
    let mut found = None::<Aabb2>;
    for y in region.min.y.max(0.0) as i32..(region.max.y.ceil() as i32).min(image.height) {
        for x in region.min.x.max(0.0) as i32..(region.max.x.ceil() as i32).min(image.width) {
            if image.get_color(x, y).r > 20 {
                let point = Vec2::new(x as f32, y as f32);
                found = Some(match found {
                    None => Aabb2 {
                        min: point,
                        max: point + Vec2::ONE,
                    },
                    Some(bounds) => Aabb2 {
                        min: bounds.min.min(point),
                        max: bounds.max.max(point + Vec2::ONE),
                    },
                });
            }
        }
    }
    found
}

#[test]
#[ignore = "requires a native display/OpenGL context; scripts/native_smoke.sh runs serially"]
fn native_font_bounds_scales_quality_cache_and_unload() {
    let (mut raylib, thread) = raylib::init()
        .size(960, 540)
        .hidden()
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let mut assets = Assets::new(None);
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fonts");
    let path = directory.join("LiberationSans-Regular.ttf");
    let smooth = assets
        .load_font(&thread, &path, FontOptions::default())
        .unwrap();
    assert_eq!(
        assets
            .load_font(&thread, &path, FontOptions::default())
            .unwrap(),
        smooth
    );
    assert!(
        assets
            .load_font(
                &thread,
                &directory.join("missing.ttf"),
                FontOptions::default()
            )
            .is_err()
    );
    let corrupt =
        std::env::temp_dir().join(format!("rayengine-bad-font-{}.ttf", std::process::id()));
    std::fs::write(&corrupt, "invalid font").unwrap();
    assert!(
        assets
            .load_font(&thread, &corrupt, FontOptions::default())
            .is_err()
    );
    std::fs::write(
        &corrupt,
        include_bytes!("../../examples/fonts/LiberationSans-Regular.ttf"),
    )
    .unwrap();
    let recovered = assets
        .load_font(&thread, &corrupt, FontOptions::default())
        .unwrap();
    assets.unload_font(recovered);
    std::fs::remove_file(corrupt).unwrap();
    let fixed = assets
        .load_font(
            &thread,
            &path,
            FontOptions {
                raster_size: 16,
                rasterization: FontRasterization::Fixed,
                ..Default::default()
            },
        )
        .unwrap();
    let nearest = assets
        .load_font(
            &thread,
            &path,
            FontOptions {
                raster_size: 16,
                rasterization: FontRasterization::Fixed,
                sampling: FontSampling::Nearest,
                ..Default::default()
            },
        )
        .unwrap();
    let pixel = assets
        .load_font(
            &thread,
            &directory.join("PressStart2P-Regular.ttf"),
            FontOptions {
                raster_size: 16,
                sampling: FontSampling::Nearest,
                rasterization: FontRasterization::Fixed,
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(fixed, nearest);
    let counts = assets.resource_counts();
    assert_eq!(counts.fonts, 4);
    assert_eq!(counts.font_atlases, 4);
    assert!(counts.font_bytes > 0);
    let logical = Vec2::new(960.0, 540.0);
    let view = Viewport::new(logical, logical, ScaleMode::Fit).unwrap();
    let artifact_directory = std::env::var_os("RAYENGINE_FONT_ARTIFACTS").map(PathBuf::from);
    if let Some(path) = &artifact_directory {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut records = Vec::new();
    for (name, display_scale, dpi, quality) in [
        ("small", 0.75, 1.0, 1.0),
        ("native", 1.0, 1.0, 1.0),
        ("fractional", 1.5, 1.0, 1.0),
        ("dpi2", 1.0, 2.0, 1.0),
        ("supersampled2", 1.0, 1.0, 2.0),
        ("dpi2-supersampled2", 1.0, 2.0, 2.0),
        ("return-native", 1.0, 1.0, 1.0),
    ] {
        let scale = display_scale * dpi * quality;
        let (width, height) = ((logical.x * scale) as u32, (logical.y * scale) as u32);
        let mut target = raylib.load_render_texture(&thread, width, height).unwrap();
        let mut probes = Vec::new();
        let before = assets.resource_counts();
        {
            let mut frame = Frame {
                ui_target: None,
                counters: None,
                raylib: &mut raylib,
                thread: &thread,
                target: &mut target,
                assets: &mut assets,
                viewport: view,
                alpha: 0.0,
                index: 0,
                delta: std::time::Duration::ZERO,
            };
            frame.clear(Color::BLACK);
            frame.ui(|ui| {
                for (column, font) in [smooth, fixed, nearest, pixel].into_iter().enumerate() {
                    for (row, size) in [16.0, 24.0, 40.0].into_iter().enumerate() {
                        let position =
                            Vec2::new(12.0 + column as f32 * 240.0, 12.0 + row as f32 * 145.0);
                        let text = if font == pixel { "Ag?" } else { "Agj W?" };
                        let style = TextStyle {
                            spacing: 1.25,
                            ..TextStyle::new(font, size)
                        };
                        let measured = ui.measure_text(text, style).unwrap();
                        let drawn = ui.text_with(text, position, style, Color::WHITE).unwrap();
                        assert_eq!(measured, drawn);
                        probes.push((position, measured, size));
                        let bounds = Aabb2 {
                            min: position + Vec2::new(0.0, 70.0),
                            max: position + Vec2::new(205.0, 120.0),
                        };
                        let mut state = UiState::with_capacity(1);
                        state.update(&[UiRegion::new(UiId(1), bounds)], UiInput::default());
                        ui.try_button(
                            bounds,
                            "Ag?",
                            state.response(UiId(1)).unwrap(),
                            UiButtonStyle {
                                font: Some(font),
                                font_size: size,
                                normal: Color::BLACK,
                                hovered: Color::BLACK,
                                spacing: 1.25,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                    }
                }
                let style = TextStyle::new(smooth, 20.0);
                let fallback = ui
                    .text_with("A🦀\nB", Vec2::new(12.0, 455.0), style, Color::WHITE)
                    .unwrap();
                assert_eq!(fallback.missing_glyphs, 1);
                assert!(
                    ui.text_with("NUL\0", Vec2::ZERO, style, Color::RED)
                        .is_err()
                );
                if scale > 1.0 {
                    assert!(
                        ui.text_with("big", Vec2::ZERO, TextStyle::new(smooth, 512.0), Color::RED)
                            .is_err()
                    );
                }
            });
        }
        let mut image = target.texture().load_image().unwrap();
        image.flip_vertical();
        for (position, measured, size) in probes {
            let expected = Aabb2 {
                min: (position + measured.ink_bounds.min) * scale,
                max: (position + measured.ink_bounds.max) * scale,
            };
            let region = Aabb2 {
                min: position * scale,
                max: (position + Vec2::new(230.0, 65.0)) * scale,
            };
            let actual = ink(&image, region).expect("label must render visible ink");
            // Fixed 16px atlases have up to one source-pixel padding when enlarged.
            let tolerance = (size / 16.0 * scale).max(2.0) + 1.0;
            assert!(
                actual.min.abs_diff_eq(expected.min, tolerance),
                "{name}: min actual={actual:?} expected={expected:?}"
            );
            assert!(
                actual.max.abs_diff_eq(expected.max, tolerance),
                "{name}: max actual={actual:?} expected={expected:?}"
            );
            let bounds = Aabb2 {
                min: position + Vec2::new(0.0, 70.0),
                max: position + Vec2::new(205.0, 120.0),
            };
            let button_ink = ink(
                &image,
                Aabb2 {
                    min: bounds.min * scale,
                    max: bounds.max * scale,
                },
            )
            .unwrap();
            assert!(
                button_ink
                    .center()
                    .abs_diff_eq(bounds.center() * scale, tolerance),
                "{name}: button ink must be centered"
            );
        }
        assert!(
            assets
                .fonts
                .atlas_sizes(smooth)
                .iter()
                .any(|size| *size as f32 >= 40.0 * scale)
        );
        assert_eq!(assets.fonts.atlas_sizes(fixed), vec![16]);
        assert_eq!(assets.fonts.atlas_sizes(nearest), vec![16]);
        let after = assets.resource_counts();
        if name == "return-native" {
            assert_eq!(before.font_atlases, after.font_atlases);
        }
        records.push(serde_json::json!({"name":name,"display_scale":display_scale,"physical_dpi":dpi,"render_scale":quality,"target":[width,height],"atlases":assets.fonts.atlas_sizes(smooth),"font_bytes":after.font_bytes,"new_atlases":after.font_atlases-before.font_atlases}));
        if let Some(path) = &artifact_directory {
            let png = image.export_image_to_memory(".png").unwrap();
            std::fs::write(path.join(format!("{name}.png")), &*png).unwrap();
        }
        // Use the runner's flipped, bilinear presentation, including 2x
        // downsampling to physical display resolution.
        let physical_scale = display_scale * dpi;
        let mut presented = raylib
            .load_render_texture(
                &thread,
                (logical.x * physical_scale) as u32,
                (logical.y * physical_scale) as u32,
            )
            .unwrap();
        target
            .texture()
            .set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
        {
            let mut draw = raylib.begin_texture_mode(&thread, &mut presented);
            draw.clear_background(Color::BLACK);
            draw.draw_texture_pro(
                target.texture(),
                Rectangle::new(0.0, 0.0, width as f32, -(height as f32)),
                Rectangle::new(
                    0.0,
                    0.0,
                    logical.x * physical_scale,
                    logical.y * physical_scale,
                ),
                Vector2::zero(),
                0.0,
                Color::WHITE,
            );
        }
        let mut final_image = presented.texture().load_image().unwrap();
        final_image.flip_vertical();
        let measured = assets
            .measure_text(
                "Agj W?",
                TextStyle {
                    spacing: 1.25,
                    ..TextStyle::new(smooth, 40.0)
                },
            )
            .unwrap();
        let position = Vec2::new(12.0, 302.0);
        let actual = ink(
            &final_image,
            Aabb2 {
                min: position * physical_scale,
                max: (position + Vec2::new(230.0, 65.0)) * physical_scale,
            },
        )
        .unwrap();
        let expected = Aabb2 {
            min: (position + measured.ink_bounds.min) * physical_scale,
            max: (position + measured.ink_bounds.max) * physical_scale,
        };
        assert!(actual.min.abs_diff_eq(expected.min, 2.0));
        assert!(actual.max.abs_diff_eq(expected.max, 2.0));
        if let Some(path) = &artifact_directory {
            let png = final_image.export_image_to_memory(".png").unwrap();
            std::fs::write(path.join(format!("{name}-presented.png")), &*png).unwrap();
        }
    }
    if let Some(path) = &artifact_directory {
        std::fs::write(path.join("comparison.json"), serde_json::to_vec_pretty(&serde_json::json!({"sdk":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"software_gl":std::env::var("LIBGL_ALWAYS_SOFTWARE").ok(),"gl_vendor_renderer_version":gpu::environment(&thread),"backend":if cfg!(feature="wayland") { "x11+wayland" } else { "x11" },"cases":records})).unwrap()).unwrap();
    }
    let before = assets.resource_counts();
    assert!(assets.unload_font(smooth));
    assert!(!assets.unload_font(smooth));
    assert!(
        assets
            .measure_text("Ag?", TextStyle::new(smooth, 24.0))
            .is_err()
    );
    let fresh = assets
        .load_font(&thread, &path, FontOptions::default())
        .unwrap();
    assert_ne!(fresh, smooth);
    assert!(assets.resource_counts().font_atlases < before.font_atlases);
    // Releasing all fonts returns diagnostics to zero while the GL context lives.
    for font in [fresh, fixed, nearest, pixel] {
        assert!(assets.unload_font(font));
    }
    assert_eq!(assets.fonts.usage(), (0, 0, 0));
}
