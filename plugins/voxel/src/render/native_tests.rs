use super::*;
use crate::{
    BlockDef, BlockId, BlockRegistry, Chunk, FaceShading, MeshInput, MeshingMode, MeshingOptions,
    RenderKind,
};
use rayengine::{diagnostics::DiagnosticsConfig, raylib::prelude::Image};
use std::{path::PathBuf, sync::Arc};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn camera() -> Camera3D {
    Camera3D {
        position: Vec3::new(5.0, 4.5, 10.0),
        target: Vec3::new(5.0, 4.5, 4.0),
        up: Vec3::Y,
        vertical_fov: 60.0,
    }
}
fn scene(kind: RenderKind) -> VoxelWorld {
    let mut registry = BlockRegistry::new();
    let mut blue = BlockDef::new("test:blue");
    blue.textures = [TileId(1); 6];
    let back = registry.register(blue).unwrap();
    let mut green = BlockDef::new("test:green");
    green.render = kind;
    let front = registry.register(green).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 1);
    let mut chunk = Chunk::filled(registry, BlockId::AIR).unwrap();
    for x in [4, 5] {
        chunk
            .set(crate::LocalPos::new(x, 4, 3).unwrap(), back)
            .unwrap();
        chunk
            .set(crate::LocalPos::new(x, 4, 4).unwrap(), front)
            .unwrap();
    }
    world.insert_chunk(ChunkPos::default(), chunk).unwrap();
    world
}
fn geometry(world: &VoxelWorld, mode: MeshingMode) -> ChunkMesh {
    MeshInput::capture(world, ChunkPos::default())
        .unwrap()
        .build(MeshingOptions {
            mode,
            shading: FaceShading([255; 6]),
            ..Default::default()
        })
        .unwrap()
}
// Real first upload followed by rejected second upload. SDK native mesh tests
// separately inject GL allocation failures; this checks whole-transaction rollback.
struct FailingSink<'borrow, S> {
    context: &'borrow mut S,
    count: usize,
}
impl<'audio, S: MeshSink<'audio>> MeshSink<'audio> for FailingSink<'_, S> {
    fn upload(&mut self, data: &MeshData) -> Result<MeshId, Error> {
        self.count += 1;
        if self.count == 2 {
            Err(Error::Asset("injected second-batch upload failure".into()))
        } else {
            self.context.upload(data)
        }
    }
    fn assets(&self) -> &Assets<'audio> {
        self.context.assets()
    }
    fn assets_mut(&mut self) -> &mut Assets<'audio> {
        self.context.assets_mut()
    }
}
struct Probe {
    world: VoxelWorld,
    mode: MeshingMode,
    path: PathBuf,
    materials: VoxelMaterials,
    chunk: RenderedChunk,
}
impl Game for Probe {
    fn init(&mut self, context: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let texture = context.texture(&self.path)?;
        let tiles = [
            TileTexture {
                tile: TileId(0),
                texture,
                rect: Vec4::new(0.0, 0.0, 0.5, 1.0),
            },
            TileTexture {
                tile: TileId(1),
                texture,
                rect: Vec4::new(0.5, 0.0, 0.5, 1.0),
            },
        ];
        let counts = context.assets.resource_counts();
        for bad in [
            Vec4::ZERO,
            Vec4::splat(f32::NAN),
            Vec4::new(-0.1, 0.0, 1.0, 1.0),
            Vec4::new(0.5, 0.0, 0.6, 1.0),
        ] {
            assert!(
                VoxelMaterials::create(
                    context,
                    &[TileTexture {
                        rect: bad,
                        ..tiles[0]
                    }],
                    0.5
                )
                .is_err()
            );
            assert_eq!(context.assets.resource_counts(), counts);
        }
        assert!(VoxelMaterials::create(context, &[tiles[0], tiles[0]], 0.5).is_err());
        assert!(VoxelMaterials::create(context, &tiles, f32::NAN).is_err());
        assert_eq!(context.assets.resource_counts(), counts);
        self.materials = VoxelMaterials::create(context, &tiles, 0.5)?;
        let mesh = geometry(&self.world, self.mode);
        assert_eq!(mesh.stats().batches, 2);
        self.chunk
            .upload_init(&self.world, &mesh, &self.materials, context)?;
        assert_eq!(
            context.assets.resource_counts().generated_mesh_bytes,
            mesh.stats().buffer_bytes as u64
        );
        let old: Vec<_> = self.chunk.batches.iter().map(|b| b.mesh).collect();
        let before = context.assets.resource_counts();
        let mut failing = FailingSink { context, count: 0 };
        assert!(
            self.chunk
                .install(&self.world, &mesh, &self.materials, &mut failing)
                .is_err()
        );
        assert_eq!(failing.count, 2);
        assert_eq!(context.assets.resource_counts(), before);
        assert_eq!(
            self.chunk
                .batches
                .iter()
                .map(|b| b.mesh)
                .collect::<Vec<_>>(),
            old
        );
        assert!(old.iter().all(|&id| context.assets.mesh(id).is_some()));
        // Missing, wrong-alpha and unloaded shader dependencies reject before commit.
        assert!(
            self.chunk
                .upload_init(&self.world, &mesh, &VoxelMaterials::new(), context)
                .is_err()
        );
        let key = mesh.batches()[0].surface();
        let real = self.materials.surface(key).unwrap();
        let wrong = context.material(MaterialDesc {
            alpha: AlphaMode::Blend,
            ..Default::default()
        })?;
        self.materials.bind(key, wrong)?;
        assert!(
            self.chunk
                .upload_init(&self.world, &mesh, &self.materials, context)
                .is_err()
        );
        context.assets.unload_material(wrong);
        let shader = context.shader_from_source(None, include_str!("repeat.fs"))?;
        let broken = context.material(MaterialDesc {
            shader: Some(shader),
            ..Default::default()
        })?;
        context.assets.unload_shader(shader);
        self.materials.bind(key, broken)?;
        assert!(
            self.chunk
                .upload_init(&self.world, &mesh, &self.materials, context)
                .is_err()
        );
        context.assets.unload_material(broken);
        self.materials.bind(key, real)?;
        assert_eq!(context.assets.resource_counts(), before);
        // Edit/stale and reinstall receipts never replace installed geometry.
        self.world
            .set_block(BlockPos::new(0, 0, 0), BlockId::from_raw(1))
            .unwrap();
        assert!(
            self.chunk
                .upload_init(&self.world, &mesh, &self.materials, context)
                .is_err()
        );
        assert_eq!(self.chunk.stats(), mesh.stats());
        self.world
            .set_block(BlockPos::new(0, 0, 0), BlockId::AIR)
            .unwrap();
        let current = geometry(&self.world, self.mode);
        let owner = self.world.remove_chunk(ChunkPos::default()).unwrap();
        self.world.insert_chunk(ChunkPos::default(), owner).unwrap();
        assert!(
            self.chunk
                .upload_init(&self.world, &current, &self.materials, context)
                .is_err()
        );
        assert_eq!(
            self.chunk
                .batches
                .iter()
                .map(|b| b.mesh)
                .collect::<Vec<_>>(),
            old
        );
        // Current empty results deliberately release old batches.
        let owner = self.world.remove_chunk(ChunkPos::default()).unwrap();
        self.world
            .insert_chunk(
                ChunkPos::default(),
                Chunk::filled(self.world.shared_registry(), BlockId::AIR).unwrap(),
            )
            .unwrap();
        let empty = geometry(&self.world, self.mode);
        self.chunk
            .upload_init(&self.world, &empty, &self.materials, context)?;
        assert_eq!(context.assets.resource_counts().meshes, 0);
        self.world.insert_chunk(ChunkPos::default(), owner).unwrap();
        let mesh = geometry(&self.world, self.mode);
        self.chunk
            .upload_init(&self.world, &mesh, &self.materials, context)?;
        self.chunk.unload(context.assets);
        self.chunk.unload(context.assets);
        assert_eq!(context.assets.resource_counts().meshes, 0);
        self.chunk
            .upload_init(&self.world, &mesh, &self.materials, context)?;
        // Built-in teardown preserves textures and caller-owned bindings.
        let custom = context.material(MaterialDesc::default())?;
        let mut temporary = VoxelMaterials::create(context, &tiles, 0.5)?;
        temporary.bind(key, custom)?;
        temporary.unload(context.assets);
        temporary.unload(context.assets);
        assert!(context.assets.material(custom).is_some());
        assert!(context.assets.texture(texture).is_some());
        context.assets.unload_material(custom);
        assert_eq!(context.assets.resource_counts(), before);
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        if frame.index == 1 {
            let mesh = geometry(&self.world, self.mode);
            self.chunk
                .replace(&self.world, &mesh, &self.materials, frame)
                .unwrap();
        }
        // Draw immediately after a partial upload failure, so final image checks
        // also exercise the retained geometry without an intervening success.
        let mesh = geometry(&self.world, self.mode);
        let before = frame.assets.resource_counts();
        let old: Vec<_> = self.chunk.batches.iter().map(|b| b.mesh).collect();
        let mut failing = FailingSink {
            context: frame,
            count: 0,
        };
        assert!(
            self.chunk
                .install(&self.world, &mesh, &self.materials, &mut failing)
                .is_err()
        );
        assert_eq!(failing.count, 2);
        assert_eq!(frame.assets.resource_counts(), before);
        assert_eq!(
            self.chunk
                .batches
                .iter()
                .map(|b| b.mesh)
                .collect::<Vec<_>>(),
            old
        );
        frame.clear(Color::BLACK);
        let camera = camera();
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        let away = Camera3D {
            position: Vec3::new(5.0, 4.5, 32.0),
            target: Vec3::new(5.0, 4.5, 33.0),
            ..camera
        };
        let hidden = Frustum3D::from_camera(&away, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.world_3d(camera, |canvas| {
            assert_eq!(
                self.chunk.draw(canvas, &hidden, BlockPos::default()),
                ChunkDraw {
                    culled: true,
                    ..Default::default()
                }
            );
            assert_eq!(
                self.chunk.draw(canvas, &view, BlockPos::default()),
                ChunkDraw {
                    culled: false,
                    submitted: 2,
                    unavailable: 0
                }
            );
        });
        assert_eq!(frame.draw_counters().unwrap().meshes, 2);
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_voxel_texture_repeat_cutout_depth_culling_and_atomic_replacement() {
    let directory = Directory(
        std::env::temp_dir().join(format!("rayengine-voxel-smoke-{}", std::process::id())),
    );
    std::fs::create_dir_all(&directory.0).unwrap();
    let atlas = directory.0.join("atlas.png");
    let mut image = Image::gen_image_color(
        16,
        8,
        Color::new(Color::GREEN.r, Color::GREEN.g, Color::GREEN.b, 0),
    );
    image.draw_rectangle(8, 0, 8, 8, Color::BLUE);
    image.draw_rectangle(4, 0, 4, 8, Color::GREEN);
    std::fs::write(&atlas, &*image.export_image_to_memory(".png").unwrap()).unwrap();
    for (kind, mode, label) in [
        (RenderKind::Opaque, MeshingMode::Greedy, "opaque"),
        (RenderKind::Cutout, MeshingMode::Greedy, "cutout-greedy"),
        (RenderKind::Cutout, MeshingMode::Culled, "cutout-culled"),
    ] {
        let screenshot = directory.0.join(format!("{label}.png"));
        let mut config = Config::new("Voxel render probe");
        config.audio = false;
        config.vsync = false;
        config.window_size = (640, 640);
        config.reference_size = Vec2::splat(640.0);
        let report = App::new(config)
            .with_options(RunOptions {
                hidden: true,
                frames: Some(2),
                uncapped: true,
                screenshot: Some(screenshot.clone()),
                diagnostics: Some(DiagnosticsConfig::new("voxel/render-smoke.v1")),
                ..Default::default()
            })
            .run(Probe {
                world: scene(kind),
                mode,
                path: atlas.clone(),
                materials: VoxelMaterials::new(),
                chunk: RenderedChunk::new(ChunkPos::default()).unwrap(),
            })
            .unwrap();
        assert_eq!(report.frames, 2);
        let image = Image::load_image(screenshot.to_str().unwrap()).unwrap();
        let scale = 640.0 / (2.0 * 30_f32.to_radians().tan() * 5.0);
        for (world_x, hole) in [(4.25, true), (4.75, false), (5.25, true), (5.75, false)] {
            let expected = if hole && kind == RenderKind::Cutout {
                Color::BLUE
            } else {
                Color::GREEN
            };
            let x = (320.0 + (world_x - 5.0) * scale) as i32;
            assert_eq!(
                image.get_color(x, 320),
                expected,
                "{label} pixel at x={world_x}"
            );
        }
    }
}
