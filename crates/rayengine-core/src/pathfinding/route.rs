use super::astar::{Core, Graph, Relax, grid_moves, heuristic};
use super::{
    NavGrid, Neighborhood, PathError, PathOptions, PathStatus, cell_count, cell_of, checked_cost,
    corner_open, index,
};
use glam::{UVec2, Vec2};
use std::collections::HashMap;

/// A cell on one layer (a floor or region) of a layered navigation map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NavPoint {
    /// Index into the layer slice passed to searches.
    pub layer: u32,
    /// Cell on that layer.
    pub cell: UVec2,
}

impl NavPoint {
    /// Creates a point.
    pub const fn new(layer: u32, cell: UVec2) -> Self {
        Self { layer, cell }
    }
}

/// Handle to a link in a [`NavTopology`]. Removing a link invalidates its
/// handle, even if the slot is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LinkId {
    index: u32,
    generation: u32,
}

/// A caller-defined connection between two points, such as a door, stairs,
/// a ladder or an elevator.
///
/// Searches may step from `from` to `to` (and back when `two_way`) for
/// `cost`, which replaces the grid step cost. Disabled links are ignored.
/// The engine never interprets `tag`; route steps name the link they use, so
/// game code can play door or stair effects and look the tag up.
#[derive(Clone, Debug, PartialEq)]
pub struct NavLink<T> {
    /// Entry point.
    pub from: NavPoint,
    /// Exit point. It must be walkable for the searching agent.
    pub to: NavPoint,
    /// Traversal cost, finite and not negative.
    pub cost: f32,
    /// Whether the link can also be traversed from `to` to `from`.
    pub two_way: bool,
    /// Whether searches may use the link.
    pub enabled: bool,
    /// Game-owned traversal data.
    pub tag: T,
}

impl<T> NavLink<T> {
    /// An enabled two-way link.
    pub fn new(from: NavPoint, to: NavPoint, cost: f32, tag: T) -> Self {
        Self {
            from,
            to,
            cost,
            two_way: true,
            enabled: true,
            tag,
        }
    }

    /// Restricts the link to `from` → `to`.
    pub fn one_way(mut self) -> Self {
        self.two_way = false;
        self
    }
}

#[derive(Clone, Debug)]
struct Slot<T> {
    generation: u32,
    link: Option<NavLink<T>>,
}

/// Caller-owned links between layers and a revision counting every
/// navigation edit.
///
/// Every change made through this type advances [`revision`](Self::revision).
/// The grids themselves belong to the game, so call
/// [`mark_changed`](Self::mark_changed) after editing their cells (a door
/// cell closing, furniture placed). [`NavFinder`] rejects resuming a search
/// across a revision change, and [`Route::revalidate`] rechecks active routes
/// only when the revision moved.
///
/// ```
/// use rayengine_core::glam::UVec2;
/// use rayengine_core::pathfinding::{NavLink, NavPoint, NavTopology};
///
/// let mut topology = NavTopology::new();
/// let stairs = topology.add(NavLink::new(
///     NavPoint::new(0, UVec2::new(4, 1)),
///     NavPoint::new(1, UVec2::new(4, 1)),
///     3.0,
///     "stairs",
/// ));
/// let revision = topology.revision();
/// topology.set_enabled(stairs, false);
/// assert!(topology.revision() > revision);
/// assert_eq!(topology.get(stairs).map(|link| link.tag), Some("stairs"));
/// ```
#[derive(Clone, Debug)]
pub struct NavTopology<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
    // Slot indices leaving each point, sorted; `true` traverses a two-way
    // link backwards.
    outgoing: HashMap<NavPoint, Vec<(u32, bool)>>,
    revision: u64,
}

impl<T> Default for NavTopology<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
            outgoing: HashMap::new(),
            revision: 0,
        }
    }
}

impl<T> NavTopology<T> {
    /// Creates a topology without links at revision zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Edit counter; it changes whenever links change or
    /// [`mark_changed`](Self::mark_changed) is called.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Records an edit made outside this type, such as a grid cell becoming
    /// blocked or walkable, so pending searches and routes are rechecked.
    pub fn mark_changed(&mut self) {
        self.revision += 1;
    }

