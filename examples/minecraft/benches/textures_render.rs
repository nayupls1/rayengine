//! Opt-in seeded textured surface upload/draw. Assets and CPU geometry are untimed.
use criterion::Criterion;
use rayengine::{
    prelude::*,
    raylib::prelude::{Image, RaylibTexture2D, TextureFilter},
};
use rayengine_minecraft::{
    terrain::{Terrain, TerrainSettings, chunk_fingerprint},
    textures::{TextureSet, Tile},
};
use rayengine_voxel::prelude::*;
use std::{hint::black_box, time::Duration};
struct Bench {
    criterion: Option<Criterion>,
    world: VoxelWorld,
    mesh: ChunkMesh,
    gpu: RenderedChunk,
    materials: VoxelMaterials,
    texture: Option<TextureId>,
}
impl Game for Bench {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let atlas = TextureSet::fallback().pack();
        let png = atlas.png().unwrap();
        let image = Image::load_image_from_mem(".png", &png).unwrap();
        let texture = ctx.texture_from_image(&image)?;
        ctx.assets
            .texture(texture)
            .unwrap()
            .set_texture_filter(ctx.thread, TextureFilter::TEXTURE_FILTER_POINT);
        self.materials = VoxelMaterials::create(
            ctx,
            &Tile::ALL.map(|tile| TileTexture {
                tile: tile.id(),
                texture,
                rect: atlas.rects[tile as usize],
            }),
            0.5,
        )?;
        self.texture = Some(texture);
        self.gpu
            .upload_init(&self.world, &self.mesh, &self.materials, ctx)?;
        eprintln!(
            "minecraft_texture_render_v1: {:?}, atlas={}x{}, texture_bytes={}",
            self.mesh.stats(),
            atlas.width,
            atlas.height,
            ctx.assets.resource_counts().texture_bytes
        );
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let mut c = self.criterion.take().unwrap();
        let mut group = c.benchmark_group("minecraft_texture_render_v1");
        group.bench_function("replace_surface", |b| {
            b.iter(|| {
                self.gpu
                    .replace(
                        black_box(&self.world),
                        black_box(&self.mesh),
                        &self.materials,
                        frame,
                    )
                    .unwrap()
            })
        });
        let origin = BlockPos::new(0, 48, 0);
        let camera = Camera3D {
            position: Vec3::new(24.0, 20.0, 32.0),
            target: Vec3::splat(8.0),
            ..Default::default()
        };
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.clear(Color::BLACK);
        frame.world_3d(camera, |canvas| {
            group.bench_function("draw_surface_16", |b| {
                b.iter(|| {
                    for _ in 0..16 {
                        let report = self.gpu.draw(canvas, black_box(&view), origin);
                        assert_eq!(black_box(report).submitted, self.mesh.stats().batches);
                    }
                })
            });
        });
        group.finish();
        self.gpu.unload(frame.assets);
        self.materials.unload(frame.assets);
        frame.assets.unload_texture(self.texture.take().unwrap());
        assert_eq!(frame.assets.resource_counts().textures, 0);
        assert_eq!(frame.assets.resource_counts().meshes, 0);
        c.final_summary();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("RAYENGINE_RENDER_BENCH").as_deref() != Ok("1") {
        eprintln!("Use scripts/render_benchmark.sh save NAME minecraft_texture_render_v1");
        return Ok(());
    }
    let terrain = Terrain::new(42, TerrainSettings::default())?;
    let pos = ChunkPos::new(0, 3, 0);
    let owner = terrain.chunk(pos)?;
    assert_eq!(chunk_fingerprint(&owner), 0x0b77011da62e050d);
    let mut world = VoxelWorld::new(terrain.registry(), 7);
    world.insert_chunk(pos, owner)?;
    for face in Face::ALL {
        let p = pos.neighbor(face).unwrap();
        world.insert_chunk(p, terrain.chunk(p)?)?;
    }
    let mesh = MeshInput::capture(&world, pos)?.build(MeshingOptions::default())?;
    let stats = mesh.stats();
    assert_eq!(
        (
            stats.visible_faces,
            stats.quads,
            stats.vertices,
            stats.triangles,
            stats.batches,
            stats.buffer_bytes
        ),
        (485, 183, 732, 366, 5, 28_548)
    );
    let mut config = Config::new("Minecraft texture benchmark");
    config.audio = false;
    config.vsync = false;
    config.window_size = (64, 64);
    config.reference_size = Vec2::splat(64.0);
    App::new(config)
        .with_options(RunOptions {
            hidden: true,
            uncapped: true,
            frames: Some(1),
            ..Default::default()
        })
        .run(Bench {
            criterion: Some(
                Criterion::default()
                    .sample_size(30)
                    .warm_up_time(Duration::from_millis(500))
                    .measurement_time(Duration::from_secs(2))
                    .configure_from_args(),
            ),
            world,
            mesh,
            gpu: RenderedChunk::new(pos)?,
            materials: VoxelMaterials::new(),
            texture: None,
        })?;
    Ok(())
}
