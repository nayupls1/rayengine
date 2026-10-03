use super::*;
use crate::{prelude::*, quality::QualityTargets, targets::TargetAssets};
use rayengine_core::camera::{Camera2D, Camera3D};

fn pixels(target: &RenderTexture2D) -> Image {
    let mut image = target.texture().load_image().unwrap();
    image.flip_vertical();
    image
}
fn frame<'a>(
    rl: &'a mut RaylibHandle,
    thread: &'a RaylibThread,
    target: &'a mut RenderTexture2D,
    ui: Option<&'a mut RenderTexture2D>,
    assets: &'a mut Assets<'static>,
    viewport: Viewport,
) -> Frame<'a, 'static> {
    Frame {
        counters: None,
        raylib: rl,
        thread,
        target,
        ui_target: ui,
        assets,
        viewport,
        alpha: 0.0,
        index: 0,
        delta: std::time::Duration::ZERO,
    }
}
fn grade(
    assets: &mut Assets<'_>,
    rl: &mut RaylibHandle,
    thread: &RaylibThread,
) -> (MaterialId, MaterialId) {
    let shader = assets
        .shader_source(
            rl,
            thread,
            None,
            BuiltinEffect::ColorGrade.fragment_source(),
        )
        .unwrap();
    let gain = assets
        .uniform(shader, "gain", UniformValue::Vec3(Vec3::ONE))
        .unwrap();
    let lift = assets
        .uniform(shader, "lift", UniformValue::Vec3(Vec3::ZERO))
        .unwrap();
    let a = assets
        .create_material(
            rl,
            thread,
            MaterialDesc {
                parameters: vec![MaterialParam {
                    uniform: lift,
                    value: UniformValue::Vec3(Vec3::new(0.2, 0.0, 0.0)),
                }],
                ..BuiltinEffect::ColorGrade.material(shader)
            },
        )
        .unwrap();
    let b = assets
        .create_material(
            rl,
            thread,
            MaterialDesc {
                parameters: vec![MaterialParam {
                    uniform: gain,
                    value: UniformValue::Vec3(Vec3::new(0.5, 1.0, 1.0)),
                }],
                ..BuiltinEffect::ColorGrade.material(shader)
            },
        )
        .unwrap();
    (a, b)
}