    /// Adds a link. Panics unless its cost is finite and not negative.
    pub fn add(&mut self, link: NavLink<T>) -> LinkId {
        assert!(
            link.cost.is_finite() && link.cost >= 0.0,
            "invalid link cost"
        );
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    link: None,
                });
                (self.slots.len() - 1) as u32
            }
        };
        self.connect(index, link.from, false);
        if link.two_way && link.from != link.to {
            self.connect(index, link.to, true);
        }
        let slot = &mut self.slots[index as usize];
        slot.link = Some(link);
        self.len += 1;
        self.revision += 1;
        LinkId {
            index,
            generation: slot.generation,
        }
    }

    /// Removes a link, returning it, or `None` for a stale handle.
    pub fn remove(&mut self, id: LinkId) -> Option<NavLink<T>> {
        self.get(id)?;
        let slot = &mut self.slots[id.index as usize];
        let link = slot.link.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        self.disconnect(id.index, link.from, false);
        self.disconnect(id.index, link.to, true);
        self.len -= 1;
        self.revision += 1;
        Some(link)
    }

    /// The link behind `id`, or `None` for a stale handle.
    pub fn get(&self, id: LinkId) -> Option<&NavLink<T>> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.link.as_ref())
            .flatten()
    }

    /// Mutable access to a link's tag; tags do not affect searches.
    pub fn tag_mut(&mut self, id: LinkId) -> Option<&mut T> {
        self.get(id)?;
        self.slots[id.index as usize]
            .link
            .as_mut()
            .map(|link| &mut link.tag)
    }

    /// Enables or disables a link, for example as a door opens or closes.
    /// Returns `false` for a stale handle. The revision changes only when the
    /// state does.
    pub fn set_enabled(&mut self, id: LinkId, enabled: bool) -> bool {
        self.edit(id, |link| {
            std::mem::replace(&mut link.enabled, enabled) != enabled
        })
    }

    /// Changes a link's traversal cost. Returns `false` for a stale handle.
    /// Panics unless `cost` is finite and not negative.
    pub fn set_cost(&mut self, id: LinkId, cost: f32) -> bool {
        assert!(cost.is_finite() && cost >= 0.0, "invalid link cost");
        self.edit(id, |link| std::mem::replace(&mut link.cost, cost) != cost)
    }

    /// Live links in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (LinkId, &NavLink<T>)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let id = LinkId {
                index: index as u32,
                generation: slot.generation,
            };
            slot.link.as_ref().map(|link| (id, link))
        })
    }

    /// Number of live links.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are no links.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Links leaving `point` as `(slot, backwards)`, in slot order.
    pub(super) fn outgoing(&self, point: NavPoint) -> &[(u32, bool)] {
        self.outgoing.get(&point).map_or(&[], Vec::as_slice)
    }

    /// The live link in `slot` with its current handle.
    pub(super) fn slot(&self, slot: u32) -> (LinkId, &NavLink<T>) {
        let slot_ref = &self.slots[slot as usize];
        let link = slot_ref.link.as_ref().expect("outgoing links are live");
        let id = LinkId {
            index: slot,
            generation: slot_ref.generation,
        };
        (id, link)
    }

    fn edit(&mut self, id: LinkId, change: impl FnOnce(&mut NavLink<T>) -> bool) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        let link = self.slots[id.index as usize]
            .link
            .as_mut()
            .expect("checked above");
        if change(link) {
            self.revision += 1;
        }
        true
    }

    fn connect(&mut self, slot: u32, point: NavPoint, backwards: bool) {
        let entries = self.outgoing.entry(point).or_default();
        if let Err(at) = entries.binary_search(&(slot, backwards)) {
            entries.insert(at, (slot, backwards));
        }
    }

    fn disconnect(&mut self, slot: u32, point: NavPoint, backwards: bool) {
        if let Some(entries) = self.outgoing.get_mut(&point) {
            entries.retain(|&entry| entry != (slot, backwards));
            if entries.is_empty() {
                self.outgoing.remove(&point);
            }
        }
    }
}

