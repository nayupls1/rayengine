
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
                self.game.terrain.blocks().dirt,
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
                self.game.terrain.blocks().dirt,
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
        assert_eq!(
            frame.assets.resource_counts().meshes as usize,
            r.meshes + self.game.cracks.len()
        );
        assert!(frame.assets.resource_counts().generated_mesh_bytes as usize > r.buffer_bytes);
        assert!(r.meshes > 0);
        *self.result.lock().unwrap() = Some((r.meshes, r.buffer_bytes));
        self.game.gpu.unload(&mut self.game.cpu, frame.assets);
        self.game.cpu.shutdown();
        for mesh in self.game.cracks.drain(..) {
            frame.assets.unload_mesh(mesh);
        }
        self.game.materials.unload(frame.assets);
        frame
            .assets
            .unload_texture(self.game.texture.take().unwrap());
        let counts = frame.assets.resource_counts();
        assert_eq!(counts.meshes, 0);
        assert_eq!(counts.textures, 0);
        assert_eq!(counts.texture_bytes, 0);
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
mod survival_tests;
