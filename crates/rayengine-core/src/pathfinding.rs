//! Grid navigation: A* paths, line-of-sight smoothing, path following and
//! distance fields over caller-owned 2D grids, plus layered routes joined by
//! links ([`NavFinder`], [`NavTopology`]) and cell-by-cell [`Traffic`] for
//! small groups of agents.
//!
//! Searches read cells through [`NavGrid`], so tilemaps, voxel layers and game
//! arrays can be searched in place without copying. [`PathFinder`] and
//! [`DistanceField`] keep their buffers between queries: once their capacity
//! covers the grid, queries do not allocate. [`GridLayout`] maps cells to world
//! positions for [`PathFollower`] and collision geometry.
//!
//! Moving into a cell costs that cell's [`NavGrid::cost`] multiplied by the
//! step length: `1` orthogonally and `√2` diagonally. The start cell's cost is
//! never charged. Results are deterministic for the same grid and inputs.

mod astar;
mod field;
mod follow;
mod route;
mod smooth;
mod traffic;

pub use astar::{PathFinder, PathStatus};
pub use field::DistanceField;
pub use follow::PathFollower;
pub use route::{
    ClearanceGrid, LinkId, NavFinder, NavLink, NavOptions, NavPoint, NavStep, NavTopology, Route,
};
pub use smooth::{line_of_sight, smooth_path};
pub use traffic::{AgentId, AgentState, Traffic, TrafficEvent, TrafficOptions};

use crate::collision::Aabb2;
use glam::{IVec2, UVec2, Vec2};
use std::fmt;

/// Read-only access to a bounded navigation grid.
///
/// Implement this for an existing tilemap, voxel slice or array so searches
/// borrow its cells instead of copying them. Cell `(x, y)` exists for
/// `x < size().x` and `y < size().y`.
pub trait NavGrid {
    /// Width and height in cells.
    fn size(&self) -> UVec2;

    /// Cost of entering `cell`, or `None` when it is blocked.
    ///
    /// Only called for cells inside [`size`](Self::size). Searches reject a
    /// cost that is not finite or is below [`PathOptions::min_cost`] (or not
    /// positive for distance fields and smoothing) with
    /// [`PathError::InvalidCost`].
    fn cost(&self, cell: UVec2) -> Option<f32>;

    /// Whether `cell` can be entered, for checks that need no cost: corner
    /// rules, line of sight and clearance, and which cells get a distance
    /// field entry. Overrides are only a faster path and must return exactly
    /// `cost(cell).is_some()`, the default.
    fn walkable(&self, cell: UVec2) -> bool {
        self.cost(cell).is_some()
    }
}

impl<G: NavGrid + ?Sized> NavGrid for &G {
    fn size(&self) -> UVec2 {
        (**self).size()
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        (**self).cost(cell)
    }

    fn walkable(&self, cell: UVec2) -> bool {
        (**self).walkable(cell)
    }
}

/// Adapts a size and cost callback to [`NavGrid`] without storing cells.
///
/// ```
/// use rayengine_core::glam::UVec2;
/// use rayengine_core::pathfinding::{GridFn, NavGrid};
///
/// let walls = [[false, true], [false, false]];
/// let grid = GridFn::new(UVec2::new(2, 2), |cell: UVec2| {
///     (!walls[cell.y as usize][cell.x as usize]).then_some(1.0)
/// });
/// assert!(!grid.walkable(UVec2::new(1, 0)));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct GridFn<F> {
    size: UVec2,
    cost: F,
}

impl<F: Fn(UVec2) -> Option<f32>> GridFn<F> {
    /// Wraps `cost`, which is called only for cells inside `size`.
    pub fn new(size: UVec2, cost: F) -> Self {
        Self { size, cost }
    }
}

impl<F: Fn(UVec2) -> Option<f32>> NavGrid for GridFn<F> {
    fn size(&self) -> UVec2 {
        self.size
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        (self.cost)(cell)
    }
}