/// A view of a grid for a body wider than a cell.
///
/// A cell stays walkable only when a box with the body's half size, centered
/// on the cell, overlaps nothing but walkable cells inside the grid. Costs are
/// the center cell's. Half sizes below half a cell change nothing, matching
/// [`GridLayout::clearance`](super::GridLayout::clearance) values that
/// [`smooth_path`](super::smooth_path) accepts. Each query checks up to
/// `(2r + 1)²` cells for a reach of `r` cells.
///
/// ```
/// use rayengine_core::glam::{UVec2, Vec2};
/// use rayengine_core::pathfinding::{ClearanceGrid, CostGrid, NavGrid};
///
/// let mut grid = CostGrid::new(UVec2::new(5, 5), 1.0);
/// grid.set(UVec2::new(3, 2), None);
/// let wide = ClearanceGrid::new(&grid, Vec2::splat(0.9));
/// assert!(wide.walkable(UVec2::new(1, 2)));
/// assert!(!wide.walkable(UVec2::new(2, 2)));
/// assert!(!wide.walkable(UVec2::new(0, 2)), "the grid edge also blocks");
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ClearanceGrid<G> {
    grid: G,
    reach: UVec2,
}

impl<G: NavGrid> ClearanceGrid<G> {
    /// Wraps `grid` for a body with `half_size` in cell units. Panics unless
    /// both components are finite and not negative.
    pub fn new(grid: G, half_size: Vec2) -> Self {
        assert!(
            half_size.is_finite() && half_size.min_element() >= 0.0,
            "invalid clearance"
        );
        Self {
            grid,
            reach: reach(half_size),
        }
    }

    /// Cells the body overlaps beyond its center on each axis.
    pub fn reach(&self) -> UVec2 {
        self.reach
    }

    fn fits(&self, cell: UVec2) -> bool {
        let (size, reach) = (self.grid.size(), self.reach);
        if reach == UVec2::ZERO {
            return true;
        }
        let low = cell.as_i64vec2() - reach.as_i64vec2();
        let high = cell.as_i64vec2() + reach.as_i64vec2();
        if low.min_element() < 0 || high.x >= i64::from(size.x) || high.y >= i64::from(size.y) {
            return false;
        }
        (low.y..=high.y).all(|y| {
            (low.x..=high.x).all(|x| {
                let other = UVec2::new(x as u32, y as u32);
                other == cell || self.grid.walkable(other)
            })
        })
    }
}

impl<G: NavGrid> NavGrid for ClearanceGrid<G> {
    fn size(&self) -> UVec2 {
        self.grid.size()
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        let cost = self.grid.cost(cell)?;
        self.fits(cell).then_some(cost)
    }

    fn walkable(&self, cell: UVec2) -> bool {
        self.grid.walkable(cell) && self.fits(cell)
    }
}

/// Cells a box with `half_size`, centered on a cell, overlaps past it.
fn reach(half_size: Vec2) -> UVec2 {
    // Saturates for huge sizes, which then fit nowhere.
    (half_size - 0.5).ceil().max(Vec2::ZERO).as_uvec2()
}

/// Layered route search settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavOptions {
    /// Allowed grid moves and diagonal corner rule on every layer.
    pub neighborhood: Neighborhood,
    /// Lowest cost any cell can have; see [`PathOptions::min_cost`].
    pub min_cost: f32,
    /// The agent's half size in cell units, applied to every cell of the
    /// route as in [`ClearanceGrid`]. Default: zero, a point agent.
    pub clearance: Vec2,
    /// Maximum nodes expanded per call before returning
    /// [`PathStatus::Pending`]; `None` searches to completion. Must not be zero.
    pub budget: Option<u32>,
}

impl Default for NavOptions {
    fn default() -> Self {
        let path = PathOptions::default();
        Self {
            neighborhood: path.neighborhood,
            min_cost: path.min_cost,
            clearance: Vec2::ZERO,
            budget: path.budget,
        }
    }
}

impl NavOptions {
    fn path_options(&self) -> PathOptions {
        PathOptions {
            neighborhood: self.neighborhood,
            min_cost: self.min_cost,
            budget: self.budget,
        }
    }

