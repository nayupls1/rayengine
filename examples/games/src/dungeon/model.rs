//! Deterministic dungeon rules. No graphics, audio device, or filesystem needed.
use rayengine::{core::glam::UVec2, prelude::*};
use rayengine_particles::{Emitter, EmitterConfig};
use rayengine_tilemap::{TileId, Tilemap};
use std::{sync::Arc, time::Duration};

pub const TILE: f32 = 32.0;
pub const ROOM_COUNT: usize = 6;
pub const NAMES: [&str; ROOM_COUNT] = [
    "The waking hall",
    "Crooked passage",
    "The counterweight",
    "Cinder bridge",
    "Watchers' court",
    "Heart of the vault",
];
pub const HINTS: [&str; ROOM_COUNT] = [
    "Clear the hall. Strike toward your aim; dash through danger.",
    "They can find a way around walls. Keep moving.",
    "Push the brass block onto the diamond switch. R resets it.",
    "The glowing edges of the pits burn. Stay on the bridge.",
    "Break the watchers' circle. The shrine restores your hearts.",
    "The Warden marks its strike in red. Dash away, then punish.",
];
const PLAYER: u32 = 1;
const SOLID: u32 = 2;
const ENEMY: u32 = 4;
const BLOCK: u32 = 8;
const ZONE: u32 = 16;

#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub movement: Vec2,
    pub aim: Vec2,
    pub attack: bool,
    pub dash: bool,
    pub interact: bool,
    pub reset_block: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    NextRoom,
    Dead,
    Won,
}
#[derive(Clone, Copy)]
pub enum Cue {
    Hit,
    Dash,
    Chime,
}