/// Owned row-major grid of per-cell costs.
#[derive(Clone, Debug, PartialEq)]
pub struct CostGrid {
    size: UVec2,
    // `f32::INFINITY` marks a blocked cell.
    costs: Vec<f32>,
}

impl CostGrid {
    /// Creates a grid where every cell costs `cost`.
    /// Panics if `cost` is not finite and positive or the grid is too large.
    pub fn new(size: UVec2, cost: f32) -> Self {
        assert!(cost.is_finite() && cost > 0.0, "invalid cell cost");
        let count = cell_count(size).expect("grid is too large");
        Self {
            size,
            costs: vec![cost; count],
        }
    }

    /// Sets a cell's cost, or blocks it with `None`.
    /// Panics if `cell` is outside the grid or `cost` is not finite and positive.
    pub fn set(&mut self, cell: UVec2, cost: Option<f32>) {
        assert!(cell.cmplt(self.size).all(), "cell is outside the grid");
        let cost = match cost {
            Some(cost) => {
                assert!(cost.is_finite() && cost > 0.0, "invalid cell cost");
                cost
            }
            None => f32::INFINITY,
        };
        self.costs[index(self.size, cell)] = cost;
    }

    /// Blocks every cell in the inclusive rectangle `min..=max`, clamped to the grid.
    pub fn block_rect(&mut self, min: UVec2, max: UVec2) {
        if self.size.cmpeq(UVec2::ZERO).any() {
            return;
        }
        let max = max.min(self.size - 1);
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                self.set(UVec2::new(x, y), None);
            }
        }
    }
}

impl NavGrid for CostGrid {
    fn size(&self) -> UVec2 {
        self.size
    }

    fn cost(&self, cell: UVec2) -> Option<f32> {
        let cost = self.costs[index(self.size, cell)];
        cost.is_finite().then_some(cost)
    }
}

/// Rule for diagonal moves past the two orthogonally adjacent cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DiagonalRule {
    /// Diagonals may pass between two blocked cells.
    Always,
    /// At least one orthogonally adjacent cell must be walkable.
    IfEitherOpen,
    /// Both orthogonally adjacent cells must be walkable: no corner cutting.
    #[default]
    IfBothOpen,
}

/// Cells reachable in one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Neighborhood {
    /// Orthogonal moves only.
    Four,
    /// Orthogonal and diagonal moves, with diagonals limited by a rule.
    Eight(DiagonalRule),
}

impl Default for Neighborhood {
    fn default() -> Self {
        Self::Eight(DiagonalRule::default())
    }
}

impl Neighborhood {
    /// Rule for line segments passing exactly through a cell corner. Four-way
    /// movement never cuts corners.
    pub(crate) fn corner_rule(self) -> DiagonalRule {
        match self {
            Self::Four => DiagonalRule::IfBothOpen,
            Self::Eight(rule) => rule,
        }
    }

    pub(crate) fn steps(self) -> &'static [IVec2] {
        match self {
            Self::Four => &STEPS[..4],
            Self::Eight(_) => &STEPS,
        }
    }
}

/// A* search settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathOptions {
    /// Allowed moves and diagonal corner rule.
    pub neighborhood: Neighborhood,
    /// Lowest cost any cell can have; it scales the distance heuristic so
    /// paths stay optimal. Must be finite and positive. Default: `1.0`.
    pub min_cost: f32,
    /// Maximum cells expanded per call before returning
    /// [`PathStatus::Pending`]; `None` searches to completion. Must not be zero.
    pub budget: Option<u32>,
}

impl Default for PathOptions {
    fn default() -> Self {
        Self {
            neighborhood: Neighborhood::default(),
            min_cost: 1.0,
            budget: None,
        }
    }
}