    fn validate(&self) -> Result<(), PathError> {
        self.path_options().validate()?;
        if self.clearance.is_finite() && self.clearance.min_element() >= 0.0 {
            Ok(())
        } else {
            Err(PathError::InvalidOptions)
        }
    }
}

/// One step of a [`Route`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NavStep {
    /// Where the agent is after this step.
    pub point: NavPoint,
    /// The link traversed to arrive here, or `None` for a grid move (and the
    /// first step).
    pub link: Option<LinkId>,
}

/// A route across layers, from [`NavFinder::find_route`].
///
/// It remembers the topology revision it was planned or last validated at,
/// so callers recheck it only after edits; see [`revalidate`](Self::revalidate).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Route {
    steps: Vec<NavStep>,
    cost: f32,
    revision: u64,
}

impl Route {
    /// Creates an empty route.
    pub fn new() -> Self {
        Self::default()
    }

    /// Steps from start to goal inclusive; empty unless a route was found.
    pub fn steps(&self) -> &[NavStep] {
        &self.steps
    }

    /// Whether the route has no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Total cost when found.
    pub fn cost(&self) -> f32 {
        self.cost
    }

    /// Topology revision the route was found or last validated at.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether no navigation edit was recorded since the route was found or
    /// last validated.
    pub fn is_current<T>(&self, topology: &NavTopology<T>) -> bool {
        self.revision == topology.revision()
    }

    /// Empties the route, keeping its capacity.
    pub fn clear(&mut self) {
        self.steps.clear();
        self.cost = 0.0;
    }

    /// Whether the steps from `from` on can still be walked, after edits.
    ///
    /// Returns `true` immediately while the route [`is_current`](Self::is_current).
    /// Otherwise every remaining step is checked with `options`' neighborhood
    /// and clearance: grid moves must enter walkable cells and pass corner
    /// rules, and links must still exist, be enabled and lead to a walkable
    /// cell. A valid route is marked current, so later calls are free until
    /// the next edit. The step at `from` itself, where the agent stands, is
    /// not checked. Costs are not compared: a valid route may no longer be
    /// the cheapest one. An empty route, such as after an `Unreachable`
    /// search, or a `from` past the last step is never valid: plan again.
    pub fn revalidate<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        from: usize,
        options: &NavOptions,
    ) -> bool {
        if from >= self.steps.len() {
            return false;
        }
        if self.is_current(topology) {
            return true;
        }
        let reach = reach(options.clearance);
        let rule = options.neighborhood.corner_rule();
        let valid = self.steps.get(from..).is_some_and(|steps| {
            steps.windows(2).all(|pair| {
                let (here, next) = (pair[0].point, pair[1].point);
                let Some(grid) = layer_view(layers, next, reach) else {
                    return false;
                };
                if !grid.walkable(next.cell) {
                    return false;
                }
                match pair[1].link {
                    Some(id) => topology.get(id).is_some_and(|link| {
                        link.enabled
                            && ((link.from == here && link.to == next)
                                || (link.two_way && link.to == here && link.from == next))
                    }),
                    None => {
                        let step = next.cell.as_ivec2() - here.cell.as_ivec2();
                        here.layer == next.layer
                            && options.neighborhood.steps().contains(&step)
                            && corner_open(&grid, here.cell, step, rule)
                    }
                }
            })
        });
        if valid {
            self.revision = topology.revision();
        }
        valid
    }

    pub(super) fn steps_mut(&mut self) -> &mut Vec<NavStep> {
        &mut self.steps
    }

    pub(super) fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }
}

/// The clearance view of `point`'s layer, or `None` when the point is
/// outside every layer.
fn layer_view<G: NavGrid>(
    layers: &[G],
    point: NavPoint,
    reach: UVec2,
) -> Option<ClearanceGrid<&G>> {
    let grid = layers.get(point.layer as usize)?;
    point
        .cell
        .cmplt(grid.size())
        .all()
        .then_some(ClearanceGrid { grid, reach })
}

/// Blocks cells that `avoid` rejects, such as cells other agents hold.
struct Avoiding<'a, G, B> {
    grid: G,
    layer: u32,
    avoid: &'a B,
}

