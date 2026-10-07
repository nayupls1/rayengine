use super::{
    NavGrid, Neighborhood, PathError, PathOptions, cell_count, cell_of, check_bounds, checked_cost,
    corner_open, index, offset, step_length,
};
use glam::UVec2;
use std::{cmp::Ordering, collections::BinaryHeap};

/// Result of an A* call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathStatus {
    /// The output path holds the start, every cell stepped through, and the
    /// goal. `cost` is the summed cost of entering each cell after the start.
    Found {
        /// Total path cost.
        cost: f32,
    },
    /// No path exists; the output path is empty.
    Unreachable,
    /// The expansion budget ran out. Call [`PathFinder::resume`] with the same
    /// grid to continue; the output path is empty until the search finishes.
    Pending,
}

#[derive(Clone, Copy, Debug, Default)]
struct Node {
    // Nodes from earlier searches have older stamps and count as unvisited.
    stamp: u32,
    closed: bool,
    g: f32,
    parent: u32,
}

#[derive(Clone, Copy, Debug)]
struct Open {
    f: f32,
    g: f32,
    index: u32,
}

impl Ord for Open {
    // Reversed for a min-heap: lowest estimate, then nearest to the goal
    // (largest cost so far), then lowest cell index.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| self.g.total_cmp(&other.g))
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Open {}

#[derive(Clone, Copy, Debug)]
struct Search {
    size: UVec2,
    start: u32,
    goal: UVec2,
    options: PathOptions,
}

/// Reusable A* search state for grids up to the largest size seen.
///
/// Buffers grow to the grid's cell count on first use and are reused; later
/// searches on grids no larger than that do not allocate, apart from the open
/// queue growing past its previous peak. Paths are written into a
/// caller-owned vector so its capacity is reused too.
///
/// ```
/// use rayengine_core::glam::UVec2;
/// use rayengine_core::pathfinding::{CostGrid, PathFinder, PathOptions, PathStatus};
///
/// let mut grid = CostGrid::new(UVec2::new(5, 3), 1.0);
/// grid.block_rect(UVec2::new(2, 0), UVec2::new(2, 1));
/// let mut finder = PathFinder::new();
/// let mut path = Vec::new();
/// let status = finder
///     .find_path(&grid, UVec2::new(0, 0), UVec2::new(4, 0), &PathOptions::default(), &mut path)
///     .unwrap();
/// assert!(matches!(status, PathStatus::Found { .. }));
/// assert_eq!(path.first(), Some(&UVec2::new(0, 0)));
/// assert_eq!(path.last(), Some(&UVec2::new(4, 0)));
/// assert!(path.contains(&UVec2::new(2, 2)));
/// ```
#[derive(Clone, Debug, Default)]
pub struct PathFinder {
    core: Core,
    search: Option<Search>,
}

impl PathFinder {
    /// Creates a finder with no buffers allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates buffers for grids up to `size` ahead of the first search.
    pub fn reserve(&mut self, size: UVec2) -> Result<(), PathError> {
        self.core.reserve(cell_count(size)?);
        Ok(())
    }

    /// Searches from `start` to `goal`, replacing `path` with the result.
    ///
    /// Any pending search is cancelled. The start cell is never checked for
    /// walkability, so an agent standing in a blocked cell can still leave it.
    /// A blocked goal is unreachable, except that `start == goal` is always
    /// found with cost zero. With [`PathOptions::budget`] the search
    /// may return [`PathStatus::Pending`]; continue it with
    /// [`resume`](Self::resume).
    ///
    /// Errors for invalid options, out-of-bounds cells, oversized grids, or an
    /// invalid cell cost met during the search; errors cancel the search.
    pub fn find_path<G: NavGrid + ?Sized>(
        &mut self,
        grid: &G,
        start: UVec2,
        goal: UVec2,
        options: &PathOptions,
        path: &mut Vec<UVec2>,
    ) -> Result<PathStatus, PathError> {
        self.search = None;
        self.core.expanded = 0;
        path.clear();
        options.validate()?;
        let size = grid.size();
        self.reserve(size)?;
        check_bounds(size, start)?;
        check_bounds(size, goal)?;
        if start == goal {
            path.push(start);
            return Ok(PathStatus::Found { cost: 0.0 });
        }
        if checked_cost(grid, goal, options.min_cost)?.is_none() {
            return Ok(PathStatus::Unreachable);
        }

        let start_index = index(size, start) as u32;
        self.core
            .begin(start_index, heuristic(start, goal, options));
        self.search = Some(Search {
            size,
            start: start_index,
            goal,
            options: *options,
        });
        self.run(grid, options.budget, path)
    }