impl PathOptions {
    pub(crate) fn validate(&self) -> Result<(), PathError> {
        let budget_ok = self.budget != Some(0);
        if self.min_cost.is_finite() && self.min_cost > 0.0 && budget_ok {
            Ok(())
        } else {
            Err(PathError::InvalidOptions)
        }
    }
}

/// Invalid navigation input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathError {
    /// A start, goal or query cell is outside the grid.
    OutOfBounds(UVec2),
    /// A grid reported a cost that is nonfinite or below the allowed minimum.
    InvalidCost {
        /// Cell with the invalid cost.
        cell: UVec2,
        /// The reported cost.
        cost: f32,
    },
    /// `min_cost` is not finite and positive, or `budget` is zero.
    InvalidOptions,
    /// The grid has more cells than a search can index.
    GridTooLarge(UVec2),
    /// [`PathFinder::resume`] was called without a pending search.
    NoSearch,
    /// The grid size changed while a search was pending.
    GridResized,
    /// A route start, goal or link endpoint lies outside the layers.
    InvalidPoint(NavPoint),
    /// The [`NavTopology`] revision changed while a search was pending.
    TopologyChanged,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds(cell) => write!(f, "cell {cell} is outside the grid"),
            Self::InvalidCost { cell, cost } => write!(f, "cell {cell} has invalid cost {cost}"),
            Self::InvalidOptions => write!(f, "path options are invalid"),
            Self::GridTooLarge(size) => write!(f, "grid {size} has too many cells"),
            Self::NoSearch => write!(f, "no path search is pending"),
            Self::GridResized => write!(f, "grid size changed during a pending search"),
            Self::InvalidPoint(point) => {
                write!(f, "cell {} is outside layer {}", point.cell, point.layer)
            }
            Self::TopologyChanged => write!(f, "navigation changed during a pending search"),
        }
    }
}

impl std::error::Error for PathError {}

/// Maps grid cells to world-space cells.
///
/// Cell `(x, y)` covers `origin + (x, y) * cell_size` to one cell further along
/// both axes. In 3D, use the vector's `y` as world `z`. Searches measure
/// distance in cells, so with non-square cells a path is shortest in cell
/// steps rather than in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLayout {
    /// World position of cell `(0, 0)`'s minimum corner.
    pub origin: Vec2,
    /// Width and height of one cell in world units. Must be finite and positive.
    pub cell_size: Vec2,
}

impl GridLayout {
    /// Creates a layout. Panics for a nonfinite origin or a nonpositive or
    /// nonfinite cell size.
    pub fn new(origin: Vec2, cell_size: Vec2) -> Self {
        assert!(origin.is_finite() && cell_size.is_finite() && cell_size.min_element() > 0.0);
        Self { origin, cell_size }
    }

    /// World position of a cell's center.
    pub fn cell_center(&self, cell: UVec2) -> Vec2 {
        self.origin + (cell.as_vec2() + 0.5) * self.cell_size
    }

    /// World bounds of a cell, for example as a static collider. Neighboring
    /// cells share exact edges.
    pub fn cell_bounds(&self, cell: UVec2) -> Aabb2 {
        Aabb2 {
            min: self.origin + cell.as_vec2() * self.cell_size,
            max: self.origin + (cell + 1).as_vec2() * self.cell_size,
        }
    }

    /// A body's clearance in cell units, for
    /// [`smooth_path`] and [`line_of_sight`]: its half size divided by the
    /// cell size, such as `layout.clearance(body.half_size)`.
    pub fn clearance(&self, half_size: Vec2) -> Vec2 {
        half_size / self.cell_size
    }