impl<G: NavGrid, B: Fn(NavPoint) -> bool> NavGrid for Avoiding<'_, G, B> {
    fn size(&self) -> UVec2 {
        self.grid.size()
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        if (self.avoid)(NavPoint::new(self.layer, cell)) {
            None
        } else {
            self.grid.cost(cell)
        }
    }

    fn walkable(&self, cell: UVec2) -> bool {
        !(self.avoid)(NavPoint::new(self.layer, cell)) && self.grid.walkable(cell)
    }
}

const NO_LINK: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct NavSearch {
    start: u32,
    goal: NavPoint,
    options: NavOptions,
    revision: u64,
    // Lower bound of any route that enters the goal layer through a link.
    link_floor: f32,
}

/// Reusable A* across several grid layers joined by [`NavTopology`] links.
///
/// Each layer is a [`NavGrid`] borrowed for the call, such as one per floor.
/// Routes step between neighboring cells on a layer and through enabled
/// links, with [`NavOptions::clearance`] applied to every cell entered.
/// Buffers grow to the total cell count and are reused. Identical layers,
/// links and options give identical routes.
///
/// ```
/// use rayengine_core::glam::UVec2;
/// use rayengine_core::pathfinding::{
///     CostGrid, NavFinder, NavLink, NavOptions, NavPoint, NavTopology, PathStatus, Route,
/// };
///
/// let floors = [CostGrid::new(UVec2::new(6, 3), 1.0), CostGrid::new(UVec2::new(6, 3), 1.0)];
/// let mut topology = NavTopology::new();
/// let stairs = topology.add(NavLink::new(
///     NavPoint::new(0, UVec2::new(5, 1)),
///     NavPoint::new(1, UVec2::new(5, 1)),
///     4.0,
///     "stairs",
/// ));
/// let mut finder = NavFinder::new();
/// let mut route = Route::new();
/// let (start, goal) = (NavPoint::new(0, UVec2::new(0, 1)), NavPoint::new(1, UVec2::new(0, 1)));
/// let status = finder
///     .find_route(&floors, &topology, start, goal, &NavOptions::default(), &mut route)
///     .unwrap();
/// assert_eq!(status, PathStatus::Found { cost: 14.0 });
/// assert!(route.steps().iter().any(|step| step.link == Some(stairs)));
/// ```
#[derive(Clone, Debug, Default)]
pub struct NavFinder {
    core: Core,
    sizes: Vec<UVec2>,
    // Node index of each layer's first cell, plus the total.
    offsets: Vec<u32>,
    via: Vec<u32>,
    search: Option<NavSearch>,
}

