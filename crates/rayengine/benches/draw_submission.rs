//! Opt-in native drawing/upload workloads. Timings include driver stalls, not GPU timer queries.
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput};
use rayengine::{prelude::*, raylib::prelude::Image};
use std::{hint::black_box, path::PathBuf, time::Duration};

struct Bench {
    criterion: Option<Criterion>,
    mesh: Option<MeshId>,
    surfaces: Vec<(&'static str, MaterialId)>,
    texture_path: PathBuf,
    icon: Option<TextureId>,
}
impl Game for Bench {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.mesh = Some(ctx.mesh(&MeshData {
            positions: vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
            ],
            normals: Some(vec![Vec3::Z; 4]),
            texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
            ..MeshData::default()
        })?);
        let texture = ctx.texture(&self.texture_path)?;
        self.icon = Some(texture);
        let source = include_str!("../src/assets/materials/default.fs")
            .replace(
                "out vec4 finalColor;",
                "uniform vec3 gain;\nout vec4 finalColor;",
            )
            .replace(
                "finalColor = color;",
                "finalColor = vec4(color.rgb * gain, color.a);",
            );
        let shader = ctx.shader_from_source(None, &source)?;
        let gain = ctx.uniform(shader, "gain", UniformValue::Vec3(Vec3::ONE))?;
        ctx.assets.set_lighting(Lighting {
            ambient: Vec3::splat(0.2),
            directional: Some(DirectionalLight {
                direction: -Vec3::Z,
                color: Vec3::splat(0.5),
            }),
            points: vec![
                PointLight {
                    position: Vec3::Z * 2.0,
                    color: Vec3::splat(0.1),
                    range: 4.0
                };
                MAX_POINT_LIGHTS
            ],
        })?;
        for (name, desc) in [
            ("opaque", MaterialDesc::default()),
            (
                "lit",
                MaterialDesc {
                    shading: Shading::Lit,
                    ..MaterialDesc::default()
                },
            ),
            (
                "textured",
                MaterialDesc {
                    texture: Some(texture),
                    ..MaterialDesc::default()
                },
            ),
            (
                "cutout",
                MaterialDesc {
                    texture: Some(texture),
                    alpha: AlphaMode::Cutout(0.5),
                    ..MaterialDesc::default()
                },
            ),
            (
                "parameter",
                MaterialDesc {
                    shader: Some(shader),
                    texture: Some(texture),
                    parameters: vec![MaterialParam {
                        uniform: gain,
                        value: UniformValue::Vec3(Vec3::ONE),
                    }],
                    ..MaterialDesc::default()
                },
            ),
            (
                "blend",
                MaterialDesc {
                    alpha: AlphaMode::Blend,
                    tint: Color::new(255, 255, 255, 128),
                    ..MaterialDesc::default()
                },
            ),
        ] {
            self.surfaces.push((name, ctx.material(desc)?));
        }
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let mut criterion = self.criterion.take().expect("one benchmark frame");
        let mesh = self.mesh.unwrap();
        let mut uploads = criterion.benchmark_group("mesh_upload");
        for triangles in [1, 1_024] {
            let data = MeshData::new(
                (0..triangles)
                    .flat_map(|_| [Vec3::ZERO, Vec3::X, Vec3::Y])
                    .collect(),
            );
            let target = frame.mesh(&data).unwrap();
            uploads.bench_function(BenchmarkId::new("direct_replace", triangles), |b| {
                b.iter(|| frame.replace_mesh(target, black_box(&data)).unwrap())
            });
            uploads.bench_function(BenchmarkId::new("budgeted_replace", triangles), |b| {
                b.iter_batched(
                    || {
                        let mut queue = MeshUploadQueue::new(1, usize::MAX).unwrap();
                        queue
                            .try_push(MeshUpload {
                                tag: (),
                                revision: 0,
                                target: MeshUploadTarget::Replace(target),
                                data: data.clone(),
                            })
                            .unwrap();
                        queue
                    },
                    |mut queue| {
                        let report = frame.upload_meshes(
                            &mut queue,
                            UploadBudget {
                                max_requests: 1,
                                max_bytes: usize::MAX,
                                max_time: Duration::MAX,
                            },
                            |_, _| true,
                            |result| {
                                assert!(matches!(result.outcome, MeshUploadOutcome::Uploaded(_)));
                            },
                        );
                        assert_eq!(report.uploaded, 1);
                        black_box(report);
                    },
                    BatchSize::SmallInput,
                )
            });
            frame.assets.unload_mesh(target);
        }
        uploads.finish();
        let camera = Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0),
            target: Vec3::ZERO,
            ..Camera3D::default()
        };
        // Same real pass/100 submissions in both cases; counter enable/reset is
        // outside measurement, while per-submission branches are inside.
        let mut diagnostics = criterion.benchmark_group("diagnostics_draw");
        diagnostics.throughput(Throughput::Elements(100));
        for enabled in [false, true] {
            frame.set_draw_counters_enabled(enabled);
            diagnostics.bench_function(
                if enabled {
                    "enabled_100"
                } else {
                    "disabled_100"
                },
                |b| {
                    b.iter(|| {
                        frame.world_3d(camera, |canvas| {
                            for _ in 0..100 {
                                assert!(black_box(canvas.mesh(
                                    mesh,
                                    black_box(Transform3D::default()),
                                    Color::WHITE
                                )));
                            }
                        })
                    });
                },
            );
            black_box(frame.draw_counters());
        }
        diagnostics.finish();
        frame.set_draw_counters_enabled(false);
        criterion.bench_function("diagnostics_resources/owned_fixture", |b| {
            b.iter(|| black_box(frame.assets.resource_counts()));
        });
        let mut timing = criterion.benchmark_group("diagnostics_timing");
        for enabled in [false, true] {
            let mut stats = rayengine::diagnostics::TimingStats::default();
            timing.bench_function(
                if enabled {
                    "enabled_phase"
                } else {
                    "disabled_phase"
                },
                |b| {
                    b.iter(|| {
                        let start = enabled.then(std::time::Instant::now);
                        black_box(black_box(1_u64).wrapping_add(1));
                        if let Some(start) = start {
                            stats.record(start.elapsed());
                        }
                        black_box(&stats);
                    });
                },
            );
        }
        timing.finish();
        frame.world_3d(camera, |canvas| {
            let mut group = criterion.benchmark_group("draw_submission");
            group.throughput(Throughput::Elements(100));
            group.bench_function("default_100", |b| {
                b.iter(|| {
                    for _ in 0..100 {
                        assert!(black_box(canvas.mesh(
                            mesh,
                            black_box(Transform3D::default()),
                            Color::WHITE
                        )));
                    }
                })
            });
            for &(name, material) in &self.surfaces {
                group.bench_function(format!("{name}_100"), |b| {
                    b.iter(|| {
                        for _ in 0..100 {
                            assert!(black_box(canvas.mesh_material(
                                mesh,
                                material,
                                black_box(Transform3D::default()),
                                Color::WHITE
                            )));
                        }
                    })
                });
            }
            group.finish();
        });
        let regions: Vec<_> = (0..32)
            .map(|i| {
                UiRegion::new(
                    UiId(i),
                    UiRect::top_left(
                        Vec2::new((i % 8) as f32 * 8.0, (i / 8) as f32 * 16.0),
                        Vec2::new(8.0, 16.0),
                    )
                    .resolve(frame.viewport.logical_size),
                )
            })
            .collect();
        let mut ui = UiState::with_capacity(32);
        ui.update(
            &regions,
            UiInput {
                window_focused: true,
                ..UiInput::default()
            },
        );
        let style = UiButtonStyle {
            font_size: 3.0,
            ..UiButtonStyle::default()
        };
        frame.ui(|canvas| {
            let mut group = criterion.benchmark_group("ui_draw");
            group.throughput(Throughput::Elements(32));
            group.bench_function("buttons_32", |b| {
                b.iter(|| {
                    for region in &regions {
                        canvas.button(region.bounds, "Go", ui.response(region.id).unwrap(), style);
                    }
                })
            });
            group.bench_function("icons_32", |b| {
                b.iter(|| {
                    for region in &regions {
                        assert!(black_box(canvas.icon(
                            self.icon.unwrap(),
                            region.bounds,
                            Color::WHITE
                        )));
                    }
                })
            });
            group.finish();
        });
        criterion.final_summary();
    }
}
fn main() -> Result<(), Error> {
    if std::env::var("RAYENGINE_RENDER_BENCH").as_deref() != Ok("1") {
        eprintln!("Native benchmark is opt-in: use scripts/render_benchmark.sh save NAME");
        return Ok(());
    }
    let path =
        std::env::temp_dir().join(format!("rayengine-bench-white-{}.png", std::process::id()));
    let png = Image::gen_image_color(8, 8, Color::WHITE)
        .export_image_to_memory(".png")
        .map_err(|e| Error::Asset(e.to_string()))?;
    std::fs::write(&path, &*png)?;
    let criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();
    let mut config = Config::new("native submission benchmark");
    config.window_size = (64, 64);
    config.reference_size = Vec2::splat(64.0);
    config.vsync = false;
    let result = App::new(config)
        .with_options(RunOptions {
            frames: Some(1),
            hidden: true,
            uncapped: true,
            ..RunOptions::default()
        })
        .run(Bench {
            criterion: Some(criterion),
            mesh: None,
            surfaces: Vec::new(),
            texture_path: path.clone(),
            icon: None,
        });
    let _ = std::fs::remove_file(path);
    result?;
    Ok(())
}
