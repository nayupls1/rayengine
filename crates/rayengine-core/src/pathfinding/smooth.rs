use super::{
    DiagonalRule, NavGrid, Neighborhood, PathError, STEPS, check_bounds, checked_cost, corner_open,
    offset,
};
use glam::{DVec2, IVec2, UVec2};

/// Whether a straight segment between two cell centers crosses only walkable
/// cells.
///
/// A segment through an exact cell corner must also satisfy `neighborhood`'s
/// diagonal rule; four-way movement requires both side cells to be walkable.
/// With a positive `clearance`, the segment is swept by a square of that
/// half-width in cell units (a body's half-extent divided by the cell size),
/// and every blocked cell it would overlap fails the check; zero checks a
/// point. Errors if either cell is outside the grid, or with
/// [`PathError::InvalidOptions`] unless `0 <= clearance < 0.5`.
pub fn line_of_sight<G: NavGrid + ?Sized>(
    grid: &G,
    from: UVec2,
    to: UVec2,
    neighborhood: Neighborhood,
    clearance: f32,
) -> Result<bool, PathError> {
    check_bounds(grid.size(), from)?;
    check_bounds(grid.size(), to)?;
    check_clearance(clearance)?;
    trace(
        grid,
        from,
        to,
        neighborhood.corner_rule(),
        clearance,
        |cell, _| Ok(grid.walkable(cell)),
    )
}

/// Removes waypoints that a straight segment can skip, in place.
///
/// Keeps the first and last cells. A waypoint is skipped when the segment
/// between its neighbors has [`line_of_sight`] with `clearance` and costs no more than the
/// cells it replaces, measured as each cell's cost times the length of the
/// segment inside it. A shortcut may still cross a costlier cell when it saves
/// more elsewhere, but it never makes the path more expensive. The path should
/// come from [`PathFinder`](super::PathFinder) with the same `neighborhood`.
/// Steps of a stale path that now cross blocked cells are kept unless a clear
/// shortcut replaces them; smoothing never adds a blocked segment.
///
/// Pass the following body's half-extent in cell units as `clearance`, so
/// shortcuts keep it off wall corners; with zero, segments are checked as
/// lines between cell centers and a wide body can catch on a corner. Errors
/// leave the path unchanged for cells outside the grid or an invalid
/// `clearance` (see [`line_of_sight`]), but may leave it partly simplified
/// for a cost that is not finite and positive.
pub fn smooth_path<G: NavGrid + ?Sized>(
    grid: &G,
    path: &mut Vec<UVec2>,
    neighborhood: Neighborhood,
    clearance: f32,
) -> Result<(), PathError> {
    for &cell in path.iter() {
        check_bounds(grid.size(), cell)?;
    }
    check_clearance(clearance)?;
    if path.len() < 3 {
        return Ok(());
    }
    let rule = neighborhood.corner_rule();
    // Original path segments may cut corners the rule forbids for shortcuts.
    let along = |from, to| segment_cost(grid, from, to, DiagonalRule::Always, 0.0);
    let mut kept = 1;
    let mut anchor = path[0];
    let mut original = 0.0;
    for next in 1..path.len() {
        // A blocked step (a stale path) makes any clear shortcut an
        // improvement; without one, its endpoints are kept as they are.
        original += along(path[next - 1], path[next])?.unwrap_or(f32::INFINITY);
        if next == 1 {
            continue;
        }
        let direct = segment_cost(grid, anchor, path[next], rule, clearance)?;
        if direct.is_some_and(|direct| direct <= original * (1.0 + 1e-5)) {
            continue;
        }
        anchor = path[next - 1];
        path[kept] = anchor;
        kept += 1;
        original = along(anchor, path[next])?.unwrap_or(f32::INFINITY);
    }
    path[kept] = path[path.len() - 1];
    path.truncate(kept + 1);
    Ok(())
}

fn check_clearance(clearance: f32) -> Result<(), PathError> {
    if (0.0..0.5).contains(&clearance) {
        Ok(())
    } else {
        Err(PathError::InvalidOptions)
    }
}