impl NavFinder {
    /// Creates a finder with no buffers allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// Searches from `start` to `goal`, replacing `route` with the result.
    ///
    /// Behaves like [`PathFinder::find_path`](super::PathFinder::find_path):
    /// any pending search is cancelled, the start is never checked, a goal the
    /// agent does not fit in is unreachable, and `start == goal` is found with
    /// cost zero. Errors with [`PathError::InvalidPoint`] for a start, goal or
    /// reached link endpoint outside the layers, and as `find_path` otherwise;
    /// errors cancel the search.
    pub fn find_route<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        start: NavPoint,
        goal: NavPoint,
        options: &NavOptions,
        route: &mut Route,
    ) -> Result<PathStatus, PathError> {
        self.find_route_avoiding(layers, topology, start, goal, options, route, &|_| false)
    }

    /// [`find_route`](Self::find_route) that also treats points `avoid`
    /// accepts as blocked, for detours around other agents.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn find_route_avoiding<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        start: NavPoint,
        goal: NavPoint,
        options: &NavOptions,
        route: &mut Route,
        avoid: &impl Fn(NavPoint) -> bool,
    ) -> Result<PathStatus, PathError> {
        self.search = None;
        self.core.expanded = 0;
        route.clear();
        route.revision = topology.revision();
        options.validate()?;
        self.layout(layers)?;
        let reach = reach(options.clearance);
        for point in [start, goal] {
            layer_view(layers, point, reach).ok_or(PathError::InvalidPoint(point))?;
        }
        if start == goal {
            route.steps.push(NavStep {
                point: start,
                link: None,
            });
            return Ok(PathStatus::Found { cost: 0.0 });
        }
        let goal_grid = Avoiding {
            grid: layer_view(layers, goal, reach).expect("checked above"),
            layer: goal.layer,
            avoid,
        };
        if checked_cost(&goal_grid, goal.cell, options.min_cost)?.is_none() {
            return Ok(PathStatus::Unreachable);
        }

        let path_options = options.path_options();
        let link_floor = topology
            .iter()
            .filter(|(_, link)| link.enabled)
            .flat_map(|(_, link)| {
                let back = link.two_way.then_some(link.from);
                [Some(link.to), back].map(|exit| exit.map(|exit| (exit, link.cost)))
            })
            .flatten()
            .filter(|(exit, _)| exit.layer == goal.layer)
            .map(|(exit, cost)| cost + heuristic(exit.cell, goal.cell, &path_options))
            .fold(f32::INFINITY, f32::min);
        let search = NavSearch {
            start: self.node(start),
            goal,
            options: *options,
            revision: topology.revision(),
            link_floor,
        };
        self.core.begin(search.start, estimate(&search, start));
        self.search = Some(search);
        self.run(layers, topology, options.budget, route, avoid)
    }

    /// Continues a search that returned [`PathStatus::Pending`], expanding at
    /// most `budget` nodes (`None` for no limit).
    ///
    /// Errors with [`PathError::NoSearch`] when nothing is pending,
    /// [`PathError::InvalidOptions`] for a zero budget, and cancels the
    /// search with [`PathError::TopologyChanged`] when the topology revision
    /// moved or [`PathError::GridResized`] when the layers changed size;
    /// start a new search then.
    pub fn resume<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        budget: Option<u32>,
        route: &mut Route,
    ) -> Result<PathStatus, PathError> {
        self.resume_avoiding(layers, topology, budget, route, &|_| false)
    }

    pub(super) fn resume_avoiding<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        budget: Option<u32>,
        route: &mut Route,
        avoid: &impl Fn(NavPoint) -> bool,
    ) -> Result<PathStatus, PathError> {
        route.clear();
        let search = self.search.ok_or(PathError::NoSearch)?;
        if budget == Some(0) {
            return Err(PathError::InvalidOptions);
        }
        if search.revision != topology.revision() {
            self.search = None;
            return Err(PathError::TopologyChanged);
        }
        let same_layers = layers.len() == self.sizes.len()
            && layers
                .iter()
                .zip(&self.sizes)
                .all(|(layer, &size)| layer.size() == size);
        if !same_layers {
            self.search = None;
            return Err(PathError::GridResized);
        }
        self.run(layers, topology, budget, route, avoid)
    }

    /// Whether a budgeted search is waiting for [`resume`](Self::resume).
    pub fn is_pending(&self) -> bool {
        self.search.is_some()
    }

    /// Drops a pending search. Buffers keep their capacity.
    pub fn cancel(&mut self) {
        self.search = None;
    }

    /// Nodes expanded by the current or most recent search, across resumes.
    pub fn expanded(&self) -> u64 {
        self.core.expanded
    }

    /// Records layer sizes and node offsets and grows the buffers.
    fn layout<G: NavGrid>(&mut self, layers: &[G]) -> Result<(), PathError> {
        self.sizes.clear();
        self.offsets.clear();
        let mut total = 0_usize;
        for layer in layers {
            let size = layer.size();
            self.sizes.push(size);
            self.offsets.push(total as u32);
            total = cell_count(size)
                .ok()
                .and_then(|count| total.checked_add(count))
                .filter(|&total| total < u32::MAX as usize)
                .ok_or(PathError::GridTooLarge(size))?;
        }
        self.offsets.push(total as u32);
        self.core.reserve(total);
        if self.via.len() < total {
            self.via.resize(total, NO_LINK);
        }
        Ok(())
    }

    fn node(&self, point: NavPoint) -> u32 {
        let layer = point.layer as usize;
        self.offsets[layer] + index(self.sizes[layer], point.cell) as u32
    }

    fn run<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        budget: Option<u32>,
        route: &mut Route,
        avoid: &impl Fn(NavPoint) -> bool,
    ) -> Result<PathStatus, PathError> {
        let Some(search) = self.search else {
            return Err(PathError::NoSearch);
        };
        let goal = self.node(search.goal);
        let mut graph = LayeredGraph {
            layers,
            topology,
            sizes: &self.sizes,
            offsets: &self.offsets,
            via: &mut self.via,
            reach: reach(search.options.clearance),
            options: search.options.path_options(),
            search,
            avoid,
        };
        let status = self.core.run(&mut graph, goal, budget);
        if !matches!(status, Ok(PathStatus::Pending)) {
            self.search = None;
        }
        if let Ok(PathStatus::Found { cost }) = status {
            self.core.trace(search.start, goal, |node| {
                let layer = self.offsets.partition_point(|&offset| offset <= node) - 1;
                let cell = cell_of(self.sizes[layer], (node - self.offsets[layer]) as usize);
                let link = (node != search.start && self.via[node as usize] != NO_LINK)
                    .then(|| topology.slot(self.via[node as usize]).0);
                route.steps.push(NavStep {
                    point: NavPoint::new(layer as u32, cell),
                    link,
                });
            });
            route.steps.reverse();
            route.cost = cost;
            route.revision = search.revision;
        }
        status
    }
}

