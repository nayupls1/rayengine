//! Draw the same CPU-resolved bounds used for inventory interaction.
use super::*;
use crate::{
    hud::{CLOSE, Layout, QUIT, RESPAWN, Screen, recipe_id, slot_id},
    icons::Heart,
    persistence::SaveStatus,
    survival::{HOTBAR_SLOTS, MAX_HEALTH, MAX_PICKUPS, Stack},
};
use rayengine::raylib::prelude::RaylibDraw;
use rayengine::render::UiCanvas;

// Classic light-gray inventory palette.
const PANEL: Color = Color::new(198, 198, 198, 255);
const PANEL_LIGHT: Color = Color::new(255, 255, 255, 255);
const PANEL_DARK: Color = Color::new(85, 85, 85, 255);
const SLOT: Color = Color::new(139, 139, 139, 255);
const SLOT_HOVER: Color = Color::new(170, 170, 170, 255);
const SLOT_SOURCE: Color = Color::new(204, 177, 92, 255);
const LABEL: Color = Color::new(63, 63, 63, 255);
const GOOD: Color = Color::new(36, 112, 36, 255);
const BAD: Color = Color::new(168, 38, 38, 255);
const BUTTON: UiButtonStyle = UiButtonStyle {
    normal: Color::new(111, 111, 111, 255),
    hovered: Color::new(126, 136, 191, 255),
    pressed: Color::new(88, 98, 150, 255),
    disabled: Color::new(60, 60, 60, 255),
    text: Color::WHITE,
    focus: Color::new(255, 255, 160, 255),
    font_size: 16.0,
    font: None,
    spacing: 1.0,
};