/// Cost-weighted length of a segment between cell centers, or `None` when it
/// crosses a blocked cell, a corner the rule forbids, or comes within
/// `clearance` of a blocked cell.
fn segment_cost<G: NavGrid + ?Sized>(
    grid: &G,
    from: UVec2,
    to: UVec2,
    rule: DiagonalRule,
    clearance: f32,
) -> Result<Option<f32>, PathError> {
    let mut total = 0.0;
    let clear = trace(grid, from, to, rule, clearance, |cell, length| {
        Ok(match checked_cost(grid, cell, 0.0)? {
            Some(cost) => {
                total += cost * length;
                true
            }
            None => false,
        })
    })?;
    Ok(clear.then_some(total))
}

/// Visits each cell a center-to-center segment passes through with the
/// segment length inside it, in cell units. Stops early when `visit` returns
/// `false`, a corner crossing fails `rule`, or a square of half-width
/// `clearance` swept along the segment overlaps a blocked cell.
fn trace<G: NavGrid + ?Sized>(
    grid: &G,
    from: UVec2,
    to: UVec2,
    rule: DiagonalRule,
    clearance: f32,
    mut visit: impl FnMut(UVec2, f32) -> Result<bool, PathError>,
) -> Result<bool, PathError> {
    // With clearance below half a cell, any blocked cell the swept square
    // overlaps neighbors a traversed cell.
    let (start, end) = (from.as_dvec2() + 0.5, to.as_dvec2() + 0.5);
    let crowded = |cell: UVec2| {
        clearance > 0.0
            && STEPS.iter().any(|&step| {
                offset(grid.size(), cell, step).is_some_and(|side| {
                    !grid.walkable(side)
                        && sweeps(start, end, side.as_dvec2(), f64::from(clearance))
                })
            })
    };
    let delta = to.as_ivec2() - from.as_ivec2();
    let (dx, dy) = (delta.x.unsigned_abs(), delta.y.unsigned_abs());
    let step = delta.signum();
    let length = delta.as_vec2().length();
    // Centers sit at half-cell offsets, so the k-th x boundary is crossed at
    // t = (2k + 1) / (2 dx). Compare crossings exactly with integers.
    let (mut crossed_x, mut crossed_y) = (0_u64, 0_u64);
    let mut cell = from;
    let mut entered = 0.0_f32;
    loop {
        let next_x =
            (dx > 0 && crossed_x < u64::from(dx)).then(|| (2 * crossed_x + 1) * u64::from(dy));
        let next_y =
            (dy > 0 && crossed_y < u64::from(dy)).then(|| (2 * crossed_y + 1) * u64::from(dx));
        let (move_x, move_y) = match (next_x, next_y) {
            (None, None) => return Ok(visit(cell, (1.0 - entered) * length)? && !crowded(cell)),
            (Some(_), None) => (true, false),
            (None, Some(_)) => (false, true),
            (Some(x), Some(y)) => (x <= y, y <= x),
        };
        let exit = if move_x {
            (2 * crossed_x + 1) as f32 / (2 * dx) as f32
        } else {
            (2 * crossed_y + 1) as f32 / (2 * dy) as f32
        };
        if !visit(cell, (exit - entered) * length)? || crowded(cell) {
            return Ok(false);
        }
        entered = exit;
        let advance = IVec2::new(
            if move_x { step.x } else { 0 },
            if move_y { step.y } else { 0 },
        );
        if move_x && move_y && !corner_open(grid, cell, advance, rule) {
            return Ok(false);
        }
        crossed_x += u64::from(move_x);
        crossed_y += u64::from(move_y);
        cell = (cell.as_ivec2() + advance).as_uvec2();
    }
}

/// Whether the segment from `start` to `end` enters the open square around
/// the cell with minimum corner `min`, grown by `clearance` on every side.
fn sweeps(start: DVec2, end: DVec2, min: DVec2, clearance: f64) -> bool {
    let (low, high) = (min - clearance, min + 1.0 + clearance);
    let delta = end - start;
    let (mut enter, mut exit) = (0.0_f64, 1.0_f64);
    for axis in 0..2 {
        if delta[axis] == 0.0 {
            if start[axis] <= low[axis] || start[axis] >= high[axis] {
                return false;
            }
        } else {
            let a = (low[axis] - start[axis]) / delta[axis];
            let b = (high[axis] - start[axis]) / delta[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    enter < exit
}