pub struct ActorAnimation {
    pub player: AnimationPlayer,
    clips: [Arc<AnimationClip>; 4],
    state: usize,
}
impl ActorAnimation {
    pub fn new(row: u32) -> Self {
        let clips = std::array::from_fn(|state| {
            let (name, frames): (&str, &[u32]) = match state {
                0 => ("idle", &[0]),
                1 => ("walk", &[1, 2]),
                2 => ("attack", &[3, 4, 5]),
                _ => ("hurt", &[6, 7]),
            };
            Arc::new(
                AnimationClip::new(
                    name,
                    frames
                        .iter()
                        .map(|x| SpriteFrame {
                            region: SpriteRegion::new(x * 16, row * 16, 16, 16).unwrap(),
                            duration: Duration::from_millis(if state == 2 { 70 } else { 100 }),
                        })
                        .collect(),
                    PlaybackMode::Loop,
                )
                .unwrap(),
            )
        });
        Self {
            player: AnimationPlayer::new(clips[0].clone()),
            clips,
            state: 0,
        }
    }
    fn step(&mut self, state: usize, dt: f32) {
        if self.state != state {
            self.player.play(self.clips[state].clone());
            self.state = state;
        }
        self.player.advance(Duration::from_secs_f32(dt));
    }
}
pub struct Enemy {
    pub body: BodyId,
    pub previous: Vec2,
    pub hp: u8,
    pub boss: bool,
    pub facing: Vec2,
    pub windup: f32,
    pub recovery: f32,
    pub flash: Tween<f32>,
    pub animation: ActorAnimation,
    target: Vec2,
    path_clock: f32,
}
pub struct Room {
    pub map: Tilemap,
    pub world: PhysicsWorld2D,
    pub player: BodyId,
    pub previous: Vec2,
    pub facing: Vec2,
    pub enemies: Vec<Enemy>,
    pub blocks: Vec<(BodyId, Vec2)>,
    pub torches: Vec<Vec2>,
    pub shrine: Vec2,
    pub shrine_used: bool,
    pub plate: Option<Vec2>,
    pub plate_latched: bool,
    pub door: Vec2,
    pub door_slide: Tween<f32>,
    pub opened: bool,
    pub hp: u8,
    pub invulnerable: f32,
    pub attack_cooldown: f32,
    pub swing: f32,
    pub dash_cooldown: f32,
    pub dash_time: f32,
    dash_direction: Vec2,
    pub time: f32,
    pub number: usize,
    pub kills: u32,
    pub outcome: Outcome,
    pub flash: Tween<f32>,
    pub shake: Shake,
    pub sparks: Emitter,
    pub dust: Emitter,
    pub flames: Vec<Emitter>,
    pub animation: ActorAnimation,
    pub cues: Vec<Cue>,
    door_body: Option<BodyId>,
    door_cell: UVec2,
    door_tile: TileId,
    zones: Vec<(BodyId, u32)>,
    triggers: Events<TriggerEvent>,
    finder: PathFinder,
    path: Vec<UVec2>,
    tick: u64,
}
fn flash() -> Tween<f32> {
    let mut t = Tween::new(1.0, 0.0, Duration::from_millis(300));
    t.finish();
    t
}
fn emitter(seed: u64, rate: f32) -> Emitter {
    Emitter::new(EmitterConfig {
        capacity: 128,
        max_spawn: 32,
        rate,
        lifetime: [0.2, 0.65],
        start_size: 4.0,
        end_size: 0.0,
        velocity_spread: Vec3::new(65.0, 65.0, 0.0),
        start_color: rayengine::core::glam::Vec4::new(1.0, 0.68, 0.3, 1.0),
        seed,
        ..Default::default()
    })
    .unwrap()
}
impl Room {
    pub fn new(map: Tilemap, number: usize) -> Self {
        let mut world = PhysicsWorld2D::new(TILE * 2.0);
        let mut enemies = Vec::new();
        let mut blocks = Vec::new();
        let mut torches = Vec::new();
        let mut zones = Vec::new();
        let mut shrine = Vec2::ZERO;
        let mut plate = None;
        let mut door = Vec2::ZERO;
        let mut door_cell = UVec2::ZERO;
        let mut door_tile = TileId(0);
        let mut door_body = None;
        let (width, height) = map.dimensions();
        for y in 0..height {
            for x in 0..width {
                let Some(id) = map.tile(0, x, y) else {
                    continue;
                };
                let flags = map.palette()[id.0 as usize].collision;
                let at = map.grid_layout().cell_center(UVec2::new(x, y));
                if flags.solid {
                    let mut body = PhysicsBody2D::new(at, Shape2D::box_shape(Vec2::splat(TILE)));
                    body.kind = BodyKind::Static;
                    body.filter = CollisionFilter {
                        layers: SOLID,
                        mask: PLAYER | ENEMY | BLOCK,
                    };
                    let body = world.insert(body);
                    if flags.custom == 3 {
                        door_body = Some(body);
                        door = at;
                        door_cell = UVec2::new(x, y);
                        door_tile = id;
                    }
                }
                if flags.trigger != 0 {
                    let size = if flags.trigger == 2 {
                        TILE + 5.0
                    } else {
                        TILE - 6.0
                    };
                    let mut body = PhysicsBody2D::new(at, Shape2D::box_shape(Vec2::splat(size)));
                    body.kind = BodyKind::Static;
                    body.is_trigger = true;
                    body.filter = CollisionFilter {
                        layers: ZONE,
                        mask: if flags.trigger == 4 { BLOCK } else { PLAYER },
                    };
                    zones.push((world.insert(body), flags.trigger));
                }
                match flags.custom {
                    4 => torches.push(at),
                    5 | 6 => {
                        let boss = flags.custom == 6;
                        let mut body =
                            PhysicsBody2D::new(at, Shape2D::round(if boss { 13.0 } else { 10.0 }));
                        body.filter = CollisionFilter {
                            layers: ENEMY,
                            mask: SOLID | ENEMY | BLOCK,
                        };
                        body.mass = if boss { 3.0 } else { 1.0 };
                        enemies.push(Enemy {
                            body: world.insert(body),
                            previous: at,
                            hp: if boss { 12 } else { 3 },
                            boss,
                            facing: -Vec2::X,
                            windup: 0.0,
                            recovery: 0.4,
                            flash: flash(),
                            animation: ActorAnimation::new(if boss { 2 } else { 1 }),
                            target: at,
                            path_clock: 0.0,
                        });
                    }
                    7 => {
                        let mut body =
                            PhysicsBody2D::new(at, Shape2D::box_shape(Vec2::splat(25.0)));
                        body.filter = CollisionFilter {
                            layers: BLOCK,
                            mask: SOLID | PLAYER | ENEMY | BLOCK | ZONE,
                        };
                        body.mass = 2.0;
                        body.drag = 18.0;
                        blocks.push((world.insert(body), at));
                    }
                    8 => plate = Some(at),
                    9 => shrine = at,
                    _ => (),
                }
            }
        }
        let spawn = Vec2::new(80.0, 208.0);
        let mut body = PhysicsBody2D::new(spawn, Shape2D::round(9.0));
        body.filter = CollisionFilter {
            layers: PLAYER,
            mask: SOLID | BLOCK | ZONE,
        };
        let player = world.insert(body);
        let mut slide =
            Tween::new(0.0, 1.0, Duration::from_millis(650)).with_ease(Ease::CubicInOut);
        slide.pause();
        let flames = torches
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut e = emitter(i as u64 + 7, 8.0);
                e.set_position(p.extend(0.0)).unwrap();
                e
            })
            .collect();
        Self {
            map,
            world,
            player,
            previous: spawn,
            facing: Vec2::X,
            enemies,
            blocks,
            torches,
            shrine,
            shrine_used: false,
            plate,
            plate_latched: false,
            door,
            door_slide: slide,
            opened: false,
            hp: 6,
            invulnerable: 0.6,
            attack_cooldown: 0.0,
            swing: 0.0,
            dash_cooldown: 0.0,
            dash_time: 0.0,
            dash_direction: Vec2::X,
            time: 0.0,
            number,
            kills: 0,
            outcome: Outcome::Playing,
            flash: flash(),
            shake: Shake::new(ShakeConfig {
                max_offset: Vec2::splat(5.0),
                ..Default::default()
            })
            .unwrap(),
            sparks: emitter(123, 0.0),
            dust: emitter(234, 0.0),
            flames,
            animation: ActorAnimation::new(0),
            cues: vec![],
            door_body,
            door_cell,
            door_tile,
            zones,
            triggers: Events::default(),
            finder: PathFinder::new(),
            path: vec![],
            tick: 0,
        }
    }
    pub fn position(&self) -> Vec2 {
        self.world.body(self.player).unwrap().position
    }
    pub fn damage(&mut self) {
        if self.hp == 0 || self.invulnerable > 0.0 || self.dash_time > 0.0 {
            return;
        }
        self.hp -= 1;
        self.invulnerable = 0.95;
        self.flash.reset();
        self.shake.add_trauma(0.55);
        self.cues.push(Cue::Hit);
        if self.hp == 0 {
            self.outcome = Outcome::Dead;
        }
    }
    pub fn navigation(&self) -> CostGrid {
        let (w, h) = self.map.dimensions();
        let mut grid = CostGrid::new(UVec2::new(w, h), 1.0);
        for y in 0..h {
            for x in 0..w {
                if self.map.is_solid(x, y) {
                    grid.set(UVec2::new(x, y), None);
                }
            }
        }
        for (id, _) in &self.blocks {
            let p = self.world.body(*id).unwrap().position;
            if let Some(cell) = self.map.grid_layout().cell_at(p, UVec2::new(w, h)) {
                grid.set(cell, None);
            }
        }
        grid
    }
    fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let steps = ((b - a).length() / 6.0).ceil() as usize;
        (0..=steps).all(|i| {
            let p = a.lerp(b, i as f32 / steps.max(1) as f32);
            let cell = (p / TILE).as_uvec2();
            !self.map.is_solid(cell.x, cell.y)
        })
    }
    pub fn step(&mut self, controls: Controls, dt: f32) {
        if self.outcome != Outcome::Playing {
            return;
        }
        self.cues.clear();
        self.time += dt;
        self.tick += 1;
        self.previous = self.position();
        for timer in [
            &mut self.invulnerable,
            &mut self.attack_cooldown,
            &mut self.swing,
            &mut self.dash_cooldown,
            &mut self.dash_time,
        ] {
            *timer = (*timer - dt).max(0.0);
        }
        let duration = Duration::from_secs_f32(dt);
        self.flash.advance(duration);
        self.shake.advance(duration);
        self.door_slide.advance(duration);
        self.sparks.step(dt).unwrap();
        self.dust.step(dt).unwrap();
        for e in &mut self.flames {
            e.step(dt).unwrap();
        }
        let movement = controls.movement.clamp_length_max(1.0);
        if controls.aim.length_squared() > 0.1 {
            self.facing = controls.aim.normalize();
        } else if movement.length_squared() > 0.05 {
            self.facing = movement.normalize();
        }
        if controls.dash && self.dash_cooldown == 0.0 {
            self.dash_direction = if movement.length_squared() > 0.05 {
                movement.normalize()
            } else {
                self.facing
            };
            self.dash_time = 0.16;
            self.dash_cooldown = 0.85;
            self.cues.push(Cue::Dash);
        }
        if self.dash_time > 0.0 {
            self.dust.set_position(self.previous.extend(0.0)).unwrap();
            self.dust.burst(2);
        }
        self.world.body_mut(self.player).unwrap().velocity = if self.dash_time > 0.0 {
            self.dash_direction * 430.0
        } else {
            movement * 145.0
        };
        if controls.reset_block && !self.plate_latched {
            for (id, start) in &self.blocks {
                let b = self.world.body_mut(*id).unwrap();
                b.position = *start;
                b.velocity = Vec2::ZERO;
            }
        }
        if controls.interact && !self.shrine_used && self.previous.distance(self.shrine) < 48.0 {
            self.hp = 6;
            self.shrine_used = true;
            self.cues.push(Cue::Chime);
        }
        if controls.attack && self.attack_cooldown == 0.0 && self.dash_time == 0.0 {
            self.attack_cooldown = 0.32;
            self.swing = 0.22;
            let hits: Vec<usize> = self
                .enemies
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    let p = self.world.body(e.body).unwrap().position;
                    let d = p - self.previous;
                    (e.hp > 0
                        && d.length() < if e.boss { 67.0 } else { 57.0 }
                        && (d.length() < 18.0 || d.normalize().dot(self.facing) > 0.1)
                        && self.line_clear(self.previous, p))
                    .then_some(i)
                })
                .collect();
            for i in hits {
                let e = &mut self.enemies[i];
                e.hp -= 1;
                e.flash.reset();
                // Ordinary enemies stagger; the Warden commits to its marked attack.
                if !e.boss {
                    e.recovery = 0.38;
                    e.windup = 0.0;
                }
                let p = self.world.body(e.body).unwrap().position;
                self.sparks.set_position(p.extend(0.0)).unwrap();
                self.sparks.burst(14);
                self.shake.add_trauma(0.17);
                self.cues.push(Cue::Hit);
                if e.hp == 0 {
                    self.world.remove(e.body);
                    self.kills += 1;
                }
            }
            self.enemies.retain(|e| e.hp > 0);
        }
        let grid = self.navigation();
        let layout = self.map.grid_layout();
        let size = UVec2::new(self.map.dimensions().0, self.map.dimensions().1);
        let goal = layout.cell_at(self.previous, size).unwrap();
        let mut attacks = Vec::new();
        for enemy in &mut self.enemies {
            let body = self.world.body_mut(enemy.body).unwrap();
            enemy.previous = body.position;
            enemy.flash.advance(duration);
            enemy.recovery = (enemy.recovery - dt).max(0.0);
            let delta = self.previous - body.position;
            if enemy.windup > 0.0 {
                enemy.windup = (enemy.windup - dt).max(0.0);
                body.velocity = Vec2::ZERO;
                if enemy.windup == 0.0 {
                    attacks.push((body.position, enemy.boss));
                    enemy.recovery = if enemy.boss { 0.9 } else { 0.7 };
                }
            } else if enemy.recovery > 0.0 {
                body.velocity = Vec2::ZERO;
            } else if delta.length() < if enemy.boss { 78.0 } else { 33.0 } {
                enemy.facing = delta.normalize_or_zero();
                enemy.windup = if enemy.boss { 0.65 } else { 0.45 };
                body.velocity = Vec2::ZERO;
            } else {
                enemy.path_clock -= dt;
                if enemy.path_clock <= 0.0 || body.position.distance(enemy.target) < 3.0 {
                    enemy.path_clock = 0.18;
                    if let Some(start) = layout.cell_at(body.position, size) {
                        self.finder
                            .find_path(&grid, start, goal, &PathOptions::default(), &mut self.path)
                            .unwrap();
                        enemy.target = self
                            .path
                            .get(1)
                            .map_or(self.previous, |cell| layout.cell_center(*cell));
                    }
                }
                enemy.facing = (enemy.target - body.position).normalize_or_zero();
                body.velocity = enemy.facing
                    * if enemy.boss {
                        56.0
                    } else {
                        67.0 + self.number as f32 * 3.0
                    };
            }
            let state = if enemy.flash.value() > 0.3 {
                3
            } else if enemy.windup > 0.0 {
                2
            } else if body.velocity.length_squared() > 1.0 {
                1
            } else {
                0
            };
            enemy.animation.step(state, dt);
        }
        self.triggers.clear();
        self.world.step(
            Tick {
                index: self.tick,
                dt,
            },
            &mut self.triggers,
        );
        for (p, boss) in attacks {
            if self.position().distance(p) < if boss { 86.0 } else { 42.0 }
                && self.line_clear(p, self.position())
            {
                self.damage();
            }
        }
        let mut hazard = false;
        let mut exit = false;
        for event in self.triggers.drain() {
            if event.phase == TriggerPhase::Exit {
                continue;
            }
            if let Some((_, kind)) = self.zones.iter().find(|(id, _)| *id == event.trigger) {
                match kind {
                    1 if event.other == self.player => exit = true,
                    2 if event.other == self.player => hazard = true,
                    // Require the center on the switch, not a grazing edge.
                    4 if self.plate.is_some_and(|p| {
                        self.world
                            .body(event.other)
                            .is_some_and(|b| b.position.distance(p) < 13.0)
                    }) =>
                    {
                        self.plate_latched = true
                    }
                    _ => (),
                }
            }
        }
        if hazard {
            self.damage();
        }
        if self.enemies.is_empty() && (self.plate.is_none() || self.plate_latched) && !self.opened {
            self.opened = true;
            self.door_slide.resume();
            self.cues.push(Cue::Chime);
        }
        if self.opened
            && self.door_slide.value() > 0.92
            && let Some(id) = self.door_body.take()
        {
            self.world.remove(id);
            self.map
                .set_tile(0, self.door_cell.x, self.door_cell.y, None)
                .unwrap();
        }
        if exit && self.door_body.is_none() && self.hp > 0 {
            self.outcome = if self.number + 1 == ROOM_COUNT {
                Outcome::Won
            } else {
                Outcome::NextRoom
            };
        }
        self.animation.step(
            if self.flash.value() > 0.3 {
                3
            } else if self.swing > 0.0 {
                2
            } else if movement.length_squared() > 0.05 {
                1
            } else {
                0
            },
            dt,
        );
    }
    pub fn tile_kind(&self, x: u32, y: u32) -> u32 {
        if UVec2::new(x, y) == self.door_cell {
            return self.map.palette()[self.door_tile.0 as usize]
                .collision
                .custom;
        }
        self.map
            .tile(0, x, y)
            .map_or(0, |id| self.map.palette()[id.0 as usize].collision.custom)
    }
}