#[test]
#[ignore = "requires native OpenGL; run serially"]
fn native_post_processing_order_ui_alpha_policies_and_dpi() {
    let (mut rl, thread) = raylib::init()
        .size(320, 180)
        .hidden()
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let mut assets = Assets::new(None);
    let (a, b) = grade(&mut assets, &mut rl, &thread);
    for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
        for window in [Vec2::new(320.0, 180.0), Vec2::new(180.0, 320.0)] {
            let view = Viewport::new(window, Vec2::new(160.0, 90.0), mode).unwrap();
            for dpi in [Vec2::ONE, Vec2::splat(1.25), Vec2::splat(2.0)] {
                for quality in [
                    RenderQuality::default(),
                    RenderQuality {
                        render_scale: 2.0,
                        anti_aliasing: AntiAliasing::Fxaa,
                    },
                ] {
                    if quality != RenderQuality::default() && mode == ScaleMode::IntegerFit {
                        continue;
                    }
                    let mut plan = quality.plan(&view, dpi, mode).unwrap();
                    plan.separate_ui = true;
                    let mut targets =
                        QualityTargets::new(&mut rl, &thread, plan, mode == ScaleMode::IntegerFit)
                            .unwrap();
                    let mut fxaa = crate::quality::shader(&mut rl, &thread, quality).unwrap();
                    let mut post = PostTargets::new(
                        &mut rl,
                        &thread,
                        plan.output,
                        mode == ScaleMode::IntegerFit,
                    )
                    .unwrap();
                    for ui in [UiPlacement::AfterEffects, UiPlacement::BeforeEffects] {
                        for (materials, expected) in [(vec![a, b], 26_u8), (vec![b, a], 51)] {
                            let chain = PostProcessing { materials, ui };
                            let mut f = frame(
                                &mut rl,
                                &thread,
                                &mut targets.world,
                                targets.ui.as_mut(),
                                &mut assets,
                                view,
                            );
                            f.clear(Color::new(0, 0, 0, 128));
                            f.ui(|ui| {
                                ui.rectangle(
                                    Aabb2 {
                                        min: Vec2::ZERO,
                                        max: view.logical_size * 0.4,
                                    },
                                    Color::WHITE,
                                )
                            });
                            targets.resolve_with_ui(
                                &mut rl,
                                &thread,
                                fxaa.as_mut(),
                                ui == UiPlacement::BeforeEffects,
                            );
                            post.apply(
                                &mut rl,
                                &thread,
                                targets.presented(),
                                targets.ui.as_ref(),
                                &chain,
                                &mut assets,
                            )
                            .unwrap();
                            let image = pixels(post.presented());
                            let center = image
                                .get_color((plan.output.0 / 2) as i32, (plan.output.1 / 2) as i32);
                            // Lift is scaled by alpha, so transparent pixels stay premultiplied.
                            assert!(
                                (i16::from(center.r) - i16::from(expected) / 2).abs() <= 1,
                                "{mode:?} {dpi:?} {center:?}"
                            );
                            assert_eq!(center.a, 128);
                            let patch = image
                                .get_color((plan.output.0 / 8) as i32, (plan.output.1 / 8) as i32);
                            assert_eq!(patch.a, 255);
                            if ui == UiPlacement::AfterEffects {
                                assert_eq!(patch, Color::WHITE);
                            } else {
                                assert!(
                                    patch.r < 200 && patch.g == 255 && patch.b == 255,
                                    "{patch:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native OpenGL; run serially"]
fn native_post_processing_targets_2d_3d_sampling_resize_lifetime_and_limits() {
    let (mut rl, thread) = raylib::init()
        .size(320, 180)
        .hidden()
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let mut assets = Assets::new(None);
    let ids: Vec<_> = [
        TargetSize::Fixed(32, 16),
        TargetSize::Logical,
        TargetSize::Physical,
        TargetSize::Reference,
    ]
    .into_iter()
    .map(|size| {
        assets
            .create_render_target(RenderTargetDesc {
                size,
                filter: TargetFilter::Point,
            })
            .unwrap()
    })
    .collect();
    for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
        for window in [Vec2::new(320.0, 180.0), Vec2::new(180.0, 320.0)] {
            let view = Viewport::new(window, Vec2::new(160.0, 90.0), mode).unwrap();
            for dpi in [Vec2::ONE, Vec2::splat(1.25), Vec2::splat(2.0)] {
                assets
                    .targets
                    .sync(&mut rl, &thread, &view, dpi, 0)
                    .unwrap();
                for id in &ids {
                    let mut native = assets.targets.take(*id).unwrap();
                    let mut f = frame(&mut rl, &thread, &mut native, None, &mut assets, view);
                    f.clear(Color::BLANK);
                    f.world_3d(Camera3D::default(), |world| {
                        world.cube(Aabb3::from_center(Vec3::ZERO, Vec3::ONE), Color::BLUE)
                    });
                    f.ui(|ui| {
                        assert!(!ui.render_target(
                            *id,
                            Aabb2 {
                                min: Vec2::ZERO,
                                max: view.logical_size
                            },
                            Color::WHITE
                        ));
                        ui.rectangle(
                            Aabb2 {
                                min: Vec2::ZERO,
                                max: Vec2::new(view.logical_size.x, view.logical_size.y * 0.5),
                            },
                            Color::new(255, 0, 0, 128),
                        );
                    });
                    let image = pixels(&native);
                    assert_eq!(image.get_color(1, 1), Color::new(128, 0, 0, 128));
                    assets.targets.restore(*id, native);
                }
                assert_eq!(assets.render_target_usage().0, 4);
                let mut output = crate::quality::target(&mut rl, &thread, (160, 90), true).unwrap();
                let mut f = frame(&mut rl, &thread, &mut output, None, &mut assets, view);
                f.clear(Color::BLANK);
                f.with_target(ids[0], |nested| {
                    assert!(nested.with_target(ids[0], |_| ()).is_err());
                    assert!(!nested.assets.unload_render_target(ids[0]));
                })
                .unwrap();
                f.ui(|ui| {
                    assert!(ui.render_target(
                        ids[0],
                        Aabb2 {
                            min: Vec2::ZERO,
                            max: view.logical_size
                        },
                        Color::WHITE
                    ))
                });
                assert_eq!(pixels(&output).get_color(1, 1), Color::new(128, 0, 0, 128));
                // Sample in world space as well, preserving the upright top half.
                let mut f = frame(&mut rl, &thread, &mut output, None, &mut assets, view);
                f.clear(Color::BLANK);
                let aspect = 160.0 / 90.0;
                f.world_2d(
                    Camera2D {
                        view_height: 2.0,
                        ..Default::default()
                    },
                    |world| {
                        assert!(world.render_target(
                            ids[0],
                            Aabb2 {
                                min: Vec2::new(-aspect, -1.0),
                                max: Vec2::new(aspect, 1.0)
                            },
                            Color::WHITE
                        ))
                    },
                );
                assert_eq!(pixels(&output).get_color(1, 1), Color::new(128, 0, 0, 128));
            }
        }
    }
    for id in &ids {
        assert!(assets.unload_render_target(*id));
    }
    assert_eq!(assets.render_target_usage(), (0, 0));
    let fresh = assets
        .create_render_target(RenderTargetDesc::fixed(32, 16))
        .unwrap();
    assert!(!ids.contains(&fresh));
    assert!(assets.render_target_texture(ids[0]).is_none());
    let view = Viewport::new(
        Vec2::new(320.0, 180.0),
        Vec2::new(160.0, 90.0),
        ScaleMode::Fit,
    )
    .unwrap();
    assert!(
        assets
            .targets
            .sync(
                &mut rl,
                &thread,
                &view,
                Vec2::ONE,
                RenderQuality::MAX_TARGET_BYTES
            )
            .is_err()
    );
    assets
        .targets
        .sync(&mut rl, &thread, &view, Vec2::ONE, 0)
        .unwrap();
    // Repeated creation/unloading uses versioned slots without accumulating GPU resources.
    for _ in 0..100 {
        let id = assets
            .create_render_target(RenderTargetDesc::fixed(8, 8))
            .unwrap();
        assets
            .targets
            .sync(&mut rl, &thread, &view, Vec2::ONE, 0)
            .unwrap();
        assert!(assets.unload_render_target(id));
    }
    assert_eq!(assets.render_target_usage().0, 1);
    let mut targets = TargetAssets::default();
    targets.create(RenderTargetDesc::fixed(8192, 8192)).unwrap();
    targets.create(RenderTargetDesc::fixed(1, 1)).unwrap();
    assert!(targets.sync(&mut rl, &thread, &view, Vec2::ONE, 0).is_err());
    assert_eq!(targets.usage(), (0, 0));
}

#[test]
#[ignore = "requires native OpenGL; run serially"]
fn native_post_processing_runner_toggle_resize_screenshot_and_teardown() {
    struct Probe {
        material: Option<MaterialId>,
        target: Option<RenderTargetId>,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            let (a, _) = grade(ctx.assets, ctx.raylib, ctx.thread);
            self.material = Some(a);
            self.target = Some(ctx.assets.create_render_target(RenderTargetDesc {
                size: TargetSize::Physical,
                filter: TargetFilter::Bilinear,
            })?);
            ctx.assets.set_post_processing(PostProcessing {
                materials: vec![a],
                ..Default::default()
            })
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            let id = self.target.unwrap();
            let logical_size = frame.viewport.logical_size;
            frame
                .with_target(id, |target| {
                    target.clear(Color::BLACK);
                    target.ui(|ui| {
                        ui.rectangle(
                            Aabb2 {
                                min: Vec2::ZERO,
                                max: logical_size,
                            },
                            Color::GREEN,
                        )
                    });
                })
                .unwrap();
            frame.clear(Color::BLACK);
            frame.ui(|ui| {
                assert!(ui.render_target(
                    id,
                    Aabb2 {
                        min: Vec2::ZERO,
                        max: logical_size
                    },
                    Color::WHITE
                ))
            });
            if frame.index == 0 {
                frame.raylib.set_window_size(180, 320);
            }
            // Changes made during drawing take effect on the next frame.
            let enabled = frame.index.is_multiple_of(2);
            frame
                .assets
                .set_post_processing(PostProcessing {
                    materials: if enabled {
                        vec![self.material.unwrap()]
                    } else {
                        Vec::new()
                    },
                    ui: UiPlacement::BeforeEffects,
                })
                .unwrap();
        }
        fn shutdown(&mut self, ctx: &mut InitContext<'_, '_>) {
            assert!(ctx.assets.unload_render_target(self.target.unwrap()));
            assert_eq!(ctx.assets.render_target_usage(), (0, 0));
            ctx.assets
                .set_post_processing(PostProcessing::default())
                .unwrap();
        }
    }
    let dir = std::env::temp_dir().join(format!("rayengine-post-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
        let mut config = Config::new("post runner");
        config.window_size = (320, 180);
        config.reference_size = Vec2::new(160.0, 90.0);
        config.scale_mode = mode;
        config.vsync = false;
        config.target_fps = 60;
        let path = dir.join(format!("{mode:?}.png"));
        let report = App::new(config)
            .with_options(RunOptions {
                frames: Some(7),
                hidden: true,
                screenshot: Some(path.clone()),
                diagnostics: Some(crate::diagnostics::DiagnosticsConfig::new("post-probe.v1")),
                ..Default::default()
            })
            .run(Probe {
                material: None,
                target: None,
            })
            .unwrap();
        let metrics = report.diagnostics.unwrap();
        assert_eq!(metrics.resources.render_targets, 1);
        // Last frame has an empty chain: native world and the one custom target only.
        assert_eq!(
            metrics.settings.render_target_bytes,
            8 * u64::from(metrics.settings.render_size.0)
                * u64::from(metrics.settings.render_size.1)
                + metrics.resources.render_target_bytes
        );
        let image = Image::load_image(path.to_str().unwrap()).unwrap();
        assert_eq!(image.get_color(90, 160), Color::GREEN);
        if mode != ScaleMode::Expand {
            assert_eq!(image.get_color(0, 0), Color::new(9, 14, 24, 255));
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires native OpenGL; run serially"]
fn native_post_processing_target_materials_and_sampling_restore_blend() {
    let (mut rl, thread) = raylib::init()
        .size(160, 90)
        .hidden()
        .log_level(TraceLogLevel::LOG_WARNING)
        .build();
    let mut assets = Assets::new(None);
    let view = Viewport::new(
        Vec2::new(160.0, 90.0),
        Vec2::new(160.0, 90.0),
        ScaleMode::Fit,
    )
    .unwrap();
    let id = assets
        .create_render_target(RenderTargetDesc::fixed(16, 16))
        .unwrap();
    assets
        .targets
        .sync(&mut rl, &thread, &view, Vec2::ONE, 0)
        .unwrap();
    let mut target = assets.targets.take(id).unwrap();
    frame(&mut rl, &thread, &mut target, None, &mut assets, view).clear(Color::new(255, 0, 0, 128));
    assets.targets.restore(id, target);
    let mut output = crate::quality::target(&mut rl, &thread, (160, 90), true).unwrap();
    let mesh = assets
        .upload_mesh(
            &rl,
            &thread,
            &MeshData {
                positions: vec![
                    Vec3::new(-1.0, -1.0, 0.0),
                    Vec3::new(1.0, -1.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(-1.0, 1.0, 0.0),
                ],
                normals: Some(vec![Vec3::Z; 4]),
                texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
                indices: Some(vec![0, 1, 2, 0, 2, 3]),
                ..Default::default()
            },
        )
        .unwrap();
    assets
        .set_lighting(Lighting {
            ambient: Vec3::ONE,
            ..Default::default()
        })
        .unwrap();
    for shading in [Shading::Unlit, Shading::Lit] {
        let mat = assets
            .create_material(
                &mut rl,
                &thread,
                MaterialDesc {
                    render_target: Some(id),
                    shading,
                    alpha: AlphaMode::Blend,
                    ..Default::default()
                },
            )
            .unwrap();
        let mut f = frame(&mut rl, &thread, &mut output, None, &mut assets, view);
        f.clear(Color::BLANK);
        f.world_3d(
            Camera3D {
                position: Vec3::new(0.0, 0.0, 4.0),
                target: Vec3::ZERO,
                ..Default::default()
            },
            |world| {
                assert!(
                    world
                        .try_mesh_material(mesh, mat, Transform3D::default(), Color::WHITE)
                        .unwrap()
                );
            },
        );
        assert_eq!(
            pixels(&output).get_color(80, 45),
            Color::new(128, 0, 0, 128)
        );
    }
    let mut f = frame(&mut rl, &thread, &mut output, None, &mut assets, view);
    f.clear(Color::BLANK);
    f.ui(|ui| {
        assert!(ui.render_target(
            id,
            Aabb2 {
                min: Vec2::ZERO,
                max: Vec2::new(80.0, 90.0)
            },
            Color::WHITE
        ));
        ui.rectangle(
            Aabb2 {
                min: Vec2::new(80.0, 0.0),
                max: Vec2::new(160.0, 90.0),
            },
            Color::new(0, 255, 0, 128),
        );
    });
    let image = pixels(&output);
    assert_eq!(image.get_color(40, 45), Color::new(128, 0, 0, 128));
    assert_eq!(image.get_color(120, 45), Color::new(0, 128, 0, 128));
    assert!(assets.unload_render_target(id));
    assert!(
        assets
            .create_material(
                &mut rl,
                &thread,
                MaterialDesc {
                    render_target: Some(id),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
