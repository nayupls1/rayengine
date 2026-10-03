//! CPU layout and modal routing for the demo's inventory, pause and death screens.
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
/// Pause-menu resume button.
pub const CLOSE: UiId = UiId(200);
/// Native exit button (pause and death screens).
pub const QUIT: UiId = UiId(201);
/// Explicit death-screen respawn button.
pub const RESPAWN: UiId = UiId(202);
/// Recipes listed by the crafting panel.
pub const RECIPES: usize = Recipe::ALL.len();
/// Interactive regions of the inventory screen.
pub const INVENTORY_REGIONS: usize = INVENTORY_SLOTS + RECIPES;

/// Which full-screen overlay currently owns input. Death is derived from health.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    /// No overlay; gameplay receives input.
    #[default]
    Playing,
    /// Inventory and crafting (E).
    Inventory,
    /// Game menu (Escape); the day/night clock stops here.
    Paused,
}
/// Resolved shared bounds for drawing and hit testing in current logical units.
pub struct Layout {
    /// Inventory/crafting panel, adapting to narrow viewports.
    pub panel: Aabb2,
    /// Crafting list column (title above the first recipe).
    pub crafting: Aabb2,
    /// Inventory grid column; reserve rows above a separated hotbar row.
    pub inventory: Aabb2,
    /// Slots in inventory order: hotbar 0..9, then three reserve rows.
    pub slots: [Aabb2; INVENTORY_SLOTS],
    /// One card per recipe in Recipe::ALL order; clicking crafts once.
    pub recipes: [Aabb2; RECIPES],
    /// Hover/selection details for the item or recipe under the pointer.
    pub info: Aabb2,
    /// Wide layouts show ingredient icons; narrow ones show compact counts.
    pub wide: bool,
    /// Centered pause/death dialog.
    pub dialog: Aabb2,
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
/// Gap separating the hotbar row from the reserve rows, like the original game.
const HOTBAR_GAP: f32 = 8.0;
fn grid(min: Vec2, pitch: f32) -> [Aabb2; INVENTORY_SLOTS] {
    std::array::from_fn(|i| {
        let (column, row, gap) = if i < HOTBAR_SLOTS {
            (i, 3, HOTBAR_GAP)
        } else {
            ((i - HOTBAR_SLOTS) % 9, (i - HOTBAR_SLOTS) / 9, 0.0)
        };
        rect(
            min + Vec2::new(column as f32 * pitch, row as f32 * pitch + gap),
            Vec2::splat(pitch - 4.0),
        )
    })
}
impl Layout {
    /// Logical viewport must be at least 320×480; normal fitted SDK reference is 960×540.
    pub fn new(size: Vec2) -> Self {
        let wide = size.x >= 720.0 && size.y >= 400.0;
        let (panel, crafting, inventory, slots, recipes, info) = if wide {
            let pitch = 40.0;
            let grid_size = Vec2::new(pitch * 9.0 - 4.0, pitch * 4.0 - 4.0 + HOTBAR_GAP);
            let recipe_width = (size.x - 32.0 - grid_size.x - 56.0).min(340.0);
            let card = 38.0;
            let list_height = RECIPES as f32 * (card + 4.0) - 4.0;
            let panel_size = Vec2::new(
                16.0 + recipe_width + 24.0 + grid_size.x + 16.0,
                44.0 + list_height + 16.0,
            );
            let panel = rect(
                ((size - panel_size) * 0.5).max(Vec2::splat(8.0)),
                panel_size,
            );
            let top = panel.min + Vec2::new(16.0, 44.0);
            let crafting = rect(top, Vec2::new(recipe_width, list_height));
            let recipes = std::array::from_fn(|i| {
                rect(
                    top + Vec2::Y * (i as f32 * (card + 4.0)),
                    Vec2::new(recipe_width, card),
                )
            });
            let grid_min = Vec2::new(crafting.max.x + 24.0, top.y);
            let inventory = rect(grid_min, grid_size);
            let info = Aabb2 {
                min: Vec2::new(grid_min.x, inventory.max.y + 12.0),
                max: Vec2::new(inventory.max.x, panel.max.y - 16.0),
            };
            (
                panel,
                crafting,
                inventory,
                grid(grid_min, pitch),
                recipes,
                info,
            )
        } else {
            let width = (size.x - 16.0).min(420.0);
            let pitch = ((width - 32.0 + 4.0) / 9.0).min(40.0);
            let grid_size = Vec2::new(pitch * 9.0 - 4.0, pitch * 4.0 - 4.0 + HOTBAR_GAP);
            let card = 34.0;
            let rows = RECIPES.div_ceil(2);
            let list_height = rows as f32 * (card + 4.0) - 4.0;
            let panel_size =
                Vec2::new(width, 36.0 + grid_size.y + 14.0 + 24.0 + list_height + 14.0);
            let panel = rect(
                ((size - panel_size) * 0.5).max(Vec2::splat(8.0)),
                panel_size,
            );
            let grid_min = Vec2::new(
                panel.min.x + (width - grid_size.x) * 0.5,
                panel.min.y + 36.0,
            );
            let inventory = rect(grid_min, grid_size);
            let list_min = Vec2::new(panel.min.x + 16.0, inventory.max.y + 14.0 + 24.0);
            let card_width = (width - 32.0 - 8.0) * 0.5;
            let crafting = rect(list_min, Vec2::new(width - 32.0, list_height));
            let recipes = std::array::from_fn(|i| {
                rect(
                    list_min
                        + Vec2::new(
                            (i % 2) as f32 * (card_width + 8.0),
                            (i / 2) as f32 * (card + 4.0),
                        ),
                    Vec2::new(card_width, card),
                )
            });
            // Narrow screens show hover details on the title line.
            let info = Aabb2 {
                min: Vec2::new(panel.min.x + width * 0.42, panel.min.y + 8.0),
                max: Vec2::new(panel.max.x - 12.0, panel.min.y + 30.0),
            };
            (
                panel,
                crafting,
                inventory,
                grid(grid_min, pitch),
                recipes,
                info,
            )
        };
        let dialog_size = Vec2::new((size.x - 32.0).min(380.0), (size.y - 32.0).min(352.0));
        let dialog = rect((size - dialog_size) * 0.5, dialog_size);
        let button = Vec2::new(dialog_size.x - 48.0, 34.0);
        let close = rect(dialog.min + Vec2::new(24.0, 64.0), button);
        let quit = rect(dialog.min + Vec2::new(24.0, 106.0), button);
        let hotbar_pitch = ((size.x - 32.0) / 9.0).min(44.0);
        let hotbar_min = Vec2::new(
            (size.x - hotbar_pitch * 9.0 + 4.0) * 0.5,
            size.y - hotbar_pitch - 10.0,
        );
        let hotbar = std::array::from_fn(|i| {
            rect(
                hotbar_min + Vec2::X * (i as f32 * hotbar_pitch),
                Vec2::splat(hotbar_pitch - 4.0),
            )
        });
        Self {
            panel,
            crafting,
            inventory,
            slots,
            recipes,
            info,
            wide,
            dialog,
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
    /// E inventory toggle edge.
    pub toggle: bool,
    /// Escape edge: pause, resume, or close the inventory.
    pub pause: bool,
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
    /// The game menu is open: the world clock and timers stop.
    pub paused: bool,
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
    /// Successful crafts this tick, for feedback.
    pub crafted: Option<Recipe>,
}
/// Small reusable immediate UI state; slots are exchanged on two activations.
/// No transient carried item can disappear on close/focus loss.
pub struct Menu {
    screen: Screen,
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
            screen: Screen::Playing,
            was_dead: false,
            source: None,
            state: UiState::with_capacity(INVENTORY_REGIONS),
            regions: Vec::with_capacity(INVENTORY_REGIONS),
            suppress_mining: false,
            suppress_place: false,
            suppress_jump: false,
        }
    }
}
impl Menu {
    /// Any inventory or pause overlay is open; death is a separate always-modal screen.
    pub fn open(&self) -> bool {
        self.screen != Screen::Playing
    }
    /// Current overlay.
    pub fn screen(&self) -> Screen {
        self.screen
    }
    /// Selected source slot for a two-click exchange.
    pub fn source(&self) -> Option<usize> {
        self.source
    }
    /// Shared CPU responses for drawing.
    pub fn state(&self) -> &UiState {
        &self.state
    }
    /// Pointer or keyboard focus on a region, for highlights and details.
    pub fn highlighted(&self, id: UiId) -> bool {
        self.state
            .response(id)
            .is_some_and(|r| r.hovered || r.focused)
    }
    /// Submit the current layout and process crafting/exchanges before gameplay.
    pub fn update(&mut self, size: Vec2, input: MenuInput, survival: &mut Survival) -> MenuReport {
        let dead = survival.health.value() == 0;
        let before = self.screen;
        if dead {
            self.screen = Screen::Playing;
        } else if input.pause {
            self.screen = match self.screen {
                Screen::Playing => Screen::Paused,
                Screen::Inventory | Screen::Paused => Screen::Playing,
            };
        } else if input.toggle {
            self.screen = match self.screen {
                Screen::Playing => Screen::Inventory,
                Screen::Inventory => Screen::Playing,
                Screen::Paused => Screen::Paused,
            };
        }
        let layout = Layout::new(size);
        self.regions.clear();
        if dead {
            self.regions.extend([
                UiRegion::new(RESPAWN, layout.close),
                UiRegion::new(QUIT, layout.quit),
            ]);
        } else {
            match self.screen {
                Screen::Inventory => {
                    for (i, &bounds) in layout.slots.iter().enumerate() {
                        self.regions.push(UiRegion::new(slot_id(i), bounds));
                    }
                    for (i, &bounds) in layout.recipes.iter().enumerate() {
                        let mut region = UiRegion::new(recipe_id(i), bounds);
                        region.enabled = survival.inventory.can_craft(Recipe::ALL[i]);
                        self.regions.push(region);
                    }
                }
                Screen::Paused => self.regions.extend([
                    UiRegion::new(CLOSE, layout.close),
                    UiRegion::new(QUIT, layout.quit),
                ]),
                Screen::Playing => {}
            }
        }
        // A transition consumes its pointer/keyboard gestures without activation.
        let mut ui_input = input.ui;
        if before != self.screen || self.was_dead != dead || input.toggle || input.pause {
            ui_input.cancel = true;
        }
        if !ui_input.window_focused || ui_input.cancel || self.screen != Screen::Inventory {
            self.source = None;
        }
        self.state.update(&self.regions, ui_input);
        let activated = |id| self.state.response(id).is_some_and(|r| r.activated);
        let respawn = dead && activated(RESPAWN);
        let quit = activated(QUIT);
        let mut crafted = None;
        match self.screen {
            Screen::Inventory => {
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
                    if activated(recipe_id(i)) && survival.inventory.craft(r).is_ok() {
                        crafted = Some(r);
                    }
                }
            }
            Screen::Paused if activated(CLOSE) => self.screen = Screen::Playing,
            _ => {}
        }
        let modal =
            before != Screen::Playing || self.screen != Screen::Playing || dead || self.was_dead;
        self.was_dead = dead;
        self.suppress_mining = input.mining_down && (modal || self.suppress_mining);
        self.suppress_place = input.place_down && (modal || self.suppress_place);
        self.suppress_jump = input.jump_down && (modal || self.suppress_jump);
        MenuReport {
            modal,
            paused: self.screen == Screen::Paused,
            mining_allowed: !modal && !self.suppress_mining,
            place_allowed: !modal && !self.suppress_place,
            jump_allowed: !modal && !self.suppress_jump,
            respawn,
            quit,
            crafted,
        }
    }
}
#[cfg(test)]
mod tests;
