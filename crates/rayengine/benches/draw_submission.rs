//! Opt-in native draw submission workloads. Timings include driver stalls, not GPU timer queries.
use criterion::{Criterion, Throughput};
use rayengine::{prelude::*, raylib::prelude::Image};
use std::{hint::black_box, path::PathBuf, time::Duration};

struct Bench {
    criterion: Option<Criterion>,
    mesh: Option<MeshId>,
    surfaces: Vec<(&'static str, MaterialId)>,
    texture_path: PathBuf,
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
            texcoords: Some(vec![Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]),
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
            ..MeshData::default()
        })?);
        let texture = ctx.texture(&self.texture_path)?;
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
        for (name, desc) in [
            ("opaque", MaterialDesc::default()),
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
        let camera = Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0),
            target: Vec3::ZERO,
            ..Camera3D::default()
        };
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
        });
    let _ = std::fs::remove_file(path);
    result?;
    Ok(())
}
