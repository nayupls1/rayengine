//! Bounded, CPU-only demo survival rules. Engine and voxel plugins know no items.
use crate::persistence::bounded::Bounded;
use crate::{
    gameplay::{Interaction, InteractionInput, InteractionReport, Player, as_global},
    terrain::DemoBlocks,
};
use rayengine_voxel::{glam::DVec3, prelude::*};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Nine directly selectable slots at the beginning of the inventory.
pub const HOTBAR_SLOTS: usize = 9;
/// Fixed inventory budget, including the hotbar.
pub const INVENTORY_SLOTS: usize = 36;
/// Session-local dropped stacks; full capacity prevents further mining.
pub const MAX_PICKUPS: usize = 128;
/// Maximum health, measured in half-heart units.
pub const MAX_HEALTH: u8 = 20;
/// Game-owned item identities; tools do not stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Item {
    /// Placeable soil, also dropped by grass.
    Dirt,
    /// Placeable stone; requires a pickaxe to harvest.
    Stone,
    /// Placeable tree trunk.
    Log,
    /// Placeable foliage.
    Leaves,
    /// Crafting resource, harvested with either pickaxe.
    Coal,
    /// Crafting resource, harvested with a stone pickaxe.
    IronOre,
    /// Wood crafting ingredient; not a registered terrain block.
    Planks,
    /// Tool handle ingredient.
    Stick,
    /// First-tier rock tool.
    WoodenPickaxe,
    /// Second-tier rock tool.
    StonePickaxe,
    /// First-tier wood tool.
    WoodenAxe,
    /// Second-tier wood tool.
    StoneAxe,
}
impl Item {
    /// Short stable label for slots and pickups.
    pub fn label(self) -> &'static str {
        match self {
            Self::Dirt => "Dirt",
            Self::Stone => "Stone",
            Self::Log => "Log",
            Self::Leaves => "Leaf",
            Self::Coal => "Coal",
            Self::IronOre => "Iron",
            Self::Planks => "Plank",
            Self::Stick => "Stick",
            Self::WoodenPickaxe => "W.Pick",
            Self::StonePickaxe => "S.Pick",
            Self::WoodenAxe => "W.Axe",
            Self::StoneAxe => "S.Axe",
        }
    }
    /// Ordinary items stack to 64; tools occupy one slot each.
    pub fn stack_limit(self) -> u16 {
        match self {
            Self::WoodenPickaxe | Self::StonePickaxe | Self::WoodenAxe | Self::StoneAxe => 1,
            _ => 64,
        }
    }
    /// Shared game palette for CPU-described HUD swatches and world pickups.
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Dirt => [148, 102, 62],
            Self::Stone => [145, 150, 156],
            Self::Log | Self::WoodenPickaxe | Self::WoodenAxe => [154, 112, 66],
            Self::Leaves => [78, 150, 58],
            Self::Coal => [60, 65, 72],
            Self::IronOre => [191, 143, 119],
            Self::Planks => [200, 159, 95],
            Self::Stick => [164, 124, 74],
            Self::StonePickaxe | Self::StoneAxe => [190, 200, 211],
        }
    }
    /// Placeable subset; crafting ingredients/tools cannot masquerade as blocks.
    pub fn block(self, b: DemoBlocks) -> Option<BlockId> {
        match self {
            Self::Dirt => Some(b.dirt),
            Self::Stone => Some(b.stone),
            Self::Log => Some(b.wood),
            Self::Leaves => Some(b.leaves),
            _ => None,
        }
    }
    /// Speed multiplier relative to block hardness; wrong tools behave like a hand.
    pub fn mining_speed(self, block: BlockId, b: DemoBlocks) -> f32 {
        let rock = [b.stone, b.coal, b.iron].contains(&block);
        match self {
            Self::WoodenPickaxe if rock => 3.0,
            Self::StonePickaxe if rock => 6.0,
            Self::WoodenAxe if block == b.wood => 3.0,
            Self::StoneAxe if block == b.wood => 6.0,
            _ => 1.0,
        }
    }
}
/// Validated nonempty stack, constructed only by the inventory/pickup rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Stack {
    item: Item,
    count: u16,
}
impl Stack {
    /// Item identity.
    pub fn item(self) -> Item {
        self.item
    }
    /// Nonzero count, never above this item's stack limit.
    pub fn count(self) -> u16 {
        self.count
    }
}
/// Fixed slots; mutations never allocate or expose invalid stacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inventory {
    slots: [Option<Stack>; INVENTORY_SLOTS],
}
impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; INVENTORY_SLOTS],
        }
    }
}
impl Inventory {
    /// Read-only slot access, also convenient for later save snapshots.
    pub fn slots(&self) -> &[Option<Stack>; INVENTORY_SLOTS] {
        &self.slots
    }
    /// Total count across all slots.
    pub fn count(&self, item: Item) -> u16 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum()
    }
    /// Fill compatible stacks first, then empty slots. Returns the unaccepted count.
    pub fn insert(&mut self, item: Item, mut count: u16) -> u16 {
        for stack in self.slots.iter_mut().flatten().filter(|s| s.item == item) {
            let add = count.min(item.stack_limit() - stack.count);
            stack.count += add;
            count -= add;
        }
        for slot in &mut self.slots {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let add = count.min(item.stack_limit());
                *slot = Some(Stack { item, count: add });
                count -= add;
            }
        }
        count
    }
    /// Remove a total count atomically. Missing ingredients change nothing.
    pub fn remove(&mut self, item: Item, mut count: u16) -> bool {
        if self.count(item) < count {
            return false;
        }
        for slot in &mut self.slots {
            if let Some(stack) = slot.as_mut().filter(|s| s.item == item) {
                let take = count.min(stack.count);
                stack.count -= take;
                count -= take;
                if stack.count == 0 {
                    *slot = None;
                }
            }
        }
        true
    }
    /// Consume exactly one from a slot, keeping empty slots canonical.
    pub fn consume(&mut self, index: usize) -> bool {
        let Some(slot) = self.slots.get_mut(index) else {
            return false;
        };
        let Some(stack) = slot else {
            return false;
        };
        stack.count -= 1;
        if stack.count == 0 {
            *slot = None;
        }
        true
    }
    /// Two-click inventory rearrangement, with no transient held stack to lose.
    pub fn swap(&mut self, a: usize, b: usize) -> bool {
        if a >= INVENTORY_SLOTS || b >= INVENTORY_SLOTS {
            return false;
        }
        self.slots.swap(a, b);
        true
    }
    /// Validate ingredients and result capacity against a copy; commit only on success.
    pub fn craft(&mut self, recipe: Recipe) -> Result<(), CraftError> {
        let mut next = *self;
        for &(item, count) in recipe.ingredients() {
            if !next.remove(item, count) {
                return Err(CraftError::Ingredients);
            }
        }
        let (item, count) = recipe.output();
        if next.insert(item, count) != 0 {
            return Err(CraftError::Capacity);
        }
        *self = next;
        Ok(())
    }
    /// Same admission rules as crafting, without mutating inventory.
    pub fn can_craft(&self, recipe: Recipe) -> bool {
        let mut next = *self;
        next.craft(recipe).is_ok()
    }
}
/// Six inventory-wide recipes, without a workbench or extensive crafting grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    /// One log yields four planks.
    Planks,
    /// Two planks yield four sticks.
    Sticks,
    /// Three planks and two sticks.
    WoodenPickaxe,
    /// Three stones and two sticks.
    StonePickaxe,
    /// Three planks and two sticks.
    WoodenAxe,
    /// Three stones and two sticks.
    StoneAxe,
}
impl Recipe {
    /// Stable UI and benchmark order.
    pub const ALL: [Self; 6] = [
        Self::Planks,
        Self::Sticks,
        Self::WoodenPickaxe,
        Self::StonePickaxe,
        Self::WoodenAxe,
        Self::StoneAxe,
    ];
    /// Recipe cost, aggregated across slots.
    pub fn ingredients(self) -> &'static [(Item, u16)] {
        match self {
            Self::Planks => &[(Item::Log, 1)],
            Self::Sticks => &[(Item::Planks, 2)],
            Self::WoodenPickaxe | Self::WoodenAxe => &[(Item::Planks, 3), (Item::Stick, 2)],
            Self::StonePickaxe | Self::StoneAxe => &[(Item::Stone, 3), (Item::Stick, 2)],
        }
    }
    /// Result item/count.
    pub fn output(self) -> (Item, u16) {
        match self {
            Self::Planks => (Item::Planks, 4),
            Self::Sticks => (Item::Stick, 4),
            Self::WoodenPickaxe => (Item::WoodenPickaxe, 1),
            Self::StonePickaxe => (Item::StonePickaxe, 1),
            Self::WoodenAxe => (Item::WoodenAxe, 1),
            Self::StoneAxe => (Item::StoneAxe, 1),
        }
    }
    /// Compact cost/result text for a responsive button.
    pub fn label(self) -> &'static str {
        match self {
            Self::Planks => "4 planks: 1 log",
            Self::Sticks => "4 sticks: 2 planks",
            Self::WoodenPickaxe => "W.Pick: 3 planks + 2 sticks",
            Self::StonePickaxe => "S.Pick: 3 stone + 2 sticks",
            Self::WoodenAxe => "W.Axe: 3 planks + 2 sticks",
            Self::StoneAxe => "S.Axe: 3 stone + 2 sticks",
        }
    }
}
/// Crafting rejection leaves every slot unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftError {
    /// Recipe costs cannot be met.
    Ingredients,
    /// Result cannot fit after consuming ingredients.
    Capacity,
}
impl fmt::Display for CraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ingredients => "missing ingredients",
            Self::Capacity => "inventory full",
        })
    }
}
impl std::error::Error for CraftError {}
/// Session-local stationary pickup. Full inventories leave it in the world.
#[derive(Clone, Copy, Debug)]
pub struct Pickup {
    /// Global center; only local render coordinates are converted to f32.
    pub position: DVec3,
    /// Remaining stack, reduced by partial collection.
    pub stack: Stack,
}
/// Bounded health and peak-height fall tracking, fed only successful physics steps.
#[derive(Clone, Copy, Debug)]
pub struct Health {
    value: u8,
    peak: Option<f64>,
}
impl Default for Health {
    fn default() -> Self {
        Self {
            value: MAX_HEALTH,
            peak: None,
        }
    }
}
impl Health {
    /// Current half-heart units; zero is dead.
    pub fn value(&self) -> u8 {
        self.value
    }
    /// Saturating damage. Dead players stay dead until explicit respawn.
    pub fn damage(&mut self, amount: u8) {
        self.value = self.value.saturating_sub(amount);
    }
    /// Landing loses ceil(distance - 3) units, with contact-rounding tolerance.
    /// Upward motion updates the peak; stalled streaming/modal ticks do not feed it.
    pub fn movement(&mut self, y: f64, grounded: bool) -> u8 {
        assert!(y.is_finite());
        let peak = self.peak.unwrap_or(y).max(y);
        let damage = if grounded {
            (peak - y - 3.0 - 0.0001)
                .ceil()
                .clamp(0.0, f64::from(MAX_HEALTH)) as u8
        } else {
            0
        };
        self.peak = Some(if grounded { y } else { peak });
        if y < -16.0 {
            self.damage(MAX_HEALTH);
        } else {
            self.damage(damage);
        }
        damage
    }
    /// Reset health and fall history on a safe spawn; never heals merely opening UI.
    pub fn respawn(&mut self) {
        *self = Self::default();
    }
}
/// One tick's raw game interaction request, already routed by the UI.
#[derive(Clone, Copy, Debug, Default)]
pub struct SurvivalInput {
    /// Held mining request.
    pub mining: bool,
    /// Placement press edge.
    pub place: bool,
    /// Simulation step seconds.
    pub dt: f32,
}
/// Game-owned session progress. New worlds start with an empty inventory.
#[derive(Clone)]
pub struct Survival {
    /// Fixed inventory, including hotbar.
    pub inventory: Inventory,
    /// Health/fall state.
    pub health: Health,
    selected: usize,
    pickups: Vec<Pickup>,
}
impl Default for Survival {
    fn default() -> Self {
        Self {
            inventory: Inventory::default(),
            health: Health::default(),
            selected: 0,
            pickups: Vec::with_capacity(MAX_PICKUPS),
        }
    }
}
impl Survival {
    /// Selected hotbar slot.
    pub fn selected(&self) -> usize {
        self.selected
    }
    /// Select a hotbar slot, rejecting reserve slots/out-of-range input.
    pub fn select(&mut self, slot: usize) -> bool {
        if slot >= HOTBAR_SLOTS {
            return false;
        }
        self.selected = slot;
        true
    }
    /// Current held stack; an empty slot means bare hands.
    pub fn held(&self) -> Option<Stack> {
        self.inventory.slots[self.selected]
    }
    /// Live pickups, retained until collected; no timer silently loses resources.
    pub fn pickups(&self) -> &[Pickup] {
        &self.pickups
    }
    /// Mine/place with actual counts, tools and drops. Inventory consumption follows
    /// a successful world edit only. At the pickup cap, mining pauses before mutation.
    pub fn interact(
        &mut self,
        interaction: &mut Interaction,
        world: &mut VoxelWorld,
        player: &Player,
        input: SurvivalInput,
        blocks: DemoBlocks,
    ) -> Result<InteractionReport, VoxelError> {
        let held = self.held().map(Stack::item);
        let block = held.and_then(|i| i.block(blocks)).unwrap_or(BlockId::AIR);
        let report = interaction.apply(
            world,
            player,
            InteractionInput {
                mining: input.mining && self.health.value > 0 && self.pickups.len() < MAX_PICKUPS,
                place: input.place && self.health.value > 0,
                dt: input.dt,
                block,
            },
            |id| held.map_or(1.0, |i| i.mining_speed(id, blocks)),
        )?;
        if let Some(edit) = report.edit {
            if edit.current == BlockId::AIR {
                if let Some(item) = drop_item(edit.previous, held, blocks) {
                    self.pickups.push(Pickup {
                        position: as_global(edit.position) + DVec3::splat(0.5),
                        stack: Stack { item, count: 1 },
                    });
                }
            } else {
                assert!(self.inventory.consume(self.selected));
            }
        }
        Ok(report)
    }
    /// Collect within two blocks of the body center. Missing/full capacity retains
    /// all unaccepted counts; a single pass is bounded by MAX_PICKUPS × 36 slots.
    pub fn collect(&mut self, center: DVec3) -> u16 {
        let mut collected = 0;
        self.pickups.retain_mut(|p| {
            if p.position.distance_squared(center) <= 4.0 {
                let left = self.inventory.insert(p.stack.item, p.stack.count);
                collected += p.stack.count - left;
                p.stack.count = left;
            }
            p.stack.count != 0
        });
        collected
    }
}
/// Minimal harvest tiers; destroying rock with the wrong tool yields no item.
pub fn drop_item(block: BlockId, held: Option<Item>, b: DemoBlocks) -> Option<Item> {
    let pick = matches!(held, Some(Item::WoodenPickaxe | Item::StonePickaxe));
    if block == b.dirt || block == b.grass {
        Some(Item::Dirt)
    } else if block == b.wood {
        Some(Item::Log)
    } else if block == b.leaves {
        Some(Item::Leaves)
    } else if block == b.stone && pick {
        Some(Item::Stone)
    } else if block == b.coal && pick {
        Some(Item::Coal)
    } else if block == b.iron && held == Some(Item::StonePickaxe) {
        Some(Item::IronOre)
    } else {
        None
    }
}
/// Nearest loaded, collision-safe respawn within eight columns of the original
/// spawn. Consult edited cells instead of assuming the generated spawn is intact.
/// Returns None while spawn terrain is unavailable or every candidate is blocked.
pub fn respawn_feet(world: &VoxelWorld, spawn: BlockPos) -> Option<DVec3> {
    for radius in 0_i32..=8 {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs().max(dz.abs()) != radius {
                    continue;
                }
                let (Some(x), Some(z)) = (spawn.x.checked_add(dx), spawn.z.checked_add(dz)) else {
                    continue;
                };
                for y in (0..crate::terrain::WORLD_HEIGHT).rev() {
                    let support = BlockPos::new(x, y, z);
                    let solid = world
                        .block(support)
                        .and_then(|id| world.registry().get(id))
                        .is_some_and(|b| b.collision == CollisionKind::Solid);
                    if solid
                        && world.block(BlockPos::new(x, y + 1, z)) == Some(BlockId::AIR)
                        && world.block(BlockPos::new(x, y + 2, z)) == Some(BlockId::AIR)
                    {
                        return Some(as_global(support) + DVec3::new(0.5, 1.0, 0.5));
                    }
                }
            }
        }
    }
    None
}
#[cfg(test)]
mod tests;

