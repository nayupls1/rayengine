//! Native image checks use asymmetric, original fixtures; no installed assets.
use super::*;
use crate::terrain::{Terrain, TerrainSettings};
use rayengine::{
    prelude::*,
    raylib::prelude::{Image, RaylibTexture2D, TextureFilter},
};
use rayengine_voxel::prelude::*;
struct Probe {
    case: u64,
    terrain: Terrain,
    world: VoxelWorld,
    atlas: Atlas,
    texture: Option<TextureId>,
    materials: VoxelMaterials,
    chunk: RenderedChunk,
}
impl Game for Probe {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let image = Image::load_image_from_mem(".png", &self.atlas.png().unwrap()).unwrap();
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
                rect: self.atlas.rects[tile as usize],
            }),
            0.5,
        )?;
        self.texture = Some(texture);
        assert_eq!(ctx.assets.resource_counts().textures, 1);
        assert_eq!(
            ctx.assets.resource_counts().texture_bytes,
            u64::from(self.atlas.width) * u64::from(self.atlas.height) * 4
        );
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        let p = BlockPos::new(4, 4, 4);
        let case = self.case;
        let id = if case < 6 {
            self.terrain.blocks().grass
        } else if case < 8 {
            self.terrain.blocks().wood
        } else {
            self.terrain.blocks().leaves
        };
        self.world.set_block(p, id).unwrap();
        if case == 8 {
            self.world
                .set_block(BlockPos::new(4, 4, 3), self.terrain.blocks().stone)
                .unwrap();
        }
        let mesh = MeshInput::capture(&self.world, ChunkPos::default())
            .unwrap()
            .build(MeshingOptions {
                shading: FaceShading([255; 6]),
                ..Default::default()
            })
            .unwrap();
        self.chunk
            .replace(&self.world, &mesh, &self.materials, frame)
            .unwrap();
        let normal = match case {
            0 => Vec3::X,
            1 => -Vec3::X,
            2 => Vec3::Z,
            3 => -Vec3::Z,
            4 | 6 => Vec3::Y,
            5 => -Vec3::Y,
            _ => Vec3::Z,
        };
        let center = Vec3::splat(4.5);
        let camera = Camera3D {
            position: center + normal * 3.0,
            target: center,
            up: if normal.y != 0.0 { Vec3::Z } else { Vec3::Y },
            vertical_fov: 60.0,
        };
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 100.0).unwrap();
        frame.clear(Color::BLACK);
        frame.world_3d(camera, |canvas| {
            assert!(
                self.chunk
                    .draw(canvas, &view, BlockPos::default())
                    .submitted
                    > 0
            );
        });
        if case == 8 && frame.index == 1 {
            self.chunk.unload(frame.assets);
            self.materials.unload(frame.assets);
            frame.assets.unload_texture(self.texture.take().unwrap());
            let counts = frame.assets.resource_counts();
            assert_eq!(counts.textures, 0);
            assert_eq!(counts.texture_bytes, 0);
            assert_eq!(counts.meshes, 0);
        }
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_minecraft_texture_orientation_face_mapping_cutout_and_teardown() {
    let tiles = Tile::ALL.map(|tile| {
        let mut pixels = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let color = match tile {
                    Tile::GrassSide => {
                        if y < 8 {
                            Color::RED
                        } else {
                            Color::BLUE
                        }
                    }
                    Tile::GrassTop => Color::GREEN,
                    Tile::Dirt => Color::BROWN,
                    Tile::LogTop => Color::YELLOW,
                    Tile::LogSide => Color::ORANGE,
                    Tile::Leaves => {
                        if x < 8 {
                            Color::new(0, 255, 0, 0)
                        } else {
                            Color::GREEN
                        }
                    }
                    _ => Color::BLUE,
                };
                pixels.extend([color.r, color.g, color.b, color.a]);
            }
        }
        TileImage::new(16, pixels).unwrap()
    });
    let directory = std::env::temp_dir().join(format!(
        "rayengine-minecraft-native-textures-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    for case in 0..9 {
        let screenshot = directory.join(format!("case-{case}.png"));
        let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
        let mut world = VoxelWorld::new(terrain.registry(), 1);
        world
            .insert_chunk(
                ChunkPos::default(),
                Chunk::filled(terrain.registry(), BlockId::AIR).unwrap(),
            )
            .unwrap();
        let mut config = Config::new("Minecraft texture probe");
        config.audio = false;
        config.vsync = false;
        config.window_size = (128, 128);
        config.reference_size = Vec2::splat(128.0);
        App::new(config)
            .with_options(RunOptions {
                hidden: true,
                uncapped: true,
                frames: Some(2),
                screenshot: Some(screenshot.clone()),
                ..Default::default()
            })
            .run(Probe {
                case,
                world,
                terrain,
                atlas: TextureSet::new(tiles.clone(), "native fixture").pack(),
                texture: None,
                materials: VoxelMaterials::new(),
                chunk: RenderedChunk::new(ChunkPos::default()).unwrap(),
            })
            .unwrap();
        std::fs::copy(
            &screenshot,
            format!("/tmp/rayengine-texture-case-{case}.png"),
        )
        .unwrap();
        let image = Image::load_image(screenshot.to_str().unwrap()).unwrap();
        let (x, y) = (image.width / 2, image.height / 2);
        match case {
            0..=3 => {
                assert_eq!(
                    image.get_color(x, y - 11),
                    Color::RED,
                    "side {case} must be upright"
                );
                assert_eq!(
                    image.get_color(x, y + 11),
                    Color::BLUE,
                    "side {case} must be upright"
                );
            }
            4 => assert_eq!(image.get_color(x, y), Color::GREEN, "grass top tile"),
            5 => assert_eq!(image.get_color(x, y), Color::BROWN, "grass bottom tile"),
            6 => assert_eq!(image.get_color(x, y), Color::YELLOW, "log end tile"),
            7 => assert_eq!(image.get_color(x, y), Color::ORANGE, "log bark tile"),
            _ => {
                assert_eq!(
                    image.get_color(x - 11, y),
                    Color::BLUE,
                    "cutout hole reveals stone behind"
                );
                assert_eq!(
                    image.get_color(x + 11, y),
                    Color::GREEN,
                    "opaque leaf texel"
                );
            }
        }
    }
}
