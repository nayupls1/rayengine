# Floors, links and traffic

[Grid pathfinding](crate::guides::pathfinding) searches one grid for one
agent. A building needs more: routes between floors, doors that open and
close, furniture that moves, wide agents, and residents sharing narrow
hallways. `rayengine::pathfinding` covers these with three pieces, all
display-independent in `rayengine-core`:

- `NavFinder` searches several grid *layers* joined by caller-owned *links*
  such as stairs, ladders and doors, with agent clearance on every cell.
- `NavTopology` owns the links and a *revision* counting navigation edits, so
  pending searches and active routes notice doors closing and furniture moving.
- `Traffic` moves a small group of agents cell by cell, one per cell, with
  waiting, yielding and bounded detours.

The single-grid `PathFinder` workflow is unchanged; use it whenever one grid
and one agent at a time are enough.

## Layers and links

Each layer is a `NavGrid`, typically one per floor, passed as a slice. A
`NavPoint` names a layer index and a cell. Links join two points in any
layers: `NavLink::new(from, to, cost, tag)` creates an enabled two-way link,
and `.one_way()` restricts it to `from → to`. Taking a link costs exactly its
`cost`, in place of the grid step cost, and its exit cell must be walkable.

```rust
use rayengine::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Passage {
    Stairs,
}

let ground = CostGrid::new(UVec2::new(8, 4), 1.0);
let mut upstairs = CostGrid::new(UVec2::new(8, 4), 1.0);
upstairs.block_rect(UVec2::new(3, 0), UVec2::new(3, 2));
let floors = [ground, upstairs];

let mut topology = NavTopology::new();
let stairs = topology.add(NavLink::new(
    NavPoint::new(0, UVec2::new(7, 3)),
    NavPoint::new(1, UVec2::new(7, 3)),
    5.0,
    Passage::Stairs,
));

let mut finder = NavFinder::new();
let mut route = Route::new();
let start = NavPoint::new(0, UVec2::new(0, 0));
let goal = NavPoint::new(1, UVec2::new(0, 0));
let status = finder
    .find_route(&floors, &topology, start, goal, &NavOptions::default(), &mut route)
    .unwrap();
assert!(matches!(status, PathStatus::Found { .. }));
// Each step names the link used to reach it, so the game can look up the tag
// and play its own climbing animation or door sound.
let climb = route.steps().iter().find_map(|step| step.link).unwrap();
assert_eq!(climb, stairs);
assert_eq!(topology.get(climb).unwrap().tag, Passage::Stairs);
```

Routes include the start and goal. `NavOptions` mirrors `PathOptions`
(neighborhood, `min_cost`, `budget`) and adds `clearance`. The search is A*
with a heuristic that stays admissible across links, so routes are optimal; ties
break deterministically, so identical layers, links and options give
identical routes. A budgeted search returns `Pending` and continues with
`resume`, as in single-grid search.

The engine never interprets tags or plays traversal effects. Whether stairs
take two seconds, a door needs a key, or an elevator waits for its car is game
code: check the link in the route or in `TrafficEvent::Moved`, and use
`Traffic::hold` to keep an agent in place while the effect plays. To keep a
whole class of agents off a link, give them their own topology or disable it.

For a 3D scene, map each layer to a floor height. Within one layer, follow
consecutive grid steps with `PathFollower` as usual, reading waypoints as
world `(x, z)`; at a link step, play the traversal and continue on the next
layer.

## Topology edits and invalidation

Every change through `NavTopology` advances `revision()`: adding or removing
links, `set_enabled` (doors opening and closing) and `set_cost`. The grids
belong to the game, so call `mark_changed()` after editing their cells.

- A pending budgeted search started before an edit fails `resume` with
  `PathError::TopologyChanged` and is cancelled; start a new search.
- A `Route` remembers the revision it was found at. `route.revalidate(layers,
  topology, from, options)` returns `true` at once while nothing changed;
  otherwise it rechecks the steps after index `from` (where the agent stands):
  each grid move must still enter a walkable cell for the agent's clearance
  and pass the corner rule, and each link must still exist, be enabled and
  lead to a walkable cell. A valid route is marked current, so it is not
  rechecked until the next edit. Validation ignores cost changes: a still
  valid route may no longer be the cheapest.

```rust
use rayengine::prelude::*;

let mut floor = [CostGrid::new(UVec2::new(8, 3), 1.0)];
let mut topology = NavTopology::<()>::new();
let options = NavOptions { neighborhood: Neighborhood::Four, ..NavOptions::default() };
let mut finder = NavFinder::new();
let mut route = Route::new();
let (start, goal) = (NavPoint::new(0, UVec2::new(0, 1)), NavPoint::new(0, UVec2::new(7, 1)));
finder.find_route(&floor, &topology, start, goal, &options, &mut route).unwrap();

// A lamp in the corner changes nothing for this route.
floor[0].set(UVec2::new(7, 2), None);
topology.mark_changed();
assert!(route.revalidate(&floor, &topology, 0, &options));

// A sofa across the hallway breaks it: plan again from where the agent is.
floor[0].block_rect(UVec2::new(4, 0), UVec2::new(4, 1));
topology.mark_changed();
assert!(!route.revalidate(&floor, &topology, 0, &options));
let status = finder.find_route(&floor, &topology, start, goal, &options, &mut route).unwrap();
assert!(route.steps().iter().any(|step| step.point.cell == UVec2::new(4, 2)));
assert!(matches!(status, PathStatus::Found { .. }));
```