fn rectangle(min: Vec2, size: Vec2) -> Aabb2 {
    Aabb2 {
        min,
        max: min + size,
    }
}
fn inset(bounds: Aabb2, amount: f32) -> Aabb2 {
    Aabb2 {
        min: bounds.min + Vec2::splat(amount),
        max: bounds.max - Vec2::splat(amount),
    }
}
/// Approximate width of raylib's default font in UI units, for centering only.
fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * (size * 0.5 + 1.0)
}
fn outline<D: RaylibDraw>(ui: &mut UiCanvas<'_, D>, b: Aabb2, width: f32, color: Color) {
    ui.rectangle(rectangle(b.min, Vec2::new(b.size().x, width)), color);
    ui.rectangle(
        rectangle(
            Vec2::new(b.min.x, b.max.y - width),
            Vec2::new(b.size().x, width),
        ),
        color,
    );
    ui.rectangle(rectangle(b.min, Vec2::new(width, b.size().y)), color);
    ui.rectangle(
        rectangle(
            Vec2::new(b.max.x - width, b.min.y),
            Vec2::new(width, b.size().y),
        ),
        color,
    );
}
/// Raised panel with light top-left and dark bottom-right edges.
fn bevel<D: RaylibDraw>(ui: &mut UiCanvas<'_, D>, b: Aabb2, fill: Color, raised: bool) {
    let (light, dark) = if raised {
        (PANEL_LIGHT, PANEL_DARK)
    } else {
        (Color::new(55, 55, 55, 255), PANEL_LIGHT)
    };
    ui.rectangle(b, dark);
    ui.rectangle(
        Aabb2 {
            min: b.min,
            max: b.max - Vec2::splat(2.0),
        },
        light,
    );
    ui.rectangle(inset(b, 2.0), fill);
}
fn shadowed<D: RaylibDraw>(
    ui: &mut UiCanvas<'_, D>,
    text: &str,
    at: Vec2,
    size: f32,
    color: Color,
) {
    ui.text(
        text,
        at + Vec2::splat((size / 10.0).max(1.0)),
        size,
        Color::new(40, 40, 40, color.a),
    );
    ui.text(text, at, size, color);
}
fn centered<D: RaylibDraw>(
    ui: &mut UiCanvas<'_, D>,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
) {
    shadowed(
        ui,
        text,
        Vec2::new(x - text_width(text, size) * 0.5, y),
        size,
        color,
    );
}
fn describe(item: Item) -> &'static str {
    match item {
        Item::Dirt | Item::Stone | Item::Log | Item::Leaves => "Block - right click to place",
        Item::Coal | Item::IronOre | Item::Stick | Item::Planks => "Crafting ingredient",
        Item::WoodenPickaxe => "Mines stone and coal 3x faster",
        Item::StonePickaxe => "Mines rock 6x faster, harvests iron",
        Item::WoodenAxe => "Chops logs 3x faster",
        Item::StoneAxe => "Chops logs 6x faster",
        Item::Torch => "Hold it to light up caves and nights",
    }
}
impl TerrainPreview {
    fn item_icon<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, item: Item, bounds: Aabb2) {
        if let Some(&texture) = self.icons.get(item.index()) {
            ui.icon(texture, bounds, Color::WHITE);
        } else {
            let [r, g, b] = item.color();
            ui.rectangle(bounds, Color::new(r, g, b, 255));
        }
    }
    fn stack<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, bounds: Aabb2, stack: Option<Stack>) {
        let Some(stack) = stack else {
            return;
        };
        let pad = (bounds.size().x * 0.14).max(2.0);
        self.item_icon(ui, stack.item(), inset(bounds, pad));
        if stack.count() > 1 {
            let count = stack.count().to_string();
            let size = (bounds.size().x * 0.32).clamp(9.0, 14.0);
            shadowed(
                ui,
                &count,
                bounds.max - Vec2::new(text_width(&count, size) + 1.0, size + 1.0),
                size,
                Color::WHITE,
            );
        }
    }
    fn hearts<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, at: Vec2, size: f32) {
        let health = self.survival.health.value();
        for i in 0..MAX_HEALTH / 2 {
            let kind = if health >= 2 * i + 2 {
                Heart::Full
            } else if health == 2 * i + 1 {
                Heart::Half
            } else {
                Heart::Empty
            };
            let bounds = rectangle(
                at + Vec2::X * (f32::from(i) * (size + 1.0)),
                Vec2::splat(size),
            );
            if let Some(&texture) = self.icons.get(Item::ALL.len() + kind as usize) {
                ui.icon(texture, bounds, Color::WHITE);
            }
        }
    }
    fn gameplay_hud<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, layout: &Layout) {
        let size = ui.logical_size;
        let center = size * 0.5;
        // Crosshair.
        ui.rectangle(
            rectangle(center - Vec2::new(8.0, 1.0), Vec2::new(16.0, 2.0)),
            Color::new(240, 240, 240, 220),
        );
        ui.rectangle(
            rectangle(center - Vec2::new(1.0, 8.0), Vec2::new(2.0, 16.0)),
            Color::new(240, 240, 240, 220),
        );
        // Hotbar.
        let bar = Aabb2 {
            min: layout.hotbar[0].min - Vec2::splat(4.0),
            max: layout.hotbar[HOTBAR_SLOTS - 1].max + Vec2::splat(4.0),
        };
        ui.rectangle(bar, Color::new(0, 0, 0, 130));
        for (i, &bounds) in layout.hotbar.iter().enumerate() {
            ui.rectangle(bounds, Color::new(60, 60, 60, 170));
            outline(ui, bounds, 1.0, Color::new(120, 120, 120, 200));
            self.stack(ui, bounds, self.survival.inventory.slots()[i]);
        }
        let selected = layout.hotbar[self.survival.selected()];
        outline(ui, inset(selected, -3.0), 3.0, Color::WHITE);
        let pitch = layout.hotbar[0].size().x;
        let heart = (pitch * 0.38).clamp(10.0, 16.0);
        self.hearts(ui, Vec2::new(bar.min.x, bar.min.y - heart - 4.0), heart);
        // Newly selected item name, fading out.
        if self.item_name_timer > 0.0
            && let Some(stack) = self.survival.held()
        {
            let alpha = (self.item_name_timer / 0.5).min(1.0);
            centered(
                ui,
                stack.item().name(),
                center.x,
                bar.min.y - heart - 30.0,
                16.0,
                Color::new(255, 255, 255, (alpha * 255.0) as u8),
            );
        }
        if let Some((message, timer)) = &self.message {
            let alpha = (timer / 0.5).min(1.0);
            centered(
                ui,
                message,
                center.x,
                bar.min.y - heart - 52.0,
                16.0,
                Color::new(255, 238, 140, (alpha * 255.0) as u8),
            );
        }
        if !self.debug && self.play_time < HINT_SECONDS {
            let alpha = ((HINT_SECONDS - self.play_time) / 2.0).min(1.0);
            centered(
                ui,
                "E inventory  |  ESC menu & controls  |  F3 debug",
                center.x,
                size.y * 0.18,
                14.0,
                Color::new(255, 255, 255, (alpha * 230.0) as u8),
            );
        }
        let mut alerts = Vec::new();
        if self.waiting {
            alerts.push("Loading nearby terrain...");
        }
        if self.survival.pickups().len() == MAX_PICKUPS {
            alerts.push("Too many dropped items: collect some before mining");
        }
        for (i, alert) in alerts.into_iter().enumerate() {
            centered(
                ui,
                alert,
                center.x,
                size.y * 0.26 + i as f32 * 20.0,
                14.0,
                Color::YELLOW,
            );
        }
        if self.debug {
            self.debug_overlay(ui);
        }
    }
    fn debug_overlay<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>) {
        let p = self.player.position();
        let feet = BlockPos::new(
            p.x.floor() as i32,
            (p.y - 0.9).floor() as i32,
            p.z.floor() as i32,
        );
        let (chunk, _) = feet.split();
        let yaw = self.player.controller.yaw().to_degrees().rem_euclid(360.0);
        let facing = ["east", "south", "west", "north"][(((yaw + 45.0) / 90.0) as usize) % 4];
        let target = self
            .interaction_report
            .selected
            .map(|h| {
                let name = self
                    .world
                    .registry()
                    .get(h.block)
                    .map_or("?", |b| b.name.trim_start_matches("demo:"));
                format!(
                    "Target: {name} at {} {} {} ({:.0}% mined)",
                    h.position.x,
                    h.position.y,
                    h.position.z,
                    self.interaction_report.progress * 100.0
                )
            })
            .unwrap_or_else(|| "Target: none".into());
        let (hour, minute) = self.sky.clock();
        let r = self.report;
        let g = &self.render_report.resources;
        let held = self
            .survival
            .held()
            .map_or("empty hand", |s| s.item().name());
        let lines = [
            format!("rayengine voxel demo  (seed {})", self.terrain.seed()),
            format!("XYZ: {:.2} / {:.2} / {:.2}", p.x, p.y - 0.9, p.z),
            format!(
                "Block: {} {} {}   Chunk: {} {} {}",
                feet.x, feet.y, feet.z, chunk.x, chunk.y, chunk.z
            ),
            format!("Facing: {facing} ({yaw:.0} deg)"),
            target,
            format!(
                "Day {} {hour:02}:{minute:02}  daylight {:.0}%",
                self.sky.day(),
                self.sky.light().daylight * 100.0
            ),
            format!("Holding: {held}"),
            format!(
                "Chunks: {}/{} resident, {} jobs, {} ready",
                r.resident, r.desired, r.jobs, r.ready
            ),
            format!(
                "GPU: {} chunks, {} meshes, {} KiB",
                g.chunks,
                g.meshes,
                g.buffer_bytes / 1024
            ),
            format!(
                "Pickups: {}  debris: {}",
                self.survival.pickups().len(),
                self.debris.len()
            ),
            String::new(),
            "WASD move, mouse look, SPACE jump, SHIFT sprint".into(),
            "LMB mine, RMB place, 1-9 select, E inventory".into(),
            "ESC menu, F5 save, F3 hide debug, F10 quit".into(),
        ];
        for (i, line) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let at = Vec2::new(6.0, 6.0 + i as f32 * 17.0);
            ui.rectangle(
                rectangle(
                    at - Vec2::new(2.0, 1.0),
                    Vec2::new(text_width(line, 13.0) + 4.0, 16.0),
                ),
                Color::new(30, 30, 30, 150),
            );
            ui.text(line, at, 13.0, Color::new(224, 224, 224, 255));
        }
    }
    fn inventory_screen<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, layout: &Layout) {
        let size = ui.logical_size;
        ui.rectangle(rectangle(Vec2::ZERO, size), Color::new(0, 0, 0, 140));
        bevel(ui, layout.panel, PANEL, true);
        let title = if layout.wide {
            layout.crafting.min - Vec2::new(0.0, 24.0)
        } else {
            layout.panel.min + Vec2::new(16.0, 10.0)
        };
        ui.text(
            "Crafting",
            if layout.wide {
                title
            } else {
                layout.crafting.min - Vec2::new(0.0, 22.0)
            },
            16.0,
            LABEL,
        );
        ui.text(
            "Inventory",
            if layout.wide {
                Vec2::new(layout.inventory.min.x, title.y)
            } else {
                title
            },
            16.0,
            LABEL,
        );
        // Slots.
        for (i, &bounds) in layout.slots.iter().enumerate() {
            let fill = if self.menu.source() == Some(i) {
                SLOT_SOURCE
            } else if self.menu.highlighted(slot_id(i)) {
                SLOT_HOVER
            } else {
                SLOT
            };
            bevel(ui, bounds, fill, false);
            self.stack(ui, bounds, self.survival.inventory.slots()[i]);
            if i == self.survival.selected() {
                outline(ui, inset(bounds, -1.0), 2.0, Color::WHITE);
            }
            if self
                .menu
                .state()
                .response(slot_id(i))
                .is_some_and(|r| r.focused)
            {
                outline(ui, bounds, 2.0, BUTTON.focus);
            }
        }
        // Recipe cards.
        for (i, &bounds) in layout.recipes.iter().enumerate() {
            self.recipe_card(ui, layout, i, bounds);
        }
        self.inventory_info(ui, layout);
    }
    fn recipe_card<D: RaylibDraw>(
        &self,
        ui: &mut UiCanvas<'_, D>,
        layout: &Layout,
        i: usize,
        bounds: Aabb2,
    ) {
        let recipe = Recipe::ALL[i];
        let response = self.menu.state().response(recipe_id(i));
        let enabled = response.is_some_and(|r| r.enabled);
        let hovered = response.is_some_and(|r| r.hovered || r.focused);
        let fill = match (enabled, hovered, response.is_some_and(|r| r.held)) {
            (true, _, true) => Color::new(150, 170, 150, 255),
            (true, true, _) => Color::new(176, 204, 176, 255),
            (true, false, _) => Color::new(168, 184, 168, 255),
            (false, true, _) => Color::new(160, 160, 160, 255),
            (false, false, _) => Color::new(150, 150, 150, 255),
        };
        bevel(ui, bounds, fill, true);
        if response.is_some_and(|r| r.focused) {
            outline(ui, bounds, 2.0, BUTTON.focus);
        }
        let (output, count) = recipe.output();
        let h = bounds.size().y;
        let icon = h - 10.0;
        let icon_bounds = rectangle(bounds.min + Vec2::splat(5.0), Vec2::splat(icon));
        self.item_icon(ui, output, icon_bounds);
        if count > 1 {
            let text = count.to_string();
            shadowed(
                ui,
                &text,
                icon_bounds.max - Vec2::new(text_width(&text, 11.0), 10.0),
                11.0,
                Color::WHITE,
            );
        }
        let text_x = icon_bounds.max.x + 7.0;
        let name_color = if enabled {
            LABEL
        } else {
            Color::new(95, 95, 95, 255)
        };
        let name_size = if layout.wide { 14.0 } else { 10.0 };
        ui.text(
            output.name(),
            Vec2::new(text_x, bounds.min.y + 4.0),
            name_size,
            name_color,
        );
        // Ingredients with have/need counts.
        let mut x = text_x;
        let small = if layout.wide { 13.0 } else { 10.0 };
        let y = bounds.max.y - small - 5.0;
        for &(item, need) in recipe.ingredients() {
            let have = self.survival.inventory.count(item);
            self.item_icon(
                ui,
                item,
                rectangle(Vec2::new(x, y - 1.0), Vec2::splat(small + 1.0)),
            );
            x += small + 3.0;
            let label = if layout.wide {
                format!("{}/{need}", have.min(999))
            } else {
                need.to_string()
            };
            ui.text(
                &label,
                Vec2::new(x, y),
                small - 2.0,
                if have >= need { GOOD } else { BAD },
            );
            x += text_width(&label, small - 2.0) + 8.0;
        }
    }
    fn inventory_info<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, layout: &Layout) {
        let hovered_slot = (0..layout.slots.len())
            .find(|&i| self.menu.highlighted(slot_id(i)))
            .and_then(|i| self.survival.inventory.slots()[i]);
        let hovered_recipe =
            (0..layout.recipes.len()).find(|&i| self.menu.highlighted(recipe_id(i)));
        let (title, detail): (String, String) = if let Some(i) = hovered_recipe {
            let recipe = Recipe::ALL[i];
            let (item, count) = recipe.output();
            let needs = recipe
                .ingredients()
                .iter()
                .map(|&(item, n)| format!("{n} {}", item.name()))
                .collect::<Vec<_>>()
                .join(" + ");
            let status = if self.survival.inventory.can_craft(recipe) {
                "Click to craft"
            } else if recipe
                .ingredients()
                .iter()
                .all(|&(item, n)| self.survival.inventory.count(item) >= n)
            {
                "Inventory full"
            } else {
                "Missing ingredients"
            };
            (
                format!("{count} x {}", item.name()),
                format!("{needs}  -  {status}"),
            )
        } else if let Some(stack) = hovered_slot {
            (
                format!("{} x{}", stack.item().name(), stack.count()),
                describe(stack.item()).into(),
            )
        } else if self.menu.source().is_some() {
            ("Moving item".into(), "Click another slot to swap".into())
        } else {
            (
                "Click a slot, then another, to swap".into(),
                "Click a recipe to craft  |  E or ESC to close".into(),
            )
        };
        if layout.wide {
            let info = layout.info;
            ui.text(&title, info.min + Vec2::new(0.0, 4.0), 15.0, LABEL);
            ui.text(
                &detail,
                info.min + Vec2::new(0.0, 26.0),
                12.0,
                Color::new(80, 80, 80, 255),
            );
        } else if hovered_slot.is_some() || hovered_recipe.is_some() {
            ui.text(&title, layout.info.min + Vec2::new(0.0, 4.0), 11.0, LABEL);
        }
    }
    fn dialog<D: RaylibDraw>(
        &self,
        ui: &mut UiCanvas<'_, D>,
        layout: &Layout,
        buttons: [(UiId, &str); 2],
    ) {
        for ((id, label), bounds) in buttons.into_iter().zip([layout.close, layout.quit]) {
            match self.menu.state().response(id) {
                Some(response) => ui.button(bounds, label, response, BUTTON),
                None => {
                    ui.rectangle(bounds, BUTTON.normal);
                    centered(
                        ui,
                        label,
                        bounds.center().x,
                        bounds.min.y + 9.0,
                        16.0,
                        Color::WHITE,
                    );
                }
            }
        }
    }
    fn pause_screen<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, layout: &Layout) {
        let size = ui.logical_size;
        ui.rectangle(rectangle(Vec2::ZERO, size), Color::new(0, 0, 0, 150));
        let d = layout.dialog;
        bevel(ui, d, Color::new(48, 52, 58, 245), true);
        centered(
            ui,
            "Game Menu",
            d.center().x,
            d.min.y + 22.0,
            22.0,
            Color::WHITE,
        );
        self.dialog(
            ui,
            layout,
            [
                (CLOSE, "Back to Game"),
                (
                    QUIT,
                    if self.saving.is_some() {
                        "Save and Quit"
                    } else {
                        "Quit Game"
                    },
                ),
            ],
        );
        let mut y = layout.quit.max.y + 16.0;
        ui.text(
            "Controls",
            Vec2::new(d.min.x + 24.0, y),
            15.0,
            Color::new(255, 238, 140, 255),
        );
        y += 22.0;
        for (keys, action) in [
            ("WASD / Mouse", "Move / look"),
            ("Space / Shift", "Jump / sprint"),
            ("Left / Right click", "Mine / place"),
            ("1-9", "Select hotbar slot"),
            ("E", "Inventory & crafting"),
            ("F3 / F5 / F10", "Debug / save / quit"),
        ] {
            if y + 14.0 > d.max.y - 26.0 {
                break;
            }
            ui.text(
                keys,
                Vec2::new(d.min.x + 24.0, y),
                12.0,
                Color::new(210, 210, 210, 255),
            );
            ui.text(
                action,
                Vec2::new(d.center().x + 10.0, y),
                12.0,
                Color::new(170, 170, 170, 255),
            );
            y += 18.0;
        }
        let (hour, minute) = self.sky.clock();
        ui.text(
            &format!(
                "Day {} - {hour:02}:{minute:02} (time is paused)",
                self.sky.day()
            ),
            Vec2::new(d.min.x + 24.0, d.max.y - 22.0),
            11.0,
            Color::new(150, 150, 150, 255),
        );
    }
    fn death_screen<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>, layout: &Layout) {
        let size = ui.logical_size;
        ui.rectangle(rectangle(Vec2::ZERO, size), Color::new(110, 0, 0, 140));
        let d = layout.dialog;
        centered(
            ui,
            "You died!",
            d.center().x,
            d.min.y + 14.0,
            30.0,
            Color::WHITE,
        );
        self.dialog(ui, layout, [(RESPAWN, "Respawn"), (QUIT, "Quit Game")]);
        centered(
            ui,
            "Your inventory and world edits are kept.",
            d.center().x,
            layout.quit.max.y + 18.0,
            12.0,
            Color::new(230, 210, 210, 255),
        );
        if self.respawn_pending {
            centered(
                ui,
                "Waiting for safe, loaded spawn terrain...",
                d.center().x,
                layout.quit.max.y + 38.0,
                12.0,
                Color::YELLOW,
            );
        }
    }
    fn status<D: RaylibDraw>(&self, ui: &mut UiCanvas<'_, D>) {
        let size = ui.logical_size;
        let mut lines: Vec<(String, Color)> = Vec::new();
        if let Some(saving) = &self.saving {
            let status = if self.closing {
                Some("Saving world before exit...")
            } else {
                match saving.status() {
                    SaveStatus::Writing => Some("Saving..."),
                    SaveStatus::Failed => Some("Save failed: chunks kept loaded | F5 retry"),
                    SaveStatus::Idle | SaveStatus::Requested | SaveStatus::Saved => None,
                }
            };
            if let Some(status) = status {
                lines.push((status.into(), Color::YELLOW));
            }
            if let Some(error) = saving.error() {
                lines.push((error.to_string(), Color::RED));
            }
            if saving.history_full(&self.world) {
                lines.push((
                    "World edit limit reached: existing edited areas remain editable".into(),
                    Color::YELLOW,
                ));
            }
        }
        if let Some(error) = &self.error {
            lines.push((error.clone(), Color::RED));
        }
        for (i, (line, color)) in lines.iter().rev().enumerate() {
            let at = Vec2::new(
                size.x - text_width(line, 12.0) - 10.0,
                size.y - 20.0 - i as f32 * 16.0,
            );
            ui.rectangle(
                rectangle(
                    at - Vec2::new(3.0, 2.0),
                    Vec2::new(text_width(line, 12.0) + 6.0, 16.0),
                ),
                Color::new(0, 0, 0, 150),
            );
            ui.text(line, at, 12.0, *color);
        }
    }
    pub(super) fn draw_hud(&self, frame: &mut Frame<'_, '_>) {
        let layout = Layout::new(frame.viewport.logical_size);
        let dead = self.survival.health.value() == 0;
        frame.ui(|ui| {
            if dead {
                self.death_screen(ui, &layout);
            } else {
                match self.menu.screen() {
                    Screen::Playing => self.gameplay_hud(ui, &layout),
                    Screen::Inventory => self.inventory_screen(ui, &layout),
                    Screen::Paused => self.pause_screen(ui, &layout),
                }
            }
            self.status(ui);
        });
    }
}
