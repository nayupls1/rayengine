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
    nodes: Vec<Node>,
    open: BinaryHeap<Open>,
    stamp: u32,
    expanded: u64,
    search: Option<Search>,
}

impl PathFinder {
    /// Creates a finder with no buffers allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates buffers for grids up to `size` ahead of the first search.
    pub fn reserve(&mut self, size: UVec2) -> Result<(), PathError> {
        let count = cell_count(size)?;
        if self.nodes.len() < count {
            self.nodes.resize(count, Node::default());
        }
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
        self.expanded = 0;
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

        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            // Stamps wrapped: old nodes could look current, so forget them all.
            self.nodes.fill(Node::default());
            self.stamp = 1;
        }
        self.open.clear();
        let start_index = index(size, start);
        self.nodes[start_index] = Node {
            stamp: self.stamp,
            closed: false,
            g: 0.0,
            parent: u32::MAX,
        };
        self.push(start_index as u32, 0.0, heuristic(start, goal, options));
        self.search = Some(Search {
            size,
            start: start_index as u32,
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
        self.expanded
    }

    /// Buffer capacities and addresses, to check reuse without allocation.
    #[cfg(test)]
    pub(super) fn buffers(&self) -> (usize, *const u8, usize) {
        (
            self.nodes.capacity(),
            self.nodes.as_ptr().cast(),
            self.open.capacity(),
        )
    }

    #[cfg(test)]
    pub(super) fn set_stamp(&mut self, stamp: u32) {
        self.stamp = stamp;
    }

    fn push(&mut self, index: u32, g: f32, h: f32) {
        self.open.push(Open { f: g + h, g, index });
    }

    fn run<G: NavGrid + ?Sized>(
        &mut self,
        grid: &G,
        mut budget: Option<u32>,
        path: &mut Vec<UVec2>,
    ) -> Result<PathStatus, PathError> {
        let Some(search) = self.search else {
            return Err(PathError::NoSearch);
        };
        let Search {
            size,
            goal,
            options,
            ..
        } = search;
        let goal_index = index(size, goal) as u32;
        let rule = options.neighborhood.corner_rule();
        loop {
            if budget == Some(0) {
                return Ok(PathStatus::Pending);
            }
            let Some(open) = self.open.pop() else {
                self.search = None;
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
            if open.index == goal_index {
                self.reconstruct(size, search.start, goal_index, path);
                self.search = None;
                return Ok(PathStatus::Found { cost: open.g });
            }
            let cell = cell_of(size, open.index as usize);
            for &step in options.neighborhood.steps() {
                let Some(next) = offset(size, cell, step) else {
                    continue;
                };
                let next_index = index(size, next);
                let known = self.nodes[next_index];
                let current = known.stamp == self.stamp;
                if current && known.closed {
                    continue;
                }
                if !corner_open(grid, cell, step, rule) {
                    continue;
                }
                let cost = match checked_cost(grid, next, options.min_cost) {
                    Ok(Some(cost)) => cost,
                    Ok(None) => continue,
                    Err(error) => {
                        self.search = None;
                        return Err(error);
                    }
                };
                let g = open.g + cost * step_length(step);
                if !current || g < known.g {
                    self.nodes[next_index] = Node {
                        stamp: self.stamp,
                        closed: false,
                        g,
                        parent: open.index,
                    };
                    self.push(next_index as u32, g, heuristic(next, goal, &options));
                }
            }
        }
    }

    fn reconstruct(&self, size: UVec2, start: u32, goal: u32, path: &mut Vec<UVec2>) {
        let mut current = goal;
        loop {
            path.push(cell_of(size, current as usize));
            if current == start {
                break;
            }
            current = self.nodes[current as usize].parent;
        }
        path.reverse();
    }
}

/// Lower bound of the remaining cost: Manhattan or octile distance scaled by
/// the cheapest possible cell.
fn heuristic(cell: UVec2, goal: UVec2, options: &PathOptions) -> f32 {
    let delta = (cell.as_ivec2() - goal.as_ivec2()).abs().as_vec2();
    let distance = match options.neighborhood {
        Neighborhood::Four => delta.x + delta.y,
        Neighborhood::Eight(_) => {
            delta.max_element() + (std::f32::consts::SQRT_2 - 1.0) * delta.min_element()
        }
    };
    distance * options.min_cost
}
