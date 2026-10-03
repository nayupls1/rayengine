use super::*;
use crate::collision::{Body2D, Body3D};
use glam::Vec3;

const OPEN_EIGHT: Neighborhood = Neighborhood::Eight(DiagonalRule::Always);
const MODES: [Neighborhood; 4] = [
    Neighborhood::Four,
    Neighborhood::Eight(DiagonalRule::Always),
    Neighborhood::Eight(DiagonalRule::IfEitherOpen),
    Neighborhood::Eight(DiagonalRule::IfBothOpen),
];

fn cell(x: u32, y: u32) -> UVec2 {
    UVec2::new(x, y)
}

/// `#` is blocked, `.` costs 1 and digits cost their value.
fn parse(rows: &[&str]) -> CostGrid {
    let mut grid = CostGrid::new(cell(rows[0].len() as u32, rows.len() as u32), 1.0);
    for (y, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), rows[0].len());
        for (x, symbol) in row.chars().enumerate() {
            let cost = match symbol {
                '#' => None,
                '.' => Some(1.0),
                digit => Some(digit.to_digit(10).unwrap() as f32),
            };
            grid.set(cell(x as u32, y as u32), cost);
        }
    }
    grid
}

fn options(neighborhood: Neighborhood) -> PathOptions {
    PathOptions {
        neighborhood,
        ..PathOptions::default()
    }
}

fn find(
    grid: &impl NavGrid,
    start: UVec2,
    goal: UVec2,
    neighborhood: Neighborhood,
) -> (PathStatus, Vec<UVec2>) {
    let mut path = Vec::new();
    let status = PathFinder::new()
        .find_path(grid, start, goal, &options(neighborhood), &mut path)
        .unwrap();
    (status, path)
}

fn cost_of(status: PathStatus) -> f32 {
    match status {
        PathStatus::Found { cost } => cost,
        other => panic!("expected a path, got {other:?}"),
    }
}

/// Checks that a path is made of legal steps and returns its summed cost.
fn validate(grid: &impl NavGrid, path: &[UVec2], neighborhood: Neighborhood) -> f32 {
    let mut total = 0.0;
    for pair in path.windows(2) {
        let step = pair[1].as_ivec2() - pair[0].as_ivec2();
        assert!(
            neighborhood.steps().contains(&step),
            "illegal step {pair:?}"
        );
        assert!(corner_open(grid, pair[0], step, neighborhood.corner_rule()));
        total += grid.cost(pair[1]).expect("path enters a blocked cell") * step_length(step);
    }
    total
}

