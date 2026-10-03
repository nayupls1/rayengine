//! Playable first-person survival scene with optional bounded disk checkpoints.
use crate::gameplay::as_global;
use crate::gameplay::{Interaction, InteractionReport, Player};
use crate::persistence::{Saving, Store};
use crate::sky::Sky;
use crate::terrain::{DemoBlocks, Terrain, TerrainSettings};
use crate::{
    breaking,
    hud::{Menu, MenuInput},
    icons,
    survival::{Item, Recipe, Survival, SurvivalInput, respawn_feet},
};
mod effects;
mod hud;
mod lighting;
use crate::textures::{Atlas, TextureSet, Tile};
use effects::Debris;
use lighting::{FrameLight, WorldLighting};
use rayengine::raylib::prelude::MouseButton;
use rayengine::raylib::prelude::{Image, RaylibTexture2D, TextureFilter};
use rayengine::{prelude::*, upload::UploadBudget};
use rayengine_voxel::{glam::Mat4, prelude::*};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
const LEFT: Action = Action(0);
const RIGHT: Action = Action(1);
const FORWARD: Action = Action(2);
const BACK: Action = Action(3);
const JUMP: Action = Action(4);
const SPRINT: Action = Action(5);
const MINE: Action = Action(6);
const PLACE: Action = Action(7);
const HOTBAR: [Action; 9] = [
    Action(8),
    Action(9),
    Action(10),
    Action(11),
    Action(12),
    Action(13),
    Action(14),
    Action(15),
    Action(16),
];
const MENU: Action = Action(17);
const NEXT: Action = Action(18);
const PREVIOUS: Action = Action(19);
const ACTIVATE: Action = Action(20);
const CANCEL: Action = Action(21);
const QUIT: Action = Action(22);
const SAVE: Action = Action(23);
const PAUSE: Action = Action(24);
const DEBUG: Action = Action(25);
const AUTOSAVE_SECONDS: f32 = 10.0;
/// Horizontal streaming radius in chunks; fog distance follows it.
const STREAM_RADIUS: u32 = 2;
/// Seconds the hotbar shows the newly selected item's name.
const ITEM_NAME_SECONDS: f32 = 2.0;
/// Seconds the startup controls hint stays visible.
const HINT_SECONDS: f32 = 12.0;

