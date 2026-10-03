use super::*;
// Included in the native probe module to reuse its bounded settling helper.
use crate::{
    hud::Layout,
    survival::{Item, MAX_HEALTH},
};
pub(super) struct SurvivalProbe {
    pub(super) base: Probe,
    pub(super) screen: u8,
    pub(super) complete: Arc<Mutex<bool>>,
}
impl SurvivalProbe {
    pub(super) fn tick(&mut self, input: &mut Input, size: Vec2, pointer: Option<Vec2>) {
        let ui = UiInput::from_actions(input, pointer, UI_ACTIONS, true);
        assert!(!self.base.game.advance(input, ui, size, 1.0 / 60.0));
        input.consume_edges();
        assert!(self.base.game.error.is_none(), "{:?}", self.base.game.error);
    }
    pub(super) fn exercise(&mut self, frame: &mut Frame<'_, '_>) {
        self.base.settle(frame);
        let size = frame.viewport.logical_size;
        let b = self.base.game.terrain.blocks();
        let p = self.base.game.spawn;
        let target = BlockPos::new(p.x, p.y + 2, p.z - 1);
        for z in p.z - 3..=p.z {
            self.base
                .game
                .world
                .set_block(BlockPos::new(p.x, p.y + 2, z), BlockId::AIR)
                .unwrap();
        }
        self.base.game.world.set_block(target, b.wood).unwrap();
        self.base.game.player.controller.set_look(0.0, 0.0).unwrap();
        let mut input = Input::with_capacity(23);
        for _ in 0..3 {
            self.tick(&mut input, size, None);
        }
        input.set(MINE, true);
        for _ in 0..65 {
            self.tick(&mut input, size, None);
        }
        assert_eq!(self.base.game.world.block(target), Some(BlockId::AIR));
        assert_eq!(self.base.game.survival.inventory.count(Item::Log), 1);
        assert!(self.base.game.survival.pickups().is_empty());
        input.set(MINE, false);
        self.tick(&mut input, size, None);
        // Gather another log and craft through actual game UI routing.
        self.base.game.world.set_block(target, b.wood).unwrap();
        input.set(MINE, true);
        for _ in 0..65 {
            self.tick(&mut input, size, None);
        }
        input.set(MINE, false);
        self.tick(&mut input, size, None);
        input.set(MENU, true);
        input.set(FORWARD, true);
        input.add_pointer_delta(Vec2::splat(30.0));
        let before = self.base.game.player.position();
        let yaw = self.base.game.player.controller.yaw();
        self.tick(&mut input, size, None);
        assert_eq!(self.base.game.player.position(), before);
        assert_eq!(self.base.game.player.controller.yaw(), yaw);
        assert_eq!(self.base.game.cursor_mode(), CursorMode::Free);
        input.set(MENU, false);
        input.set(FORWARD, false);
        self.tick(&mut input, size, None);
        let l = Layout::new(size);
        for i in [0, 0, 1, 2] {
            input.set(MINE, true);
            self.tick(&mut input, size, Some(l.recipes[i].center()));
            input.set(MINE, false);
            self.tick(&mut input, size, Some(l.recipes[i].center()));
        }
        assert_eq!(
            self.base.game.survival.inventory.count(Item::WoodenPickaxe),
            1
        );
        assert!(self.base.game.world.block(target) == Some(BlockId::AIR));
        // Move crafted tool to hotbar slot zero using the same pointer regions.
        let tool = self
            .base
            .game
            .survival
            .inventory
            .slots()
            .iter()
            .position(|s| s.is_some_and(|s| s.item() == Item::WoodenPickaxe))
            .unwrap();
        for i in [tool, 0] {
            input.set(MINE, true);
            self.tick(&mut input, size, Some(l.slots[i].center()));
            input.set(MINE, false);
            self.tick(&mut input, size, Some(l.slots[i].center()));
        }
        input.set(MENU, true);
        input.set(PLACE, true);
        input.set(JUMP, true);
        self.tick(&mut input, size, None);
        assert_eq!(self.base.game.cursor_mode(), CursorMode::Captured);
        input.set(MENU, false);
        self.tick(&mut input, size, None);
        assert_eq!(self.base.game.player.position(), before); // held jump did not escape the closing gesture
        input.set(PLACE, false);
        input.set(JUMP, false);
        self.tick(&mut input, size, None);
        self.base.game.world.set_block(target, b.stone).unwrap();
        input.set(MINE, true);
        for _ in 0..42 {
            self.tick(&mut input, size, None);
        }
        assert_eq!(self.base.game.survival.inventory.count(Item::Stone), 1);
        input.set(MINE, false);
        self.tick(&mut input, size, None);
        // Place exactly one harvested stone at a collision-safe adjacent cell.
        let stone_slot = self
            .base
            .game
            .survival
            .inventory
            .slots()
            .iter()
            .position(|s| s.is_some_and(|s| s.item() == Item::Stone))
            .unwrap();
        assert!(stone_slot < 9);
        input.set(HOTBAR[stone_slot], true);
        self.tick(&mut input, size, None);
        input.set(HOTBAR[stone_slot], false);
        self.base
            .game
            .world
            .set_block(BlockPos::new(p.x, p.y + 2, p.z - 3), b.wood)
            .unwrap();
        input.set(PLACE, true);
        self.tick(&mut input, size, None);
        assert_eq!(self.base.game.survival.inventory.count(Item::Stone), 0);
        assert_eq!(
            self.base
                .game
                .world
                .block(BlockPos::new(p.x, p.y + 2, p.z - 2)),
            Some(b.stone)
        );
        input.set(PLACE, false);
        self.tick(&mut input, size, None);
        // A real swept-body fall damages health; opening the menu midfall pauses it.
        self.base
            .game
            .player
            .controller
            .teleport(self.base.game.player.controller.body.position + Vec3::Y * 8.0)
            .unwrap();
        self.tick(&mut input, size, None);
        input.set(MENU, true);
        self.tick(&mut input, size, None);
        let y = self.base.game.player.position().y;
        let health = self.base.game.survival.health.value();
        input.set(MENU, false);
        for _ in 0..10 {
            self.tick(&mut input, size, None);
        }
        assert_eq!(self.base.game.player.position().y, y);
        assert_eq!(self.base.game.survival.health.value(), health);
        input.set(MENU, true);
        self.tick(&mut input, size, None);
        input.set(MENU, false);
        for _ in 0..90 {
            self.tick(&mut input, size, None);
        }
        assert!(self.base.game.player.controller.body.grounded);
        assert!(self.base.game.survival.health.value() < MAX_HEALTH);
        // Explicit death-screen action uses edited, loaded spawn validation and keeps inventory.
        self.base.game.survival.health.damage(MAX_HEALTH);
        self.tick(&mut input, size, None);
        assert_eq!(self.base.game.cursor_mode(), CursorMode::Free);
        let inventory = self.base.game.survival.inventory;
        input.set(MINE, true);
        self.tick(&mut input, size, Some(l.close.center()));
        input.set(MINE, false);
        self.tick(&mut input, size, Some(l.close.center()));
        assert_eq!(self.base.game.survival.health.value(), MAX_HEALTH);
        assert_eq!(self.base.game.survival.inventory, inventory);
        self.tick(&mut input, size, None);
        assert_eq!(self.base.game.cursor_mode(), CursorMode::Captured);
        assert_eq!(self.base.game.interaction_report.progress, 0.0);
        // Prepare the final screenshot using genuine state, never an alternative render path.
        match self.screen {
            0 => {
                input.set(MENU, true);
                self.tick(&mut input, size, None);
                assert!(self.base.game.menu.open());
            }
            1 | 3 => {
                self.base.game.player.controller.set_look(0.0, 0.0).unwrap();
                self.base.game.survival.select(0);
                self.base.game.world.set_block(target, b.stone).unwrap();
                input.set(MINE, true);
                for _ in 0..(if self.screen == 3 { 6 } else { 32 }) {
                    self.tick(&mut input, size, None);
                }
                assert!(
                    self.base.game.interaction_report.progress > 0.0
                        && self.base.game.interaction_report.progress < 1.0
                );
            }
            // The release scenario continues from genuine respawned gameplay.
            4 => {}
            _ => {
                self.base.game.survival.health.damage(MAX_HEALTH);
                self.tick(&mut input, size, None);
            }
        }
        self.base.settle(frame);
        *self.complete.lock().unwrap() = true;
    }
}
impl Game for SurvivalProbe {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.base.game.init(ctx)
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        if frame.index == 0 {
            self.exercise(frame);
        } else {
            self.base.game.draw(frame);
        }
        if frame.index == 1 {
            self.base
                .game
                .gpu
                .unload(&mut self.base.game.cpu, frame.assets);
            self.base.game.cpu.shutdown();
            self.base.game.release(frame.assets);
            let counts = frame.assets.resource_counts();
            assert_eq!(counts.meshes, 0);
            assert_eq!(counts.textures, 0);
        }
    }
}
#[test]
#[ignore = "requires native OpenGL; scripts/native_smoke.sh runs serially"]
fn native_minecraft_survival_ui_cracks_damage_respawn_and_teardown() {
    let mut crack_pixels = Vec::new();
    for (name, screen, size) in [
        ("inventory-wide", 0, (1280, 720)),
        ("inventory-portrait", 0, (800, 1000)),
        ("breaking", 1, (800, 600)),
        ("breaking-early", 3, (800, 600)),
        ("death", 2, (800, 600)),
    ] {
        let mut config = Config::new("Minecraft survival probe");
        config.audio = false;
        config.exit_key = None;
        config.vsync = false;
        config.window_size = size;
        let complete = Arc::new(Mutex::new(false));
        let screenshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../artifacts/smoke/minecraft-survival-{name}.png"
        ));
        App::new(config)
            .with_options(RunOptions {
                hidden: true,
                uncapped: true,
                frames: Some(2),
                screenshot: Some(screenshot.clone()),
                ..Default::default()
            })
            .run(SurvivalProbe {
                base: Probe {
                    game: TerrainPreview::new(42).unwrap(),
                    result: Arc::new(Mutex::new(None)),
                },
                screen,
                complete: complete.clone(),
            })
            .unwrap();
        assert!(*complete.lock().unwrap());
        if screen == 1 || screen == 3 {
            let image = Image::load_image(screenshot.to_str().unwrap()).unwrap();
            let mut dark = 0;
            for y in 150..420 {
                for x in 150..650 {
                    let c = image.get_color(x, y);
                    if c.r < 40 && c.g < 40 && c.b < 40 {
                        dark += 1;
                    }
                }
            }
            crack_pixels.push(dark);
        }
    }
    assert_eq!(crack_pixels.len(), 2);
    assert!(
        crack_pixels[0] > crack_pixels[1] * 2 && crack_pixels[1] > 0,
        "progressive cracks: {crack_pixels:?}"
    );
}
