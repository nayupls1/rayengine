use super::*;
use rayengine::{core::glam::UVec2, raylib::prelude::Image};
use std::sync::atomic::{AtomicU64, Ordering};

fn level(number: usize) -> Tilemap {
    Tilemap::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("assets/dungeon/levels/{number}.toml")),
    )
    .unwrap()
}
fn room(number: usize) -> Room {
    Room::new(level(number), number)
}
fn tick(r: &mut Room, c: Controls) {
    r.step(c, 1.0 / 120.0);
}
fn idle(r: &mut Room, n: usize) {
    for _ in 0..n {
        tick(r, Controls::default());
    }
}
fn temp(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "embervault-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
#[test]
fn damage_respects_grace_dash_and_death() {
    let mut r = room(0);
    r.invulnerable = 0.0;
    r.damage();
    assert_eq!(r.hp, 5);
    r.damage();
    assert_eq!(r.hp, 5);
    r.invulnerable = 0.0;
    r.dash_time = 0.1;
    r.damage();
    assert_eq!(r.hp, 5);
    r.dash_time = 0.0;
    for _ in 0..5 {
        r.invulnerable = 0.0;
        r.damage();
    }
    assert_eq!(r.hp, 0);
    assert!(r.outcome == Outcome::Dead);
    r.damage();
    assert_eq!(r.hp, 0);
}
#[test]
fn walls_block_sword_and_movement() {
    let mut r = room(1);
    let enemy = r.enemies[0].body;
    r.world.body_mut(r.player).unwrap().position = Vec2::new(310.0, 112.0);
    r.world.body_mut(enemy).unwrap().position = Vec2::new(362.0, 112.0);
    let hp = r.enemies[0].hp;
    for _ in 0..120 {
        tick(
            &mut r,
            Controls {
                movement: Vec2::X,
                aim: Vec2::X,
                attack: true,
                ..Default::default()
            },
        );
    }
    assert!(r.position().x < 312.0);
    assert_eq!(r.enemies[0].hp, hp);
}
#[test]
fn door_needs_combat_and_switch_then_slides_before_transition() {
    let mut r = room(2);
    idle(&mut r, 1);
    assert!(!r.opened);
    for e in &r.enemies {
        r.world.remove(e.body);
    }
    r.enemies.clear();
    idle(&mut r, 1);
    assert!(!r.opened);
    let block = r.blocks[0].0;
    r.world.body_mut(block).unwrap().position = r.plate.unwrap();
    idle(&mut r, 2);
    assert!(r.plate_latched && r.opened);
    assert!(r.map.is_solid(19, 6));
    idle(&mut r, 100);
    assert!(!r.map.is_solid(19, 6));
    r.world.body_mut(r.player).unwrap().position = r.door;
    idle(&mut r, 1);
    assert!(r.outcome == Outcome::NextRoom);
}
#[test]
fn block_push_reset_and_enemy_separation_use_physics() {
    let mut r = room(2);
    let (block, start) = r.blocks[0];
    r.world.body_mut(r.player).unwrap().position = start - Vec2::new(24.0, 0.0);
    for _ in 0..100 {
        tick(
            &mut r,
            Controls {
                movement: Vec2::X,
                ..Default::default()
            },
        );
    }
    assert!(r.world.body(block).unwrap().position.x > start.x + 20.0);
    tick(
        &mut r,
        Controls {
            reset_block: true,
            ..Default::default()
        },
    );
    assert!(r.world.body(block).unwrap().position.distance(start) < 1.0);
    let a = r.enemies[0].body;
    let b = r.enemies[1].body;
    r.world.body_mut(a).unwrap().position = Vec2::new(176.0, 176.0);
    r.world.body_mut(b).unwrap().position = Vec2::new(178.0, 176.0);
    idle(&mut r, 1);
    assert!(
        r.world
            .body(a)
            .unwrap()
            .position
            .distance(r.world.body(b).unwrap().position)
            > 19.9
    );
}
#[test]
fn astar_goes_around_wall_and_blocks_refresh_navigation() {
    let r = room(1);
    let mut finder = PathFinder::new();
    let mut path = vec![];
    let status = finder
        .find_path(
            &r.navigation(),
            UVec2::new(8, 3),
            UVec2::new(12, 3),
            &PathOptions::default(),
            &mut path,
        )
        .unwrap();
    assert!(matches!(status, PathStatus::Found { .. }));
    assert!(path.len() > 5);
    assert!(path.iter().all(|p| !r.map.is_solid(p.x, p.y)));
    let r = room(2);
    assert!(r.navigation().cost(UVec2::new(9, 6)).is_none());
}
#[test]
fn hazards_and_shrine_are_one_use_and_dash_protects() {
    let mut r = room(3);
    r.invulnerable = 0.0;
    r.world.body_mut(r.player).unwrap().position = Vec2::new(239.0, 171.0);
    idle(&mut r, 1);
    assert_eq!(r.hp, 5);
    r.world.body_mut(r.player).unwrap().position = r.shrine;
    tick(
        &mut r,
        Controls {
            interact: true,
            ..Default::default()
        },
    );
    assert_eq!(r.hp, 6);
    assert!(r.shrine_used);
    r.hp = 4;
    tick(
        &mut r,
        Controls {
            interact: true,
            ..Default::default()
        },
    );
    assert_eq!(r.hp, 4);
}
#[test]
fn saves_roundtrip_reject_corruption_future_and_invalid_without_overwrite() {
    let path = temp("profile.save");
    let mut p = Profile {
        checkpoint: Some(4),
        crt: true,
        arrows: true,
        alternate_attack: true,
        music: 0.35,
        ..Default::default()
    };
    p.save(&path).unwrap();
    let loaded = Profile::load(&path).unwrap();
    assert_eq!(loaded.checkpoint, Some(4));
    assert!(loaded.crt && loaded.arrows && loaded.alternate_attack);
    assert_eq!(loaded.music, 0.35);
    let bytes = std::fs::read(&path).unwrap();
    p.checkpoint = Some(6);
    assert!(p.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut broken = bytes.clone();
    let n = broken.len();
    broken[n - 1] ^= 1;
    std::fs::write(&path, &broken).unwrap();
    assert!(Profile::load(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), broken);
    rayengine::save::save(&path, 99, b"{}", Default::default()).unwrap();
    assert!(Profile::load(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(Profile::load(&path).unwrap().checkpoint.is_none());
}
#[test]
fn controls_presets_keep_controller_and_menu_reachable() {
    for arrows in [false, true] {
        for alternate_attack in [false, true] {
            let b = bindings(&Profile {
                arrows,
                alternate_attack,
                ..Default::default()
            });
            b.validate().unwrap();
            for action in [
                ATTACK, DASH, INTERACT, PAUSE, INVENTORY, RESET, NEXT, PREVIOUS, ACCEPT, LESS, MORE,
            ] {
                assert!(
                    b.buttons(action)
                        .iter()
                        .any(|b| matches!(b, Button::Gamepad { .. }))
                );
            }
            for axis in [MOVE_X, MOVE_Y, AIM_X, AIM_Y] {
                assert!(
                    b.axis_bindings(axis)
                        .iter()
                        .any(|b| matches!(b.source, AxisSource::Gamepad { .. }))
                );
            }
        }
    }
}
fn toward(r: &Room, target: Vec2) -> Vec2 {
    let start = (r.position() / 32.0).as_uvec2();
    let goal = (target / 32.0).as_uvec2();
    let mut finder = PathFinder::new();
    let mut path = vec![];
    finder
        .find_path(
            &r.navigation(),
            start,
            goal,
            &PathOptions::default(),
            &mut path,
        )
        .unwrap();
    let next = path
        .get(1)
        .map_or(target, |p| r.map.grid_layout().cell_center(*p));
    (next - r.position()).normalize_or_zero()
}
fn bot(r: &Room) -> Controls {
    let mut c = Controls {
        attack: true,
        ..Default::default()
    };
    if let Some(e) = r.enemies.iter().min_by(|a, b| {
        r.world
            .body(a.body)
            .unwrap()
            .position
            .distance_squared(r.position())
            .total_cmp(
                &r.world
                    .body(b.body)
                    .unwrap()
                    .position
                    .distance_squared(r.position()),
            )
    }) {
        let p = r.world.body(e.body).unwrap().position;
        let delta = p - r.position();
        c.aim = delta;
        if e.boss && e.windup > 0.0 && delta.length() < 110.0 {
            c.movement = -delta.normalize_or_zero();
            c.dash = e.windup < 0.3;
        } else if delta.length() > 49.0 {
            c.movement = toward(r, p);
        } else if !e.boss && delta.length() < 36.0 {
            c.movement = -delta.normalize_or_zero();
        }
    } else if r.plate.is_some() && !r.plate_latched {
        let p = r.world.body(r.blocks[0].0).unwrap().position;
        if (r.position().y - p.y).abs() > 3.0 || r.position().x > p.x - 20.0 {
            c.movement = toward(r, p - Vec2::new(36.0, 0.0));
        } else {
            c.movement = Vec2::X;
        }
    } else if r.opened {
        c.movement = toward(r, r.door);
    }
    c
}
#[test]
fn complete_descent_through_all_six_rooms_with_real_combat_and_push_puzzle() {
    for number in 0..ROOM_COUNT {
        let mut r = room(number);
        for _ in 0..120 * 180 {
            let c = bot(&r);
            tick(&mut r, c);
            if r.outcome != Outcome::Playing {
                break;
            }
        }
        assert!(
            r.outcome
                == if number == 5 {
                    Outcome::Won
                } else {
                    Outcome::NextRoom
                },
            "room {number}: hp={}, enemies={}, position={:?}, open={}, plate={}, time={}",
            r.hp,
            r.enemies.len(),
            r.position(),
            r.opened,
            r.plate_latched,
            r.time
        );
    }
}

// Runs real initialization, sprite/font uploads, shader chains, streamed audio,
// simulation and StateStack drawing; screenshots are inspected by the smoke job.
struct NativeProbe {
    game: Dungeon,
    number: usize,
}
impl Game for NativeProbe {
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.game.init(ctx)?;
        let shared = self.game.shared.as_ref().unwrap().clone();
        shared.borrow_mut().start(self.number);
        self.game
            .stack
            .request(Transition::Reset(Box::new(Screen::new(
                shared,
                ScreenKind::Play,
            ))))
            .ok()
            .unwrap();
        self.game.stack.apply(ctx)?;
        let mut shared = self.game.shared.as_ref().unwrap().borrow_mut();
        for _ in 0..120 * 3 {
            let c = bot(&shared.room);
            tick(&mut shared.room, c);
        }
        shared.profile.crt = true;
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        self.game.draw(frame);
    }
    fn boundary(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.game.boundary(ctx)
    }
    fn shutdown(&mut self, ctx: &mut InitContext<'_, '_>) {
        self.game.shutdown(ctx);
    }
}
#[test]
#[ignore = "requires a native display and audio device"]
fn native_dungeon() {
    let project = ProjectManifest::load(concat!(env!("CARGO_MANIFEST_DIR"), "/rayengine.toml"))
        .unwrap()
        .resolve(None)
        .unwrap();
    for (number, size) in [(0, (1200, 800)), (2, (800, 1000)), (5, (960, 640))] {
        let save_path = temp("native.save");
        let screenshot = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../artifacts/smoke/dungeon-{number}.png"));
        let mut config = Config::new("Embervault probe")
            .with_project(&project)
            .unwrap();
        config.window_size = size;
        config.vsync = false;
        let report = App::new(config)
            .with_options(RunOptions {
                frames: Some(6),
                hidden: true,
                uncapped: true,
                screenshot: Some(screenshot.clone()),
                ..Default::default()
            })
            .run(NativeProbe {
                game: Dungeon {
                    project: project.clone(),
                    profile: Profile::default(),
                    save_path: save_path.clone(),
                    stack: StateStack::default(),
                    shared: None,
                    light: None,
                    scanlines: None,
                    uniforms: vec![],
                },
                number,
            })
            .unwrap();
        assert_eq!(report.frames, 6);
        let image = Image::load_image(screenshot.to_str().unwrap()).unwrap();
        assert_eq!((image.width, image.height), (size.0 as i32, size.1 as i32));
        let colors = image.get_image_data();
        assert!(colors.iter().any(|c| c.r > 180 && c.g > 120));
        assert!(colors.iter().any(|c| c.g > 100 && c.g > c.r));
        std::fs::remove_file(save_path).unwrap();
    }
}