    /// Cell containing `point` in a grid of `size`, or `None` outside it.
    /// Points on a shared edge belong to the cell with the larger coordinate.
    pub fn cell_at(&self, point: Vec2, size: UVec2) -> Option<UVec2> {
        let estimate = ((point - self.origin) / self.cell_size).floor();
        let mut cell = UVec2::ZERO;
        for axis in 0..2 {
            // Compare against the same edges as `cell_bounds`, correcting
            // rounding in the division.
            let edge = |i: u32| self.origin[axis] + i as f32 * self.cell_size[axis];
            let count = size[axis];
            if !point[axis].is_finite() || point[axis] < edge(0) || point[axis] >= edge(count) {
                return None;
            }
            let mut i = estimate[axis].clamp(0.0, (count - 1) as f32) as u32;
            while i > 0 && point[axis] < edge(i) {
                i -= 1;
            }
            while i + 1 < count && point[axis] >= edge(i + 1) {
                i += 1;
            }
            cell[axis] = i;
        }
        Some(cell)
    }
}

/// Orthogonal steps first, then diagonals; this order breaks exact ties.
const STEPS: [IVec2; 8] = [
    IVec2::new(1, 0),
    IVec2::new(0, 1),
    IVec2::new(-1, 0),
    IVec2::new(0, -1),
    IVec2::new(1, 1),
    IVec2::new(-1, 1),
    IVec2::new(-1, -1),
    IVec2::new(1, -1),
];

pub(crate) fn cell_count(size: UVec2) -> Result<usize, PathError> {
    // Leave `u32::MAX` free as a sentinel and keep indices in `u32`.
    let count = u64::from(size.x) * u64::from(size.y);
    if count < u64::from(u32::MAX) {
        Ok(count as usize)
    } else {
        Err(PathError::GridTooLarge(size))
    }
}

pub(crate) fn index(size: UVec2, cell: UVec2) -> usize {
    cell.y as usize * size.x as usize + cell.x as usize
}

pub(crate) fn cell_of(size: UVec2, index: usize) -> UVec2 {
    let width = size.x as usize;
    UVec2::new((index % width) as u32, (index / width) as u32)
}

pub(crate) fn offset(size: UVec2, cell: UVec2, step: IVec2) -> Option<UVec2> {
    let next = cell.as_ivec2() + step;
    (next.cmpge(IVec2::ZERO).all() && next.as_uvec2().cmplt(size).all()).then(|| next.as_uvec2())
}

pub(crate) fn step_length(step: IVec2) -> f32 {
    if step.x != 0 && step.y != 0 {
        std::f32::consts::SQRT_2
    } else {
        1.0
    }
}

/// Whether a move from `cell` along `step` passes the corner rule. Both side
/// cells are inside the grid whenever the destination is.
pub(crate) fn corner_open<G: NavGrid + ?Sized>(
    grid: &G,
    cell: UVec2,
    step: IVec2,
    rule: DiagonalRule,
) -> bool {
    if step.x == 0 || step.y == 0 || rule == DiagonalRule::Always {
        return true;
    }
    let side = |offset_step| {
        offset(grid.size(), cell, offset_step).is_some_and(|side| grid.walkable(side))
    };
    let (a, b) = (side(IVec2::new(step.x, 0)), side(IVec2::new(0, step.y)));
    match rule {
        DiagonalRule::Always => true,
        DiagonalRule::IfEitherOpen => a || b,
        DiagonalRule::IfBothOpen => a && b,
    }
}

/// Reads a cost, rejecting values that are nonfinite or below `min_cost`.
/// A zero `min_cost` still requires positive costs.
pub(crate) fn checked_cost<G: NavGrid + ?Sized>(
    grid: &G,
    cell: UVec2,
    min_cost: f32,
) -> Result<Option<f32>, PathError> {
    match grid.cost(cell) {
        Some(cost) if !(cost.is_finite() && cost > 0.0 && cost >= min_cost) => {
            Err(PathError::InvalidCost { cell, cost })
        }
        cost => Ok(cost),
    }
}

pub(crate) fn check_bounds(size: UVec2, cell: UVec2) -> Result<(), PathError> {
    if cell.cmplt(size).all() {
        Ok(())
    } else {
        Err(PathError::OutOfBounds(cell))
    }
}

#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod tests;
