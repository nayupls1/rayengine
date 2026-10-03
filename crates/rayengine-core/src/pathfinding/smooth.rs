use super::{
    DiagonalRule, NavGrid, Neighborhood, PathError, check_bounds, checked_cost, corner_open,
};
use glam::{IVec2, UVec2};

/// Whether a straight segment between two cell centers crosses only walkable
/// cells.
///
/// A segment through an exact cell corner must also satisfy `neighborhood`'s
/// diagonal rule; four-way movement requires both side cells to be walkable.
/// Errors if either cell is outside the grid.
pub fn line_of_sight<G: NavGrid + ?Sized>(
    grid: &G,
    from: UVec2,
    to: UVec2,
    neighborhood: Neighborhood,
) -> Result<bool, PathError> {
    check_bounds(grid.size(), from)?;
    check_bounds(grid.size(), to)?;
    trace(grid, from, to, neighborhood.corner_rule(), |cell, _| {
        Ok(grid.walkable(cell))
    })
}

/// Removes waypoints that a straight segment can skip, in place.
///
/// Keeps the first and last cells. A waypoint is skipped when the segment
/// between its neighbors has [`line_of_sight`] and costs no more than the
/// cells it replaces, measured as each cell's cost times the length of the
/// segment inside it. Shortcuts therefore never cut through more expensive
/// terrain. The path should come from [`PathFinder`](super::PathFinder) with
/// the same `neighborhood`. Errors leave the path unchanged for cells outside
/// the grid, but may leave it partly simplified for a cost that is not finite
/// and positive.
///
/// Segments are checked as lines between cell centers. A body wider than a
/// point can still brush a wall corner; slide it with
/// [`Body2D::move_and_slide`](crate::collision::Body2D::move_and_slide).
pub fn smooth_path<G: NavGrid + ?Sized>(
    grid: &G,
    path: &mut Vec<UVec2>,
    neighborhood: Neighborhood,
) -> Result<(), PathError> {
    for &cell in path.iter() {
        check_bounds(grid.size(), cell)?;
    }
    if path.len() < 3 {
        return Ok(());
    }
    let rule = neighborhood.corner_rule();
    // Original path segments may cut corners the rule forbids for shortcuts.
    let along = |from, to| segment_cost(grid, from, to, DiagonalRule::Always);
    let mut kept = 1;
    let mut anchor = path[0];
    let mut original = 0.0;
    for next in 1..path.len() {
        let step = along(path[next - 1], path[next])?;
        let Some(step) = step else {
            // Not a valid grid step: any clear shortcut is an improvement.
            original = f32::INFINITY;
            continue;
        };
        original += step;
        if next == 1 {
            continue;
        }
        let direct = segment_cost(grid, anchor, path[next], rule)?;
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

/// Cost-weighted length of a segment between cell centers, or `None` when it
/// crosses a blocked cell or a corner the rule forbids.
fn segment_cost<G: NavGrid + ?Sized>(
    grid: &G,
    from: UVec2,
    to: UVec2,
    rule: DiagonalRule,
) -> Result<Option<f32>, PathError> {
    let mut total = 0.0;
    let clear = trace(grid, from, to, rule, |cell, length| {
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
/// `false` or a corner crossing fails `rule`.
fn trace<G: NavGrid + ?Sized>(
    grid: &G,
    from: UVec2,
    to: UVec2,
    rule: DiagonalRule,
    mut visit: impl FnMut(UVec2, f32) -> Result<bool, PathError>,
) -> Result<bool, PathError> {
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
            (None, None) => return visit(cell, (1.0 - entered) * length),
            (Some(_), None) => (true, false),
            (None, Some(_)) => (false, true),
            (Some(x), Some(y)) => (x <= y, y <= x),
        };
        let exit = if move_x {
            (2 * crossed_x + 1) as f32 / (2 * dx) as f32
        } else {
            (2 * crossed_y + 1) as f32 / (2 * dy) as f32
        };
        if !visit(cell, (exit - entered) * length)? {
            return Ok(false);
        }
        entered = exit;
        let offset = IVec2::new(
            if move_x { step.x } else { 0 },
            if move_y { step.y } else { 0 },
        );
        if move_x && move_y && !corner_open(grid, cell, offset, rule) {
            return Ok(false);
        }
        crossed_x += u64::from(move_x);
        crossed_y += u64::from(move_y);
        cell = (cell.as_ivec2() + offset).as_uvec2();
    }
}
