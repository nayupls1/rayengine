//! Embervault: a six-room action dungeon showcasing the optional SDK systems.
mod controls;
mod model;
mod profile;
#[cfg(test)]
mod tests;
mod view;

use controls::*;
use model::*;
use profile::Profile;
use rayengine::{
    manifest::{ProjectManifest, ResolvedManifest},
    prelude::*,
};
use rayengine_tilemap::Tilemap;
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
use view::{GOLD, INK, MUTED, PAPER, TEAL, rect, text};

struct Shared {
    room: Room,
    levels: Vec<String>,
    profile: Profile,
    save_path: PathBuf,
    actors: TextureId,
    font: FontId,
    tracks: Vec<MusicId>,
    sounds: [SoundId; 3],
    message: String,
    intro: bool,
}
impl Shared {
    fn save(&mut self) {
        self.message = match self.profile.save(&self.save_path) {
            Ok(()) => String::new(),
            Err(e) => {
                eprintln!("Embervault save: {e}");
                "SAVE FAILED - CHECK TERMINAL / SAVE PATH".into()
            }
        };
    }
    fn start(&mut self, room: usize) {
        self.room = Room::new(
            Tilemap::from_toml(&self.levels[room]).expect("validated levels"),
            room,
        );
        self.profile.checkpoint = Some(room);
        self.save();
    }
    fn volumes(&self, ctx: &Update<'_, '_>) {
        let mut audio = ctx.assets.audio();
        audio
            .buses_mut()
            .set_volume(BusId::MUSIC, self.profile.music)
            .unwrap();
        audio
            .buses_mut()
            .set_volume(BusId::SFX, self.profile.sfx)
            .unwrap();
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScreenKind {
    Title,
    Play,
    Pause,
    Journal,
    Settings,
    Dialogue,
    Dead,
    Won,
}
struct Screen {
    shared: Rc<RefCell<Shared>>,
    kind: ScreenKind,
    ui: UiState,
    entrance: Tween<f32>,
    last_pointer: Option<Vec2>,
    navigation_initialized: bool,
}
impl Screen {
    fn new(shared: Rc<RefCell<Shared>>, kind: ScreenKind) -> Self {
        Self {
            shared,
            kind,
            ui: UiState::with_capacity(8),
            entrance: Tween::new(22.0, 0.0, Duration::from_millis(240)).with_ease(Ease::CubicOut),
            last_pointer: None,
            navigation_initialized: false,
        }
    }
    fn transition(&self, commands: &mut StateCommands, kind: ScreenKind, push: bool) {
        let state = Box::new(Self::new(self.shared.clone(), kind));
        request(
            commands,
            if push {
                Transition::Push(state)
            } else {
                Transition::Reset(state)
            },
        );
    }
    fn labels(&self) -> Vec<String> {
        let s = self.shared.borrow();
        match self.kind {
            ScreenKind::Title => vec![
                "Begin a new descent".into(),
                if let Some(room) = s.profile.checkpoint {
                    format!("Continue - chamber {}", room + 1)
                } else {
                    "Continue - no checkpoint".into()
                },
                "Settings".into(),
                "Quit".into(),
            ],
            ScreenKind::Pause => vec![
                "Resume".into(),
                "Field journal".into(),
                "Settings".into(),
                "Return to title".into(),
            ],
            ScreenKind::Settings => vec![
                format!("Music  {}%", (s.profile.music * 100.0).round()),
                format!("Effects  {}%", (s.profile.sfx * 100.0).round()),
                format!(
                    "CRT scanlines: {}",
                    if s.profile.crt { "ON" } else { "OFF" }
                ),
                format!(
                    "Move keys: {}",
                    if s.profile.arrows { "ARROWS" } else { "WASD" }
                ),
                format!(
                    "Strike key: {}",
                    if s.profile.alternate_attack { "K" } else { "J" }
                ),
                "Back".into(),
            ],
            ScreenKind::Dead => vec!["Retry this chamber".into(), "Return to title".into()],
            ScreenKind::Won => vec!["Another descent".into(), "Return to title".into()],
            ScreenKind::Journal | ScreenKind::Dialogue => vec!["Continue".into()],
            ScreenKind::Play => vec![],
        }
    }
    fn regions(&self) -> Vec<UiRegion> {
        let labels = self.labels();
        let y = match self.kind {
            ScreenKind::Settings => 240.0,
            ScreenKind::Dialogue | ScreenKind::Journal => 470.0,
            _ => 322.0,
        };
        labels
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let mut region = UiRegion::new(
                    UiId(i as u64),
                    rect(284.0, y + i as f32 * 44.0, 392.0, 36.0),
                );
                if self.kind == ScreenKind::Title && i == 1 {
                    region.enabled = self.shared.borrow().profile.checkpoint.is_some();
                }
                if self.kind == ScreenKind::Settings && i < 2 {
                    region.draggable = true;
                }
                region
            })
            .collect()
    }
    fn activate(&mut self, index: usize, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        match self.kind {
            ScreenKind::Title => match index {
                0 | 1 => {
                    {
                        let mut s = self.shared.borrow_mut();
                        let room = if index == 1 {
                            s.profile.checkpoint.unwrap_or(0)
                        } else {
                            0
                        };
                        s.intro = index == 0;
                        s.start(room);
                    }
                    self.transition(commands, ScreenKind::Play, false);
                }
                2 => self.transition(commands, ScreenKind::Settings, true),
                _ => ctx.quit(),
            },
            ScreenKind::Pause => match index {
                0 => request(commands, Transition::Pop),
                1 => self.transition(commands, ScreenKind::Journal, true),
                2 => self.transition(commands, ScreenKind::Settings, true),
                _ => self.transition(commands, ScreenKind::Title, false),
            },
            ScreenKind::Settings => {
                if index == 5 {
                    self.shared.borrow_mut().save();
                    request(commands, Transition::Pop);
                    return;
                }
                let mut s = self.shared.borrow_mut();
                match index {
                    2 => s.profile.crt = !s.profile.crt,
                    3 => s.profile.arrows = !s.profile.arrows,
                    4 => s.profile.alternate_attack = !s.profile.alternate_attack,
                    _ => (),
                }
                if index == 3 || index == 4 {
                    ctx.bindings
                        .replace(bindings(&s.profile).config())
                        .expect("valid control presets");
                }
                s.save();
            }
            ScreenKind::Journal | ScreenKind::Dialogue => request(commands, Transition::Pop),
            ScreenKind::Dead | ScreenKind::Won => {
                if index == 0 {
                    let mut s = self.shared.borrow_mut();
                    let room = if self.kind == ScreenKind::Won {
                        0
                    } else {
                        s.profile.checkpoint.unwrap_or(0)
                    };
                    s.start(room);
                }
                self.transition(
                    commands,
                    if index == 0 {
                        ScreenKind::Play
                    } else {
                        ScreenKind::Title
                    },
                    false,
                );
            }
            ScreenKind::Play => (),
        }
    }
}
fn request(commands: &mut StateCommands, transition: Transition) {
    assert!(commands.request(transition).is_ok());
}
impl State for Screen {
    fn policy(&self) -> StatePolicy {
        StatePolicy {
            draw_below: !matches!(self.kind, ScreenKind::Title | ScreenKind::Play),
            ..Default::default()
        }
    }
    fn enter(
        &mut self,
        ctx: &mut InitContext<'_, '_>,
        _: &mut StateResources,
    ) -> Result<(), Error> {
        let s = self.shared.borrow();
        if matches!(self.kind, ScreenKind::Play | ScreenKind::Title) {
            ctx.assets.audio().crossfade(
                s.tracks[s.room.number],
                MusicOptions::default(),
                Duration::from_millis(900),
            )?;
        }
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>, commands: &mut StateCommands) {
        if self.kind == ScreenKind::Play {
            if ctx.input.pressed(PAUSE) || !ctx.window_focused {
                self.transition(commands, ScreenKind::Pause, true);
                return;
            }
            if ctx.input.pressed(INVENTORY) {
                self.transition(commands, ScreenKind::Journal, true);
                return;
            }
            if self.shared.borrow().intro {
                self.shared.borrow_mut().intro = false;
                self.transition(commands, ScreenKind::Dialogue, true);
                return;
            }
            let outcome;
            {
                let mut s = self.shared.borrow_mut();
                let stick = Vec2::new(ctx.input.value(AIM_X), ctx.input.value(AIM_Y));
                // A stationary mouse never overrides movement/controller facing. Clicking
                // aims immediately; moving the pointer previews the direction this tick.
                let pointer_aim = ctx
                    .pointer
                    .filter(|p| Some(*p) != self.last_pointer || ctx.input.down(PRIMARY));
                let aim = if stick.length_squared() > 0.1 {
                    stick
                } else if let Some(p) = pointer_aim {
                    p - view::world_to_ui(s.room.position())
                } else {
                    Vec2::ZERO
                };
                self.last_pointer = ctx.pointer;
                s.room.step(
                    Controls {
                        movement: Vec2::new(ctx.input.value(MOVE_X), ctx.input.value(MOVE_Y)),
                        aim,
                        attack: ctx.input.down(ATTACK),
                        dash: ctx.input.pressed(DASH),
                        interact: ctx.input.pressed(INTERACT),
                        reset_block: ctx.input.pressed(RESET),
                    },
                    ctx.tick.dt,
                );
                for cue in &s.room.cues {
                    let id = s.sounds[match cue {
                        Cue::Hit => 0,
                        Cue::Dash => 1,
                        Cue::Chime => 2,
                    }];
                    let _ = ctx.assets.play_sound(id, SoundOptions::default());
                }
                outcome = s.room.outcome;
                if outcome == Outcome::NextRoom {
                    let next = s.room.number + 1;
                    s.start(next);
                    let _ = ctx.assets.audio().crossfade(
                        s.tracks[next],
                        MusicOptions::default(),
                        Duration::from_millis(900),
                    );
                }
                if outcome == Outcome::Won {
                    s.profile.checkpoint = None;
                    s.profile.victories = s.profile.victories.saturating_add(1);
                    s.save();
                }
            }
            if outcome == Outcome::Dead {
                self.transition(commands, ScreenKind::Dead, true);
            }
            if outcome == Outcome::Won {
                self.transition(commands, ScreenKind::Won, true);
            }
            return;
        }
        if ctx.input.pressed(PAUSE)
            && !matches!(
                self.kind,
                ScreenKind::Title | ScreenKind::Dead | ScreenKind::Won
            )
        {
            if self.kind == ScreenKind::Settings {
                self.shared.borrow_mut().save();
            }
            request(commands, Transition::Pop);
            return;
        }
        let regions = self.regions();
        let mut input = ctx.ui_input(UI);
        if !self.navigation_initialized && ctx.window_focused {
            input.next = true;
            input.previous = false;
            self.navigation_initialized = true;
        }
        self.ui.update(&regions, input);
        if self.kind == ScreenKind::Settings {
            let mut changed = false;
            for (i, region) in regions.iter().enumerate().take(2) {
                if let Some(response) = self.ui.response(region.id) {
                    let mut s = self.shared.borrow_mut();
                    let value = if i == 0 {
                        &mut s.profile.music
                    } else {
                        &mut s.profile.sfx
                    };
                    if (response.drag_started || response.held)
                        && let Some(p) = ctx.pointer
                    {
                        *value = ((p.x - region.bounds.min.x - 12.0)
                            / (region.bounds.size().x - 24.0))
                            .clamp(0.0, 1.0);
                        changed = true;
                    }
                    if response.focused {
                        let direction =
                            ctx.input.pressed(MORE) as i8 - ctx.input.pressed(LESS) as i8;
                        if direction != 0 {
                            *value = (*value + direction as f32 * 0.05).clamp(0.0, 1.0);
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                self.shared.borrow().volumes(ctx);
            }
        }
        for region in &regions {
            if self.ui.response(region.id).is_some_and(|r| r.activated) {
                self.activate(region.id.0 as usize, ctx, commands);
                break;
            }
        }
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>, _: &mut StateCommands) {
        self.entrance.advance(frame.delta);
        if matches!(self.kind, ScreenKind::Play | ScreenKind::Title) {
            view::draw_world(&self.shared.borrow(), frame);
        }
        if self.kind == ScreenKind::Play {
            view::hud(&self.shared.borrow(), frame);
            return;
        }
        let labels = self.labels();
        let regions = self.regions();
        let s = self.shared.borrow();
        let font = s.font;
        let offset = self.entrance.value();
        frame.ui(|ui| {
            ui.rectangle(rect(0.0, 0.0, 960.0, 640.0), Color::new(7, 12, 22, 205));
            ui.rectangle(
                rect(242.0, 106.0 + offset, 476.0, 470.0),
                Color::new(19, 29, 41, 250),
            );
            ui.rectangle(rect(242.0, 106.0 + offset, 476.0, 2.0), GOLD);
            ui.rectangle(
                rect(262.0, 126.0 + offset, 436.0, 1.0),
                Color::new(69, 75, 73, 255),
            );
            text(
                ui,
                font,
                "A LANTERN AGAINST THE DARK",
                284.0,
                151.0 + offset,
                14.0,
                GOLD,
            );
            let title = match self.kind {
                ScreenKind::Title => "EMBERVAULT",
                ScreenKind::Pause => "TAKE A BREATH",
                ScreenKind::Journal => "FIELD JOURNAL",
                ScreenKind::Settings => "MAKE IT YOURS",
                ScreenKind::Dialogue => "THE LAST LIGHT",
                ScreenKind::Dead => "THE FLAME FADES",
                ScreenKind::Won => "DAWN RETURNS",
                _ => "",
            };
            text(ui, font, title, 284.0, 187.0 + offset, 32.0, PAPER);
            match self.kind {
                ScreenKind::Title => {
                    text(
                        ui,
                        font,
                        "Six chambers. One stolen ember.",
                        284.0,
                        250.0 + offset,
                        17.0,
                        MUTED,
                    );
                    text(
                        ui,
                        font,
                        "Bring the light home.",
                        284.0,
                        279.0 + offset,
                        17.0,
                        TEAL,
                    );
                }
                ScreenKind::Pause => {
                    text(
                        ui,
                        font,
                        "Your chamber is checkpointed.",
                        284.0,
                        252.0 + offset,
                        17.0,
                        MUTED,
                    );
                }
                ScreenKind::Dead => {
                    text(
                        ui,
                        font,
                        "Rise again at this chamber's gate.",
                        284.0,
                        252.0 + offset,
                        16.0,
                        MUTED,
                    );
                    text(
                        ui,
                        font,
                        "Watch for red rings. Dash to evade.",
                        284.0,
                        279.0 + offset,
                        16.0,
                        TEAL,
                    );
                }
                ScreenKind::Won => {
                    text(
                        ui,
                        font,
                        "The Warden falls. The ember is free.",
                        284.0,
                        252.0 + offset,
                        16.0,
                        TEAL,
                    );
                    text(
                        ui,
                        font,
                        &format!("Completed descents: {}", s.profile.victories),
                        284.0,
                        279.0 + offset,
                        16.0,
                        GOLD,
                    );
                }
                ScreenKind::Journal | ScreenKind::Dialogue => {
                    let lines = if self.kind == ScreenKind::Dialogue {
                        [
                            "Keeper: The vault has swallowed dawn.",
                            "Carry this lantern to its heart.",
                            "Defeat the watchers to unseal each gate.",
                            "Red rings warn of a coming strike.",
                            "Rest at blue shrines. Each chamber saves.",
                            "Your sword and your courage are enough.",
                        ]
                    } else {
                        [
                            "LANTERN   Reveals the stone around you.",
                            "SWORD     Hold strike; aim or face a foe.",
                            "DASH      Brief protection. Recharges.",
                            "SHRINE    E / Y restores six hearts once.",
                            "SWITCH    Push the block. R / LB resets.",
                            "GATE      Clear foes, then head east.",
                        ]
                    };
                    for (i, line) in lines.iter().enumerate() {
                        text(
                            ui,
                            font,
                            line,
                            270.0,
                            251.0 + offset + i as f32 * 30.0,
                            15.0,
                            if i == 0 { TEAL } else { MUTED },
                        );
                    }
                }
                _ => (),
            }
            let style = UiButtonStyle {
                normal: Color::new(30, 46, 58, 255),
                hovered: Color::new(43, 68, 77, 255),
                pressed: Color::new(55, 87, 89, 255),
                text: PAPER,
                focus: GOLD,
                font: Some(font),
                font_size: 17.0,
                spacing: 0.0,
                ..Default::default()
            };
            for (i, (region, label)) in regions.iter().zip(&labels).enumerate() {
                // Interaction bounds remain stationary while decorative framing slides in.
                if let Some(response) = self.ui.response(region.id) {
                    ui.button(region.bounds, label, response, style);
                } else {
                    ui.rectangle(region.bounds, style.normal);
                    text(
                        ui,
                        font,
                        label,
                        region.bounds.min.x + 16.0,
                        region.bounds.min.y + 9.0,
                        17.0,
                        if region.enabled { PAPER } else { MUTED },
                    );
                }
                if self.kind == ScreenKind::Settings && i < 2 {
                    let value = if i == 0 {
                        s.profile.music
                    } else {
                        s.profile.sfx
                    };
                    ui.rectangle(
                        rect(
                            region.bounds.min.x + 12.0,
                            region.bounds.max.y - 5.0,
                            368.0,
                            2.0,
                        ),
                        INK,
                    );
                    ui.rectangle(
                        rect(
                            region.bounds.min.x + 12.0,
                            region.bounds.max.y - 5.0,
                            368.0 * value,
                            2.0,
                        ),
                        TEAL,
                    );
                }
            }
            text(
                ui,
                font,
                if self.kind == ScreenKind::Settings {
                    "DRAG / LEFT-RIGHT VOLUME   ENTER TO CHANGE"
                } else {
                    "ARROWS / D-PAD TO CHOOSE   ENTER / A SELECT"
                },
                266.0,
                546.0,
                13.0,
                MUTED,
            );
            if !s.message.is_empty() {
                text(ui, font, &s.message, 260.0, 585.0, 13.0, GOLD);
            }
        });
    }
}

struct Dungeon {
    project: ResolvedManifest,
    profile: Profile,
    save_path: PathBuf,
    stack: StateStack,
    shared: Option<Rc<RefCell<Shared>>>,
    light: Option<MaterialId>,
    scanlines: Option<MaterialId>,
    uniforms: Vec<UniformId>,
}
impl Game for Dungeon {
    fn bindings(&self) -> Bindings {
        bindings(&self.profile)
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let level_names = self
            .project
            .settings
            .game
            .get("levels")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Config("game.levels must list six dungeon files".into()))?;
        if level_names.len() != ROOM_COUNT {
            return Err(Error::Config("Embervault requires six rooms".into()));
        }
        let mut levels = vec![];
        for level in level_names {
            let name = level
                .as_str()
                .ok_or_else(|| Error::Config("level name must be a string".into()))?;
            let contents = std::fs::read_to_string(self.project.asset(name).map_err(config_error)?)
                .map_err(|e| Error::Config(e.to_string()))?;
            let map = Tilemap::from_toml(&contents).map_err(|e| Error::Config(e.to_string()))?;
            if map.dimensions() != (21, 13) {
                return Err(Error::Config("dungeon rooms must be 21 x 13 tiles".into()));
            }
            levels.push(contents);
        }
        let room = Room::new(Tilemap::from_toml(&levels[0]).unwrap(), 0);
        let font = ctx.fonts(&self.project)?["ember"];
        let actors = ctx.texture(
            self.project
                .asset("dungeon/actors.png")
                .map_err(config_error)?,
        )?;
        let mut tracks = vec![];
        for i in 0..ROOM_COUNT {
            tracks.push(
                ctx.music(
                    self.project
                        .asset(format!("dungeon/room-{i}.wav"))
                        .map_err(config_error)?,
                )?,
            );
        }
        let sounds = [
            ctx.sound(
                self.project
                    .asset("dungeon/hit.wav")
                    .map_err(config_error)?,
            )?,
            ctx.sound(
                self.project
                    .asset("dungeon/dash.wav")
                    .map_err(config_error)?,
            )?,
            ctx.sound(
                self.project
                    .asset("dungeon/chime.wav")
                    .map_err(config_error)?,
            )?,
        ];
        ctx.assets
            .audio()
            .buses_mut()
            .set_volume(BusId::MUSIC, self.profile.music)
            .unwrap();
        ctx.assets
            .audio()
            .buses_mut()
            .set_volume(BusId::SFX, self.profile.sfx)
            .unwrap();
        let shader = ctx.shader_from_source(None, include_str!("lantern.fs"))?;
        for name in ["lantern", "torch0", "torch1", "torch2", "torch3"] {
            self.uniforms
                .push(ctx.uniform(shader, name, UniformValue::Vec2(Vec2::ZERO))?);
        }
        self.uniforms
            .push(ctx.uniform(shader, "time", UniformValue::Float(0.0))?);
        self.uniforms
            .push(ctx.uniform(shader, "hurt", UniformValue::Float(0.0))?);
        self.light = Some(ctx.material(MaterialDesc {
            shader: Some(shader),
            alpha: AlphaMode::Blend,
            ..Default::default()
        })?);
        let crt = ctx.shader_from_source(None, BuiltinEffect::Scanlines.fragment_source())?;
        ctx.uniform(crt, "strength", UniformValue::Float(0.13))?;
        ctx.uniform(crt, "lines", UniformValue::Float(320.0))?;
        self.scanlines = Some(ctx.material(BuiltinEffect::Scanlines.material(crt))?);
        let shared = Rc::new(RefCell::new(Shared {
            room,
            levels,
            profile: self.profile.clone(),
            save_path: self.save_path.clone(),
            actors,
            font,
            tracks,
            sounds,
            message: String::new(),
            intro: false,
        }));
        self.stack = StateStack::new(
            Screen::new(shared.clone(), ScreenKind::Title),
            bindings(&self.profile),
        );
        self.shared = Some(shared);
        self.stack.init(ctx)
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        self.stack.fixed_update(ctx);
    }
    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        self.stack.draw(frame);
    }
    fn boundary(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.stack.boundary(ctx)?;
        if let Some(error) = self.stack.take_error() {
            return Err(error);
        }
        let s = self.shared.as_ref().unwrap().borrow();
        let positions = std::iter::once(s.room.position()).chain(s.room.torches.iter().copied());
        for (binding, p) in self.uniforms.iter().take(5).zip(positions) {
            ctx.assets
                .set_uniform(*binding, UniformValue::Vec2(view::world_to_ui(p)))?;
        }
        ctx.assets
            .set_uniform(self.uniforms[5], UniformValue::Float(s.room.time))?;
        ctx.assets.set_uniform(
            self.uniforms[6],
            UniformValue::Float(if s.room.hp <= 2 { 0.65 } else { 0.0 }),
        )?;
        let mut materials = vec![self.light.unwrap()];
        if s.profile.crt {
            materials.push(self.scanlines.unwrap());
        }
        ctx.assets.set_post_processing(PostProcessing {
            materials,
            ui: UiPlacement::AfterEffects,
        })
    }
    fn shutdown(&mut self, ctx: &mut InitContext<'_, '_>) {
        if let Some(shared) = &self.shared {
            shared.borrow_mut().save();
        }
        self.stack.shutdown(ctx);
    }
}

/// Launch the game, respecting CLI manifest/profile overrides and portable saves.
pub fn run() -> Result<(), Error> {
    let manifest = std::env::var_os("RAYENGINE_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/rayengine.toml")));
    let selected = std::env::var("RAYENGINE_PROFILE").ok();
    let project = ProjectManifest::load(manifest)
        .map_err(config_error)?
        .resolve(selected.as_deref())
        .map_err(config_error)?;
    let mut config = Config::new("Embervault").with_project(&project)?;
    config.exit_key = None;
    let save_path = profile::path();
    let profile = Profile::load(&save_path).map_err(Error::Config)?;
    App::new(config)
        .with_options(RunOptions::from_env()?)
        .run(Dungeon {
            project,
            profile,
            save_path,
            stack: StateStack::default(),
            shared: None,
            light: None,
            scanlines: None,
            uniforms: vec![],
        })?;
    Ok(())
}

fn config_error(error: impl std::fmt::Display) -> Error {
    Error::Config(error.to_string())
}
