//! Playable first-person voxel scene; survival, textures and saves arrive later.
use crate::gameplay::{Interaction, InteractionReport, Player};
use crate::terrain::{GENERATOR_VERSION, Terrain, TerrainSettings};
use rayengine::raylib::prelude::MouseButton;
use rayengine::{prelude::*, upload::UploadBudget};
use rayengine_voxel::prelude::*;
use std::{sync::Arc, time::Duration};
const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const FORWARD: Action = Action(2);
const BACK: Action = Action(3);
const JUMP: Action = Action(4);
const SPRINT: Action = Action(5);
const MINE: Action = Action(6);
const PLACE: Action = Action(7);
const DIRT: Action = Action(8);
const STONE: Action = Action(9);
const WOOD: Action = Action(10);
const ACTIONS: FirstPersonActions = FirstPersonActions {
    left: LEFT,
    right: RIGHT,
    forward: FORWARD,
    back: BACK,
    jump: JUMP,
    sprint: Some(SPRINT),
    turn_left: None,
    turn_right: None,
};
/// Streamed first-person scene of the recipe used by the headless tools.
/// Original flat fallback colors require no installed Minecraft assets.
pub struct TerrainPreview {
    terrain: Arc<Terrain>,
    world: VoxelWorld,
    cpu: ChunkStreamer,
    gpu: StreamRenderer,
    materials: VoxelMaterials,
    focus: ChunkPos,
    player: Player,
    interaction: Interaction,
    interaction_report: InteractionReport,
    place_block: BlockId,
    waiting: bool,
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
            player: Player::new(spawn.feet())?,
            interaction: Interaction::default(),
            interaction_report: InteractionReport::default(),
            place_block: terrain.blocks().dirt,
            waiting: true,
            terrain,
            cpu,
            gpu: StreamRenderer::new(StreamRenderConfig {
                max_chunks: 160,
                max_meshes: 4096,
                ..Default::default()
            })?,
            materials: VoxelMaterials::new(),
            focus,
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
    fn cursor_mode(&self) -> CursorMode {
        CursorMode::Captured
    }
    fn bindings(&self) -> Bindings {
        Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(FORWARD, KeyboardKey::KEY_W)
            .bind(BACK, KeyboardKey::KEY_S)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(SPRINT, KeyboardKey::KEY_LEFT_SHIFT)
            .bind(MINE, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(PLACE, Button::Mouse(MouseButton::MOUSE_BUTTON_RIGHT))
            .bind(DIRT, KeyboardKey::KEY_ONE)
            .bind(STONE, KeyboardKey::KEY_TWO)
            .bind(WOOD, KeyboardKey::KEY_THREE)
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
        for (action, block) in [
            (DIRT, self.terrain.blocks().dirt),
            (STONE, self.terrain.blocks().stone),
            (WOOD, self.terrain.blocks().wood),
        ] {
            if ctx.input.pressed(action) {
                self.place_block = block;
            }
        }
        match self.player.step(
            &self.world,
            FirstPersonInput::from_actions(ctx.input, ACTIONS),
            ctx.tick.dt,
        ) {
            Ok(_) => self.waiting = false,
            Err(ColliderError::Unloaded(_)) => self.waiting = true,
            Err(e) => {
                self.waiting = true;
                self.error = Some(e.to_string());
            }
        }
        self.focus = self.player.focus();
        match self.interaction.step(
            &mut self.world,
            &self.player,
            ctx.input.down(MINE),
            ctx.input.pressed(PLACE),
            ctx.tick.dt,
            self.place_block,
        ) {
            Ok(report) => self.interaction_report = report,
            Err(e) => self.error = Some(e.to_string()),
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
        let origin = self.player.origin;
        let camera = self.player.controller.camera(frame.alpha);
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        frame.clear(Color::new(104, 158, 194, 255));
        frame.world_3d(camera, |canvas| {
            for chunk in self.gpu.chunks() {
                chunk.draw(canvas, &view, origin);
            }
            if let Some(hit) = self.interaction_report.selected
                && let Ok(mut bounds) = block_bounds(hit.position, origin)
            {
                bounds.min -= Vec3::splat(0.002);
                bounds.max += Vec3::splat(0.002);
                canvas.wire_cube(bounds, Color::BLACK);
            }
        });
        frame.ui(|ui| {
            ui.circle(ui.logical_size * 0.5, 2.0, Color::WHITE);
            ui.text(
                &format!(
                    "Minecraft demo | seed {} | generator v{GENERATOR_VERSION}",
                    self.terrain.info().seed
                ),
                Vec2::splat(20.0),
                18.0,
                Color::WHITE,
            );
            ui.text(
                "WASD move | mouse look | SPACE jump | SHIFT sprint | ESC exit",
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
            ui.text(
                "Hold LMB mine | RMB place | 1 dirt  2 stone  3 wood",
                Vec2::new(20.0, 104.0),
                18.0,
                Color::WHITE,
            );
            let held = &self.world.registry().get(self.place_block).unwrap().name;
            let target = self
                .interaction_report
                .selected
                .map(|hit| self.world.registry().get(hit.block).unwrap().name.as_str())
                .unwrap_or("none");
            ui.text(
                &format!(
                    "Held: {held} | target: {target} | mining {:.0}%",
                    self.interaction_report.progress * 100.0
                ),
                Vec2::new(20.0, 132.0),
                18.0,
                Color::WHITE,
            );
            if self.waiting {
                ui.text(
                    "Waiting for nearby terrain",
                    Vec2::new(20.0, 160.0),
                    18.0,
                    Color::YELLOW,
                );
            }
            if self.report.pinned > 0 {
                ui.text(
                    "Edited chunks retained in this session; disk saves arrive later",
                    Vec2::new(20.0, 188.0),
                    16.0,
                    Color::YELLOW,
                );
            }
            if let Some(error) = &self.error {
                ui.text(error, Vec2::new(20.0, 216.0), 18.0, Color::RED);
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
    impl Probe {
        fn settle(&mut self, frame: &mut Frame<'_, '_>) {
            // Give this integration probe a bounded settle phase within one native frame.
            // Normal interactive scenes use exactly one tick/pump per presented frame.
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
        }
    }
    impl Game for Probe {
        fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
            self.game.init(ctx)
        }
        fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
        fn draw(&mut self, frame: &mut Frame<'_, '_>) {
            self.settle(frame);
            let p = self
                .game
                .terrain
                .find_spawn(0, 0, 16, 1089)
                .unwrap()
                .support;
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
            self.game
                .player
                .step(&self.game.world, FirstPersonInput::default(), 1.0 / 60.0)
                .unwrap();
            assert!(self.game.player.controller.body.grounded);
            self.game.player.controller.set_look(0.0, -1.5).unwrap();
            let report = self
                .game
                .interaction
                .step(
                    &mut self.game.world,
                    &self.game.player,
                    true,
                    false,
                    1.1,
                    self.game.place_block,
                )
                .unwrap();
            assert_eq!(report.edit.unwrap().position, p);
            assert_eq!(self.game.world.block(p), Some(BlockId::AIR));
            let chunk = p.split().0;
            assert!(
                !self
                    .game
                    .gpu
                    .chunk(chunk)
                    .unwrap()
                    .dependencies()
                    .unwrap()
                    .is_current(&self.game.world)
            );
            let mut colliders = Vec::new();
            let bounds = block_bounds(p, self.game.player.origin).unwrap();
            self.game
                .world
                .collect_colliders(
                    bounds,
                    self.game.player.origin,
                    64,
                    MissingColliders::Reject,
                    &mut colliders,
                )
                .unwrap();
            assert!(!colliders.contains(&bounds));
            self.settle(frame);
            assert!(
                self.game
                    .gpu
                    .chunk(chunk)
                    .unwrap()
                    .dependencies()
                    .unwrap()
                    .is_current(&self.game.world)
            );
            let before = self.game.player.position();
            for _ in 0..8 {
                self.game
                    .player
                    .step(&self.game.world, FirstPersonInput::default(), 1.0 / 60.0)
                    .unwrap();
            }
            assert!(self.game.player.position().y < before.y - 0.05);
            self.game
                .player
                .controller
                .teleport(self.game.player.controller.body.position + Vec3::Y * 2.0)
                .unwrap();
            let report = self
                .game
                .interaction
                .step(
                    &mut self.game.world,
                    &self.game.player,
                    false,
                    true,
                    1.0 / 60.0,
                    self.game.place_block,
                )
                .unwrap();
            assert_eq!(report.edit.unwrap().position, p);
            self.game
                .world
                .collect_colliders(
                    bounds,
                    self.game.player.origin,
                    64,
                    MissingColliders::Reject,
                    &mut colliders,
                )
                .unwrap();
            assert!(colliders.contains(&bounds));
            self.settle(frame);
            assert!(
                self.game
                    .gpu
                    .chunk(chunk)
                    .unwrap()
                    .dependencies()
                    .unwrap()
                    .is_current(&self.game.world)
            );
            for _ in 0..90 {
                self.game
                    .player
                    .step(&self.game.world, FirstPersonInput::default(), 1.0 / 60.0)
                    .unwrap();
            }
            assert!(self.game.player.controller.body.grounded);
            assert!((self.game.player.position().y - before.y).abs() < 0.001);
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
    fn native_minecraft_mining_placement_collision_remesh_and_teardown() {
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