impl<'de> Deserialize<'de> for Stack {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: Item,
            count: u16,
        }
        let Fields { item, count } = Fields::deserialize(d)?;
        if count == 0 || count > item.stack_limit() {
            return Err(serde::de::Error::custom("invalid item stack count"));
        }
        Ok(Self { item, count })
    }
}
/// Validated game save representation; runtime invariants stay private.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurvivalState {
    slots: Bounded<Option<Stack>, INVENTORY_SLOTS>,
    selected: usize,
    health: u8,
    fall_peak: Option<f64>,
    pickups: Bounded<PickupState, MAX_PICKUPS>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PickupState {
    position: [f64; 3],
    stack: Stack,
}
impl Survival {
    /// Owned bounded inventory/health/fall/pickup snapshot, without transient UI.
    pub fn snapshot(&self) -> SurvivalState {
        SurvivalState {
            slots: Bounded(self.inventory.slots.to_vec()),
            selected: self.selected,
            health: self.health.value,
            fall_peak: self.health.peak,
            pickups: Bounded(
                self.pickups
                    .iter()
                    .map(|p| PickupState {
                        position: p.position.to_array(),
                        stack: p.stack,
                    })
                    .collect(),
            ),
        }
    }
}
impl SurvivalState {
    /// Reject invalid health, slot counts, selected indices and pickup coordinates
    /// before constructing live state. Fall history survives an airborne reload.
    pub fn restore(&self) -> Result<Survival, crate::persistence::Error> {
        let invalid = || crate::persistence::Error::Invalid("invalid survival state".into());
        if self.slots.0.len() != INVENTORY_SLOTS
            || self.selected >= HOTBAR_SLOTS
            || self.health > MAX_HEALTH
            || self.fall_peak.is_some_and(|p| {
                !p.is_finite() || p < f64::from(i32::MIN) || p > f64::from(i32::MAX) + 1.0
            })
            || self.pickups.0.len() > MAX_PICKUPS
        {
            return Err(invalid());
        }
        let mut result = Survival {
            inventory: Inventory::default(),
            selected: self.selected,
            health: Health {
                value: self.health,
                peak: self.fall_peak,
            },
            pickups: Vec::with_capacity(MAX_PICKUPS),
        };
        result.inventory.slots.copy_from_slice(&self.slots.0);
        for p in &self.pickups.0 {
            let position = DVec3::from_array(p.position);
            if !position.is_finite()
                || position.min_element() < f64::from(i32::MIN)
                || position.max_element() > f64::from(i32::MAX) + 1.0
            {
                return Err(invalid());
            }
            result.pickups.push(Pickup {
                position,
                stack: p.stack,
            });
        }
        Ok(result)
    }
}