    /// Continues a search that returned [`PathStatus::Pending`], expanding at
    /// most `budget` cells (`None` for no limit).
    ///
    /// Pass the same, unchanged grid. Errors with [`PathError::NoSearch`] when
    /// nothing is pending, [`PathError::GridResized`] (keeping the search
    /// pending) when the grid size changed, or [`PathError::InvalidOptions`]
    /// for a zero budget.
    pub fn resume<G: NavGrid + ?Sized>(
        &mut self,
        grid: &G,
        budget: Option<u32>,
        path: &mut Vec<UVec2>,
    ) -> Result<PathStatus, PathError> {
        path.clear();
        let search = self.search.ok_or(PathError::NoSearch)?;
        if budget == Some(0) {
            return Err(PathError::InvalidOptions);
        }
        if grid.size() != search.size {
            return Err(PathError::GridResized);
        }
        self.run(grid, budget, path)
    }

    /// Whether a budgeted search is waiting for [`resume`](Self::resume).
    pub fn is_pending(&self) -> bool {
        self.search.is_some()
    }

    /// Drops a pending search. Buffers keep their capacity.
    pub fn cancel(&mut self) {
        self.search = None;
    }

    /// Cells expanded by the current or most recent search, across resumes.
    pub fn expanded(&self) -> u64 {
        self.core.expanded
    }

    /// Buffer capacities and addresses, to check reuse without allocation.
    #[cfg(test)]
    pub(super) fn buffers(&self) -> (usize, *const u8, usize) {
        self.core.buffers()
    }

    #[cfg(test)]
    pub(super) fn set_stamp(&mut self, stamp: u32) {
        self.core.stamp = stamp;
    }

    fn run<G: NavGrid + ?Sized>(
        &mut self,
        grid: &G,
        budget: Option<u32>,
        path: &mut Vec<UVec2>,
    ) -> Result<PathStatus, PathError> {
        let Some(search) = self.search else {
            return Err(PathError::NoSearch);
        };
        let goal = index(search.size, search.goal) as u32;
        let mut graph = GridGraph {
            grid,
            size: search.size,
            goal: search.goal,
            options: search.options,
        };
        let status = self.core.run(&mut graph, goal, budget);
        if !matches!(status, Ok(PathStatus::Pending)) {
            self.search = None;
        }
        if let Ok(PathStatus::Found { .. }) = status {
            self.core.trace(search.start, goal, |node| {
                path.push(cell_of(search.size, node as usize))
            });
            path.reverse();
        }
        status
    }
}

/// One grid's moves for the shared search core.
struct GridGraph<'a, G: ?Sized> {
    grid: &'a G,
    size: UVec2,
    goal: UVec2,
    options: PathOptions,
}

impl<G: NavGrid + ?Sized> Graph for GridGraph<'_, G> {
    fn expand(&mut self, node: u32, relax: &mut Relax<'_>) -> Result<(), PathError> {
        let cell = cell_of(self.size, node as usize);
        grid_moves(
            self.grid,
            cell,
            0,
            &self.options,
            relax,
            |next| heuristic(next, self.goal, &self.options),
            |_| {},
        )
    }
}

/// Offers every legal grid move out of `cell` to `relax`, calling `improved`
/// with each node whose best route it became. Node indices are the cell index
/// plus `base`, so several grids can share one search.
pub(super) fn grid_moves<G: NavGrid + ?Sized>(
    grid: &G,
    cell: UVec2,
    base: u32,
    options: &PathOptions,
    relax: &mut Relax<'_>,
    heuristic: impl Fn(UVec2) -> f32,
    mut improved: impl FnMut(u32),
) -> Result<(), PathError> {
    let size = grid.size();
    let rule = options.neighborhood.corner_rule();
    for &step in options.neighborhood.steps() {
        let Some(next) = offset(size, cell, step) else {
            continue;
        };
        let node = base + index(size, next) as u32;
        if relax.closed(node) || !corner_open(grid, cell, step, rule) {
            continue;
        }
        let Some(cost) = checked_cost(grid, next, options.min_cost)? else {
            continue;
        };
        if relax.offer(node, cost * step_length(step), heuristic(next)) {
            improved(node);
        }
    }
    Ok(())
}