A door can be modeled either way. A door *cell* in the grid blocks when the
game sets it to `None` (then `mark_changed`). A door *link* spanning a wall
closes with `set_enabled(id, false)`, which also carries a tag for the
opening animation.

## Clearance

`NavOptions::clearance` is the agent's half size in cell units, as returned
by `GridLayout::clearance(body.half_size)`. Every cell a route enters, link
exits included, must fit a box of that size centered on the cell, inside the
grid: half sizes below `0.5` change nothing, up to `1.5` need the 3×3 block
around the cell open, and so on. A sofa-carrier with half size `0.6` (1.2
cells wide) therefore cannot pass a one-cell door that a person can. The same rule is available for
single-grid searches by wrapping a grid in `ClearanceGrid`:

```rust
use rayengine::prelude::*;

let mut grid = CostGrid::new(UVec2::new(9, 7), 1.0);
grid.block_rect(UVec2::new(0, 3), UVec2::new(8, 3));
grid.set(UVec2::new(4, 3), Some(1.0)); // a one-cell doorway
let (start, goal) = (UVec2::new(4, 1), UVec2::new(4, 5));
let mut finder = PathFinder::new();
let mut path = Vec::new();
let options = PathOptions::default();
let narrow = finder.find_path(&grid, start, goal, &options, &mut path).unwrap();
assert!(matches!(narrow, PathStatus::Found { .. }));

let wide = ClearanceGrid::new(&grid, Vec2::splat(0.6));
let blocked = finder.find_path(&wide, start, goal, &options, &mut path).unwrap();
assert_eq!(blocked, PathStatus::Unreachable);
```

Each clearance check reads up to `(2r + 1)²` cells for a reach of `r` cells,
so wide agents cost more per expanded cell.

## Traffic for small groups

`Traffic` owns one `NavFinder` and a list of agents. Each `tick`:

1. Rechecks routes after topology edits. A broken route is dropped
   (`TrafficEvent::Rerouted`) and planned again; an unreachable goal is
   retried once the revision moves.
2. Plans routes round-robin within `plan_budget` A* expansions shared by all
   agents; a long search continues on the next tick.
3. Moves agents in priority order (higher `priority` first, then the agent
   added first), each at most one step. An agent only enters a cell no other
   agent holds, and waits a tick rather than cross a diagonal another agent
   crossed this tick or turn into a cell another agent left this tick, so
   agents never overlap or push into each other; a cell freed earlier in the
   tick may be entered by the agent straight behind, moving the same way.
   Bodies drawn moving in a straight line between their cells over a tick stay
   a full cell apart with four-way moves; with eight-way moves an agent may
   step diagonally past another's cell, so they come within √½ ≈ 0.71 of a
   cell, and bodies wider than that overlap briefly.

Add agents with `add(position, clearance, priority)` and send them with
`set_goal`. The game animates bodies between the cells in
`TrafficEvent::Moved`, for example by lerping over the tick interval or with a
`PathFollower`, and reacts to `Arrived`, `Unreachable`, `Yielding` and
`Stuck`.

```rust
use rayengine::prelude::*;

// Two rooms joined by a one-cell hallway.
let mut map = CostGrid::new(UVec2::new(15, 3), 1.0);
map.block_rect(UVec2::new(4, 0), UVec2::new(10, 0));
map.block_rect(UVec2::new(4, 2), UVec2::new(10, 2));
let floors = [map];
let topology = NavTopology::<()>::new();
let mut traffic = Traffic::new(TrafficOptions {
    neighborhood: Neighborhood::Four,
    ..TrafficOptions::default()
});
let west = traffic.add(NavPoint::new(0, UVec2::new(0, 1)), Vec2::ZERO, 1);
let east = traffic.add(NavPoint::new(0, UVec2::new(14, 1)), Vec2::ZERO, 0);
traffic.set_goal(west, Some(NavPoint::new(0, UVec2::new(14, 1))));
traffic.set_goal(east, Some(NavPoint::new(0, UVec2::new(0, 1))));

let mut events = Vec::new();
let mut yielded = Vec::new();
for _ in 0..60 {
    traffic.tick(&floors, &topology, &mut events).unwrap();
    for event in &events {
        if let TrafficEvent::Yielding { agent, to } = *event {
            yielded.push((agent, to));
        }
    }
}
// The lower-priority agent backed out of the hallway and let the other pass.
assert_eq!(yielded, [(east, west)]);
assert_eq!(traffic.state(west), Some(AgentState::Arrived));
assert_eq!(traffic.state(east), Some(AgentState::Arrived));
```

