//! Complete version-one scenario using the real survival/update/draw/save paths.
use super::{survival_tests::SurvivalProbe, *};
use crate::{
    persistence::{PlayerState, Store},
    survival::{MAX_HEALTH, SurvivalState},
    terrain::chunk_fingerprint,
    textures::TextureSet,
};
use rayengine_core::save::SaveOptions;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone)]
struct Expected {
    player: PlayerState,
    survival: SurvivalState,
    edits: [(BlockPos, BlockId); 2],
    untouched: ChunkPos,
    fingerprint: u64,
    edited_chunks: usize,
}
struct ReleaseProbe {
    scene: SurvivalProbe,
    reload: bool,
    expected: Arc<Mutex<Option<Expected>>>,
    complete: Arc<Mutex<bool>>,
}
impl ReleaseProbe {
    fn wait_save(&mut self, frame: &mut Frame<'_, '_>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.scene.base.game.draw(frame);
            let saving = self.scene.base.game.saving.as_ref().unwrap();
            assert!(saving.error().is_none(), "{:?}", saving.error());
            if saving.settled() {
                break;
            }
            assert!(Instant::now() < deadline, "release checkpoint timed out");
            std::thread::yield_now();
        }
    }
    fn play(&mut self, frame: &mut Frame<'_, '_>) {
        // This deterministic fixture exercises gather -> UI crafting/equipment ->
        // mine/place -> real fall damage -> death-screen respawn. Assertions run
        // inside the shared survival scene; screen4 leaves the scene playable.
        self.scene.exercise(frame);
        let support = self.scene.base.game.spawn;
        let blocks = self.scene.base.game.terrain.blocks();
        let edits = [
            (
                BlockPos::new(support.x, support.y + 2, support.z - 1),
                BlockId::AIR,
            ),
            (
                BlockPos::new(support.x, support.y + 2, support.z - 2),
                blocks.stone,
            ),
        ];
        let mut input = Input::with_capacity(24);
        input.set(SAVE, true);
        self.scene
            .tick(&mut input, frame.viewport.logical_size, None);
        self.wait_save(frame);
        assert!(
            !self
                .scene
                .base
                .game
                .world
                .chunk(edits[0].0.split().0)
                .unwrap()
                .is_dirty()
        );
        // A distant safe spawn deterministically exercises streaming travel. The
        // intervening terrain is not inserted by the test: normal loaders build it.
        let far = self
            .scene
            .base
            .game
            .terrain
            .find_spawn(80, -80, 16, 1089)
            .unwrap();
        self.scene.base.game.player = Player::new(far.feet()).unwrap();
        // This fixture relocates to a new spawn; reset the old height history so
        // the synthetic relocation is not counted as a thirty-block fall.
        self.scene.base.game.survival.health.respawn();
        self.scene.base.game.focus = self.scene.base.game.player.focus();
        self.scene.base.settle(frame);
        for (position, _) in edits {
            assert!(
                self.scene
                    .base
                    .game
                    .world
                    .chunk(position.split().0)
                    .is_none()
            );
        }
        input.set(SAVE, false);
        self.scene
            .tick(&mut input, frame.viewport.logical_size, None);
        let before = self.scene.base.game.player.position();
        input.set(FORWARD, true);
        input.set(JUMP, true);
        input.add_pointer_delta(Vec2::new(15.0, -2.0));
        for _ in 0..20 {
            self.scene
                .tick(&mut input, frame.viewport.logical_size, None);
        }
        assert!(
            self.scene.base.game.player.position().distance(before) > 0.05,
            "movement: before={before:?} after={:?} waiting={} menu={} health={} focus={:?} chunks={}",
            self.scene.base.game.player.position(),
            self.scene.base.game.waiting,
            self.scene.base.game.menu.open(),
            self.scene.base.game.survival.health.value(),
            self.scene.base.game.focus,
            self.scene.base.game.world.len()
        );
        assert!(self.scene.base.game.player.controller.yaw().abs() > 0.001);
        input.set(FORWARD, false);
        input.set(JUMP, false);
        input.set(SAVE, true);
        self.scene
            .tick(&mut input, frame.viewport.logical_size, None);
        self.wait_save(frame);
        let game = &self.scene.base.game;
        let untouched = far.support.split().0;
        let fingerprint = chunk_fingerprint(game.world.chunk(untouched).unwrap());
        assert_eq!(
            fingerprint,
            chunk_fingerprint(&game.terrain.chunk(untouched).unwrap())
        );
        let expected = Expected {
            player: PlayerState::capture(&game.player),
            survival: game.survival.snapshot(),
            edits,
            untouched,
            fingerprint,
            edited_chunks: game.saving.as_ref().unwrap().checkpoint().edited_chunks(),
        };
        assert!(
            expected.edited_chunks > 0
                && expected.edited_chunks <= crate::persistence::MAX_EDITED_CHUNKS
        );
        assert_eq!(game.survival.health.value(), MAX_HEALTH);
        // The real quit path must wait for the latest pose, inventory and edits.
        input.set(QUIT, true);
        let game = &mut self.scene.base.game;
        assert!(!game.advance(
            &input,
            UiInput::default(),
            frame.viewport.logical_size,
            1.0 / 60.0
        ));
        assert!(game.closing);
        assert_eq!(game.cursor_mode(), CursorMode::Free);
        input.consume_edges();
        self.wait_save(frame);
        assert!(self.scene.base.game.advance(
            &input,
            UiInput::default(),
            frame.viewport.logical_size,
            1.0 / 60.0
        ));
        *self.expected.lock().unwrap() = Some(expected);
    }
    fn reload(&mut self, frame: &mut Frame<'_, '_>) {
        let expected = self.expected.lock().unwrap().clone().unwrap();
        let game = &self.scene.base.game;
        assert_eq!(PlayerState::capture(&game.player), expected.player);
        assert_eq!(game.survival.snapshot(), expected.survival);
        assert_eq!(
            game.saving.as_ref().unwrap().checkpoint().edited_chunks(),
            expected.edited_chunks
        );
        self.scene.base.settle(frame);
        assert_eq!(
            chunk_fingerprint(
                self.scene
                    .base
                    .game
                    .world
                    .chunk(expected.untouched)
                    .unwrap()
            ),
            expected.fingerprint
        );
        // Return to the edited region. The ordinary saved loader and renderer
        // must restore it after both eviction and process-style scene recreation.
        let spawn = self
            .scene
            .base
            .game
            .terrain
            .find_spawn(0, 0, 16, 1089)
            .unwrap();
        self.scene.base.game.player = Player::new(spawn.feet()).unwrap();
        self.scene.base.game.focus = self.scene.base.game.player.focus();
        self.scene.base.settle(frame);
        for (position, block) in expected.edits {
            assert_eq!(self.scene.base.game.world.block(position), Some(block));
            assert!(
                self.scene
                    .base
                    .game
                    .gpu
                    .chunk(position.split().0)
                    .unwrap()
                    .dependencies()
                    .unwrap()
                    .is_current(&self.scene.base.game.world)
            );
        }
        // Leave the exact restored state installed; inspection must not change
        // the final checkpoint when the second scene is dropped.
        self.scene.base.game.player = expected.player.restore().unwrap();
        self.scene.base.game.focus = self.scene.base.game.player.focus();
        self.scene.base.settle(frame);
        // A zero-time production tick checks loaded colliders/readiness without
        // moving the restored airborne pose or changing its saved fall history.
        assert!(!self.scene.base.game.advance(
            &Input::with_capacity(24),
            UiInput::default(),
            frame.viewport.logical_size,
            0.0
        ));
        assert!(!self.scene.base.game.waiting);
        assert_eq!(
            PlayerState::capture(&self.scene.base.game.player),
            expected.player
        );
        assert_eq!(self.scene.base.game.survival.snapshot(), expected.survival);
    }
}
impl Game for ReleaseProbe {
    fn cursor_mode(&self) -> CursorMode {
        self.scene.base.game.cursor_mode()
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.scene.base.game.init(ctx)
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        if frame.index == 0 {
            if self.reload {
                self.reload(frame);
            } else {
                self.play(frame);
            }
            *self.complete.lock().unwrap() = true;
        } else {
            self.scene.base.game.draw(frame);
        }
        if frame.index == 1 {
            let base = &mut self.scene.base;
            let resources = base.game.gpu.resources();
            assert!(
                resources.chunks <= 160
                    && resources.meshes <= 4096
                    && resources.buffer_bytes <= 64 * 1024 * 1024
            );
            assert_eq!(
                frame.assets.resource_counts().meshes as usize,
                resources.meshes + breaking::STAGES
            );
            base.game.gpu.unload(&mut base.game.cpu, frame.assets);
            base.game.cpu.shutdown();
            for mesh in base.game.cracks.drain(..) {
                frame.assets.unload_mesh(mesh);
            }
            base.game.materials.unload(frame.assets);
            frame
                .assets
                .unload_texture(base.game.texture.take().unwrap());
            // The SDK owns its built-in material shader until run shutdown;
            // every scene-owned resource and payload must already be gone.
            assert_eq!(
                frame.assets.resource_counts(),
                rayengine::diagnostics::ResourceCounts {
                    shaders: 2,
                    ..Default::default()
                }
            );
        }
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_minecraft_release_play_travel_save_and_reload() {
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let dir = Directory(std::env::temp_dir().join(format!(
            "rayengine-release-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    let slot = dir.0.join("world.save");
    let artifact = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/smoke");
    let expected = Arc::new(Mutex::new(None));
    let mut original = None;
    for reload in [false, true] {
        let game = TerrainPreview::with_save(
            if reload { None } else { Some(42) },
            TextureSet::fallback(),
            Store::open(&slot, SaveOptions::default()).unwrap(),
        )
        .unwrap();
        let outcome = game.save_outcome();
        let complete = Arc::new(Mutex::new(false));
        let mut config = Config::new("Minecraft v0.0.1 release scenario");
        config.audio = false;
        config.exit_key = None;
        config.vsync = false;
        config.window_size = if reload { (800, 1000) } else { (1280, 720) };
        App::new(config)
            .with_options(RunOptions {
                hidden: true,
                uncapped: true,
                frames: Some(2),
                screenshot: Some(artifact.join(format!(
                    "minecraft-release-{}.png",
                    if reload { "reload" } else { "play" }
                ))),
                ..Default::default()
            })
            .run(ReleaseProbe {
                scene: SurvivalProbe {
                    base: Probe {
                        game,
                        result: Arc::new(Mutex::new(None)),
                    },
                    screen: 4,
                    complete: Arc::new(Mutex::new(false)),
                },
                reload,
                expected: expected.clone(),
                complete: complete.clone(),
            })
            .unwrap();
        outcome.check().unwrap();
        assert!(*complete.lock().unwrap());
        let bytes = fs::read(&slot).unwrap();
        if let Some(original) = &original {
            assert_eq!(&bytes, original);
        } else {
            original = Some(bytes);
        }
    }
    let expected = expected.lock().unwrap().clone().unwrap();
    let store = Store::open(&slot, SaveOptions::default()).unwrap();
    let snapshot = store.load().unwrap().unwrap();
    assert_eq!(
        PlayerState::capture(&snapshot.player().unwrap()),
        expected.player
    );
    assert_eq!(snapshot.survival().unwrap().snapshot(), expected.survival);
    fs::create_dir_all(&artifact).unwrap();
    fs::write(artifact.join("minecraft-release.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version":1, "scenario":"minecraft_release_v1", "seed":42,
        "checks":["gather","craft","mine","place","fall_damage","respawn","travel_eviction","save","reload","render_receipts","teardown"],
        "player":expected.player, "survival":expected.survival, "edited_chunks":expected.edited_chunks,
        "untouched_fingerprint":format!("{:016x}",expected.fingerprint),
        "bounds":{"resident_chunks":160,"gpu_meshes":4096,"gpu_buffer_bytes":64*1024*1024,"crack_meshes":breaking::STAGES}
    })).unwrap()).unwrap();
}