/// Retain this handle before passing the game to App::run to report final native-
/// close failures after the scene is dropped. No result means init never completed.
#[derive(Clone, Default)]
pub struct SaveOutcome(Arc<Mutex<Option<Result<(), String>>>>);
impl SaveOutcome {
    /// Surface the final checkpoint error to the caller/CLI after App::run.
    pub fn check(&self) -> Result<(), crate::persistence::Error> {
        match &*self.0.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(Err(error)) => Err(crate::persistence::Error::Invalid(error.clone())),
            _ => Ok(()),
        }
    }
}
const UI_ACTIONS: UiActions = UiActions {
    primary: MINE,
    next: NEXT,
    previous: PREVIOUS,
    activate: ACTIVATE,
    cancel: CANCEL,
};
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
/// Original fallback textures require no installed Minecraft assets.
pub struct TerrainPreview {
    terrain: Arc<Terrain>,
    world: VoxelWorld,
    cpu: ChunkStreamer,
    gpu: StreamRenderer,
    materials: VoxelMaterials,
    lighting: WorldLighting,
    atlas: Option<Atlas>,
    texture: Option<TextureId>,
    /// Item icons in Item::ALL order, then full/half/empty hearts.
    icons: Vec<TextureId>,
    sky: Sky,
    debris: Debris,
    debug: bool,
    play_time: f32,
    item_name_timer: f32,
    message: Option<(String, f32)>,
    focus: ChunkPos,
    player: Player,
    interaction: Interaction,
    interaction_report: InteractionReport,
    survival: Survival,
    menu: Menu,
    spawn: BlockPos,
    respawn_pending: bool,
    respawn_retry: f32,
    cracks: Vec<MeshId>,
    waiting: bool,
    report: StreamReport,
    render_report: StreamRenderReport,
    error: Option<String>,
    saving: Option<Saving>,
    save_timer: f32,
    closing: bool,
    initialized: bool,
    save_outcome: SaveOutcome,
}
impl TerrainPreview {
    /// Chooses a safe spawn and starts bounded CPU generation workers.
    pub fn new(seed: u64) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_textures(seed, TextureSet::fallback())
    }
    /// Starts the game with an explicitly validated imported or fallback texture set.
    pub fn with_textures(
        seed: u64,
        textures: TextureSet,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let terrain = Arc::new(Terrain::new(seed, TerrainSettings::default())?);
        let spawn = terrain.find_spawn(0, 0, 16, 1089)?;
        Self::build(
            terrain,
            spawn.support,
            Player::new(spawn.feet())?,
            Survival::default(),
            textures,
            None,
        )
    }
    /// Load an explicitly chosen slot before native init. Missing slots use seed42
    /// unless a seed is supplied; incompatible/corrupt files return an error.
    pub fn with_save(
        seed: Option<u64>,
        textures: TextureSet,
        store: Store,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let saving = Saving::open(store, seed)?;
        let checkpoint = saving.checkpoint();
        Self::build(
            checkpoint.terrain(),
            checkpoint.spawn(),
            checkpoint.player()?,
            checkpoint.survival()?,
            textures,
            Some(saving),
        )
    }
    /// Handle for the final close-time checkpoint, including filesystem failures.
    pub fn save_outcome(&self) -> SaveOutcome {
        self.save_outcome.clone()
    }
    fn build(
        terrain: Arc<Terrain>,
        spawn: BlockPos,
        player: Player,
        survival: Survival,
        textures: TextureSet,
        saving: Option<Saving>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let focus = player.focus();
        let config = StreamConfig {
            radius: STREAM_RADIUS,
            vertical_radius: 2,
            max_resident: 160,
            ..Default::default()
        };
        let recipe = terrain.clone();
        let saved = saving.as_ref().map(Saving::loader);
        let cpu = ChunkStreamer::new(config, move |pos, registry, token| {
            if let Some(saved) = &saved {
                saved.load(pos, registry, token)
            } else {
                generate_chunk(
                    recipe.as_ref(),
                    pos,
                    &GenerationContext::new(registry, &|| token.is_cancelled()),
                )
            }
        })?;
        Ok(Self {
            world: VoxelWorld::new(terrain.registry(), config.max_resident),
            player,
            interaction: Interaction::default(),
            interaction_report: InteractionReport::default(),
            survival,
            menu: Menu::default(),
            spawn,
            respawn_pending: false,
            respawn_retry: 0.0,
            cracks: Vec::with_capacity(breaking::STAGES),
            waiting: true,
            terrain,
            cpu,
            gpu: StreamRenderer::new(StreamRenderConfig {
                max_chunks: 160,
                max_meshes: 4096,
                ..Default::default()
            })?,
            materials: VoxelMaterials::new(),
            lighting: WorldLighting::default(),
            atlas: Some(textures.pack()),
            texture: None,
            icons: Vec::with_capacity(Item::ALL.len() + 3),
            sky: Sky::default(),
            debris: Debris::default(),
            debug: false,
            play_time: 0.0,
            item_name_timer: 0.0,
            message: None,
            focus,
            report: StreamReport::default(),
            render_report: StreamRenderReport::default(),
            error: None,
            saving,
            save_timer: 0.0,
            closing: false,
            initialized: false,
            save_outcome: SaveOutcome::default(),
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
/// Debris tint for a broken block.
fn block_color(block: BlockId, b: DemoBlocks) -> [u8; 3] {
    if block == b.grass {
        [91, 157, 55]
    } else if block == b.dirt {
        Item::Dirt.color()
    } else if block == b.wood {
        Item::Log.color()
    } else if block == b.leaves {
        Item::Leaves.color()
    } else if block == b.coal {
        [70, 74, 80]
    } else if block == b.iron {
        [170, 150, 135]
    } else {
        Item::Stone.color()
    }
}
fn texture_from_png(ctx: &mut InitContext<'_, '_>, png: &[u8]) -> Result<TextureId, Error> {
    let image = Image::load_image_from_mem(".png", png).map_err(|e| Error::Asset(e.to_string()))?;
    let texture = ctx.texture_from_image(&image)?;
    ctx.assets
        .texture(texture)
        .unwrap()
        .set_texture_filter(ctx.thread, TextureFilter::TEXTURE_FILTER_POINT);
    Ok(texture)
}
impl TerrainPreview {
    /// Create every native resource, recording each handle as soon as it exists
    /// so a failure part-way can be fully released.
    fn load(&mut self, ctx: &mut InitContext<'_, '_>, atlas: &Atlas) -> Result<(), Error> {
        let bytes = atlas.png().map_err(|e| Error::Asset(e.to_string()))?;
        let texture = texture_from_png(ctx, &bytes)?;
        self.texture = Some(texture);
        for icon in icons::items(atlas).iter().chain(&icons::hearts()) {
            let png = icon.png().map_err(|e| Error::Asset(e.to_string()))?;
            let id = texture_from_png(ctx, &png)?;
            self.icons.push(id);
        }
        let tiles = Tile::ALL.map(|tile| TileTexture {
            tile: tile.id(),
            texture,
            rect: atlas.rects[tile as usize],
        });
        self.materials = self
            .lighting
            .create(ctx, &tiles, &self.icons[..Item::ALL.len()])?;
        for stage in 0..breaking::STAGES {
            let mesh = ctx.mesh(&breaking::mesh(stage))?;
            self.cracks.push(mesh);
        }
        Ok(())
    }
    /// Meshes owned by the game itself, beyond streamed chunk meshes.
    #[cfg(test)]
    fn local_meshes(&self) -> usize {
        self.cracks.len() + usize::from(self.lighting.cube.is_some())
    }
    fn release(&mut self, assets: &mut rayengine::assets::Assets<'_>) {
        for mesh in self.cracks.drain(..) {
            assets.unload_mesh(mesh);
        }
        self.lighting.unload(assets);
        self.materials = VoxelMaterials::new();
        for id in self.icons.drain(..).chain(self.texture.take()) {
            assets.unload_texture(id);
        }
    }
    fn notify_crafted(&mut self, recipe: Recipe) {
        let (item, count) = recipe.output();
        self.message = Some((format!("Crafted {count} x {}", item.name()), 2.5));
    }
    fn try_respawn(&mut self) -> bool {
        let Some(feet) = respawn_feet(&self.world, self.spawn) else {
            return false;
        };
        match Player::new(feet) {
            Ok(player) => {
                self.player = player;
                self.survival.health.respawn();
                self.interaction.reset();
                self.interaction_report = InteractionReport::default();
                self.focus = self.player.focus();
                self.respawn_pending = false;
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                false
            }
        }
    }
}
impl TerrainPreview {
    fn advance(&mut self, input: &Input, ui_input: UiInput, size: Vec2, dt: f32) -> bool {
        if self.closing {
            self.player.controller.previous = self.player.controller.body.position;
            let saving = self.saving.as_ref().expect("persistent quit");
            if saving.error().is_some() {
                self.closing = false;
            } else {
                return saving.settled();
            }
        }
        if let Some(saving) = &mut self.saving {
            if input.pressed(SAVE) {
                saving.retry();
            }
            self.save_timer += dt;
            if self.save_timer >= AUTOSAVE_SECONDS {
                self.save_timer = 0.0;
                saving.request();
            }
        }
        if input.pressed(DEBUG) {
            self.debug = !self.debug;
        }
        let report = self.menu.update(
            size,
            MenuInput {
                ui: ui_input,
                toggle: input.pressed(MENU),
                pause: input.pressed(PAUSE),
                mining_down: input.down(MINE),
                place_down: input.down(PLACE),
                jump_down: input.down(JUMP),
            },
            &mut self.survival,
        );
        if !report.paused {
            self.sky.advance(dt);
            self.play_time += dt;
            self.item_name_timer = (self.item_name_timer - dt).max(0.0);
            self.debris.step(dt);
            if let Some((_, timer)) = &mut self.message {
                *timer -= dt;
                if *timer <= 0.0 {
                    self.message = None;
                }
            }
        }
        if let Some(recipe) = report.crafted {
            self.notify_crafted(recipe);
        }
        let quit = report.quit || input.pressed(QUIT);
        if quit {
            if let Some(saving) = &mut self.saving {
                saving.retry();
                self.closing = true;
                self.interaction.reset();
                self.interaction_report = InteractionReport::default();
                self.player.controller.previous = self.player.controller.body.position;
                return false;
            }
            return true;
        }
        if report.respawn {
            self.respawn_pending = true;
            self.respawn_retry = 0.0;
        }
        if self.respawn_pending {
            self.focus = self.spawn.split().0;
            self.respawn_retry -= dt;
            if self.respawn_retry <= 0.0 {
                self.respawn_retry = 0.25;
                self.try_respawn();
            }
        }
        if report.modal {
            self.interaction.reset();
            self.interaction_report = InteractionReport::default();
            self.player.controller.previous = self.player.controller.body.position;
            return quit;
        }
        for (slot, action) in HOTBAR.into_iter().enumerate() {
            if input.pressed(action) {
                self.survival.select(slot);
                self.interaction.reset();
                self.item_name_timer = ITEM_NAME_SECONDS;
            }
        }
        let mut movement = FirstPersonInput::from_actions(input, ACTIONS);
        movement.jump_pressed &= report.jump_allowed;
        // Seed peak tracking before motion, so the first airborne tick is included.
        self.survival.health.movement(
            self.player.position().y,
            self.player.controller.body.grounded,
        );
        match self.player.step(&self.world, movement, dt) {
            Ok(_) => {
                self.waiting = false;
                self.survival.health.movement(
                    self.player.position().y,
                    self.player.controller.body.grounded,
                );
            }
            Err(ColliderError::Unloaded(_)) => self.waiting = true,
            Err(e) => {
                self.waiting = true;
                self.error = Some(e.to_string());
            }
        }
        self.focus = self.player.focus();
        if self.waiting || self.survival.health.value() == 0 {
            self.interaction.reset();
            self.interaction_report = InteractionReport::default();
            return quit;
        }
        let interaction_input = SurvivalInput {
            mining: report.mining_allowed && input.down(MINE),
            place: report.place_allowed && input.pressed(PLACE),
            dt,
        };
        let admission = if interaction_input.mining || interaction_input.place {
            self.saving
                .as_ref()
                .map(|saving| saving.admission(&self.world))
        } else {
            None
        };
        match self.survival.interact_admitted(
            &mut self.interaction,
            &mut self.world,
            &self.player,
            interaction_input,
            self.terrain.blocks(),
            |p| {
                admission
                    .as_ref()
                    .is_none_or(|admission| admission.allows(p))
            },
        ) {
            Ok(report) => {
                if let Some(edit) = report.edit
                    && edit.current == BlockId::AIR
                {
                    self.debris.burst(
                        edit.position,
                        block_color(edit.previous, self.terrain.blocks()),
                    );
                }
                self.interaction_report = report;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.survival.collect(self.player.position());
        quit
    }
}
impl Game for TerrainPreview {
    fn cursor_mode(&self) -> CursorMode {
        if self.closing || self.menu.open() || self.survival.health.value() == 0 {
            CursorMode::Free
        } else {
            CursorMode::Captured
        }
    }
    fn bindings(&self) -> Bindings {
        let mut bindings = Bindings::new()
            .bind(LEFT, KeyboardKey::KEY_A)
            .bind(RIGHT, KeyboardKey::KEY_D)
            .bind(FORWARD, KeyboardKey::KEY_W)
            .bind(BACK, KeyboardKey::KEY_S)
            .bind(JUMP, KeyboardKey::KEY_SPACE)
            .bind(SPRINT, KeyboardKey::KEY_LEFT_SHIFT)
            .bind(MINE, Button::Mouse(MouseButton::MOUSE_BUTTON_LEFT))
            .bind(PLACE, Button::Mouse(MouseButton::MOUSE_BUTTON_RIGHT))
            .bind(MENU, KeyboardKey::KEY_E)
            .bind(PAUSE, KeyboardKey::KEY_ESCAPE)
            .bind(DEBUG, KeyboardKey::KEY_F3)
            .bind(NEXT, KeyboardKey::KEY_TAB)
            .bind(NEXT, KeyboardKey::KEY_DOWN)
            .bind(PREVIOUS, KeyboardKey::KEY_UP)
            .bind(ACTIVATE, KeyboardKey::KEY_ENTER)
            .bind(ACTIVATE, KeyboardKey::KEY_SPACE)
            .bind(CANCEL, KeyboardKey::KEY_BACKSPACE)
            .bind(QUIT, KeyboardKey::KEY_F10)
            .bind(SAVE, KeyboardKey::KEY_F5);
        for (action, key) in HOTBAR.into_iter().zip([
            KeyboardKey::KEY_ONE,
            KeyboardKey::KEY_TWO,
            KeyboardKey::KEY_THREE,
            KeyboardKey::KEY_FOUR,
            KeyboardKey::KEY_FIVE,
            KeyboardKey::KEY_SIX,
            KeyboardKey::KEY_SEVEN,
            KeyboardKey::KEY_EIGHT,
            KeyboardKey::KEY_NINE,
        ]) {
            bindings = bindings.bind(action, key);
        }
        bindings
    }
    fn init(&mut self, ctx: &mut InitContext<'_, '_>) -> Result<(), Error> {
        let atlas = self
            .atlas
            .take()
            .ok_or_else(|| Error::Asset("texture atlas already initialized".into()))?;
        let result = self.load(ctx, &atlas);
        if result.is_err() {
            self.release(ctx.assets);
        }
        result?;
        self.initialized = true;
        if let Some(saving) = &mut self.saving {
            saving.request();
        }
        Ok(())
    }
    fn fixed_update(&mut self, ctx: &mut Update<'_, '_>) {
        if self.advance(
            ctx.input,
            ctx.ui_input(UI_ACTIONS),
            ctx.viewport.logical_size,
            ctx.tick.dt,
        ) {
            ctx.quit();
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        if let Some(saving) = &mut self.saving {
            saving.poll(&mut self.world);
        }
        let saving = &mut self.saving;
        match self.cpu.tick(&mut self.world, self.focus, |_, _, _| {
            if let Some(saving) = saving {
                saving.request();
            }
            Eviction::Keep
        }) {
            Ok(report) => self.report = report,
            Err(e) => self.error = Some(e.to_string()),
        }
        if let Some(saving) = &mut self.saving {
            saving.start(&mut self.world, &self.player, &self.survival);
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
        let frame_alpha = frame.alpha;
        let camera = self.player.controller.camera(frame.alpha);
        let view = Frustum3D::from_camera(&camera, &frame.viewport, 0.05, 4000.0).unwrap();
        let sky = self.sky.light();
        let torch = self
            .survival
            .held()
            .and_then(|stack| stack.item().light_radius())
            .map(|range| (camera.position - Vec3::Y * 0.25, range));
        // Fog ends inside the guaranteed streamed radius, hiding chunk edges.
        let reach = (STREAM_RADIUS * 16) as f32;
        if let Err(e) = self.lighting.apply(
            frame.assets,
            &FrameLight {
                sky,
                view: camera.position,
                torch,
                fog: Vec2::new(reach * 0.5, reach + 2.0),
            },
        ) {
            self.error = Some(e.to_string());
        }
        let to_u8 = |c: Vec3| {
            let c = (c * 255.0).round();
            Color::new(c.x as u8, c.y as u8, c.z as u8, 255)
        };
        frame.clear(to_u8(sky.sky));
        let time = self.play_time;
        frame.world_3d(camera, |canvas| {
            // Sun and moon sit beyond the terrain, opposite each other.
            for (direction, size, color) in [
                (sky.sun, 34.0, Color::new(255, 241, 186, 255)),
                (-sky.sun, 26.0, Color::new(214, 222, 236, 255)),
            ] {
                if direction.y > -0.1 {
                    canvas.cube(
                        Aabb3::from_center(camera.position + direction * 400.0, Vec3::splat(size)),
                        color,
                    );
                }
            }
            for chunk in self.gpu.chunks() {
                chunk.draw(canvas, &view, origin);
            }
            if let Some(cube) = self.lighting.cube {
                for (i, pickup) in self.survival.pickups().iter().enumerate() {
                    let center = (pickup.position - as_global(origin)).as_vec3();
                    if (center - self.player.controller.body.position).length_squared()
                        > 64.0 * 64.0
                    {
                        continue;
                    }
                    let phase = time * 1.6 + i as f32 * 0.7;
                    let transform =
                        Mat4::from_translation(center + Vec3::Y * (phase.sin() * 0.08 - 0.15))
                            * Mat4::from_rotation_y(phase)
                            * Mat4::from_scale(Vec3::splat(0.28));
                    let material = self.lighting.items[pickup.stack.item().index()];
                    canvas.mesh_material_matrix(cube, material, transform, Color::WHITE);
                }
            }
            self.debris.draw(canvas, origin, frame_alpha);
            if let Some(hit) = self.interaction_report.selected
                && self.world.block(hit.position) == Some(hit.block)
                && let Ok(mut bounds) = block_bounds(hit.position, origin)
            {
                if let Some(stage) = breaking::stage(self.interaction_report.progress) {
                    canvas.mesh_matrix(
                        self.cracks[stage],
                        Mat4::from_translation(bounds.min),
                        Color::new(28, 25, 23, 255),
                    );
                }
                bounds.min -= Vec3::splat(0.002);
                bounds.max += Vec3::splat(0.002);
                canvas.wire_cube(bounds, Color::new(20, 20, 20, 255));
            }
        });
        self.draw_hud(frame);
    }
}
impl Drop for TerrainPreview {
    fn drop(&mut self) {
        if self.initialized
            && let Some(saving) = &mut self.saving
        {
            let result = saving
                .finish(&mut self.world, &self.player, &self.survival)
                .map_err(|e| e.to_string());
            *self
                .save_outcome
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(result);
        }
    }
}
#[cfg(test)]
mod tests;