/// Admissible and consistent: on the goal layer a route either stays on it
/// (at least the grid distance) or last arrives through a link (at least
/// `link_floor`); elsewhere it must arrive through a link.
fn estimate(search: &NavSearch, point: NavPoint) -> f32 {
    if point.layer == search.goal.layer {
        heuristic(point.cell, search.goal.cell, &search.options.path_options())
            .min(search.link_floor)
    } else {
        search.link_floor
    }
}

struct LayeredGraph<'a, G, T, B> {
    layers: &'a [G],
    topology: &'a NavTopology<T>,
    sizes: &'a [UVec2],
    offsets: &'a [u32],
    via: &'a mut [u32],
    reach: UVec2,
    options: PathOptions,
    search: NavSearch,
    avoid: &'a B,
}

/// The view of `layer` a search moves through.
fn view<'a, G: NavGrid, B>(
    layers: &'a [G],
    layer: u32,
    reach: UVec2,
    avoid: &'a B,
) -> Avoiding<'a, ClearanceGrid<&'a G>, B> {
    Avoiding {
        grid: ClearanceGrid {
            grid: &layers[layer as usize],
            reach,
        },
        layer,
        avoid,
    }
}

impl<G: NavGrid, T, B: Fn(NavPoint) -> bool> Graph for LayeredGraph<'_, G, T, B> {
    fn expand(&mut self, node: u32, relax: &mut Relax<'_>) -> Result<(), PathError> {
        let Self {
            layers,
            topology,
            sizes,
            offsets,
            via,
            reach,
            options,
            search,
            avoid,
        } = self;
        let layer = offsets.partition_point(|&offset| offset <= node) - 1;
        let base = offsets[layer];
        let here = NavPoint::new(layer as u32, cell_of(sizes[layer], (node - base) as usize));
        grid_moves(
            &view(layers, here.layer, *reach, *avoid),
            here.cell,
            base,
            options,
            relax,
            |cell| estimate(search, NavPoint::new(here.layer, cell)),
            |next| via[next as usize] = NO_LINK,
        )?;
        for &(slot, backwards) in topology.outgoing(here) {
            let (_, link) = topology.slot(slot);
            if !link.enabled {
                continue;
            }
            let exit = if backwards { link.from } else { link.to };
            let Some(&size) = sizes.get(exit.layer as usize) else {
                return Err(PathError::InvalidPoint(exit));
            };
            if !exit.cell.cmplt(size).all() {
                return Err(PathError::InvalidPoint(exit));
            }
            let next = offsets[exit.layer as usize] + index(size, exit.cell) as u32;
            if relax.closed(next) {
                continue;
            }
            let grid = view(layers, exit.layer, *reach, *avoid);
            if checked_cost(&grid, exit.cell, options.min_cost)?.is_none() {
                continue;
            }
            if relax.offer(next, link.cost, estimate(search, exit)) {
                via[next as usize] = slot;
            }
        }
        Ok(())
    }
}
