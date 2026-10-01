//! Explicit textured chunk rendering. SPACE edits a border block and rebuilds its neighbors.
use rayengine::{
    prelude::*,
    raylib::prelude::{Image, RaylibTexture2D, TextureFilter},
};
use rayengine_voxel::{glam::Vec4, prelude::*};
use std::{path::PathBuf, sync::Arc};

const EDIT: Action = Action(0);
struct Demo {
    world: VoxelWorld,
    chunks: Vec<RenderedChunk>,
    materials: VoxelMaterials,
    dirty: Vec<ChunkPos>,
    stone: BlockId,
    placed: bool,
    atlas: PathBuf,
    error: Option<String>,
}
impl Demo {
    fn new(atlas: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let mut registry = BlockRegistry::new();
        let mut grass = BlockDef::new("demo:grass");
        grass.textures = [TileId(0); 6];
        grass.textures[Face::PosY.index()] = TileId(1);
        grass.textures[Face::NegY.index()] = TileId(2);
        let grass = registry.register(grass)?;
        let mut stone = BlockDef::new("demo:stone");
        stone.textures = [TileId(2); 6];
        let stone = registry.register(stone)?;
        let mut leaves = BlockDef::new("demo:leaves");
        leaves.render = RenderKind::Cutout;
        leaves.textures = [TileId(3); 6];
        let leaves = registry.register(leaves)?;
        let registry = Arc::new(registry);
        let mut world = VoxelWorld::new(registry.clone(), 4);
        let mut chunks = Vec::new();
        for z in -1..=0 {
            for x in -1..=0 {
                let pos = ChunkPos::new(x, 0, z);
                let mut chunk = Chunk::filled(registry.clone(), BlockId::AIR)?;
                for y in 0..3 {
                    for z in 0..16 {
                        for x in 0..16 {
                            chunk
                                .set(LocalPos::new(x, y, z)?, if y == 2 { grass } else { stone })?;
                        }
                    }
                }
                world.insert_chunk(pos, chunk)?;
                chunks.push(RenderedChunk::new(pos)?);
            }
        }
        for x in -5..=-3 {
            for y in 3..=6 {
                world.set_block(BlockPos::new(x, y, 5), stone)?;
            }
        }
        for x in 3..=6 {
            for y in 4..=6 {
                for z in -5..=-3 {
                    world.set_block(BlockPos::new(x, y, z), leaves)?;
                }
            }
        }
        Ok(Self {
            world,
            chunks,
            materials: VoxelMaterials::new(),
            dirty: Vec::with_capacity(4),
            stone,
            placed: false,
            atlas,
            error: None,
        })
    }
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new().bind(EDIT, KeyboardKey::KEY_SPACE)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let texture = ctx.texture(&self.atlas)?;
        ctx.assets
            .texture(texture)
            .unwrap()
            .set_texture_filter(ctx.thread, TextureFilter::TEXTURE_FILTER_POINT);
        let tiles: Vec<_> = (0..4)
            .map(|i| TileTexture {
                tile: TileId(i),
                texture,
                rect: Vec4::new(f32::from(i) * 0.25, 0.0, 0.25, 1.0),
            })
            .collect();
        self.materials = VoxelMaterials::create(ctx, &tiles, 0.5)?;
        for chunk in &mut self.chunks {
            let mesh = MeshInput::capture(&self.world, chunk.position())
                .and_then(|input| input.build(MeshingOptions::default()))
                .map_err(|e| Error::Asset(e.to_string()))?;
            chunk.upload_init(&self.world, &mesh, &self.materials, ctx)?;
        }
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        if ctx.input.pressed(EDIT) {
            self.placed = !self.placed;
            match self.world.set_block(
                BlockPos::new(0, 3, 0),
                if self.placed {
                    self.stone
                } else {
                    BlockId::AIR
                },
            ) {
                Ok(Some(edit)) => {
                    for &pos in edit.affected_chunks.as_slice() {
                        if self.world.chunk(pos).is_some() && !self.dirty.contains(&pos) {
                            self.dirty.push(pos);
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        // Explicit synchronous rebuilds; stream_render shows the optional bounded scheduler.
        for pos in self.dirty.drain(..) {
            let result = MeshInput::capture(&self.world, pos)
                .and_then(|input| input.build(MeshingOptions::default()));
            match result {
                Ok(mesh) => {
                    let chunk = self
                        .chunks
                        .iter_mut()
                        .find(|chunk| chunk.position() == pos)
                        .unwrap();
                    self.error = chunk
                        .replace(&self.world, &mesh, &self.materials, frame)
                        .err()
                        .map(|e| e.to_string());
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        frame.clear(Color::new(104, 158, 194, 255));
        let camera = Camera3D {
            position: Vec3::new(27.0, 22.0, 31.0),
            target: Vec3::new(0.0, 2.0, 0.0),
            ..Default::default()
        };
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.world_3d(camera, |canvas| {
            for chunk in &self.chunks {
                chunk.draw(canvas, &view, BlockPos::default());
            }
        });
        let quads: usize = self.chunks.iter().map(|chunk| chunk.stats().quads).sum();
        let bytes: usize = self
            .chunks
            .iter()
            .map(|chunk| chunk.stats().buffer_bytes)
            .sum();
        frame.ui(|ui| {
            ui.text(
                "Textured voxel chunks | SPACE edit border block | ESC exit",
                Vec2::splat(20.0),
                18.0,
                Color::WHITE,
            );
            ui.text(
                &format!("{quads} merged quads | {bytes} buffer bytes"),
                Vec2::new(20.0, 48.0),
                18.0,
                Color::WHITE,
            );
            if let Some(error) = &self.error {
                ui.text(error, Vec2::new(20.0, 76.0), 18.0, Color::RED);
            }
        });
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path =
        std::env::temp_dir().join(format!("rayengine-voxel-atlas-{}.png", std::process::id()));
    // Original procedural fallback tiles; no installed game/assets required.
    let mut atlas = Image::gen_image_color(64, 16, Color::new(42, 126, 57, 0));
    for (i, color) in [
        Color::new(120, 83, 50, 255),
        Color::new(87, 157, 57, 255),
        Color::new(132, 140, 150, 255),
    ]
    .into_iter()
    .enumerate()
    {
        atlas.draw_rectangle(i as i32 * 16, 0, 16, 16, color);
        for y in (0..16).step_by(4) {
            for x in (0..16).step_by(4) {
                atlas.draw_rectangle(
                    i as i32 * 16 + x,
                    y,
                    2,
                    2,
                    Color::new(
                        color.r.saturating_add(20),
                        color.g.saturating_add(20),
                        color.b.saturating_add(20),
                        255,
                    ),
                );
            }
        }
    }
    atlas.draw_rectangle(0, 0, 16, 4, Color::new(87, 157, 57, 255));
    for y in 0..16 {
        for x in 0..16 {
            if (x + y) % 3 != 0 {
                atlas.draw_rectangle(48 + x, y, 1, 1, Color::new(42, 126, 57, 255));
            }
        }
    }
    std::fs::write(&path, &*atlas.export_image_to_memory(".png")?)?;
    let result = (|| {
        let mut config = Config::new("Voxel rendering");
        config.audio = false;
        App::new(config)
            .with_options(RunOptions::from_env()?)
            .run(Demo::new(path.clone())?)?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })();
    let _ = std::fs::remove_file(path);
    result
}
