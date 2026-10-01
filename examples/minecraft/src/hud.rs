//! CPU layout and modal routing for the demo's inventory/death screen.
use crate::survival::{HOTBAR_SLOTS, INVENTORY_SLOTS, Recipe, Survival};
use rayengine_core::{collision::Aabb2, glam::Vec2, ui::*};

/// Stable UI identity for an inventory slot.
pub fn slot_id(index: usize) -> UiId {
    UiId(index as u64 + 1)
}
/// Stable UI identity for a recipe in Recipe::ALL.
pub fn recipe_id(index: usize) -> UiId {
    UiId(index as u64 + 100)
}
/// Resume button.
pub const CLOSE: UiId = UiId(200);
/// Native exit button.
pub const QUIT: UiId = UiId(201);
/// Explicit death-screen respawn button.
pub const RESPAWN: UiId = UiId(202);
/// Resolved shared bounds for drawing and hit testing in current logical units.
pub struct Layout {
    /// Centered panel, adapting to narrow viewports.
    pub panel: Aabb2,
    /// Hotbar followed by three reserve rows, nine columns each.
    pub slots: [Aabb2; INVENTORY_SLOTS],
    /// Two-column, three-row crafting list.
    pub recipes: [Aabb2; 6],
    /// Resume/respawn button.
    pub close: Aabb2,
    /// Quit button.
    pub quit: Aabb2,
    /// Bottom-centered in-game hotbar.
    pub hotbar: [Aabb2; HOTBAR_SLOTS],
}
fn rect(min: Vec2, size: Vec2) -> Aabb2 {
    Aabb2 {
        min,
        max: min + size,
    }
}
impl Layout {
    /// Logical viewport must be at least 320×480; normal fitted SDK reference is 960×540.
    pub fn new(size: Vec2) -> Self {
        let width = (size.x - 32.0).min(680.0);
        let panel_size = Vec2::new(width, 464.0);
        let panel = rect((size - panel_size) * 0.5, panel_size);
        let pitch = ((width - 32.0) / 9.0).min(48.0);
        let grid_min = Vec2::new(
            panel.min.x + (width - pitch * 9.0) * 0.5,
            panel.min.y + 70.0,
        );
        let slots = std::array::from_fn(|i| {
            rect(
                grid_min + Vec2::new((i % 9) as f32, (i / 9) as f32) * pitch,
                Vec2::splat(pitch - 4.0),
            )
        });
        let recipe_y = grid_min.y + pitch * 4.0 + 20.0;
        let recipe_width = (width - 44.0) * 0.5;
        let recipes = std::array::from_fn(|i| {
            rect(
                Vec2::new(
                    panel.min.x + 16.0 + (i % 2) as f32 * (recipe_width + 12.0),
                    recipe_y + (i / 2) as f32 * 34.0,
                ),
                Vec2::new(recipe_width, 30.0),
            )
        });
        let button_y = panel.max.y - 46.0;
        let close = rect(
            Vec2::new(panel.min.x + 16.0, button_y),
            Vec2::new(recipe_width, 30.0),
        );
        let quit = rect(
            Vec2::new(close.max.x + 12.0, button_y),
            Vec2::new(recipe_width, 30.0),
        );
        let hotbar_min = Vec2::new((size.x - pitch * 9.0) * 0.5, size.y - pitch - 16.0);
        let hotbar = std::array::from_fn(|i| {
            rect(
                hotbar_min + Vec2::X * (i as f32 * pitch),
                Vec2::splat(pitch - 4.0),
            )
        });
        Self {
            panel,
            slots,
            recipes,
            close,
            quit,
            hotbar,
        }
    }
}
/// UI tick with game-defined physical bindings already sampled.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuInput {
    /// Shared pointer/navigation input.
    pub ui: UiInput,
    /// E/Escape toggle edge.
    pub toggle: bool,
    /// Held left button, for suppression through release after modal transitions.
    pub mining_down: bool,
    /// Held right button.
    pub place_down: bool,
    /// Held jump key.
    pub jump_down: bool,
}
/// Explicit UI decisions consumed once by gameplay.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuReport {
    /// Pause movement/look/interactions on opening, open, closing and death ticks.
    pub modal: bool,
    /// Held mining may resume only after a physical release.
    pub mining_allowed: bool,
    /// Placement may resume only after a physical release.
    pub place_allowed: bool,
    /// Jump may resume only after a physical release.
    pub jump_allowed: bool,
    /// Explicit respawn activation.
    pub respawn: bool,
    /// Explicit quit activation.
    pub quit: bool,
}
/// Small reusable immediate UI state; slots are exchanged on two activations.
/// No transient carried item can disappear on close/focus loss.
pub struct Menu {
    open: bool,
    was_dead: bool,
    source: Option<usize>,
    state: UiState,
    regions: Vec<UiRegion>,
    suppress_mining: bool,
    suppress_place: bool,
    suppress_jump: bool,
}
impl Default for Menu {
    fn default() -> Self {
        Self {
            open: false,
            was_dead: false,
            source: None,
            state: UiState::with_capacity(44),
            regions: Vec::with_capacity(44),
            suppress_mining: false,
            suppress_place: false,
            suppress_jump: false,
        }
    }
}
impl Menu {
    /// Inventory open state; death is a separate always-modal screen.
    pub fn open(&self) -> bool {
        self.open
    }
    /// Selected source slot for a two-click exchange.
    pub fn source(&self) -> Option<usize> {
        self.source
    }
    /// Shared CPU responses for drawing.
    pub fn state(&self) -> &UiState {
        &self.state
    }
    /// Submit the current layout and process crafting/exchanges before gameplay.
    pub fn update(&mut self, size: Vec2, input: MenuInput, survival: &mut Survival) -> MenuReport {
        let dead = survival.health.value() == 0;
        let was_open = self.open;
        if dead {
            self.open = false;
            self.source = None;
        } else if input.toggle {
            self.open = !self.open;
            self.source = None;
        }
        let layout = Layout::new(size);
        self.regions.clear();
        if dead {
            self.regions.extend([
                UiRegion::new(RESPAWN, layout.close),
                UiRegion::new(QUIT, layout.quit),
            ]);
        } else if self.open {
            for (i, &bounds) in layout.slots.iter().enumerate() {
                self.regions.push(UiRegion::new(slot_id(i), bounds));
            }
            for (i, &bounds) in layout.recipes.iter().enumerate() {
                let mut region = UiRegion::new(recipe_id(i), bounds);
                region.enabled = survival.inventory.can_craft(Recipe::ALL[i]);
                self.regions.push(region);
            }
            self.regions.extend([
                UiRegion::new(CLOSE, layout.close),
                UiRegion::new(QUIT, layout.quit),
            ]);
        }
        // A transition consumes its pointer/keyboard gestures without activation.
        let mut ui_input = input.ui;
        if was_open != self.open || self.was_dead != dead || input.toggle {
            ui_input.cancel = true;
        }
        if !ui_input.window_focused || ui_input.cancel {
            self.source = None;
        }
        self.state.update(&self.regions, ui_input);
        let activated = |id| self.state.response(id).is_some_and(|r| r.activated);
        let respawn = dead && activated(RESPAWN);
        let quit = activated(QUIT);
        if self.open {
            for i in 0..INVENTORY_SLOTS {
                if activated(slot_id(i)) {
                    if let Some(from) = self.source.take() {
                        survival.inventory.swap(from, i);
                    } else {
                        self.source = Some(i);
                    }
                    if i < HOTBAR_SLOTS {
                        survival.select(i);
                    }
                }
            }
            for (i, r) in Recipe::ALL.into_iter().enumerate() {
                if activated(recipe_id(i)) {
                    let _ = survival.inventory.craft(r);
                }
            }
            if activated(CLOSE) {
                self.open = false;
                self.source = None;
            }
        }
        let modal = was_open || self.open || dead || self.was_dead;
        self.was_dead = dead;
        self.suppress_mining = input.mining_down && (modal || self.suppress_mining);
        self.suppress_place = input.place_down && (modal || self.suppress_place);
        self.suppress_jump = input.jump_down && (modal || self.suppress_jump);
        MenuReport {
            modal,
            mining_allowed: !modal && !self.suppress_mining,
            place_allowed: !modal && !self.suppress_place,
            jump_allowed: !modal && !self.suppress_jump,
            respawn,
            quit,
        }
    }
}
#[cfg(test)]
mod tests;
