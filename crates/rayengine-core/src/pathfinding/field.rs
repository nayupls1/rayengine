use super::{
    NavGrid, Neighborhood, PathError, cell_count, cell_of, check_bounds, checked_cost, corner_open,
    index, offset, step_length,
};
use glam::UVec2;
use std::{cmp::Ordering, collections::BinaryHeap};

#[derive(Clone, Copy, Debug)]
struct Open {
    distance: f32,
    index: u32,
}

impl Ord for Open {
    // Reversed for a min-heap: nearest first, then lowest cell index.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
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

/// Cost from every cell to the nearest of one or more goals (a Dijkstra map).
///
/// One computation serves any number of agents chasing the same target:
/// each agent calls [`next_step`](Self::next_step) for its cell. Distances use
/// the same costs and corner rules as [`PathFinder`](super::PathFinder), so a
/// chain of next steps follows an optimal path. Unlike `PathFinder`, which
/// lets an agent leave a blocked start cell, blocked cells have no distance
/// and no next step. Buffers are reused; computing
/// again for a grid no larger than before does not allocate, apart from the
/// queue growing past its previous peak.
///
/// ```
/// use rayengine_core::glam::UVec2;
/// use rayengine_core::pathfinding::{CostGrid, DistanceField, Neighborhood};
///
/// let mut grid = CostGrid::new(UVec2::new(3, 3), 1.0);
/// grid.block_rect(UVec2::new(1, 0), UVec2::new(1, 1));
/// let mut field = DistanceField::new();
/// field.compute(&grid, [UVec2::new(2, 0)], Neighborhood::Four).unwrap();
/// assert_eq!(field.distance(UVec2::new(0, 0)), Some(6.0));
/// assert_eq!(field.next_step(&grid, UVec2::new(0, 0)), Some(UVec2::new(0, 1)));
/// assert_eq!(field.distance(UVec2::new(1, 0)), None);
/// ```
#[derive(Clone, Debug, Default)]
pub struct DistanceField {
    size: UVec2,
    neighborhood: Neighborhood,
    distances: Vec<f32>,
    open: BinaryHeap<Open>,
}

impl DistanceField {
    /// Creates an empty field with no buffers allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// Recomputes distances to the nearest walkable goal.
    ///
    /// Blocked goals are ignored; with no walkable goals every cell is
    /// unreachable. Costs must be finite and positive. On error the field is
    /// left empty, as if computed for a zero-sized grid.
    pub fn compute<G: NavGrid + ?Sized>(
        &mut self,
        grid: &G,
        goals: impl IntoIterator<Item = UVec2>,
        neighborhood: Neighborhood,
    ) -> Result<(), PathError> {
        let result = self.fill(grid, goals, neighborhood);
        if result.is_err() {
            self.size = UVec2::ZERO;
            self.distances.clear();
        }
        self.open.clear();
        result
    }

    fn fill<G: NavGrid + ?Sized>(
        &mut self,
        grid: &G,
        goals: impl IntoIterator<Item = UVec2>,
        neighborhood: Neighborhood,
    ) -> Result<(), PathError> {
        let size = grid.size();
        let count = cell_count(size)?;
        self.size = size;
        self.neighborhood = neighborhood;
        self.distances.clear();
        self.distances.resize(count, f32::INFINITY);
        self.open.clear();
        for goal in goals {
            check_bounds(size, goal)?;
            let goal_index = index(size, goal);
            if checked_cost(grid, goal, 0.0)?.is_some() && self.distances[goal_index] != 0.0 {
                self.distances[goal_index] = 0.0;
                self.open.push(Open {
                    distance: 0.0,
                    index: goal_index as u32,
                });
            }
        }
        let rule = neighborhood.corner_rule();
        while let Some(open) = self.open.pop() {
            let current = open.index as usize;
            if open.distance > self.distances[current] {
                continue;
            }
            let cell = cell_of(size, current);
            // Neighbors pay this cell's cost to step into it.
            let Some(cost) = checked_cost(grid, cell, 0.0)? else {
                continue;
            };
            for &step in neighborhood.steps() {
                let Some(from) = offset(size, cell, step) else {
                    continue;
                };
                // Corner rules are symmetric, so check the move toward `cell`.
                if !corner_open(grid, from, -step, rule) || !grid.walkable(from) {
                    continue;
                }
                let from_index = index(size, from);
                let distance = open.distance + cost * step_length(step);
                if distance < self.distances[from_index] {
                    self.distances[from_index] = distance;
                    self.open.push(Open {
                        distance,
                        index: from_index as u32,
                    });
                }
            }
        }
        Ok(())
    }

    /// Size of the grid the field was computed for.
    pub fn size(&self) -> UVec2 {
        self.size
    }

    /// Cost from `cell` to the nearest goal, or `None` when the cell is
    /// blocked, unreachable, or outside the computed grid.
    pub fn distance(&self, cell: UVec2) -> Option<f32> {
        if !cell.cmplt(self.size).all() {
            return None;
        }
        let distance = self.distances[index(self.size, cell)];
        distance.is_finite().then_some(distance)
    }

    /// The neighbor to move to from `cell` along an optimal path, or `None`
    /// at a goal, from an unreachable cell, or for a grid of another size.
    ///
    /// Pass the grid the field was computed from. Exact ties prefer
    /// orthogonal moves in the order +x, +y, -x, -y.
    pub fn next_step<G: NavGrid + ?Sized>(&self, grid: &G, cell: UVec2) -> Option<UVec2> {
        if grid.size() != self.size {
            return None;
        }
        let here = self.distance(cell)?;
        let rule = self.neighborhood.corner_rule();
        let mut best: Option<(f32, UVec2)> = None;
        for &step in self.neighborhood.steps() {
            let Some(next) = offset(self.size, cell, step) else {
                continue;
            };
            let (Some(distance), Some(cost)) = (self.distance(next), grid.cost(next)) else {
                continue;
            };
            if !corner_open(grid, cell, step, rule) {
                continue;
            }
            let total = distance + cost * step_length(step);
            if best.is_none_or(|(best, _)| total < best) {
                best = Some((total, next));
            }
        }
        // Only step when it makes progress, so goals and stale grids stop.
        best.filter(|&(_, next)| self.distances[index(self.size, next)] < here)
            .map(|(_, next)| next)
    }
}