### Waiting and failure behavior

| Situation | What happens |
| --- | --- |
| Planning a route | Plan around cells held by idle, arrived and `Unreachable` agents, which will not make way; if that finds no route, plan through them. Agents on the move are ignored. |
| Next cell held by an agent moving away | Wait (`Waiting`); move once it is free. |
| Head-on: the blocker's next cell is this agent's cell | The lower-priority agent searches through free cells, on its layer and across enabled two-way links, and only through or off the other's goal once the other has no room either (the pocket behind it closes when the other arrives), visiting at most `(2 × yield_radius + 1)²` cells (every cell within `yield_radius` steps in the open, and further back along a hallway), for the nearest cell off the other's remaining route, walks there (`Yielding`), waits until the other's route no longer crosses the cells it backed through, then plans again. Both drop queued detours, and the other does not plan new ones while it is being made room for. The yielding agent also stops waiting and plans again once the other stops going anywhere or waits on it, directly or through a queue of agents. |
| Head-on, and the lower-priority agent has nowhere to go | The higher-priority agent tries to step aside instead. |
| Head-on, and neither has room | An agent queued behind them steps aside off the route of the agent at the far end, making room. An agent already waiting aside for one of them moves further aside. |
| The way aside gets blocked by another agent | Search a new way aside; with none, stop yielding and plan again. |
| Blocked for `patience` ticks | Plan a detour treating cells held by other agents as walls, at most `max_detours` times per goal, then once every `give_up` ticks while still blocked; none while another agent stands on the goal itself. A failed detour keeps the current route and the agent keeps waiting. |
| No progress for `give_up` ticks | Progress is a step onto a cell the agent has not stood on since its goal was set or an edit rerouted it; waiting, waiting aside and stepping forward again over old cells count against it, while steps back when yielding, the walk back over them afterwards (once per stretch without progress), steps of a detour back over old cells, holds and planning do not. Report `Stuck` once, until the agent progresses, gets a new goal or is rerouted by an edit. The agent keeps trying, so it moves on if the way clears; give it another goal, or move the blocker, to resolve it. |
| No route at all | `Unreachable` once; retried after the next topology edit. |
| A route broken by an edit | `Rerouted`, then planned again from the current cell. |

Only agents following a route make way. Agents standing on their goal, idle
or `Unreachable` hold their cell: a resident parked in a one-cell hallway
blocks it until given another goal, so move unreachable and stuck agents on
as well. The yield search only stops on a cell off the other agent's
remaining route, so two agents meeting on a loop whose cheap way round is
that route itself, for example a one-cell ring closed by a costly link,
both report `Stuck`. Two agents facing
each other in a sealed dead end both report `Stuck` and wait face to face,
which is the expected outcome rather than an endless shuffle. Elsewhere,
two agents meeting in a one-cell hallway get past each other as long as a
side cell lies within the yield search behind one of them: up to
`(2 × yield_radius + 1)² − 1` cells back along a one-cell hallway, 168 by
default. These rules
resolve two agents meeting, and most meetings of a few more, but they are
local: when three or more agents crowd a long one-cell hallway, a jam that
needs several of them to back far out can remain, with agents waiting or
stepping back and forth until `Stuck`. Wider hallways, passing bays or giving
a stuck agent another goal resolve it. Agents occupy
one cell each regardless of clearance; clearance keeps them off walls, not off
each other.

### Bounded and deterministic work

Per tick, planning expands at most `plan_budget` nodes, each yield search
visits at most `(2 × yield_radius + 1)²` cells, jam checks follow a chain of
at most all agents, route checks after an edit cost one step check per
remaining step (each reading up to `(2r + 1)²` cells with clearance), and
detours are capped per goal, then limited to one per `give_up` blocked ticks.
Progress tracking remembers each cell an agent stands on until its next goal
or reroute.
Agents, links and events are processed in a fixed order, so the same layers,
topology, agents and calls produce the same moves on every run.

## Limits

Navigation stays on grids: there is no navigation mesh, continuous local
avoidance or formation movement. Traffic suits a handful to a few dozen agents
moving one cell per tick; crowds chasing one target are cheaper with a
`DistanceField`. Link costs replace step costs and are not scaled by cell
costs; links do not move agents in the world, which stays game code.

## Example

A two-floor house: red, blue, gold and lime residents run errands between
rooms and floors. Purple dots mark the stairs, which hold a resident for a few
steps; the green bar is a door link, toggled with D. Click a floor cell to
place or remove furniture and watch routes that cross it re-plan; a resident
whose errand gets walled off, or who is stuck in a jam, moves on to the next
one. Residents meeting in a
hallway step aside into a room. Run it with
`cargo run -p rayengine --example navigation`:
