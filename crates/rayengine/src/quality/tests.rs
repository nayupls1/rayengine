use super::*;
use crate::{assets::Assets, render::Frame};
use rayengine_core::{
    camera::Camera3D,
    collision::Aabb3,
    glam::{Vec2, Vec3},
    viewport::{ScaleMode, Viewport},
};

fn image(target: &RenderTexture2D) -> Image {
    let mut image = target.texture().load_image().unwrap();
    image.flip_vertical();
    image
}
fn edge_pixels(image: &Image, y_start: i32, y_end: i32) -> usize {
    (y_start..y_end)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let c = image.get_color(x, y);
            c.r > 8 && c.r < 247 && c.r == c.g && c.g == c.b
        })
        .count()
}
fn patch(image: &Image) -> Vec<Color> {
    let scale = image.width as f32 / 320.0;
    let pixel = |n: f32| (n * scale).round() as i32;
    (pixel(8.0)..pixel(66.0))
        .flat_map(|y| (pixel(8.0)..pixel(240.0)).map(move |x| image.get_color(x, y)))
        .collect()
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_quality_offscreen_edges_text_alpha_dpi_and_cleanup() {
    let (mut rl, thread) = raylib::init()
        .size(320, 180)
        .hidden()
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let font = rl
        .load_font_from_memory(
            &thread,
            ".ttf",
            include_bytes!("../../examples/assets/iAWriterMonoS-Regular.ttf"),
            48,
            None,
        )
        .unwrap();
    font.texture()
        .set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
    let mut assets = Assets::new(None);
    for dpi in [1.0f32, 1.25, 2.0] {
        let mut counts = Vec::new();
        let mut text = None;
        // Actual Frame passes -> quality resolve -> native offscreen readback.
        // Diagonal geometry and sphere silhouettes use identical coordinates/workloads.
        for quality in [
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
        ] {
            let view = Viewport::new(
                Vec2::new(320.0, 180.0),
                Vec2::new(320.0, 180.0),
                ScaleMode::Fit,
            )
            .unwrap();
            let plan = quality
                .plan(&view, Vec2::splat(dpi), ScaleMode::Fit)
                .unwrap();
            let mut targets = QualityTargets::new(&mut rl, &thread, plan, false).unwrap();
            let mut fxaa = shader(&mut rl, &thread, quality).unwrap();
            {
                let mut frame = Frame {
                    counters: None,
                    raylib: &mut rl,
                    thread: &thread,
                    target: &mut targets.world,
                    ui_target: targets.ui.as_mut(),
                    assets: &mut assets,
                    viewport: view,
                    alpha: 0.0,
                    index: 0,
                };
                frame.clear(Color::BLACK);
                frame.with_raylib(|raw| {
                    let s = plan.world.0 as f32 / 320.0;
                    raw.draw_triangle(
                        Vector2::new(15.0 * s, 155.0 * s),
                        Vector2::new(135.0 * s, 155.0 * s),
                        Vector2::new(15.0 * s, 78.0 * s),
                        Color::WHITE,
                    );
                });
                frame.world_3d(
                    Camera3D {
                        position: Vec3::new(3.0, 1.0, 5.0),
                        target: Vec3::ZERO,
                        ..Default::default()
                    },
                    |canvas| {
                        canvas.cube(
                            Aabb3::from_center(Vec3::new(1.6, -0.7, 0.0), Vec3::splat(0.9)),
                            Color::WHITE,
                        );
                        canvas.sphere(Vec3::new(2.7, -0.7, 0.0), 0.45, Color::WHITE);
                    },
                );
                frame.ui(|ui| {
                    assert_eq!(ui.pixel_scale(), Vec2::splat(dpi));
                    ui.rectangle(
                        rayengine_core::collision::Aabb2 {
                            min: Vec2::new(8.0, 8.0),
                            max: Vec2::new(240.0, 66.0),
                        },
                        Color::BLACK,
                    );
                    ui.text("Native text", Vec2::new(12.0, 12.0), 16.0, Color::WHITE);
                    ui.raw.draw_text_ex(
                        &font,
                        "Custom font Aa 012",
                        Vector2::new(12.0 * dpi, 34.0 * dpi),
                        24.0 * dpi,
                        dpi,
                        Color::WHITE,
                    );
                    ui.rectangle(
                        rayengine_core::collision::Aabb2 {
                            min: Vec2::new(280.0, 10.0),
                            max: Vec2::new(310.0, 30.0),
                        },
                        Color::new(255, 0, 0, 128),
                    );
                });
            }
            let world = image(&targets.world);
            targets.resolve(&mut rl, &thread, fxaa.as_mut());
            let resolved = image(targets.presented());
            assert_eq!(
                (resolved.width, resolved.height),
                ((320.0 * dpi) as i32, (180.0 * dpi) as i32)
            );
            counts.push(edge_pixels(
                &resolved,
                (70.0 * dpi) as i32,
                (175.0 * dpi) as i32,
            ));
            if quality.anti_aliasing == AntiAliasing::Fxaa && quality.render_scale == 1.0 {
                assert_eq!(
                    edge_pixels(&world, (70.0 * dpi) as i32, (175.0 * dpi) as i32),
                    counts[0],
                    "world rasterization is unchanged; filtering acts on resolved offscreen output"
                );
                assert!(
                    counts[1] > counts[0] + 30,
                    "FXAA should add edge coverage pixels: {counts:?}"
                );
            }
            let current = patch(&resolved);
            if let Some(native) = &text {
                assert_eq!(
                    &current, native,
                    "default and custom UI glyphs must remain byte-identical"
                );
            } else {
                text = Some(current);
            }
            let alpha = resolved.get_color((290.0 * dpi) as i32, (20.0 * dpi) as i32);
            assert!(
                (i32::from(alpha.r) - 128).abs() <= 1 && alpha.g == 0,
                "UI alpha must not be multiplied twice: {alpha:?}"
            );
            // Old targets and shader drop inside the live context, including repeated runs.
        }
        assert!(
            counts[2] > counts[0] + 30,
            "2x SSAA must create coverage pixels: {counts:?}"
        );
        eprintln!("offscreen gray edge pixels [native,fxaa,2x,2x+fxaa]: {counts:?}");
    }
    for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
        for dpi in [Vec2::ONE, Vec2::splat(2.0), Vec2::splat(1.25)] {
            let view =
                Viewport::new(Vec2::new(240.0, 320.0), Vec2::new(320.0, 180.0), mode).unwrap();
            let quality = if mode == ScaleMode::IntegerFit {
                RenderQuality::default()
            } else {
                RenderQuality {
                    render_scale: 2.0,
                    anti_aliasing: AntiAliasing::Fxaa,
                }
            };
            let plan = quality.plan(&view, dpi, mode).unwrap();
            let targets =
                QualityTargets::new(&mut rl, &thread, plan, mode == ScaleMode::IntegerFit).unwrap();
            assert_eq!(
                (
                    targets.world.texture().width as u32,
                    targets.world.texture().height as u32
                ),
                plan.world
            );
            assert_eq!(
                (
                    targets.presented().texture().width as u32,
                    targets.presented().texture().height as u32
                ),
                plan.output
            );
        }
    }
}

#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_quality_transparent_world_and_pixel_art_point_resolve() {
    let (mut rl, thread) = raylib::init()
        .size(320, 180)
        .hidden()
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let view = Viewport::new(
        Vec2::new(320.0, 180.0),
        Vec2::new(320.0, 180.0),
        ScaleMode::Fit,
    )
    .unwrap();
    for quality in [
        RenderQuality {
            anti_aliasing: AntiAliasing::Fxaa,
            ..Default::default()
        },
        RenderQuality {
            render_scale: 2.0,
            ..Default::default()
        },
    ] {
        let plan = quality.plan(&view, Vec2::ONE, ScaleMode::Fit).unwrap();
        let mut targets = QualityTargets::new(&mut rl, &thread, plan, false).unwrap();
        let color = Color::new(80, 100, 120, 128);
        rl.begin_texture_mode(&thread, &mut targets.world)
            .clear_background(color);
        rl.begin_texture_mode(&thread, targets.ui.as_mut().unwrap())
            .clear_background(Color::BLANK);
        let mut fxaa = shader(&mut rl, &thread, quality).unwrap();
        targets.resolve(&mut rl, &thread, fxaa.as_mut());
        assert_eq!(
            image(targets.presented()).get_color(100, 100),
            color,
            "resolve must preserve world texture alpha without double blending"
        );
    }
    let pixel_view = Viewport::new(
        Vec2::new(320.0, 180.0),
        Vec2::new(16.0, 9.0),
        ScaleMode::IntegerFit,
    )
    .unwrap();
    let plan = RenderQuality::default()
        .plan(&pixel_view, Vec2::splat(2.0), ScaleMode::IntegerFit)
        .unwrap();
    assert_eq!(plan.world, (16, 9));
    let mut targets = QualityTargets::new(&mut rl, &thread, plan, true).unwrap();
    {
        let mut draw = rl.begin_texture_mode(&thread, &mut targets.world);
        draw.clear_background(Color::BLACK);
        for x in (0..16).step_by(2) {
            draw.draw_rectangle(x, 0, 1, 9, Color::WHITE);
        }
    }
    let mut output = target(&mut rl, &thread, (320, 180), false).unwrap();
    {
        let mut draw = rl.begin_texture_mode(&thread, &mut output);
        draw.clear_background(Color::BLACK);
        blit(&mut draw, targets.world.texture(), (320, 180));
    }
    let output = image(&output);
    for y in 0..output.height {
        for x in 0..output.width {
            let color = output.get_color(x, y);
            assert!(
                color == Color::BLACK || color == Color::WHITE,
                "IntegerFit must never smooth texels: {color:?}"
            );
        }
    }
}