/// Independent Bellman-Ford reference: cost from `start` to every cell.
fn reference(grid: &impl NavGrid, start: UVec2, neighborhood: Neighborhood) -> Vec<f32> {
    let size = grid.size();
    let mut distance = vec![f32::INFINITY; (size.x * size.y) as usize];
    distance[index(size, start)] = 0.0;
    let open = |x: i32, y: i32| {
        x >= 0
            && y >= 0
            && (x as u32) < size.x
            && (y as u32) < size.y
            && grid.cost(cell(x as u32, y as u32)).is_some()
    };
    let mut changed = true;
    while changed {
        changed = false;
        for y in 0..size.y as i32 {
            for x in 0..size.x as i32 {
                let here = distance[(y as u32 * size.x + x as u32) as usize];
                if !here.is_finite() {
                    continue;
                }
                for dy in -1..=1 {
                    for dx in -1_i32..=1 {
                        let diagonal = dx != 0 && dy != 0;
                        if (dx, dy) == (0, 0)
                            || (diagonal && neighborhood == Neighborhood::Four)
                            || !open(x + dx, y + dy)
                        {
                            continue;
                        }
                        if diagonal {
                            let sides = (open(x + dx, y), open(x, y + dy));
                            let allowed = match neighborhood {
                                Neighborhood::Eight(DiagonalRule::IfEitherOpen) => {
                                    sides.0 || sides.1
                                }
                                Neighborhood::Eight(DiagonalRule::IfBothOpen) => sides.0 && sides.1,
                                _ => true,
                            };
                            if !allowed {
                                continue;
                            }
                        }
                        let next = cell((x + dx) as u32, (y + dy) as u32);
                        let length = if diagonal {
                            std::f32::consts::SQRT_2
                        } else {
                            1.0
                        };
                        let total = here + grid.cost(next).unwrap() * length;
                        let slot = &mut distance[index(size, next)];
                        if total < *slot - 1e-4 {
                            *slot = total;
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    distance
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
}

fn random_grid(rng: &mut Rng, size: UVec2) -> CostGrid {
    let mut grid = CostGrid::new(size, 1.0);
    for y in 0..size.y {
        for x in 0..size.x {
            let roll = rng.next() % 10;
            grid.set(
                cell(x, y),
                match roll {
                    0..=2 => None,
                    3 => Some(4.0),
                    4 => Some(1.5),
                    _ => Some(1.0),
                },
            );
        }
    }
    grid
}

#[test]
fn open_grids_find_straight_and_octile_paths() {
    let grid = CostGrid::new(cell(6, 4), 1.0);
    let (status, path) = find(&grid, cell(0, 0), cell(5, 3), Neighborhood::Four);
    assert_eq!(cost_of(status), 8.0);
    assert_eq!(path.len(), 9);
    assert_eq!((path[0], path[8]), (cell(0, 0), cell(5, 3)));
    assert_eq!(validate(&grid, &path, Neighborhood::Four), 8.0);

    let (status, path) = find(&grid, cell(0, 0), cell(5, 3), Neighborhood::default());
    let expected = 2.0 + 3.0 * std::f32::consts::SQRT_2;
    assert!((cost_of(status) - expected).abs() < 1e-5);
    assert_eq!(path.len(), 6);
    assert!((validate(&grid, &path, Neighborhood::default()) - expected).abs() < 1e-5);

    let (status, path) = find(&grid, cell(2, 2), cell(2, 2), Neighborhood::Four);
    assert_eq!(
        (status, path),
        (PathStatus::Found { cost: 0.0 }, vec![cell(2, 2)])
    );
}

#[test]
fn unreachable_and_blocked_goals_return_empty_paths() {
    let grid = parse(&["..#..", "..#..", "..#.."]);
    let mut finder = PathFinder::new();
    let mut path = vec![cell(9, 9)];
    for mode in MODES {
        let status = finder
            .find_path(&grid, cell(0, 1), cell(4, 1), &options(mode), &mut path)
            .unwrap();
        assert_eq!(status, PathStatus::Unreachable);
        assert!(path.is_empty() && !finder.is_pending());
        // Only the left side is explored before giving up.
        assert_eq!(finder.expanded(), 6);
    }
    let status = finder
        .find_path(
            &grid,
            cell(0, 0),
            cell(2, 0),
            &PathOptions::default(),
            &mut path,
        )
        .unwrap();
    assert_eq!((status, finder.expanded()), (PathStatus::Unreachable, 0));

    // The start cell is never checked, so agents can walk out of a wall.
    let (status, path) = find(&grid, cell(2, 1), cell(3, 1), Neighborhood::Four);
    assert_eq!((cost_of(status), path), (1.0, vec![cell(2, 1), cell(3, 1)]));
}

#[test]
fn diagonal_rules_control_corner_cutting() {
    let squeeze = parse(&[".#", "#."]);
    let corner = parse(&[".#", ".."]);
    let cases = [
        (
            DiagonalRule::Always,
            Some(std::f32::consts::SQRT_2),
            Some(std::f32::consts::SQRT_2),
        ),
        (
            DiagonalRule::IfEitherOpen,
            None,
            Some(std::f32::consts::SQRT_2),
        ),
        (DiagonalRule::IfBothOpen, None, Some(2.0)),
    ];
    for (rule, through_squeeze, past_corner) in cases {
        let mode = Neighborhood::Eight(rule);
        for (grid, expected) in [(&squeeze, through_squeeze), (&corner, past_corner)] {
            let (status, path) = find(grid, cell(0, 0), cell(1, 1), mode);
            match expected {
                Some(cost) => {
                    assert_eq!(cost_of(status), cost, "{rule:?}");
                    assert_eq!(validate(grid, &path, mode), cost);
                }
                None => assert_eq!(status, PathStatus::Unreachable, "{rule:?}"),
            }
        }
    }
    let (status, _) = find(&corner, cell(0, 0), cell(1, 1), Neighborhood::Four);
    assert_eq!(cost_of(status), 2.0);
}

#[test]
fn per_cell_costs_choose_the_cheapest_route() {
    // Crossing the swamp costs 9 per cell; walking around costs 1 per cell.
    let grid = parse(&[".....", ".999.", "....."]);
    let (status, path) = find(&grid, cell(0, 1), cell(4, 1), Neighborhood::Four);
    assert_eq!(cost_of(status), 6.0);
    assert!(
        path.iter()
            .all(|cell| cell.y != 1 || cell.x == 0 || cell.x == 4)
    );

    // A short cheap-enough crossing beats a long detour.
    let grid = parse(&[".2.", ".#.", ".#.", ".#.", "..."]);
    let (status, path) = find(&grid, cell(0, 0), cell(2, 0), Neighborhood::Four);
    assert_eq!((cost_of(status), path.len()), (3.0, 3));

    // Costs below the default minimum are rejected; a lower minimum accepts them.
    let grid = parse(&["...", "..."]);
    let mut cheap = grid.clone();
    cheap.set(cell(1, 0), Some(0.5));
    let mut finder = PathFinder::new();
    let mut path = Vec::new();
    let error = finder
        .find_path(
            &cheap,
            cell(0, 0),
            cell(2, 0),
            &PathOptions::default(),
            &mut path,
        )
        .unwrap_err();
    assert_eq!(
        error,
        PathError::InvalidCost {
            cell: cell(1, 0),
            cost: 0.5
        }
    );
    assert!(!finder.is_pending() && path.is_empty());
    let relaxed = PathOptions {
        min_cost: 0.5,
        neighborhood: Neighborhood::Four,
        ..PathOptions::default()
    };
    let status = finder
        .find_path(&cheap, cell(0, 0), cell(2, 0), &relaxed, &mut path)
        .unwrap();
    assert_eq!((cost_of(status), path.len()), (1.5, 3));

    for bad in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
        let grid = GridFn::new(cell(2, 1), |c: UVec2| {
            Some(if c.x == 1 { bad } else { 1.0 })
        });
        let result = finder.find_path(
            &grid,
            cell(0, 0),
            cell(1, 0),
            &PathOptions::default(),
            &mut path,
        );
        assert!(
            matches!(result, Err(PathError::InvalidCost { .. })),
            "{bad}"
        );
    }
}

#[test]
fn equal_cost_ties_are_deterministic_across_reuse() {
    let grid = CostGrid::new(cell(9, 9), 1.0);
    for mode in MODES {
        let (status, expected) = find(&grid, cell(0, 0), cell(8, 5), mode);
        let mut finder = PathFinder::new();
        let mut path = Vec::new();
        // A larger earlier grid leaves stale nodes in the reused buffers.
        finder
            .find_path(
                &CostGrid::new(cell(20, 20), 1.0),
                cell(19, 19),
                cell(0, 0),
                &options(mode),
                &mut path,
            )
            .unwrap();
        for _ in 0..3 {
            let again = finder
                .find_path(&grid, cell(0, 0), cell(8, 5), &options(mode), &mut path)
                .unwrap();
            assert_eq!((again, &path), (status, &expected));
        }
    }
    // Orthogonal steps are preferred first, so the four-way path leads along +x.
    let (_, path) = find(&grid, cell(0, 0), cell(2, 2), Neighborhood::Four);
    assert_eq!(path[1], cell(1, 0));
}

#[test]
fn paths_match_a_reference_on_random_weighted_grids() {
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    let mut finder = PathFinder::new();
    let mut field = DistanceField::new();
    let mut path = Vec::new();
    for round in 0..40 {
        let size = cell(3 + rng.next() % 14, 3 + rng.next() % 14);
        let grid = random_grid(&mut rng, size);
        let start = cell(rng.next() % size.x, rng.next() % size.y);
        let goal = cell(rng.next() % size.x, rng.next() % size.y);
        for mode in MODES {
            let expected = reference(&grid, start, mode)[index(size, goal)];
            let status = finder
                .find_path(&grid, start, goal, &options(mode), &mut path)
                .unwrap();
            let goal_open = grid.walkable(goal) || start == goal;
            if expected.is_finite() && goal_open {
                let cost = cost_of(status);
                assert!(
                    (cost - expected).abs() < 1e-3,
                    "round {round} {mode:?}: {cost} vs {expected}"
                );
                assert!((validate(&grid, &path, mode) - cost).abs() < 1e-3);
                assert_eq!((path[0], *path.last().unwrap()), (start, goal));
            } else {
                assert_eq!(status, PathStatus::Unreachable, "round {round} {mode:?}");
            }

            // Distances to the start are the reverse problem: walk the field back.
            field.compute(&grid, [start], mode).unwrap();
            let Some(distance) = field.distance(goal) else {
                assert!(!expected.is_finite() || !grid.walkable(start) || !grid.walkable(goal));
                continue;
            };
            let mut walked = 0.0;
            let mut current = goal;
            while let Some(next) = field.next_step(&grid, current) {
                walked +=
                    grid.cost(next).unwrap() * step_length(next.as_ivec2() - current.as_ivec2());
                current = next;
            }
            assert_eq!(current, start);
            assert!((walked - distance).abs() < 1e-3, "round {round} {mode:?}");
        }
    }
}

#[test]
fn budgets_pause_and_resume_to_the_same_result() {
    let grid = parse(&[
        "..........",
        ".########.",
        ".#......#.",
        ".#.####.#.",
        ".#.#..#.#.",
        "...#..#...",
    ]);
    let goal = cell(4, 4);
    let mut finder = PathFinder::new();
    let mut path = Vec::new();
    let full = finder
        .find_path(&grid, cell(0, 5), goal, &PathOptions::default(), &mut path)
        .unwrap();
    let expected = path.clone();
    let total = finder.expanded();

    let budgeted = PathOptions {
        budget: Some(3),
        ..PathOptions::default()
    };
    let mut status = finder
        .find_path(&grid, cell(0, 5), goal, &budgeted, &mut path)
        .unwrap();
    let mut calls = 1;
    while status == PathStatus::Pending {
        assert!(finder.is_pending() && path.is_empty());
        assert!(finder.expanded() <= 3 * calls);
        status = finder.resume(&grid, Some(3), &mut path).unwrap();
        calls += 1;
    }
    assert_eq!((status, &path, finder.expanded()), (full, &expected, total));
    assert!(calls > 3);

    assert_eq!(
        finder.resume(&grid, None, &mut path),
        Err(PathError::NoSearch)
    );
    finder
        .find_path(&grid, cell(0, 5), goal, &budgeted, &mut path)
        .unwrap();
    assert_eq!(
        finder.resume(&grid, Some(0), &mut path),
        Err(PathError::InvalidOptions)
    );
    let bigger = CostGrid::new(cell(11, 6), 1.0);
    assert_eq!(
        finder.resume(&bigger, None, &mut path),
        Err(PathError::GridResized)
    );
    assert!(finder.is_pending());
    assert_eq!(finder.resume(&grid, None, &mut path), Ok(full));
    finder
        .find_path(&grid, cell(0, 5), goal, &budgeted, &mut path)
        .unwrap();
    finder.cancel();
    assert_eq!(
        finder.resume(&grid, None, &mut path),
        Err(PathError::NoSearch)
    );
}

#[test]
fn invalid_queries_are_rejected() {
    let grid = CostGrid::new(cell(4, 4), 1.0);
    let mut finder = PathFinder::new();
    let mut path = Vec::new();
    let mut search = |start, goal, options: PathOptions| {
        finder.find_path(&grid, start, goal, &options, &mut path)
    };
    let defaults = PathOptions::default();
    assert_eq!(
        search(cell(4, 0), cell(0, 0), defaults),
        Err(PathError::OutOfBounds(cell(4, 0)))
    );
    assert_eq!(
        search(cell(0, 0), cell(0, 9), defaults),
        Err(PathError::OutOfBounds(cell(0, 9)))
    );
    for bad in [
        PathOptions {
            budget: Some(0),
            ..defaults
        },
        PathOptions {
            min_cost: 0.0,
            ..defaults
        },
        PathOptions {
            min_cost: f32::NAN,
            ..defaults
        },
        PathOptions {
            min_cost: f32::INFINITY,
            ..defaults
        },
    ] {
        assert_eq!(
            search(cell(0, 0), cell(1, 1), bad),
            Err(PathError::InvalidOptions)
        );
    }
    let huge = GridFn::new(UVec2::splat(70_000), |_| Some(1.0));
    assert_eq!(
        finder.find_path(&huge, cell(0, 0), cell(1, 1), &defaults, &mut path),
        Err(PathError::GridTooLarge(UVec2::splat(70_000)))
    );
    assert!(
        PathError::InvalidCost {
            cell: cell(1, 2),
            cost: 0.5
        }
        .to_string()
        .contains("[1, 2]")
    );
}

#[test]
fn steady_state_queries_reuse_buffers() {
    let mut grid = CostGrid::new(cell(64, 64), 1.0);
    grid.block_rect(cell(10, 0), cell(10, 60));
    grid.block_rect(cell(30, 4), cell(30, 63));
    let mut finder = PathFinder::new();
    finder.reserve(grid.size()).unwrap();
    let mut path = Vec::new();
    let search = |finder: &mut PathFinder, path: &mut Vec<UVec2>| {
        finder
            .find_path(
                &grid,
                cell(0, 0),
                cell(63, 63),
                &PathOptions::default(),
                path,
            )
            .unwrap()
    };
    search(&mut finder, &mut path);
    let buffers = finder.buffers();
    let path_buffer = (path.capacity(), path.as_ptr());
    for _ in 0..5 {
        search(&mut finder, &mut path);
        assert_eq!(finder.buffers(), buffers);
        assert_eq!((path.capacity(), path.as_ptr()), path_buffer);
    }
    // Smaller grids fit the same buffers.
    finder
        .find_path(
            &CostGrid::new(cell(8, 8), 1.0),
            cell(0, 0),
            cell(7, 7),
            &PathOptions::default(),
            &mut path,
        )
        .unwrap();
    assert_eq!(finder.buffers().1, buffers.1);
}

#[test]
fn stamp_wraparound_forgets_old_searches() {
    let grid = parse(&["....", ".##.", "...."]);
    let (expected, expected_path) = find(&grid, cell(0, 1), cell(3, 1), Neighborhood::Four);
    let mut finder = PathFinder::new();
    let mut path = Vec::new();
    finder.set_stamp(u32::MAX - 1);
    // This search uses stamp `u32::MAX` and leaves closed nodes behind.
    finder
        .find_path(
            &grid,
            cell(0, 0),
            cell(3, 2),
            &options(Neighborhood::Four),
            &mut path,
        )
        .unwrap();
    let status = finder
        .find_path(
            &grid,
            cell(0, 1),
            cell(3, 1),
            &options(Neighborhood::Four),
            &mut path,
        )
        .unwrap();
    assert_eq!((status, path), (expected, expected_path));
}

#[test]
fn line_of_sight_checks_cells_and_exact_corners() {
    let grid = parse(&[".....", "..#..", "....."]);
    let rule = Neighborhood::default();
    assert!(line_of_sight(&grid, cell(0, 0), cell(4, 0), rule, Vec2::ZERO).unwrap());
    assert!(!line_of_sight(&grid, cell(0, 1), cell(4, 1), rule, Vec2::ZERO).unwrap());
    assert!(!line_of_sight(&grid, cell(0, 0), cell(4, 2), rule, Vec2::ZERO).unwrap());
    assert!(line_of_sight(&grid, cell(0, 0), cell(1, 2), rule, Vec2::ZERO).unwrap());
    assert!(line_of_sight(&grid, cell(1, 1), cell(1, 1), rule, Vec2::ZERO).unwrap());
    assert_eq!(
        line_of_sight(&grid, cell(0, 0), cell(5, 0), rule, Vec2::ZERO),
        Err(PathError::OutOfBounds(cell(5, 0)))
    );

    // The segment (0,0)-(2,2) passes exactly through cell corners.
    let squeeze = parse(&[".#.", "#..", "..."]);
    let corner = parse(&[".#.", "...", "..."]);
    for (mode, through_squeeze, past_corner) in [
        (Neighborhood::Four, false, false),
        (Neighborhood::Eight(DiagonalRule::Always), true, true),
        (Neighborhood::Eight(DiagonalRule::IfEitherOpen), false, true),
        (Neighborhood::Eight(DiagonalRule::IfBothOpen), false, false),
    ] {
        assert_eq!(
            line_of_sight(&squeeze, cell(0, 0), cell(2, 2), mode, Vec2::ZERO),
            Ok(through_squeeze)
        );
        assert_eq!(
            line_of_sight(&corner, cell(0, 0), cell(2, 2), mode, Vec2::ZERO),
            Ok(past_corner)
        );
    }
}

#[test]
fn smoothing_removes_redundant_waypoints_without_entering_walls() {
    let grid = parse(&["........", "........", "...##...", "...##...", "........"]);
    for mode in MODES {
        let (status, mut path) = find(&grid, cell(0, 3), cell(7, 2), mode);
        cost_of(status);
        let original = path.clone();
        smooth_path(&grid, &mut path, mode, Vec2::ZERO).unwrap();
        assert!(path.len() < original.len(), "{mode:?}");
        assert_eq!((path[0], *path.last().unwrap()), (cell(0, 3), cell(7, 2)));
        for pair in path.windows(2) {
            assert!(
                line_of_sight(&grid, pair[0], pair[1], mode, Vec2::ZERO).unwrap(),
                "{mode:?} {path:?}"
            );
        }
        // Kept waypoints are a subsequence of the grid path.
        let mut rest = original.iter();
        assert!(path.iter().all(|kept| rest.any(|cell| cell == kept)));
    }

    // An open L-shaped path becomes a single segment.
    let open = CostGrid::new(cell(5, 5), 1.0);
    let mut path = vec![
        cell(0, 0),
        cell(1, 0),
        cell(2, 0),
        cell(2, 1),
        cell(2, 2),
        cell(3, 2),
    ];
    smooth_path(&open, &mut path, Neighborhood::Four, Vec2::ZERO).unwrap();
    assert_eq!(path, vec![cell(0, 0), cell(3, 2)]);

    // A stale path through newly blocked cells gains no blocked shortcut.
    let mut stale = open.clone();
    stale.set(cell(1, 1), None);
    stale.set(cell(2, 1), None);
    let mut path = vec![cell(0, 0), cell(1, 0), cell(2, 1), cell(3, 1), cell(4, 1)];
    let original = path.clone();
    smooth_path(&stale, &mut path, Neighborhood::default(), Vec2::ZERO).unwrap();
    assert_eq!(path, original);

    let mut short = vec![cell(0, 0), cell(1, 1)];
    smooth_path(&open, &mut short, Neighborhood::Four, Vec2::ZERO).unwrap();
    assert_eq!(short, vec![cell(0, 0), cell(1, 1)]);
    let mut outside = vec![cell(0, 0), cell(9, 0)];
    assert_eq!(
        smooth_path(&open, &mut outside, Neighborhood::Four, Vec2::ZERO),
        Err(PathError::OutOfBounds(cell(9, 0)))
    );
    assert_eq!(outside, vec![cell(0, 0), cell(9, 0)]);
    for clearance in [
        Vec2::new(0.1, -0.1),
        Vec2::new(0.5, 0.0),
        Vec2::new(0.0, f32::NAN),
    ] {
        assert_eq!(
            smooth_path(&open, &mut short, Neighborhood::Four, clearance),
            Err(PathError::InvalidOptions)
        );
        assert_eq!(
            line_of_sight(&open, cell(0, 0), cell(1, 1), Neighborhood::Four, clearance),
            Err(PathError::InvalidOptions)
        );
    }
}

#[test]
fn clearance_keeps_shortcuts_off_wall_corners() {
    // The segment (8,10)-(9,4) passes 1/14 of a cell (in the max norm) from
    // the corner of the wall at (9,8).
    let mut grid = CostGrid::new(cell(12, 12), 1.0);
    grid.set(cell(9, 8), None);
    let mode = Neighborhood::Four;
    assert!(line_of_sight(&grid, cell(8, 10), cell(9, 4), mode, Vec2::ZERO).unwrap());
    assert!(line_of_sight(&grid, cell(8, 10), cell(9, 4), mode, Vec2::splat(0.07)).unwrap());
    assert!(!line_of_sight(&grid, cell(8, 10), cell(9, 4), mode, Vec2::splat(0.08)).unwrap());
    // Clearance is per axis: here only its x extent reaches the corner.
    let thin = Vec2::new(0.0, 0.45);
    assert!(line_of_sight(&grid, cell(8, 10), cell(9, 4), mode, thin).unwrap());
    let wide = Vec2::new(0.09, 0.0);
    assert!(!line_of_sight(&grid, cell(8, 10), cell(9, 4), mode, wide).unwrap());
    // Passing a wall face at exactly the clearance is allowed.
    assert!(line_of_sight(&grid, cell(8, 0), cell(8, 11), mode, Vec2::splat(0.49)).unwrap());

    let (status, original) = find(&grid, cell(8, 10), cell(9, 4), mode);
    cost_of(status);
    let mut path = original.clone();
    smooth_path(&grid, &mut path, mode, Vec2::ZERO).unwrap();
    assert_eq!(path, vec![cell(8, 10), cell(9, 4)]);
    path.clone_from(&original);
    smooth_path(&grid, &mut path, mode, Vec2::splat(0.3)).unwrap();
    assert!(path.len() > 2);
    for pair in path.windows(2) {
        assert!(line_of_sight(&grid, pair[0], pair[1], mode, Vec2::splat(0.3)).unwrap());
    }
}

#[test]
fn wide_bodies_follow_smoothed_paths_on_random_maps() {
    let mut rng = Rng(0x5eed_cafe);
    let mut finder = PathFinder::new();
    let mut follower = PathFollower::new(0.05);
    let mut path = Vec::new();
    let layout = GridLayout::new(Vec2::ZERO, Vec2::ONE);
    let dt = 1.0 / 60.0;
    for round in 0..600 {
        let grid = random_grid(&mut rng, UVec2::new(16, 12));
        let solids: Vec<_> = (0..12)
            .flat_map(|y| (0..16).map(move |x| cell(x, y)))
            .filter(|&cell| !grid.walkable(cell))
            .map(|cell| layout.cell_bounds(cell))
            .collect();
        let start = cell(rng.next() % 16, rng.next() % 12);
        let goal = cell(rng.next() % 16, rng.next() % 12);
        let (size, speed) = if round % 2 == 0 {
            (0.6, 4.0)
        } else {
            (0.8, 8.0)
        };
        let mode = MODES[round % 4];
        let walkable = grid.walkable(start) && grid.walkable(goal);
        let found = walkable
            && matches!(
                finder.find_path(&grid, start, goal, &options(mode), &mut path),
                Ok(PathStatus::Found { .. })
            );
        // Diagonal squeezes leave no room for a body; only the strict rule
        // guarantees clearance along the grid path itself.
        if !found
            || mode != Neighborhood::Eight(DiagonalRule::IfBothOpen) && mode != Neighborhood::Four
        {
            continue;
        }
        smooth_path(&grid, &mut path, mode, Vec2::splat(size / 2.0)).unwrap();
        follower.set_cells(&layout, &path);
        let mut body = Body2D::new(layout.cell_center(start), Vec2::splat(size));
        for _ in 0..3000 {
            follower.steer_body_2d(&mut body, speed, dt);
            // With clearance the body never even slides along a wall.
            let free = body.position + body.velocity * dt;
            body.move_and_slide(dt, &solids);
            assert!(
                body.position.distance(free) < 1e-4,
                "round {round}: {path:?}"
            );
            if follower.is_finished() {
                break;
            }
        }
        assert!(follower.is_finished(), "round {round}: {path:?}");
    }
}

#[test]
fn smoothing_keeps_detours_around_expensive_cells() {
    let grid = parse(&[".....", ".999.", "....."]);
    let mut path = vec![
        cell(0, 0),
        cell(1, 0),
        cell(2, 0),
        cell(3, 0),
        cell(4, 0),
        cell(4, 1),
        cell(4, 2),
    ];
    smooth_path(&grid, &mut path, Neighborhood::Four, Vec2::ZERO).unwrap();
    // The diagonal (0,0)-(4,2) would cross the swamp, so the corner stays.
    assert_eq!(path, vec![cell(0, 0), cell(4, 0), cell(4, 2)]);
    let open = CostGrid::new(cell(5, 3), 1.0);
    let mut path = vec![
        cell(0, 0),
        cell(1, 0),
        cell(2, 0),
        cell(3, 0),
        cell(4, 0),
        cell(4, 1),
        cell(4, 2),
    ];
    smooth_path(&open, &mut path, Neighborhood::Four, Vec2::ZERO).unwrap();
    assert_eq!(path, vec![cell(0, 0), cell(4, 2)]);
}

#[test]
fn distance_fields_serve_many_agents_and_multiple_goals() {
    let grid = parse(&["....#....", "....#....", "........."]);
    let mut field = DistanceField::new();
    field
        .compute(
            &grid,
            [cell(0, 0), cell(8, 0), cell(4, 0)],
            Neighborhood::Four,
        )
        .unwrap();
    assert_eq!(field.size(), grid.size());
    assert_eq!(field.distance(cell(4, 0)), None);
    assert_eq!(field.distance(cell(0, 0)), Some(0.0));
    assert_eq!(field.distance(cell(2, 0)), Some(2.0));
    assert_eq!(field.distance(cell(4, 2)), Some(6.0));
    assert_eq!(field.next_step(&grid, cell(0, 0)), None);
    assert_eq!(field.next_step(&grid, cell(1, 0)), Some(cell(0, 0)));
    assert_eq!(field.next_step(&grid, cell(9, 9)), None);
    assert_eq!(
        field.next_step(&CostGrid::new(cell(2, 2), 1.0), cell(1, 0)),
        None
    );

    // Agents chase the goal using only their own cell.
    let mut agents = [cell(3, 2), cell(5, 2), cell(8, 2)];
    for _ in 0..12 {
        for agent in &mut agents {
            if let Some(next) = field.next_step(&grid, *agent) {
                *agent = next;
            }
        }
    }
    assert_eq!(agents, [cell(0, 0), cell(8, 0), cell(8, 0)]);

    field
        .compute(&grid, [cell(4, 1)], Neighborhood::Four)
        .unwrap();
    assert_eq!(field.distance(cell(0, 0)), None);
    let bad = GridFn::new(cell(3, 1), |c: UVec2| {
        Some(if c.x == 2 { f32::NAN } else { 1.0 })
    });
    assert!(matches!(
        field.compute(&bad, [cell(0, 0)], Neighborhood::Four),
        Err(PathError::InvalidCost { .. })
    ));
    assert_eq!(
        (field.size(), field.distance(cell(0, 0))),
        (UVec2::ZERO, None)
    );
    assert_eq!(
        field.compute(&grid, [cell(9, 0)], OPEN_EIGHT),
        Err(PathError::OutOfBounds(cell(9, 0)))
    );
}

#[test]
fn layouts_map_cells_and_world_points() {
    let layout = GridLayout::new(Vec2::new(-4.0, 2.0), Vec2::splat(2.0));
    let size = cell(4, 3);
    assert_eq!(layout.cell_center(cell(1, 2)), Vec2::new(-1.0, 7.0));
    assert_eq!(
        layout.cell_bounds(cell(0, 0)),
        Aabb2 {
            min: Vec2::new(-4.0, 2.0),
            max: Vec2::new(-2.0, 4.0)
        }
    );
    assert_eq!(layout.cell_at(Vec2::new(-4.0, 2.0), size), Some(cell(0, 0)));
    assert_eq!(layout.cell_at(Vec2::new(-2.0, 3.9), size), Some(cell(1, 0)));
    assert_eq!(
        layout.cell_at(Vec2::new(3.99, 7.99), size),
        Some(cell(3, 2))
    );
    assert_eq!(layout.cell_at(Vec2::new(4.0, 3.0), size), None);
    assert_eq!(layout.cell_at(Vec2::new(-4.1, 3.0), size), None);
    assert_eq!(layout.cell_at(Vec2::new(f32::NAN, 3.0), size), None);
    for cell in [cell(0, 0), cell(3, 2), cell(2, 1)] {
        assert_eq!(layout.cell_at(layout.cell_center(cell), size), Some(cell));
    }

    // Rectangular cells, as in a tilemap with 2x1 tiles.
    let wide = GridLayout::new(Vec2::ZERO, Vec2::new(2.0, 1.0));
    assert_eq!(wide.cell_center(cell(1, 2)), Vec2::new(3.0, 2.5));
    assert_eq!(wide.cell_bounds(cell(1, 2)).max, Vec2::new(4.0, 3.0));
    assert_eq!(wide.cell_at(Vec2::new(3.9, 0.5), size), Some(cell(1, 0)));
    assert_eq!(wide.clearance(Vec2::splat(0.3)), Vec2::new(0.15, 0.3));
}

#[test]
fn adapters_and_cost_grids_share_cells() {
    let mut grid = CostGrid::new(cell(3, 2), 2.0);
    grid.block_rect(cell(1, 1), cell(9, 9));
    assert_eq!(grid.cost(cell(0, 0)), Some(2.0));
    assert!(!grid.walkable(cell(2, 1)) && !NavGrid::walkable(&&grid, cell(1, 1)));
    let borrowed: &dyn NavGrid = &grid;
    assert_eq!(
        (borrowed.size(), borrowed.cost(cell(0, 1))),
        (cell(3, 2), Some(2.0))
    );
    let (status, _) = find(&borrowed, cell(0, 1), cell(2, 0), Neighborhood::Four);
    assert_eq!(cost_of(status), 6.0);
    CostGrid::new(UVec2::ZERO, 1.0).block_rect(cell(0, 0), cell(1, 1));
}

#[test]
#[should_panic(expected = "outside the grid")]
fn cost_grids_reject_cells_outside() {
    CostGrid::new(cell(2, 2), 1.0).set(cell(2, 0), None);
}

#[test]
fn followers_reach_waypoints_without_turning_back() {
    let mut follower = PathFollower::new(0.1);
    assert!(follower.is_finished());
    assert_eq!(follower.velocity(Vec2::ZERO, 5.0, 0.1), Vec2::ZERO);
    follower.set_waypoints([Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)]);
    assert_eq!(follower.remaining().len(), 3);

    // Starting ahead of the first waypoint skips it instead of backtracking.
    let velocity = follower.velocity(Vec2::new(1.0, 0.0), 5.0, 0.1);
    assert_eq!(velocity, Vec2::new(5.0, 0.0));
    assert_eq!(follower.next_waypoint(), Some(Vec2::new(10.0, 0.0)));

    // Overshooting an intermediate waypoint moves on rather than reversing.
    let velocity = follower.velocity(Vec2::new(10.5, 0.0), 5.0, 0.1);
    assert_eq!(follower.next_waypoint(), Some(Vec2::new(10.0, 10.0)));
    assert!(velocity.y > 0.0 && velocity.length() > 4.99);

    // Near the final waypoint the speed only covers the remaining distance.
    let velocity = follower.velocity(Vec2::new(10.0, 9.8), 5.0, 0.1);
    assert!((velocity - Vec2::new(0.0, 2.0)).length() < 1e-4);
    assert_eq!(
        follower.velocity(Vec2::new(10.0, 9.95), 5.0, 0.1),
        Vec2::ZERO
    );
    assert!(follower.is_finished() && follower.remaining().is_empty());
    assert_eq!(follower.waypoints().len(), 3);

    let layout = GridLayout::new(Vec2::ZERO, Vec2::ONE);
    follower.set_cells(&layout, &[cell(0, 0), cell(0, 2)]);
    let mut body = Body3D::new(Vec3::new(0.5, 1.0, 0.5), Vec3::splat(0.5));
    body.velocity.y = -3.0;
    follower.steer_body_3d(&mut body, 2.0, 0.1);
    assert_eq!(body.velocity, Vec3::new(0.0, -3.0, 2.0));
    follower.clear();
    assert!(follower.is_finished() && follower.waypoints().is_empty());
}

#[test]
fn followers_finish_only_at_the_goal() {
    let mut follower = PathFollower::new(0.1);
    // Beyond the final waypoint but far off the path: not finished.
    follower.set_waypoints([Vec2::ZERO, Vec2::new(5.0, 0.0)]);
    let velocity = follower.velocity(Vec2::new(5.1, 10.0), 1.0, 0.1);
    assert!(!follower.is_finished());
    assert!(velocity.y < 0.0);
    // Overshooting the final waypoint along the path turns back too.
    let velocity = follower.velocity(Vec2::new(5.5, 0.0), 1.0, 0.1);
    assert!(!follower.is_finished() && velocity.x < 0.0);

    // Pushed aside past a corner, the character goes back to round it.
    follower.set_waypoints([Vec2::ZERO, Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0)]);
    follower.velocity(Vec2::new(4.0, 0.0), 1.0, 0.1);
    follower.velocity(Vec2::new(5.5, 2.0), 1.0, 0.1);
    assert_eq!(follower.next_waypoint(), Some(Vec2::new(5.0, 0.0)));
    // Off the path beside the first waypoint, the character returns to it.
    follower.set_waypoints([Vec2::ZERO, Vec2::new(5.0, 0.0)]);
    follower.velocity(Vec2::new(0.4, 0.3), 1.0, 0.1);
    assert_eq!(follower.next_waypoint(), Some(Vec2::ZERO));
}

#[test]
fn fast_followers_do_not_orbit_corners() {
    // Steps longer than the arrival radius, approaching a corner at an angle.
    let (speed, dt) = (8.0, 1.0 / 60.0);
    for i in 0..200 {
        let mut follower = PathFollower::new(0.05);
        follower.set_waypoints([Vec2::ZERO, Vec2::new(5.0, 0.0), Vec2::new(5.0, 5.0)]);
        let mut position = Vec2::new(
            4.0 + 0.7 * (i % 20) as f32 / 20.0,
            0.9 + 0.01 * (i / 20) as f32,
        );
        for _ in 0..200 {
            position += follower.velocity(position, speed, dt) * dt;
        }
        assert!(follower.is_finished(), "start {i}");
        assert!(position.distance(Vec2::new(5.0, 5.0)) <= 0.05);
    }
}

#[test]
fn agents_navigate_around_walls_with_bodies() {
    let grid = parse(&[
        "..........",
        "..#####...",
        "......#...",
        "####..#.##",
        "......#...",
        "..#####...",
        "..........",
    ]);
    let layout = GridLayout::new(Vec2::new(-5.0, -3.0), Vec2::ONE);
    let solids: Vec<_> = (0..grid.size().y)
        .flat_map(|y| (0..grid.size().x).map(move |x| cell(x, y)))
        .filter(|&cell| !grid.walkable(cell))
        .map(|cell| layout.cell_bounds(cell))
        .collect();
    let mode = Neighborhood::default();
    let mut finder = PathFinder::new();
    let mut follower = PathFollower::new(0.05);
    let mut path = Vec::new();
    for (start, goal) in [
        (cell(0, 4), cell(9, 0)),
        (cell(4, 2), cell(0, 6)),
        (cell(9, 6), cell(0, 0)),
    ] {
        let mut body = Body2D::new(layout.cell_center(start), Vec2::splat(0.6));
        let status = finder
            .find_path(&grid, start, goal, &PathOptions::default(), &mut path)
            .unwrap();
        cost_of(status);
        smooth_path(&grid, &mut path, mode, Vec2::splat(0.3)).unwrap();
        follower.set_cells(&layout, &path);
        for _ in 0..600 {
            follower.steer_body_2d(&mut body, 4.0, 1.0 / 60.0);
            body.move_and_slide(1.0 / 60.0, &solids);
            assert!(solids.iter().all(|solid| !solid.intersects(&body.bounds())));
        }
        assert!(follower.is_finished(), "{start} -> {goal}");
        assert!(body.position.distance(layout.cell_center(goal)) < 0.06);
    }
}
