use super::*;
use crate::{
    persistence::{SaveStatus, Store},
    survival::Item,
    textures::TextureSet,
};
use rayengine_core::save::SaveOptions;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct SaveProbe {
    base: Probe,
    reload: bool,
    exercised: bool,
    complete: Arc<Mutex<bool>>,
}
impl SaveProbe {
    fn wait_save(&mut self, frame: &mut Frame<'_, '_>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.base.game.draw(frame);
            let saving = self.base.game.saving.as_ref().unwrap();
            assert!(saving.error().is_none(), "{:?}", saving.error());
            if saving.settled() {
                break;
            }
            assert!(Instant::now() < deadline, "checkpoint timed out");
            std::thread::yield_now();
        }
    }
    fn tick(&mut self, input: &mut Input, size: Vec2) -> bool {
        let quit = self.base.game.advance(
            input,
            UiInput::from_actions(input, None, UI_ACTIONS, true),
            size,
            1.0 / 60.0,
        );
        input.consume_edges();
        quit
    }
    fn exercise(&mut self, frame: &mut Frame<'_, '_>) {
        self.base.settle(frame);
        let p = self.base.game.spawn;
        let target = BlockPos::new(p.x, p.y + 2, p.z - 1);
        if self.reload {
            assert_eq!(self.base.game.world.block(target), Some(BlockId::AIR));
            assert_eq!(self.base.game.survival.inventory.count(Item::Log), 1);
            assert_eq!(self.base.game.survival.inventory.count(Item::Dirt), 4);
            assert_eq!(self.base.game.survival.inventory.count(Item::Stone), 2);
            assert_eq!(self.base.game.survival.health.value(), 17);
            assert!((self.base.game.player.controller.yaw() - 0.7).abs() < 0.0001);
            assert_eq!(
                self.base
                    .game
                    .saving
                    .as_ref()
                    .unwrap()
                    .checkpoint()
                    .terrain()
                    .info()
                    .seed,
                42
            );
            *self.complete.lock().unwrap() = true;
            return;
        }
        for z in p.z - 3..=p.z {
            self.base
                .game
                .world
                .set_block(BlockPos::new(p.x, p.y + 2, z), BlockId::AIR)
                .unwrap();
        }
        self.base
            .game
            .world
            .set_block(target, self.base.game.terrain.blocks().wood)
            .unwrap();
        self.base.game.player.controller.set_look(0.0, 0.0).unwrap();
        let mut input = Input::with_capacity(24);
        input.set(MINE, true);
        for _ in 0..65 {
            assert!(!self.tick(&mut input, frame.viewport.logical_size));
        }
        input.set(MINE, false);
        assert_eq!(self.base.game.survival.inventory.count(Item::Log), 1);
        assert_eq!(self.base.game.world.block(target), Some(BlockId::AIR));
        // F5 also works while inventory freezes gameplay.
        input.set(MENU, true);
        input.set(SAVE, true);
        assert!(!self.tick(&mut input, frame.viewport.logical_size));
        assert!(self.base.game.menu.open());
        let frozen = self.base.game.player.position();
        self.wait_save(frame);
        assert_eq!(self.base.game.player.position(), frozen);
        let snapshot = self.base.game.saving.as_ref().unwrap().checkpoint();
        assert_eq!(snapshot.survival().unwrap().inventory.count(Item::Log), 1);
        assert_eq!(
            snapshot
                .chunk(target.split().0)
                .unwrap()
                .get(target.split().1),
            BlockId::AIR
        );
        // Quit queues the newest state, waits for it and keeps the cursor released.
        self.base.game.survival.inventory.insert(Item::Dirt, 4);
        self.base.game.survival.health.damage(3);
        input.set(SAVE, false);
        input.set(QUIT, true);
        assert!(!self.tick(&mut input, frame.viewport.logical_size));
        assert!(self.base.game.closing);
        assert_eq!(self.base.game.cursor_mode(), CursorMode::Free);
        self.wait_save(frame);
        assert!(self.tick(&mut input, frame.viewport.logical_size));
        assert_eq!(
            self.base.game.saving.as_ref().unwrap().status(),
            SaveStatus::Saved
        );
        // A final native-close Drop captures state newer than that settled request.
        self.base.game.survival.inventory.insert(Item::Stone, 2);
        self.base
            .game
            .player
            .controller
            .set_look(0.7, -0.1)
            .unwrap();
        *self.complete.lock().unwrap() = true;
    }
}
impl Game for SaveProbe {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.base.game.init(ctx)
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        if !self.exercised {
            self.exercise(frame);
            self.exercised = true;
        }
        self.base.game.draw(frame);
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_minecraft_save_f5_quit_native_close_and_reload() {
    let dir = Directory(std::env::temp_dir().join(format!(
            "rayengine-native-save-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    let slot = dir.0.join("nested/world.save");
    for reload in [false, true] {
        let game = TerrainPreview::with_save(
            None,
            TextureSet::fallback(),
            Store::open(&slot, SaveOptions::default()).unwrap(),
        )
        .unwrap();
        let outcome = game.save_outcome();
        let complete = Arc::new(Mutex::new(false));
        let mut config = Config::new("Minecraft persistence probe");
        config.audio = false;
        config.vsync = false;
        config.window_size = (640, 360);
        App::new(config)
            .with_options(RunOptions {
                hidden: true,
                uncapped: true,
                frames: Some(2),
                screenshot: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../artifacts/smoke/minecraft-save-{}.png",
                    if reload { "reload" } else { "write" }
                ))),
                ..Default::default()
            })
            .run(SaveProbe {
                base: Probe {
                    game,
                    result: Arc::new(Mutex::new(None)),
                },
                reload,
                exercised: false,
                complete: complete.clone(),
            })
            .unwrap();
        outcome.check().unwrap();
        assert!(*complete.lock().unwrap());
    }
}
