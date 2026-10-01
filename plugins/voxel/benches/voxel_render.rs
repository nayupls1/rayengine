//! Opt-in native chunk submission/replacement; generation and fixture setup are untimed.
use criterion::{BenchmarkId, Criterion, Throughput};
use rayengine::{prelude::*, raylib::prelude::Image};
use rayengine_voxel::prelude::*;
use std::{hint::black_box, path::PathBuf, sync::Arc, time::Duration};

#[path = "streaming/render.rs"]
mod streaming;

struct Fixture {
    name: &'static str,
    mode: &'static str,
    world: Arc<VoxelWorld>,
    mesh: ChunkMesh,
    gpu: RenderedChunk,
}
struct Bench {
    criterion: Option<Criterion>,
    path: PathBuf,
    materials: VoxelMaterials,
    fixtures: Vec<Fixture>,
}
impl Game for Bench {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let texture = ctx.texture(&self.path)?;
        self.materials =
            VoxelMaterials::create(ctx, &[TileTexture::whole(TileId(0), texture)], 0.5)?;
        for name in ["solid", "terrain", "checkerboard", "cutout"] {
            let mut registry = BlockRegistry::new();
            let mut def = BlockDef::new("bench:block");
            if name == "cutout" {
                def.render = RenderKind::Cutout;
            }
            let id = registry.register(def).unwrap();
            let registry = Arc::new(registry);
            let cells = (0..4096)
                .map(|i| {
                    let p = LocalPos::from_index(i).unwrap();
                    let filled = match name {
                        "terrain" => p.y() < 4 + p.x() / 4 + p.z() / 4,
                        "checkerboard" => (p.x() + p.y() + p.z()).is_multiple_of(2),
                        _ => true,
                    };
                    if filled { id } else { BlockId::AIR }
                })
                .collect();
            let mut world = VoxelWorld::new(registry.clone(), 1);
            world
                .insert_chunk(
                    ChunkPos::default(),
                    Chunk::from_blocks(registry, cells).unwrap(),
                )
                .unwrap();
            let world = Arc::new(world);
            for (label, mode) in [
                ("culled", MeshingMode::Culled),
                ("greedy", MeshingMode::Greedy),
            ] {
                let input = MeshInput::capture(&world, ChunkPos::default()).unwrap();
                let mesh = input
                    .build(MeshingOptions {
                        mode,
                        ..Default::default()
                    })
                    .unwrap();
                let mut gpu = RenderedChunk::new(ChunkPos::default()).unwrap();
                gpu.upload_init(&world, &mesh, &self.materials, ctx)?;
                eprintln!("voxel_render/{name}/{label}: {:?}", mesh.stats());
                self.fixtures.push(Fixture {
                    name,
                    mode: label,
                    world: world.clone(),
                    mesh,
                    gpu,
                });
            }
        }
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let mut c = self.criterion.take().unwrap();
        let mut uploads = c.benchmark_group("voxel_upload");
        for fixture in &mut self.fixtures {
            uploads.bench_function(BenchmarkId::new(fixture.name, fixture.mode), |b| {
                b.iter(|| {
                    fixture
                        .gpu
                        .replace(
                            black_box(&fixture.world),
                            black_box(&fixture.mesh),
                            &self.materials,
                            frame,
                        )
                        .unwrap()
                });
            });
        }
        uploads.finish();
        streaming::workloads(&mut c, frame, &self.materials);
        frame.clear(Color::BLACK);
        let camera = Camera3D {
            position: Vec3::new(24.0, 20.0, 32.0),
            target: Vec3::splat(8.0),
            ..Default::default()
        };
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        let away = Camera3D {
            target: camera.position + Vec3::Z,
            ..camera
        };
        let hidden = Frustum3D::from_camera(&away, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.world_3d(camera, |canvas| {
            let mut draws = c.benchmark_group("voxel_draw");
            draws.throughput(Throughput::Elements(16));
            for fixture in &self.fixtures {
                draws.bench_function(BenchmarkId::new(fixture.name, fixture.mode), |b| {
                    b.iter(|| {
                        for _ in 0..16 {
                            let report =
                                fixture
                                    .gpu
                                    .draw(canvas, black_box(&view), BlockPos::default());
                            assert_eq!(black_box(report).submitted, fixture.mesh.stats().batches);
                        }
                    });
                });
            }
            draws.finish();
            let fixture = &self.fixtures[0];
            c.bench_function("voxel_culling/rejected_64", |b| {
                b.iter(|| {
                    for _ in 0..64 {
                        assert!(
                            black_box(fixture.gpu.draw(
                                canvas,
                                black_box(&hidden),
                                BlockPos::default()
                            ))
                            .culled
                        );
                    }
                });
            });
        });
        black_box(frame.assets.resource_counts());
        c.final_summary();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("RAYENGINE_RENDER_BENCH").as_deref() != Ok("1") {
        eprintln!("Use scripts/render_benchmark.sh save NAME voxel_");
        return Ok(());
    }
    let path =
        std::env::temp_dir().join(format!("rayengine-voxel-bench-{}.png", std::process::id()));
    let png = Image::gen_image_color(8, 8, Color::WHITE).export_image_to_memory(".png")?;
    std::fs::write(&path, &*png)?;
    let mut config = Config::new("Voxel native benchmark");
    config.audio = false;
    config.vsync = false;
    config.window_size = (64, 64);
    config.reference_size = Vec2::splat(64.0);
    let c = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2))
        .configure_from_args();
    let result = App::new(config)
        .with_options(RunOptions {
            frames: Some(1),
            hidden: true,
            uncapped: true,
            ..Default::default()
        })
        .run(Bench {
            criterion: Some(c),
            path: path.clone(),
            materials: VoxelMaterials::new(),
            fixtures: Vec::new(),
        });
    let _ = std::fs::remove_file(path);
    result?;
    Ok(())
}