/// A graph searched by [`Core`]: nodes are dense indices.
pub(super) trait Graph {
    /// Offers each move out of `node` through [`Relax::offer`].
    fn expand(&mut self, node: u32, relax: &mut Relax<'_>) -> Result<(), PathError>;
}

/// Resumable A* over dense node indices, shared by grid and layered searches.
#[derive(Clone, Debug, Default)]
pub(super) struct Core {
    nodes: Vec<Node>,
    open: BinaryHeap<Open>,
    stamp: u32,
    pub(super) expanded: u64,
}

impl Core {
    pub(super) fn reserve(&mut self, count: usize) {
        if self.nodes.len() < count {
            self.nodes.resize(count, Node::default());
        }
    }

    /// Starts a search at `start`, forgetting earlier searches. Call
    /// [`reserve`](Self::reserve) for the node count first.
    pub(super) fn begin(&mut self, start: u32, heuristic: f32) {
        self.expanded = 0;
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            // Stamps wrapped: old nodes could look current, so forget them all.
            self.nodes.fill(Node::default());
            self.stamp = 1;
        }
        self.open.clear();
        self.nodes[start as usize] = Node {
            stamp: self.stamp,
            closed: false,
            g: 0.0,
            parent: u32::MAX,
        };
        self.open.push(Open {
            f: heuristic,
            g: 0.0,
            index: start,
        });
    }

    /// Expands at most `budget` nodes. `Found` leaves the path for
    /// [`trace`](Self::trace).
    pub(super) fn run(
        &mut self,
        graph: &mut impl Graph,
        goal: u32,
        mut budget: Option<u32>,
    ) -> Result<PathStatus, PathError> {
        loop {
            if budget == Some(0) {
                return Ok(PathStatus::Pending);
            }
            let Some(open) = self.open.pop() else {
                return Ok(PathStatus::Unreachable);
            };
            let node = &mut self.nodes[open.index as usize];
            // Skip queue entries superseded by a cheaper route.
            if node.closed || open.g > node.g {
                continue;
            }
            node.closed = true;
            self.expanded += 1;
            if let Some(remaining) = &mut budget {
                *remaining -= 1;
            }
            if open.index == goal {
                return Ok(PathStatus::Found { cost: open.g });
            }
            graph.expand(
                open.index,
                &mut Relax {
                    nodes: &mut self.nodes,
                    open: &mut self.open,
                    stamp: self.stamp,
                    from: open.index,
                    g: open.g,
                },
            )?;
        }
    }

    /// Visits the found path's nodes from `goal` back to `start`.
    pub(super) fn trace(&self, start: u32, goal: u32, mut visit: impl FnMut(u32)) {
        let mut current = goal;
        loop {
            visit(current);
            if current == start {
                break;
            }
            current = self.nodes[current as usize].parent;
        }
    }

    #[cfg(test)]
    fn buffers(&self) -> (usize, *const u8, usize) {
        (
            self.nodes.capacity(),
            self.nodes.as_ptr().cast(),
            self.open.capacity(),
        )
    }
}

/// Records moves out of the node being expanded.
pub(super) struct Relax<'a> {
    nodes: &'a mut [Node],
    open: &'a mut BinaryHeap<Open>,
    stamp: u32,
    from: u32,
    g: f32,
}

impl Relax<'_> {
    /// Whether `node` is already final; skip computing its cost.
    pub(super) fn closed(&self, node: u32) -> bool {
        let known = self.nodes[node as usize];
        known.stamp == self.stamp && known.closed
    }

    /// Offers a move to `node` costing `cost`, with `heuristic` the remaining
    /// lower bound. Returns whether it became the node's best route.
    pub(super) fn offer(&mut self, node: u32, cost: f32, heuristic: f32) -> bool {
        let known = self.nodes[node as usize];
        let current = known.stamp == self.stamp;
        let g = self.g + cost;
        if current && (known.closed || g >= known.g) {
            return false;
        }
        self.nodes[node as usize] = Node {
            stamp: self.stamp,
            closed: false,
            g,
            parent: self.from,
        };
        self.open.push(Open {
            f: g + heuristic,
            g,
            index: node,
        });
        true
    }
}

/// Lower bound of the remaining cost: Manhattan or octile distance scaled by
/// the cheapest possible cell.
pub(super) fn heuristic(cell: UVec2, goal: UVec2, options: &PathOptions) -> f32 {
    let delta = (cell.as_ivec2() - goal.as_ivec2()).abs().as_vec2();
    let distance = match options.neighborhood {
        Neighborhood::Four => delta.x + delta.y,
        Neighborhood::Eight(_) => {
            delta.max_element() + (std::f32::consts::SQRT_2 - 1.0) * delta.min_element()
        }
    };
    distance * options.min_cost
}
