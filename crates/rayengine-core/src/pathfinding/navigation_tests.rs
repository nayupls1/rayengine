use super::*;
use glam::Vec2;
use std::collections::HashMap;

fn cell(x: u32, y: u32) -> UVec2 {
    UVec2::new(x, y)
}

fn at(layer: u32, x: u32, y: u32) -> NavPoint {
    NavPoint::new(layer, cell(x, y))
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

fn route<T>(
    layers: &[CostGrid],
    topology: &NavTopology<T>,
    start: NavPoint,
    goal: NavPoint,
    options: &NavOptions,
) -> (PathStatus, Route) {
    let mut route = Route::new();
    let status = NavFinder::new()
        .find_route(layers, topology, start, goal, options, &mut route)
        .unwrap();
    (status, route)
}

fn points(route: &Route) -> Vec<NavPoint> {
    route.steps().iter().map(|step| step.point).collect()
}

/// Independent Bellman-Ford over every layer cell and link.
fn reference<T>(
    layers: &[CostGrid],
    topology: &NavTopology<T>,
    start: NavPoint,
    options: &NavOptions,
) -> HashMap<NavPoint, f32> {
    let grids: Vec<_> = layers
        .iter()
        .map(|layer| ClearanceGrid::new(layer, options.clearance))
        .collect();
    let mut distance = HashMap::new();
    distance.insert(start, 0.0_f32);
    let rule = options.neighborhood.corner_rule();
    loop {
        let mut changed = false;
        for (layer, grid) in grids.iter().enumerate() {
            let size = grid.size();
            for y in 0..size.y {
                for x in 0..size.x {
                    let from = at(layer as u32, x, y);
                    let Some(&here) = distance.get(&from) else {
                        continue;
                    };
                    let mut relax = |to: NavPoint, cost: f32| {
                        let total = here + cost;
                        if distance.get(&to).is_none_or(|&known| total < known - 1e-4) {
                            distance.insert(to, total);
                            changed = true;
                        }
                    };
                    for &step in options.neighborhood.steps() {
                        let Some(next) = offset(size, from.cell, step) else {
                            continue;
                        };
                        if let Some(cost) = grid.cost(next)
                            && corner_open(grid, from.cell, step, rule)
                        {
                            relax(NavPoint::new(from.layer, next), cost * step_length(step));
                        }
                    }
                    for (_, link) in topology.iter().filter(|(_, link)| link.enabled) {
                        let mut exits = vec![];
                        if link.from == from {
                            exits.push(link.to);
                        }
                        if link.two_way && link.to == from {
                            exits.push(link.from);
                        }
                        for exit in exits {
                            if grids[exit.layer as usize].walkable(exit.cell) {
                                relax(exit, link.cost);
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            return distance;
        }
    }
}

/// Checks every step is a legal move or link and returns the summed cost.
fn check_steps<T>(
    layers: &[CostGrid],
    topology: &NavTopology<T>,
    route: &Route,
    options: &NavOptions,
) -> f32 {
    let mut total = 0.0;
    for pair in route.steps().windows(2) {
        let (here, next) = (pair[0].point, pair[1].point);
        let grid = ClearanceGrid::new(&layers[next.layer as usize], options.clearance);
        let cost = grid.cost(next.cell).expect("route enters a blocked cell");
        total += match pair[1].link {
            Some(id) => {
                let link = topology.get(id).unwrap();
                assert!(link.enabled);
                assert!(
                    (link.from, link.to) == (here, next)
                        || (link.two_way && (link.to, link.from) == (here, next))
                );
                link.cost
            }
            None => {
                assert_eq!(here.layer, next.layer);
                let step = next.cell.as_ivec2() - here.cell.as_ivec2();
                assert!(options.neighborhood.steps().contains(&step));
                assert!(corner_open(
                    &grid,
                    here.cell,
                    step,
                    options.neighborhood.corner_rule()
                ));
                cost * step_length(step)
            }
        };
    }
    total
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, limit: u32) -> u32 {
        self.next() % limit
    }
}

fn house() -> (Vec<CostGrid>, NavTopology<&'static str>, LinkId) {
    let ground = parse(&[
        "#########", //
        "#...#...#",
        "#.......#",
        "#...#...#",
        "#########",
    ]);
    let upstairs = parse(&[
        "#########", //
        "#.......#",
        "#.#####.#",
        "#.......#",
        "#########",
    ]);
    let mut topology = NavTopology::new();
    let stairs = topology.add(NavLink::new(at(0, 7, 3), at(1, 7, 3), 3.0, "stairs"));
    (vec![ground, upstairs], topology, stairs)
}

#[test]
fn inter_floor_routes_use_links_and_match_a_reference() {
    let (layers, topology, stairs) = house();
    let options = NavOptions::default();
    let (start, goal) = (at(0, 1, 1), at(1, 1, 1));
    let (status, route) = route(&layers, &topology, start, goal, &options);
    let PathStatus::Found { cost } = status else {
        panic!("expected a route, got {status:?}");
    };
    let expected = reference(&layers, &topology, start, &options)[&goal];
    assert!((cost - expected).abs() < 1e-4, "{cost} vs {expected}");
    assert!((check_steps(&layers, &topology, &route, &options) - cost).abs() < 1e-4);
    assert_eq!(route.steps().first().unwrap().point, start);
    assert_eq!(route.steps().last().unwrap().point, goal);
    let linked: Vec<_> = route.steps().iter().filter_map(|step| step.link).collect();
    assert_eq!(linked, [stairs]);
    assert_eq!(route.cost(), cost);
    assert!(route.is_current(&topology));

    // Two-way links work downstairs too; one-way links do not.
    let (back, _) = self::route(&layers, &topology, goal, start, &options);
    assert!(matches!(back, PathStatus::Found { .. }));
    let mut one_way = NavTopology::new();
    one_way.add(NavLink::new(at(0, 7, 3), at(1, 7, 3), 3.0, "stairs").one_way());
    let (up, _) = self::route(&layers, &one_way, start, goal, &options);
    let (down, _) = self::route(&layers, &one_way, goal, start, &options);
    assert!(matches!(up, PathStatus::Found { .. }));
    assert_eq!(down, PathStatus::Unreachable);
}

#[test]
fn routes_match_a_reference_on_random_layered_maps() {
    let mut rng = Rng(17);
    for round in 0..120 {
        let layer_count = 1 + rng.below(3);
        let layers: Vec<CostGrid> = (0..layer_count)
            .map(|_| {
                let size = cell(3 + rng.below(6), 3 + rng.below(6));
                let mut grid = CostGrid::new(size, 1.0);
                for y in 0..size.y {
                    for x in 0..size.x {
                        let cost = match rng.below(10) {
                            0..=2 => None,
                            3 => Some(4.0),
                            4 => Some(1.5),
                            _ => Some(1.0),
                        };
                        grid.set(cell(x, y), cost);
                    }
                }
                grid
            })
            .collect();
        let random_point = |rng: &mut Rng| {
            let layer = rng.below(layer_count);
            let size = layers[layer as usize].size();
            at(layer, rng.below(size.x), rng.below(size.y))
        };
        let mut topology = NavTopology::new();
        for _ in 0..rng.below(5) {
            let link = NavLink::new(
                random_point(&mut rng),
                random_point(&mut rng),
                rng.below(6) as f32 * 0.5,
                (),
            );
            let link = if rng.below(2) == 0 {
                link.one_way()
            } else {
                link
            };
            let id = topology.add(link);
            if rng.below(4) == 0 {
                topology.set_enabled(id, false);
            }
        }
        let options = NavOptions {
            neighborhood: [
                Neighborhood::Four,
                Neighborhood::Eight(DiagonalRule::Always),
                Neighborhood::Eight(DiagonalRule::IfBothOpen),
            ][round % 3],
            clearance: Vec2::splat([0.0, 0.3, 0.7][round / 3 % 3]),
            ..NavOptions::default()
        };
        let start = random_point(&mut rng);
        let goal = random_point(&mut rng);
        let expected = reference(&layers, &topology, start, &options);
        let (status, found) = route(&layers, &topology, start, goal, &options);
        let goal_open =
            ClearanceGrid::new(&layers[goal.layer as usize], options.clearance).walkable(goal.cell);
        match (status, expected.get(&goal)) {
            (PathStatus::Found { cost }, Some(&best)) if start == goal || goal_open => {
                assert!(
                    (cost - best).abs() < 1e-3,
                    "round {round}: {cost} vs {best}"
                );
                let walked = check_steps(&layers, &topology, &found, &options);
                assert!((walked - cost).abs() < 1e-3, "round {round}");
            }
            (PathStatus::Unreachable, None) => assert!(found.is_empty()),
            (PathStatus::Unreachable, Some(_)) => assert!(!goal_open, "round {round}"),
            other => panic!("round {round}: {other:?}"),
        }

        // Budgeted searches give the same route, never exceeding the budget.
        let mut finder = NavFinder::new();
        let mut budgeted = Route::new();
        let budget = 1 + rng.below(4);
        let options = NavOptions {
            budget: Some(budget),
            ..options
        };
        let mut status = finder
            .find_route(&layers, &topology, start, goal, &options, &mut budgeted)
            .unwrap();
        let mut calls = 1;
        while status == PathStatus::Pending {
            status = finder
                .resume(&layers, &topology, Some(budget), &mut budgeted)
                .unwrap();
            calls += 1;
        }
        assert!(finder.expanded() <= u64::from(budget) * calls);
        assert_eq!(budgeted, found, "round {round}");
    }
}

#[test]
fn closing_a_door_invalidates_routes_and_pending_searches() {
    // Two rooms joined by a door cell at (4, 2), and a door link upstairs.
    let mut layers = vec![parse(&[
        "#########", //
        "#...#...#",
        "#.......#",
        "#...#...#",
        "#########",
    ])];
    let mut topology = NavTopology::<()>::new();
    let options = NavOptions::default();
    let (start, goal) = (at(0, 1, 2), at(0, 7, 2));
    let (_, mut found) = route(&layers, &topology, start, goal, &options);
    assert!(points(&found).contains(&at(0, 4, 2)));

    // A budgeted search is in flight when the door closes.
    let mut finder = NavFinder::new();
    let mut pending = Route::new();
    let budgeted = NavOptions {
        budget: Some(2),
        ..options
    };
    let status = finder
        .find_route(&layers, &topology, start, goal, &budgeted, &mut pending)
        .unwrap();
    assert_eq!(status, PathStatus::Pending);

    layers[0].set(cell(4, 2), None);
    topology.mark_changed();
    assert!(!found.is_current(&topology));
    assert!(!found.revalidate(&layers, &topology, 0, &options));
    assert_eq!(
        finder.resume(&layers, &topology, Some(2), &mut pending),
        Err(PathError::TopologyChanged)
    );
    assert!(!finder.is_pending());
    let (status, _) = route(&layers, &topology, start, goal, &options);
    assert_eq!(status, PathStatus::Unreachable);

    // A door modelled as a link closes by disabling it.
    let door = topology.add(NavLink::new(at(0, 3, 2), at(0, 5, 2), 2.0, ()));
    let (status, mut through_door) = route(&layers, &topology, start, goal, &options);
    assert_eq!(status, PathStatus::Found { cost: 6.0 });
    assert!(
        through_door
            .steps()
            .iter()
            .any(|step| step.link == Some(door))
    );
    let revision = topology.revision();
    assert!(topology.set_enabled(door, false));
    assert!(
        topology.set_enabled(door, false),
        "unchanged state is still a live id"
    );
    assert_eq!(topology.revision(), revision + 1);
    assert!(!through_door.revalidate(&layers, &topology, 0, &options));
    let (status, _) = route(&layers, &topology, start, goal, &options);
    assert_eq!(status, PathStatus::Unreachable);
    topology.set_enabled(door, true);
    assert!(through_door.revalidate(&layers, &topology, 0, &options));
    assert!(through_door.is_current(&topology));
}

#[test]
fn furniture_invalidates_only_routes_it_blocks() {
    let mut layers = vec![CostGrid::new(cell(8, 5), 1.0)];
    let mut topology = NavTopology::<()>::new();
    let options = NavOptions {
        neighborhood: Neighborhood::Four,
        ..NavOptions::default()
    };
    let (start, goal) = (at(0, 0, 2), at(0, 7, 2));
    let (_, mut found) = route(&layers, &topology, start, goal, &options);
    assert_eq!(found.steps().len(), 8);

    // A chair in a corner leaves the route valid.
    layers[0].set(cell(7, 4), None);
    topology.mark_changed();
    assert!(found.revalidate(&layers, &topology, 0, &options));
    assert!(found.is_current(&topology));

    // A sofa across the route breaks it; the agent halfway along replans.
    layers[0].block_rect(cell(4, 1), cell(4, 3));
    topology.mark_changed();
    assert!(!found.revalidate(&layers, &topology, 2, &options));
    let (status, detour) = route(&layers, &topology, at(0, 2, 2), goal, &options);
    assert_eq!(status, PathStatus::Found { cost: 9.0 });
    assert!(
        points(&detour)
            .iter()
            .all(|point| layers[0].walkable(point.cell))
    );

    // Steps already walked are not rechecked.
    let (_, mut walked) = route(&layers, &topology, start, at(0, 3, 2), &options);
    layers[0].set(cell(1, 2), None);
    topology.mark_changed();
    assert!(walked.revalidate(&layers, &topology, 2, &options));
    assert!(walked.is_current(&topology));
    // A step index past the end, or an empty route, needs a new search.
    assert!(!walked.revalidate(&layers, &topology, 4, &options));
    walked.clear();
    assert!(!walked.revalidate(&layers, &topology, 0, &options));
}

#[test]
fn clearance_applies_to_narrow_openings_and_link_exits() {
    let grid = parse(&[
        "...........", //
        "...........",
        "...........",
        "#####.#####",
        "...........",
        "...........",
        "...........",
    ]);
    let layers = [grid];
    let topology = NavTopology::<()>::new();
    let (start, goal) = (at(0, 1, 1), at(0, 1, 5));
    let narrow = NavOptions::default();
    let wide = NavOptions {
        clearance: Vec2::splat(0.6),
        ..NavOptions::default()
    };
    let (status, _) = route(&layers, &topology, start, goal, &narrow);
    assert!(matches!(status, PathStatus::Found { .. }));
    let (status, _) = route(&layers, &topology, start, goal, &wide);
    assert_eq!(status, PathStatus::Unreachable);

    // Widening the opening to three cells lets the wide agent through, and
    // every cell it enters keeps its whole body clear of walls.
    let mut wider = layers[0].clone();
    wider.set(cell(4, 3), Some(1.0));
    wider.set(cell(6, 3), Some(1.0));
    let layers = [wider];
    let (status, found) = route(&layers, &topology, start, goal, &wide);
    assert!(matches!(status, PathStatus::Found { .. }));
    let body = ClearanceGrid::new(&layers[0], wide.clearance);
    assert_eq!(body.reach(), cell(1, 1));
    assert!(
        points(&found)
            .iter()
            .skip(1)
            .all(|point| body.walkable(point.cell))
    );
    assert!(points(&found).contains(&at(0, 5, 3)));

    // A link into a cramped cell is unusable for the wide agent.
    let floors = [
        CostGrid::new(cell(5, 5), 1.0),
        parse(&["...", ".#.", "..."]),
    ];
    let mut topology = NavTopology::new();
    topology.add(NavLink::new(at(0, 2, 2), at(1, 0, 0), 1.0, ()));
    let (status, _) = route(&floors, &topology, at(0, 0, 0), at(1, 0, 0), &narrow);
    assert!(matches!(status, PathStatus::Found { .. }));
    let (status, _) = route(&floors, &topology, at(0, 1, 1), at(1, 0, 0), &wide);
    assert_eq!(status, PathStatus::Unreachable);
}

#[test]
fn topology_handles_and_revisions() {
    let mut topology = NavTopology::new();
    assert!(topology.is_empty());
    let a = topology.add(NavLink::new(at(0, 0, 0), at(1, 0, 0), 1.0, 'a'));
    let b = topology.add(NavLink::new(at(0, 1, 0), at(1, 1, 0), 1.0, 'b'));
    assert_eq!(topology.len(), 2);
    let revision = topology.revision();
    assert_eq!(topology.remove(a).map(|link| link.tag), Some('a'));
    assert_eq!(topology.remove(a), None);
    assert!(!topology.set_enabled(a, false));
    let c = topology.add(NavLink::new(at(0, 2, 0), at(1, 2, 0), 1.0, 'c'));
    assert_ne!(a, c, "a reused slot gets a fresh handle");
    assert_eq!(topology.get(a), None);
    assert_eq!(topology.revision(), revision + 2);
    *topology.tag_mut(b).unwrap() = 'B';
    assert_eq!(
        topology.revision(),
        revision + 2,
        "tags do not affect routes"
    );
    assert!(topology.set_cost(b, 2.0));
    assert_eq!(topology.revision(), revision + 3);
    let tags: Vec<_> = topology.iter().map(|(_, link)| link.tag).collect();
    assert_eq!(tags, ['c', 'B']);
    assert_eq!(topology.outgoing(at(0, 0, 0)), &[]);
    assert_eq!(topology.outgoing(at(1, 1, 0)), &[(1, true)]);
}

#[test]
fn invalid_route_inputs_are_errors() {
    let layers = [CostGrid::new(cell(3, 3), 1.0)];
    let mut topology = NavTopology::new();
    let mut finder = NavFinder::new();
    let mut found = Route::new();
    let options = NavOptions::default();
    assert_eq!(
        finder.find_route(
            &layers,
            &topology,
            at(1, 0, 0),
            at(0, 0, 0),
            &options,
            &mut found
        ),
        Err(PathError::InvalidPoint(at(1, 0, 0)))
    );
    assert_eq!(
        finder.find_route(
            &layers,
            &topology,
            at(0, 0, 0),
            at(0, 3, 0),
            &options,
            &mut found
        ),
        Err(PathError::InvalidPoint(at(0, 3, 0)))
    );
    let bad = NavOptions {
        clearance: Vec2::new(-1.0, 0.0),
        ..options
    };
    assert_eq!(
        finder.find_route(
            &layers,
            &topology,
            at(0, 0, 0),
            at(0, 2, 2),
            &bad,
            &mut found
        ),
        Err(PathError::InvalidOptions)
    );
    topology.add(NavLink::new(at(0, 0, 0), at(2, 0, 0), 1.0, ()));
    assert_eq!(
        finder.find_route(
            &layers,
            &topology,
            at(0, 0, 0),
            at(0, 2, 2),
            &options,
            &mut found
        ),
        Err(PathError::InvalidPoint(at(2, 0, 0)))
    );
    assert!(!finder.is_pending());
    assert_eq!(
        finder.resume(&layers, &topology, None, &mut found),
        Err(PathError::NoSearch)
    );
}

fn run_traffic(
    traffic: &mut Traffic,
    layers: &[CostGrid],
    topology: &NavTopology<()>,
    ticks: usize,
    log: &mut Vec<TrafficEvent>,
) {
    let mut events = Vec::new();
    for _ in 0..ticks {
        traffic.tick(layers, topology, &mut events).unwrap();
        log.extend_from_slice(&events);
        // One agent per cell, always.
        let mut held: Vec<_> = traffic
            .agents()
            .map(|id| traffic.position(id).unwrap())
            .collect();
        let count = held.len();
        held.sort_by_key(|point| (point.layer, point.cell.y, point.cell.x));
        held.dedup();
        assert_eq!(held.len(), count, "two agents share a cell");
    }
}

#[test]
fn agents_meeting_in_a_corridor_take_turns() {
    // Rooms on both ends of a one-cell corridor.
    let layers = [parse(&[
        "....#######....",
        "...............",
        "....#######....",
    ])];
    let topology = NavTopology::new();
    let options = TrafficOptions {
        neighborhood: Neighborhood::Four,
        ..TrafficOptions::default()
    };
    let simulate = || {
        let mut traffic = Traffic::new(options);
        let west = traffic.add(at(0, 0, 1), Vec2::ZERO, 0);
        let east = traffic.add(at(0, 14, 1), Vec2::ZERO, 0);
        traffic.set_goal(west, Some(at(0, 14, 1)));
        traffic.set_goal(east, Some(at(0, 0, 1)));
        let mut log = Vec::new();
        run_traffic(&mut traffic, &layers, &topology, 60, &mut log);
        (traffic, west, east, log)
    };
    let (traffic, west, east, log) = simulate();
    assert_eq!(traffic.state(west), Some(AgentState::Arrived));
    assert_eq!(traffic.state(east), Some(AgentState::Arrived));
    // The later agent stepped aside once, for the earlier one.
    let yields: Vec<_> = log
        .iter()
        .filter(|event| matches!(event, TrafficEvent::Yielding { .. }))
        .collect();
    assert_eq!(
        yields,
        [&TrafficEvent::Yielding {
            agent: east,
            to: west
        }]
    );
    assert!(
        !log.iter()
            .any(|event| matches!(event, TrafficEvent::Stuck(_)))
    );
    // Identical inputs replay identically.
    assert_eq!(simulate().3, log);
}

#[test]
fn agents_in_a_dead_end_wait_and_report_stuck() {
    // A sealed corridor: neither agent can step aside.
    let layers = [parse(&["......"])];
    let topology = NavTopology::new();
    let options = TrafficOptions {
        give_up: 10,
        ..TrafficOptions::default()
    };
    let mut traffic = Traffic::new(options);
    let a = traffic.add(at(0, 0, 0), Vec2::ZERO, 1);
    let b = traffic.add(at(0, 5, 0), Vec2::ZERO, 0);
    traffic.set_goal(a, Some(at(0, 5, 0)));
    traffic.set_goal(b, Some(at(0, 0, 0)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 30, &mut log);
    // They close in, then wait face to face instead of pushing.
    let (pa, pb) = (traffic.position(a).unwrap(), traffic.position(b).unwrap());
    assert_eq!(pb.cell.x, pa.cell.x + 1);
    assert_eq!(traffic.state(a), Some(AgentState::Stuck));
    assert_eq!(traffic.state(b), Some(AgentState::Stuck));
    let stuck = log
        .iter()
        .filter(|event| matches!(event, TrafficEvent::Stuck(_)))
        .count();
    assert_eq!(stuck, 2, "stuck is reported once per agent");

    // A new goal releases them; `a` then waits behind `b`, parked on the
    // shared goal.
    traffic.set_goal(b, Some(at(0, 5, 0)));
    run_traffic(&mut traffic, &layers, &topology, 10, &mut log);
    assert_eq!(traffic.state(b), Some(AgentState::Arrived));
    assert_eq!(traffic.position(a), Some(at(0, 4, 0)));
    assert!(matches!(
        traffic.state(a),
        Some(AgentState::Waiting | AgentState::Stuck)
    ));
}

#[test]
fn traffic_detours_around_a_parked_agent() {
    let layers = [parse(&[
        ".......", //
        ".......", ".......",
    ])];
    let topology = NavTopology::new();
    let options = TrafficOptions {
        neighborhood: Neighborhood::Four,
        patience: 2,
        ..TrafficOptions::default()
    };
    let mut traffic = Traffic::new(options);
    let parked = traffic.add(at(0, 3, 1), Vec2::ZERO, 5);
    let walker = traffic.add(at(0, 0, 1), Vec2::ZERO, 0);
    traffic.set_goal(walker, Some(at(0, 6, 1)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 20, &mut log);
    assert_eq!(traffic.state(walker), Some(AgentState::Arrived));
    assert_eq!(traffic.position(parked), Some(at(0, 3, 1)));
}

#[test]
fn traffic_reroutes_after_edits_and_retries_unreachable_goals() {
    let mut layers = vec![
        parse(&[
            "############", //
            "#..........#",
            "#..........#",
            "############",
        ]),
        CostGrid::new(cell(3, 3), 1.0),
    ];
    let mut topology = NavTopology::new();
    let stairs = topology.add(NavLink::new(at(0, 10, 1), at(1, 1, 1), 2.0, ()));
    topology.set_enabled(stairs, false);
    let mut traffic = Traffic::new(TrafficOptions {
        plan_budget: 4,
        ..TrafficOptions::default()
    });
    let agent = traffic.add(at(0, 1, 1), Vec2::ZERO, 0);
    traffic.set_goal(agent, Some(at(1, 2, 2)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 10, &mut log);
    assert_eq!(traffic.state(agent), Some(AgentState::Unreachable));
    assert_eq!(
        log.iter()
            .filter(|event| matches!(event, TrafficEvent::Unreachable(_)))
            .count(),
        1
    );

    // Opening the stairs lets the agent plan again; a small budget spreads
    // the search over several ticks.
    topology.set_enabled(stairs, true);
    log.clear();
    run_traffic(&mut traffic, &layers, &topology, 2, &mut log);
    assert_eq!(traffic.state(agent), Some(AgentState::Planning));
    run_traffic(&mut traffic, &layers, &topology, 4, &mut log);
    assert!(
        log.iter()
            .any(|event| matches!(event, TrafficEvent::Moved { .. }))
    );
    let position = traffic.position(agent).unwrap();
    assert_eq!(position.layer, 0);

    // Furniture dropped on the route makes it replan around.
    let ahead = traffic.remaining(agent)[1].point;
    layers[0].set(ahead.cell, None);
    topology.mark_changed();
    log.clear();
    run_traffic(&mut traffic, &layers, &topology, 30, &mut log);
    assert_eq!(log.first(), Some(&TrafficEvent::Rerouted(agent)));
    assert!(log.iter().any(|event| matches!(
        event,
        TrafficEvent::Moved { link: Some(id), .. } if *id == stairs
    )));
    assert_eq!(traffic.state(agent), Some(AgentState::Arrived));
    assert_eq!(traffic.position(agent), Some(at(1, 2, 2)));
}

#[test]
fn yielding_drops_a_queued_detour() {
    // A ring: the detour around the blocker is long, so it is still being
    // planned when the blocker turns head-on and the agent steps aside.
    let layers = [parse(&[
        "...............",
        ".#############.",
        "...............",
    ])];
    let topology = NavTopology::<()>::new();
    let goal = at(0, 10, 0);
    for plan_budget in 18..=34 {
        let mut traffic = Traffic::new(TrafficOptions {
            neighborhood: Neighborhood::Four,
            patience: 1,
            plan_budget,
            ..TrafficOptions::default()
        });
        let blocker = traffic.add(at(0, 6, 0), Vec2::ZERO, 5);
        let walker = traffic.add(at(0, 5, 0), Vec2::ZERO, 0);
        traffic.set_goal(walker, Some(goal));
        let mut events = Vec::new();
        let mut sent = false;
        for _ in 0..80 {
            traffic.tick(&layers, &topology, &mut events).unwrap();
            if !sent && traffic.state(walker) == Some(AgentState::Waiting) {
                traffic.set_goal(blocker, Some(at(0, 0, 0)));
                sent = true;
            }
            let reached = events.iter().any(|event| {
                matches!(event, TrafficEvent::Moved { agent, to, .. } if *agent == walker && *to == goal)
            });
            if reached {
                assert!(
                    events.contains(&TrafficEvent::Arrived(walker)),
                    "budget {plan_budget}"
                );
            }
        }
        assert!(sent);
        assert_eq!(
            traffic.state(walker),
            Some(AgentState::Arrived),
            "budget {plan_budget}"
        );
    }
}

#[test]
fn priority_ties_go_to_the_agent_added_first() {
    let layers = [parse(&["....."])];
    let topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(TrafficOptions::default());
    let early = traffic.add(at(0, 4, 0), Vec2::ZERO, 0);
    let removed = traffic.add(at(0, 2, 0), Vec2::ZERO, 0);
    traffic.remove(removed);
    // Reuses the removed agent's slot, ahead of `early` in slot order.
    let late = traffic.add(at(0, 0, 0), Vec2::ZERO, 0);
    // Both want (2, 0); the agent added first moves first and takes it.
    traffic.set_goal(early, Some(at(0, 2, 0)));
    traffic.set_goal(late, Some(at(0, 2, 0)));
    let mut events = Vec::new();
    traffic.tick(&layers, &topology, &mut events).unwrap();
    traffic.tick(&layers, &topology, &mut events).unwrap();
    assert_eq!(traffic.position(early), Some(at(0, 2, 0)));
    assert_eq!(traffic.position(late), Some(at(0, 1, 0)));
}

#[test]
fn a_blocked_way_aside_is_replaced() {
    let layers = [parse(&[
        "..########", //
        "..........",
        "..########",
    ])];
    let topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(TrafficOptions {
        neighborhood: Neighborhood::Four,
        ..TrafficOptions::default()
    });
    let high = traffic.add(at(0, 6, 1), Vec2::ZERO, 5);
    let low = traffic.add(at(0, 5, 1), Vec2::ZERO, 0);
    let third = traffic.add(at(0, 0, 0), Vec2::ZERO, 9);
    traffic.set_goal(high, Some(at(0, 0, 1)));
    traffic.set_goal(low, Some(at(0, 9, 1)));
    let mut events = Vec::new();
    let mut sent = false;
    for _ in 0..60 {
        traffic.tick(&layers, &topology, &mut events).unwrap();
        // While `low` backs out of the hallway, a third agent takes the cell
        // it was heading for.
        if !sent
            && events
                .iter()
                .any(|event| matches!(event, TrafficEvent::Yielding { .. }))
        {
            traffic.set_goal(third, Some(at(0, 1, 2)));
            sent = true;
        }
    }
    assert!(sent);
    assert_eq!(traffic.position(third), Some(at(0, 1, 2)));
    assert_eq!(traffic.state(high), Some(AgentState::Arrived));
    assert_eq!(traffic.state(low), Some(AgentState::Arrived));
}

#[test]
fn agents_behind_a_jam_make_room() {
    // A hallway with a dead end to the west and a branch north. `west` and
    // `north` meet head-on with no room to step aside: `north` has the dead
    // end behind it, `west` has `last` queued behind it.
    let layers = [parse(&[
        "###.###", //
        "###.###", "###.###", ".......",
    ])];
    let topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(TrafficOptions {
        neighborhood: Neighborhood::Four,
        ..TrafficOptions::default()
    });
    let west = traffic.add(at(0, 2, 3), Vec2::ZERO, 2);
    let north = traffic.add(at(0, 1, 3), Vec2::ZERO, 1);
    let last = traffic.add(at(0, 3, 3), Vec2::ZERO, 0);
    traffic.set_goal(west, Some(at(0, 0, 3)));
    traffic.set_goal(north, Some(at(0, 3, 0)));
    traffic.set_goal(last, Some(at(0, 1, 3)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 40, &mut log);
    for agent in [west, north, last] {
        assert_eq!(traffic.state(agent), Some(AgentState::Arrived));
    }
    // `last` stepped aside first, off the route of `north`.
    let first = log
        .iter()
        .find(|event| matches!(event, TrafficEvent::Yielding { .. }));
    assert_eq!(
        first,
        Some(&TrafficEvent::Yielding {
            agent: last,
            to: north
        })
    );
}

#[test]
fn stuck_is_reported_once_while_the_goal_is_taken() {
    let layers = [parse(&[
        "......", //
        "#####.",
    ])];
    let topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(TrafficOptions::default());
    let parked = traffic.add(at(0, 5, 0), Vec2::ZERO, 0);
    let walker = traffic.add(at(0, 0, 0), Vec2::ZERO, 0);
    traffic.set_goal(walker, Some(at(0, 5, 0)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 200, &mut log);
    assert_eq!(traffic.position(walker), Some(at(0, 4, 0)));
    assert_eq!(traffic.state(walker), Some(AgentState::Stuck));
    let stuck = log
        .iter()
        .filter(|event| matches!(event, TrafficEvent::Stuck(_)))
        .count();
    assert_eq!(stuck, 1);

    // Once the goal frees up, the walker moves on and arrives.
    traffic.set_goal(parked, Some(at(0, 5, 1)));
    run_traffic(&mut traffic, &layers, &topology, 10, &mut log);
    assert_eq!(traffic.state(walker), Some(AgentState::Arrived));
}

#[test]
fn agents_meeting_deep_in_a_long_corridor_take_turns() {
    // The meeting point is far more than `yield_radius` steps from any room,
    // so the agent stepping aside walks all the way back.
    let wall = format!("....{}....", "#".repeat(24));
    let open = ".".repeat(32);
    let layers = [parse(&[&wall, &open, &wall])];
    let topology = NavTopology::<()>::new();
    let options = TrafficOptions {
        neighborhood: Neighborhood::Four,
        ..TrafficOptions::default()
    };
    let end = 31;
    for (west_priority, east_priority) in [(0, 0), (1, 0), (0, 1)] {
        let mut traffic = Traffic::new(options);
        let west = traffic.add(at(0, 0, 1), Vec2::ZERO, west_priority);
        let east = traffic.add(at(0, end, 1), Vec2::ZERO, east_priority);
        traffic.set_goal(west, Some(at(0, end, 1)));
        traffic.set_goal(east, Some(at(0, 0, 1)));
        let mut log = Vec::new();
        run_traffic(&mut traffic, &layers, &topology, 200, &mut log);
        assert_eq!(traffic.state(west), Some(AgentState::Arrived));
        assert_eq!(traffic.state(east), Some(AgentState::Arrived));
        assert!(
            !log.iter()
                .any(|event| matches!(event, TrafficEvent::Stuck(_)))
        );
    }

    // With a room behind only one of them, that one steps back into it,
    // whatever the priorities.
    let wall = format!("{}....", "#".repeat(28));
    let layers = [parse(&[&wall, &open, &wall])];
    for (west_priority, east_priority) in [(0, 1), (1, 0)] {
        let mut traffic = Traffic::new(options);
        let west = traffic.add(at(0, 2, 1), Vec2::ZERO, west_priority);
        let east = traffic.add(at(0, end, 1), Vec2::ZERO, east_priority);
        traffic.set_goal(west, Some(at(0, end, 1)));
        traffic.set_goal(east, Some(at(0, 0, 1)));
        let mut log = Vec::new();
        run_traffic(&mut traffic, &layers, &topology, 200, &mut log);
        assert_eq!(traffic.state(west), Some(AgentState::Arrived));
        assert_eq!(traffic.state(east), Some(AgentState::Arrived));
    }
}

#[test]
fn agents_shuffling_in_a_jam_report_stuck() {
    // Two floors joined by stairs at both ends of their bottom hallways, with
    // furniture narrowing the way: four agents crowd the upstairs hallway.
    let mut ground = parse(&[
        "################",
        "#......#.......#",
        "#......#.......#",
        "#......#.......#",
        "#......#.......#",
        "####.######.####",
        "#..............#",
        "################",
    ]);
    ground.set(cell(1, 2), None);
    ground.set(cell(5, 1), None);
    let mut upstairs = parse(&[
        "################",
        "#.....#........#",
        "#.....#........#",
        "#.....#........#",
        "#..............#",
        "####.#####.#####",
        "#..............#",
        "################",
    ]);
    upstairs.set(cell(4, 6), None);
    let layers = [ground, upstairs];
    let mut topology = NavTopology::new();
    for x in [1, 14] {
        topology.add(NavLink::new(at(0, x, 6), at(1, x, 6), 4.0, ()));
    }
    let mut traffic = Traffic::new(TrafficOptions {
        neighborhood: Neighborhood::Four,
        ..TrafficOptions::default()
    });
    let agents = [
        (at(1, 14, 6), at(1, 3, 2), 0),
        (at(1, 13, 6), at(0, 12, 1), -3),
        (at(1, 12, 6), at(0, 13, 3), -1),
        (at(1, 9, 6), at(0, 4, 1), -2),
    ]
    .map(|(start, goal, priority)| {
        let id = traffic.add(start, Vec2::ZERO, priority);
        traffic.set_goal(id, Some(goal));
        id
    });
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 1000, &mut log);
    // Agents that never get through are reported, however they move.
    for id in agents {
        if traffic.state(id) != Some(AgentState::Arrived) {
            assert!(log.contains(&TrafficEvent::Stuck(id)), "{id:?}");
        }
    }
}

#[test]
fn walking_back_after_a_long_yield_or_a_reroute_is_not_stuck() {
    // Meetings deep in hallways longer than `give_up`: the agent stepping
    // aside retreats all the way to its room and walks the same cells back.
    let options = TrafficOptions {
        neighborhood: Neighborhood::Four,
        ..TrafficOptions::default()
    };
    for length in [80, 160] {
        let wall = format!("....{}....", "#".repeat(length));
        let open = ".".repeat(length + 8);
        let layers = [parse(&[&wall, &open, &wall])];
        let topology = NavTopology::<()>::new();
        let end = length as u32 + 7;
        let mut traffic = Traffic::new(options);
        let west = traffic.add(at(0, 0, 1), Vec2::ZERO, 1);
        let east = traffic.add(at(0, end, 1), Vec2::ZERO, 0);
        traffic.set_goal(west, Some(at(0, end, 1)));
        traffic.set_goal(east, Some(at(0, 0, 1)));
        let mut log = Vec::new();
        run_traffic(&mut traffic, &layers, &topology, 400, &mut log);
        assert_eq!(traffic.state(west), Some(AgentState::Arrived));
        assert_eq!(traffic.state(east), Some(AgentState::Arrived));
        assert!(log.contains(&TrafficEvent::Yielding {
            agent: east,
            to: west
        }));
        assert!(
            !log.iter()
                .any(|event| matches!(event, TrafficEvent::Stuck(_))),
            "{length}"
        );
    }

    // Two hallways joined at the west end; the lone agent walks the lower
    // one until an edit blocks it and opens the upper one, sending it back.
    let open = ".".repeat(62);
    let wall = format!(".{}.", "#".repeat(60));
    let mut layers = vec![parse(&[&open, &wall, &open])];
    layers[0].set(cell(55, 0), None);
    let mut topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(options);
    let agent = traffic.add(at(0, 0, 2), Vec2::ZERO, 0);
    traffic.set_goal(agent, Some(at(0, 61, 1)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 55, &mut log);
    layers[0].set(cell(58, 2), None);
    layers[0].set(cell(55, 0), Some(1.0));
    topology.mark_changed();
    run_traffic(&mut traffic, &layers, &topology, 200, &mut log);
    assert!(log.contains(&TrafficEvent::Rerouted(agent)));
    assert_eq!(traffic.state(agent), Some(AgentState::Arrived));
    assert!(
        !log.iter()
            .any(|event| matches!(event, TrafficEvent::Stuck(_)))
    );
}

#[test]
fn a_huge_yield_radius_does_not_overflow() {
    let layers = [parse(&[
        "....#######....",
        "...............",
        "....#######....",
    ])];
    let topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(TrafficOptions {
        neighborhood: Neighborhood::Four,
        yield_radius: u32::MAX,
        ..TrafficOptions::default()
    });
    let west = traffic.add(at(0, 0, 1), Vec2::ZERO, 0);
    let east = traffic.add(at(0, 14, 1), Vec2::ZERO, 0);
    traffic.set_goal(west, Some(at(0, 14, 1)));
    traffic.set_goal(east, Some(at(0, 0, 1)));
    let mut log = Vec::new();
    run_traffic(&mut traffic, &layers, &topology, 60, &mut log);
    assert_eq!(traffic.state(west), Some(AgentState::Arrived));
    assert_eq!(traffic.state(east), Some(AgentState::Arrived));
}

#[test]
#[should_panic(expected = "cell already held")]
fn adding_an_agent_on_a_held_cell_panics() {
    let mut traffic = Traffic::new(TrafficOptions::default());
    traffic.add(at(0, 0, 0), Vec2::ZERO, 0);
    traffic.add(at(0, 0, 0), Vec2::ZERO, 0);
}

#[test]
fn agents_meeting_past_stairs_step_back_down_them() {
    // The upstairs hallway has no side cell: the agent that came up the
    // stairs makes way by going back down into the room.
    let layers = [
        CostGrid::new(cell(5, 3), 1.0),
        CostGrid::new(cell(5, 1), 1.0),
    ];
    let mut topology = NavTopology::new();
    let stairs = topology.add(NavLink::new(at(0, 2, 0), at(1, 0, 0), 2.0, ()));
    for (up_priority, down_priority) in [(0, 1), (1, 0), (0, 0)] {
        let mut traffic = Traffic::new(TrafficOptions {
            neighborhood: Neighborhood::Four,
            ..TrafficOptions::default()
        });
        let up = traffic.add(at(0, 2, 1), Vec2::ZERO, up_priority);
        let down = traffic.add(at(1, 4, 0), Vec2::ZERO, down_priority);
        traffic.set_goal(up, Some(at(1, 4, 0)));
        traffic.set_goal(down, Some(at(0, 0, 2)));
        let mut log = Vec::new();
        run_traffic(&mut traffic, &layers, &topology, 100, &mut log);
        assert_eq!(traffic.state(up), Some(AgentState::Arrived));
        assert_eq!(traffic.state(down), Some(AgentState::Arrived));
        assert!(
            !log.iter()
                .any(|event| matches!(event, TrafficEvent::Stuck(_)))
        );
        assert!(log.iter().any(|event| matches!(
            event,
            TrafficEvent::Moved { agent, link: Some(id), to, .. }
                if *agent == up && *id == stairs && to.layer == 0
        )));
    }
}

#[test]
fn agents_do_not_cross_diagonally_in_one_tick() {
    let layers = [CostGrid::new(cell(4, 4), 1.0)];
    let topology = NavTopology::<()>::new();
    let mut traffic = Traffic::new(TrafficOptions::default());
    let a = traffic.add(at(0, 1, 1), Vec2::ZERO, 0);
    let b = traffic.add(at(0, 2, 1), Vec2::ZERO, 0);
    traffic.set_goal(a, Some(at(0, 2, 2)));
    traffic.set_goal(b, Some(at(0, 1, 2)));
    let mut events = Vec::new();
    for _ in 0..10 {
        traffic.tick(&layers, &topology, &mut events).unwrap();
        let moves: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TrafficEvent::Moved { from, to, .. } => Some((*from, *to)),
                _ => None,
            })
            .collect();
        for &(from, to) in &moves {
            let other = (at(0, from.cell.x, to.cell.y), at(0, to.cell.x, from.cell.y));
            assert!(
                from.cell.x == to.cell.x
                    || from.cell.y == to.cell.y
                    || !moves.contains(&other) && !moves.contains(&(other.1, other.0)),
                "{moves:?}"
            );
        }
    }
    assert_eq!(traffic.state(a), Some(AgentState::Arrived));
    assert_eq!(traffic.state(b), Some(AgentState::Arrived));
}

#[test]
fn walking_back_along_a_detour_is_not_stuck() {
    // A ring hallway; a parked agent blocks the near end of the goal, so the
    // detour runs back along the way the agent came.
    let mut rows = vec![".".repeat(60); 5];
    for row in &mut rows[1..4] {
        *row = format!(".{}.", "#".repeat(58));
    }
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let layers = [parse(&rows)];
    let topology = NavTopology::<()>::new();
    for neighborhood in [Neighborhood::Four, TrafficOptions::default().neighborhood] {
        let mut traffic = Traffic::new(TrafficOptions {
            neighborhood,
            ..TrafficOptions::default()
        });
        let agent = traffic.add(at(0, 0, 2), Vec2::ZERO, 0);
        traffic.add(at(0, 59, 0), Vec2::ZERO, 0);
        traffic.set_goal(agent, Some(at(0, 59, 2)));
        let mut log = Vec::new();
        run_traffic(&mut traffic, &layers, &topology, 300, &mut log);
        assert_eq!(traffic.state(agent), Some(AgentState::Arrived));
        assert!(
            !log.iter()
                .any(|event| matches!(event, TrafficEvent::Stuck(_))),
            "{neighborhood:?}"
        );
    }
}
