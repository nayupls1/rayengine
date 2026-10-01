//! Bounded streaming and multi-frame uploads; arrow keys move focus, SPACE edits.
use rayengine::{prelude::*, upload::UploadBudget};
use rayengine_voxel::prelude::*;
use std::{sync::Arc, time::Duration};
const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const FORWARD: Action = Action(2);
const BACK: Action = Action(3);
const EDIT: Action = Action(4);
struct Demo {
    world: VoxelWorld,
    cpu: ChunkStreamer,
    gpu: StreamRenderer,
    materials: VoxelMaterials,
    stone: BlockId,
    focus: ChunkPos,
    dirty: bool,
    report: StreamReport,
    render: StreamRenderReport,
}
impl Demo {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut registry = BlockRegistry::new();
        let stone = registry.register(BlockDef::new("demo:stone"))?;
        let config = StreamConfig::default();
        let cpu = ChunkStreamer::new(config, move |p, registry, cancel| {
            let mut chunk = Chunk::filled(registry, BlockId::AIR)?;
            // Small, deliberately simple fixture; terrain generation is game-owned.
            for z in 0..16 {
                for x in 0..16 {
                    if cancel.is_cancelled() {
                        return Err(VoxelError::Allocation);
                    }
                    let height = 2 + (p.x.wrapping_add(p.z).rem_euclid(3) as u8);
                    for y in 0..height {
                        chunk.set(LocalPos::new(x, y, z)?, stone)?;
                    }
                }
            }
            chunk.mark_saved(chunk.revision()); // Reproducible unedited terrain.
            Ok(chunk)
        })?;
        Ok(Self {
            world: VoxelWorld::new(Arc::new(registry), config.max_resident),
            cpu,
            gpu: StreamRenderer::new(StreamRenderConfig::default())?,
            materials: VoxelMaterials::new(),
            stone,
            focus: ChunkPos::default(),
            dirty: false,
            report: StreamReport::default(),
            render: StreamRenderReport::default(),
        })
    }
}
impl Game for Demo {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_LEFT)
            .bind(RIGHT, KeyboardKey::KEY_RIGHT)
            .bind(FORWARD, KeyboardKey::KEY_UP)
            .bind(BACK, KeyboardKey::KEY_DOWN)
            .bind(EDIT, KeyboardKey::KEY_SPACE)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let material = ctx.material(MaterialDesc {
            tint: Color::new(112, 172, 89, 255),
            ..Default::default()
        })?;
        self.materials.bind(
            SurfaceKey {
                tile: TileId(0),
                layer: MeshLayer::Opaque,
            },
            material,
        )?;
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        let dx = i32::from(ctx.input.pressed(RIGHT)) - i32::from(ctx.input.pressed(LEFT));
        let dz = i32::from(ctx.input.pressed(BACK)) - i32::from(ctx.input.pressed(FORWARD));
        let next = ChunkPos::new(
            self.focus.x.saturating_add(dx),
            0,
            self.focus.z.saturating_add(dz),
        );
        if next.origin().is_ok() {
            self.focus = next;
        }
        if ctx.input.pressed(EDIT) {
            let cell = self.focus.block(LocalPos::new(8, 6, 8).unwrap()).unwrap();
            self.dirty = !self.dirty;
            let _ = self
                .world
                .set_block(cell, if self.dirty { self.stone } else { BlockId::AIR });
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        // This demo has no save backend: edited chunks stay pinned instead of losing edits.
        self.report = self
            .cpu
            .tick(&mut self.world, self.focus, |_, _, _| Eviction::Keep)
            .unwrap();
        self.render = self.gpu.pump(
            &self.world,
            &mut self.cpu,
            &self.materials,
            frame,
            UploadBudget {
                max_requests: 2,
                max_bytes: 512 * 1024,
                max_time: Duration::from_millis(2),
            },
        );
        frame.clear(Color::new(104, 158, 194, 255));
        let origin = self.focus.origin().unwrap();
        let camera = Camera3D {
            position: Vec3::new(43.0, 38.0, 51.0),
            target: Vec3::new(8.0, 2.0, 8.0),
            ..Default::default()
        };
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.world_3d(camera, |canvas| {
            for chunk in self.gpu.chunks() {
                chunk.draw(canvas, &view, origin);
            }
        });
        frame.ui(|ui| {
            ui.text(
                "ARROWS move chunk focus | SPACE edit | ESC exit",
                Vec2::splat(20.0),
                18.0,
                Color::WHITE,
            );
            ui.text(
                &format!(
                    "resident {} | jobs {} | ready {} | pinned {} | GPU {} meshes, {} bytes",
                    self.report.resident,
                    self.report.jobs,
                    self.report.ready,
                    self.report.pinned,
                    self.render.resources.meshes,
                    self.render.resources.buffer_bytes
                ),
                Vec2::new(20.0, 48.0),
                18.0,
                Color::WHITE,
            );
            if let Some(error) = &self.render.error {
                ui.text(&error.to_string(), Vec2::new(20.0, 76.0), 18.0, Color::RED);
            }
        });
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new("Voxel streaming");
    config.audio = false;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Demo::new()?)?;
    Ok(())
}
