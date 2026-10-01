//! Draw the same CPU-resolved bounds used for inventory interaction.
use super::*;
use crate::{
    hud::{CLOSE, Layout, QUIT, RESPAWN, recipe_id, slot_id},
    survival::{MAX_HEALTH, MAX_PICKUPS, Recipe, Stack},
};
use rayengine::raylib::prelude::RaylibDraw;
use rayengine::render::UiCanvas;
fn rectangle(min: Vec2, size: Vec2) -> Aabb2 {
    Aabb2 {
        min,
        max: min + size,
    }
}
fn slot<D: RaylibDraw>(
    ui: &mut UiCanvas<'_, D>,
    bounds: Aabb2,
    stack: Option<Stack>,
    selected: bool,
    label: Option<usize>,
) {
    ui.rectangle(
        bounds,
        if selected {
            Color::new(216, 181, 88, 255)
        } else {
            Color::new(49, 57, 64, 240)
        },
    );
    let inner = Aabb2 {
        min: bounds.min + Vec2::splat(2.0),
        max: bounds.max - Vec2::splat(2.0),
    };
    ui.rectangle(inner, Color::new(24, 29, 35, 240));
    if let Some(stack) = stack {
        let [r, g, b] = stack.item().color();
        ui.rectangle(
            rectangle(
                bounds.min + Vec2::new(5.0, 5.0),
                Vec2::new(bounds.size().x - 10.0, 5.0),
            ),
            Color::new(r, g, b, 255),
        );
        let font = (bounds.size().x / 4.2).min(11.0);
        ui.text(
            stack.item().label(),
            bounds.min + Vec2::new(4.0, 13.0),
            font,
            Color::WHITE,
        );
        ui.text(
            &stack.count().to_string(),
            bounds.min + Vec2::new(4.0, bounds.size().y - 14.0),
            10.0,
            Color::LIGHTGRAY,
        );
    }
    if let Some(i) = label {
        ui.text(
            &(i + 1).to_string(),
            Vec2::new(bounds.max.x - 11.0, bounds.max.y - 14.0),
            10.0,
            Color::GRAY,
        );
    }
}
impl TerrainPreview {
    pub(super) fn draw_hud(&self, frame: &mut Frame<'_, '_>) {
        let layout = Layout::new(frame.viewport.logical_size);
        let dead = self.survival.health.value() == 0;
        frame.ui(|ui| {
            let size = ui.logical_size;
            if !self.menu.open() && !dead {
                ui.circle(size * 0.5, 2.0, Color::WHITE);
                let target = self
                    .interaction_report
                    .selected
                    .map(|h| {
                        self.world
                            .registry()
                            .get(h.block)
                            .unwrap()
                            .name
                            .trim_start_matches("demo:")
                    })
                    .unwrap_or("none");
                ui.text(
                    "WASD move | mouse look | SPACE jump | SHIFT sprint",
                    Vec2::new(16.0, 16.0),
                    14.0,
                    Color::WHITE,
                );
                ui.text(
                    "LMB mine | RMB place | 1-9 select | E/ESC inventory | F10 quit",
                    Vec2::new(16.0, 36.0),
                    14.0,
                    Color::WHITE,
                );
                ui.text(
                    &format!(
                        "Target: {target} | mining {:.0}%",
                        self.interaction_report.progress * 100.0
                    ),
                    Vec2::new(16.0, 56.0),
                    14.0,
                    Color::WHITE,
                );
                for (i, &bounds) in layout.hotbar.iter().enumerate() {
                    slot(
                        ui,
                        bounds,
                        self.survival.inventory.slots()[i],
                        i == self.survival.selected(),
                        Some(i),
                    );
                }
                let health_pos = Vec2::new(layout.hotbar[0].min.x, layout.hotbar[0].min.y - 24.0);
                ui.rectangle(
                    rectangle(health_pos, Vec2::new(160.0, 16.0)),
                    Color::new(45, 24, 27, 230),
                );
                ui.rectangle(
                    rectangle(
                        health_pos,
                        Vec2::new(
                            160.0 * f32::from(self.survival.health.value()) / f32::from(MAX_HEALTH),
                            16.0,
                        ),
                    ),
                    Color::new(190, 55, 59, 255),
                );
                ui.text(
                    &format!("Health {}/20", self.survival.health.value()),
                    health_pos + Vec2::new(5.0, 1.0),
                    12.0,
                    Color::WHITE,
                );
                if self.waiting {
                    ui.text(
                        "Waiting for nearby terrain",
                        Vec2::new(16.0, 78.0),
                        14.0,
                        Color::YELLOW,
                    );
                }
                if self.survival.pickups().len() == MAX_PICKUPS {
                    ui.text(
                        "Pickup limit reached: collect items before mining",
                        Vec2::new(16.0, 98.0),
                        14.0,
                        Color::YELLOW,
                    );
                }
            } else {
                ui.rectangle(rectangle(Vec2::ZERO, size), Color::new(0, 0, 0, 160));
                ui.rectangle(layout.panel, Color::new(19, 25, 33, 250));
                ui.text(
                    if dead {
                        "You died"
                    } else {
                        "Inventory & crafting"
                    },
                    layout.panel.min + Vec2::new(16.0, 14.0),
                    22.0,
                    Color::WHITE,
                );
                let width = layout.panel.size().x;
                let small = (width / 45.0).min(14.0);
                ui.text(
                    if dead {
                        "Respawn keeps inventory. World edits stay for this session."
                    } else {
                        "Click two slots to swap. First row is the hotbar."
                    },
                    layout.panel.min + Vec2::new(16.0, 43.0),
                    small,
                    Color::LIGHTGRAY,
                );
                if !dead {
                    for (i, &bounds) in layout.slots.iter().enumerate() {
                        let selected = self.menu.source() == Some(i)
                            || self
                                .menu
                                .state()
                                .response(slot_id(i))
                                .is_some_and(|r| r.hovered || r.focused);
                        slot(
                            ui,
                            bounds,
                            self.survival.inventory.slots()[i],
                            selected,
                            if i < 9 { Some(i) } else { None },
                        );
                    }
                    for (i, &bounds) in layout.recipes.iter().enumerate() {
                        let style = UiButtonStyle {
                            font_size: (bounds.size().x / 18.0).min(13.0),
                            ..Default::default()
                        };
                        if let Some(response) = self.menu.state().response(recipe_id(i)) {
                            ui.button(bounds, Recipe::ALL[i].label(), response, style);
                        } else {
                            ui.rectangle(bounds, style.normal);
                            ui.text(
                                Recipe::ALL[i].label(),
                                bounds.min + Vec2::splat(6.0),
                                style.font_size,
                                Color::WHITE,
                            );
                        }
                    }
                    ui.text(
                        "Tab/Up/Down focus | Enter/Space select | E/ESC resume",
                        Vec2::new(layout.panel.min.x + 16.0, layout.close.min.y - 21.0),
                        small,
                        Color::LIGHTGRAY,
                    );
                } else if self.respawn_pending {
                    ui.text(
                        "Waiting for safe, loaded spawn terrain",
                        layout.panel.min + Vec2::new(16.0, 90.0),
                        small,
                        Color::YELLOW,
                    );
                }
                for (id, bounds, label) in [
                    (
                        if dead { RESPAWN } else { CLOSE },
                        layout.close,
                        if dead { "Respawn" } else { "Resume" },
                    ),
                    (QUIT, layout.quit, "Quit"),
                ] {
                    if let Some(response) = self.menu.state().response(id) {
                        ui.button(
                            bounds,
                            label,
                            response,
                            UiButtonStyle {
                                font_size: 16.0,
                                ..Default::default()
                            },
                        );
                    } else {
                        ui.rectangle(bounds, Color::new(37, 49, 66, 255));
                        ui.text(label, bounds.min + Vec2::new(8.0, 6.0), 16.0, Color::WHITE);
                    }
                }
            }
            if let Some(error) = &self.error {
                ui.text(error, Vec2::new(16.0, 118.0), 12.0, Color::RED);
            }
        });
    }
}
