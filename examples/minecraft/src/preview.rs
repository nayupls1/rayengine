//! Optional native terrain preview; movement/mining/survival arrive in later issues.
use crate::terrain::{GENERATOR_VERSION, Terrain, TerrainSettings};
use rayengine::{prelude::*, upload::UploadBudget};
use rayengine_voxel::prelude::*;
use std::{sync::Arc, time::Duration};
const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const FORWARD: Action = Action(2);
const BACK: Action = Action(3);
/// Streamed overview of the same seeded recipe used by the headless tools.
/// Original flat fallback colors require no installed Minecraft assets.
pub struct TerrainPreview {
    terrain: Arc<Terrain>,
    world: VoxelWorld,
    cpu: ChunkStreamer,
    gpu: StreamRenderer,
    materials: VoxelMaterials,
    focus: ChunkPos,
    support: BlockPos,
    report: StreamReport,
    render_report: StreamRenderReport,
    error: Option<String>,
}
impl TerrainPreview {
    /// Chooses a safe spawn and starts bounded CPU generation workers.
    pub fn new(seed: u64) -> Result<Self, Box<dyn std::error::Error>> {
        let terrain = Arc::new(Terrain::new(seed, TerrainSettings::default())?);
        let spawn = terrain.find_spawn(0, 0, 16, 1089)?;
        let support = spawn.support;
        let focus = support.split().0;
        let config = StreamConfig {
            radius: 2,
            vertical_radius: 2,
            max_resident: 160,
            ..Default::default()
        };
        let recipe = terrain.clone();
        let cpu = ChunkStreamer::new(config, move |pos, registry, token| {
            let cancelled = || token.is_cancelled();
            generate_chunk(
                recipe.as_ref(),
                pos,
                &GenerationContext::new(registry, &cancelled),
            )
        })?;
        Ok(Self {
            world: VoxelWorld::new(terrain.registry(), config.max_resident),
            terrain,
            cpu,
            gpu: StreamRenderer::new(StreamRenderConfig {
                max_chunks: 160,
                max_meshes: 4096,
                ..Default::default()
            })?,
            materials: VoxelMaterials::new(),
            focus,
            support,
            report: StreamReport::default(),
            render_report: StreamRenderReport::default(),
            error: None,
        })
    }
    /// Latest bounded CPU scheduling counters.
    pub fn report(&self) -> StreamReport {
        self.report
    }
    /// Latest upload/resource report; old geometry remains installed on failure.
    pub fn render_report(&self) -> &StreamRenderReport {
        &self.render_report
    }
}
impl Game for TerrainPreview {
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_LEFT)
            .bind(RIGHT, KeyboardKey::KEY_RIGHT)
            .bind(FORWARD, KeyboardKey::KEY_UP)
            .bind(BACK, KeyboardKey::KEY_DOWN)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        for (i, color) in [
            Color::new(63, 63, 69, 255),
            Color::new(126, 130, 137, 255),
            Color::new(130, 91, 58, 255),
            Color::new(92, 158, 62, 255),
            Color::new(68, 72, 78, 255),
            Color::new(173, 142, 115, 255),
            Color::new(116, 83, 48, 255),
            Color::new(47, 119, 49, 255),
        ]
        .into_iter()
        .enumerate()
        {
            let layer = if i == 7 {
                MeshLayer::Cutout
            } else {
                MeshLayer::Opaque
            };
            let alpha = if i == 7 {
                AlphaMode::Cutout(0.5)
            } else {
                AlphaMode::Opaque
            };
            let material = ctx.material(MaterialDesc {
                tint: color,
                alpha,
                ..Default::default()
            })?;
            self.materials.bind(
                SurfaceKey {
                    tile: TileId(i as u16),
                    layer,
                },
                material,
            )?;
        }
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        let dx = i32::from(ctx.input.pressed(RIGHT)) - i32::from(ctx.input.pressed(LEFT));
        let dz = i32::from(ctx.input.pressed(BACK)) - i32::from(ctx.input.pressed(FORWARD));
        if dx != 0 || dz != 0 {
            let next = ChunkPos::new(
                self.focus.x.saturating_add(dx),
                self.focus.y,
                self.focus.z.saturating_add(dz),
            );
            if let Ok(origin) = next.origin() {
                let x = origin.x + 8;
                let z = origin.z + 8;
                self.support = BlockPos::new(x, self.terrain.surface_height(x, z), z);
                self.focus = self.support.split().0;
            }
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        match self
            .cpu
            .tick(&mut self.world, self.focus, |_, _, _| Eviction::Keep)
        {
            Ok(report) => self.report = report,
            Err(e) => self.error = Some(e.to_string()),
        }
        self.render_report = self.gpu.pump(
            &self.world,
            &mut self.cpu,
            &self.materials,
            frame,
            UploadBudget {
                max_requests: 8,
                max_bytes: 2 * 1024 * 1024,
                max_time: Duration::from_millis(3),
            },
        );
        if let Some(e) = &self.render_report.error {
            self.error = Some(e.to_string());
        }
        let origin = self.focus.origin().unwrap();
        let target = Vec3::new(
            (i64::from(self.support.x) - i64::from(origin.x)) as f32 + 0.5,
            (i64::from(self.support.y) - i64::from(origin.y)) as f32 + 1.0,
            (i64::from(self.support.z) - i64::from(origin.z)) as f32 + 0.5,
        );
        let camera = Camera3D {
            position: target + Vec3::new(38.0, 34.0, 44.0),
            target,
            ..Default::default()
        };
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.clear(Color::new(104, 158, 194, 255));
        frame.world_3d(camera, |canvas| {
            for chunk in self.gpu.chunks() {
                chunk.draw(canvas, &view, origin);
            }
        });
        frame.ui(|ui| {
            ui.text(
                &format!(
                    "Terrain preview | seed {} | generator v{GENERATOR_VERSION}",
                    self.terrain.info().seed
                ),
                Vec2::splat(20.0),
                18.0,
                Color::WHITE,
            );
            ui.text(
                "ARROWS move focus | ESC exit",
                Vec2::new(20.0, 48.0),
                18.0,
                Color::WHITE,
            );
            ui.text(
                &format!(
                    "resident {} | jobs {} | GPU {} meshes / {} bytes",
                    self.report.resident,
                    self.report.jobs,
                    self.render_report.resources.meshes,
                    self.render_report.resources.buffer_bytes
                ),
                Vec2::new(20.0, 76.0),
                18.0,
                Color::WHITE,
            );
            if let Some(error) = &self.error {
                ui.text(error, Vec2::new(20.0, 104.0), 18.0, Color::RED);
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Mutex, time::Instant};
    struct Probe {
        game: TerrainPreview,
        result: Arc<Mutex<Option<(usize, usize)>>>,
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.game.init(ctx)
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            // Give this integration probe a bounded settle phase within one native frame.
            // Normal interactive previews use exactly one tick/pump per presented frame.
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                self.game.draw(frame);
                assert!(self.game.error.is_none(), "{:?}", self.game.error);
                let r = self.game.report;
                assert!(r.jobs <= 4 && r.mesh_slots <= 4 && r.resident <= 160);
                let gpu = self.game.render_report.resources;
                assert!(gpu.meshes <= 4096 && gpu.buffer_bytes <= 64 * 1024 * 1024);
                if r.resident == r.desired
                    && r.jobs == 0
                    && r.ready == 0
                    && r.mesh_slots == 0
                    && gpu.chunks == r.desired
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "terrain preview failed to settle"
                );
                std::thread::yield_now();
            }
            let p = self.game.support;
            assert_eq!(
                self.game.world.block(p),
                Some(self.game.terrain.blocks().grass)
            );
            assert_eq!(
                self.game.world.block(BlockPos::new(p.x, p.y + 1, p.z)),
                Some(BlockId::AIR)
            );
            assert_eq!(
                self.game.world.block(BlockPos::new(p.x, p.y + 2, p.z)),
                Some(BlockId::AIR)
            );
            let r = self.game.gpu.resources();
            assert_eq!(frame.assets.resource_counts().meshes as usize, r.meshes);
            assert_eq!(
                frame.assets.resource_counts().generated_mesh_bytes as usize,
                r.buffer_bytes
            );
            assert!(r.meshes > 0);
            *self.result.lock().unwrap() = Some((r.meshes, r.buffer_bytes));
            self.game.gpu.unload(&mut self.game.cpu, frame.assets);
            self.game.cpu.shutdown();
            assert_eq!(frame.assets.resource_counts().meshes, 0);
        }
    }
    #[test]
    #[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
    fn native_minecraft_generation_streaming_spawn_and_teardown() {
        let mut config = Config::new("Minecraft terrain probe");
        config.audio = false;
        config.vsync = false;
        config.window_size = (128, 128);
        let result = Arc::new(Mutex::new(None));
        App::new(config)
            .with_options(RunOptions {
                hidden: true,
                uncapped: true,
                frames: Some(1),
                ..Default::default()
            })
            .run(Probe {
                game: TerrainPreview::new(42).unwrap(),
                result: result.clone(),
            })
            .unwrap();
        assert!(result.lock().unwrap().is_some());
    }
}
