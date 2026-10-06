use super::route::{
    ClearanceGrid, LinkId, NavFinder, NavOptions, NavPoint, NavStep, NavTopology, Route,
};
use super::{NavGrid, Neighborhood, PathError, PathStatus, corner_open, offset};
use glam::{UVec2, Vec2};
use std::collections::{HashMap, HashSet, VecDeque};

/// Handle to an agent in [`Traffic`]. Removing the agent invalidates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentId {
    index: u32,
    generation: u32,
}

/// [`Traffic`] settings. `plan_budget` and `yield_radius` bound the work of
/// each call to [`Traffic::tick`] however crowded or blocked the map is;
/// `patience` and `give_up` count ticks, and `max_detours` counts per goal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrafficOptions {
    /// Allowed grid moves and diagonal corner rule.
    pub neighborhood: Neighborhood,
    /// Lowest cost any cell can have; see [`NavOptions::min_cost`].
    pub min_cost: f32,
    /// A* nodes expanded per tick, shared by all agents that need a route.
    /// Must not be zero. Default: `256`.
    pub plan_budget: u32,
    /// Ticks an agent waits behind another before planning a detour around
    /// the agents in its way. Default: `3`.
    pub patience: u32,
    /// Detours an agent may plan per goal; past this, a blocked agent tries
    /// one more every `give_up` ticks. Default: `2`.
    pub max_detours: u32,
    /// Ticks without progress before an agent is reported
    /// [`Stuck`](AgentState::Stuck). Progress is a step onto a cell the agent
    /// has not stood on since its goal was set; steps back while yielding,
    /// holds and planning are not counted against it. Default: `40`.
    pub give_up: u32,
    /// Reach of the search for a cell to step aside into when yielding: it
    /// visits at most `(2 × yield_radius + 1)²` cells, covering every cell
    /// within `yield_radius` steps in the open and reaching further back
    /// along hallways. Default: `6`.
    pub yield_radius: u32,
}

impl Default for TrafficOptions {
    fn default() -> Self {
        let nav = NavOptions::default();
        Self {
            neighborhood: nav.neighborhood,
            min_cost: nav.min_cost,
            plan_budget: 256,
            patience: 3,
            max_detours: 2,
            give_up: 40,
            yield_radius: 6,
        }
    }
}

/// What an agent is doing after the last tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentState {
    /// No goal.
    Idle,
    /// Waiting for search budget, or searching.
    Planning,
    /// Following its route.
    Moving,
    /// Its next cell is held by another agent.
    Waiting,
    /// Stepping aside for an agent coming the other way, or waiting aside
    /// until it has passed.
    Yielding,
    /// Standing on its goal.
    Arrived,
    /// No route exists; the search is retried after the next navigation edit.
    Unreachable,
    /// No progress for [`TrafficOptions::give_up`] ticks, whether waiting or
    /// stepping back and forth. The agent keeps trying and moves on if the
    /// way clears; give it another goal to stop.
    Stuck,
}

/// Something that happened during [`Traffic::tick`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrafficEvent {
    /// The agent stepped to a neighboring cell or through a link. Play
    /// door or stair effects for `link` here.
    Moved {
        /// Agent that moved.
        agent: AgentId,
        /// Previous point.
        from: NavPoint,
        /// New point, now held by the agent.
        to: NavPoint,
        /// The link traversed, if any.
        link: Option<LinkId>,
    },
    /// The agent reached its goal.
    Arrived(AgentId),
    /// No route to the goal exists.
    Unreachable(AgentId),
    /// A navigation edit broke the agent's route; it is planning again.
    Rerouted(AgentId),
    /// The agent is stepping aside for `to`.
    Yielding {
        /// Agent stepping aside.
        agent: AgentId,
        /// Agent it makes room for.
        to: AgentId,
    },
    /// The agent made no progress for [`TrafficOptions::give_up`] ticks.
    /// Reported once until it progresses again or gets a new goal.
    Stuck(AgentId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    None,
    Fresh,
    Detour,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Moved {
    Still,
    Held,
    // Onto a cell it had not stood on since its goal was set.
    Ahead,
    // Back onto a cell it had stood on before.
    Back,
}

#[derive(Clone, Copy, Debug)]
struct Yield {
    to: AgentId,
}

#[derive(Clone, Debug)]
struct Agent {
    generation: u32,
    live: bool,
    position: NavPoint,
    clearance: Vec2,
    priority: i32,
    // When the agent was added; breaks priority ties.
    added: u64,
    goal: Option<NavPoint>,
    route: Route,
    // Index of `position` in the route.
    at: usize,
    state: AgentState,
    plan: Plan,
    blocked: u32,
    detours: u32,
    hold: u32,
    yielding: Option<Yield>,
    cannot_yield: bool,
    // Cells stood on since the goal was set; reaching a new one is progress.
    trail: HashSet<NavPoint>,
    // Ticks without progress, and whether `Stuck` was reported for them.
    stalled: u32,
    reported: bool,
    // What the agent did this tick.
    moved: Moved,
    // Revision at which the goal was last found unreachable.
    failed: u64,
}

impl Agent {
    /// Sorts agents that move first, and win head-on meetings, first.
    fn rank(&self) -> (std::cmp::Reverse<i32>, u64) {
        (std::cmp::Reverse(self.priority), self.added)
    }

    fn next_step(&self) -> Option<NavStep> {
        self.route.steps().get(self.at + 1).copied()
    }

    fn remaining(&self) -> &[NavStep] {
        self.route.steps().get(self.at..).unwrap_or(&[])
    }

    fn reset(&mut self) {
        self.route.clear();
        self.at = 0;
        self.plan = if self.goal.is_some() {
            Plan::Fresh
        } else {
            Plan::None
        };
        self.state = if self.goal.is_some() {
            AgentState::Planning
        } else {
            AgentState::Idle
        };
        self.blocked = 0;
        self.detours = 0;
        self.yielding = None;
        self.cannot_yield = false;
    }

    /// Starts counting progress afresh from the current cell.
    fn restart_progress(&mut self) {
        self.trail.clear();
        self.trail.insert(self.position);
        self.stalled = 0;
        self.reported = false;
    }
}

/// Moves a small group of agents cell by cell over layered navigation,
/// keeping one agent per cell.
///
/// Each [`tick`](Self::tick) plans routes within a shared search budget, then
/// lets every agent advance at most one step, in priority order. An agent only
/// enters a cell no other agent holds, so agents never share a cell or push.
/// Cells are held one per agent whatever its clearance, which keeps agents
/// off walls but not off each other. When
/// its next cell is taken it waits; two agents meeting head-on in a corridor
/// resolve it by the lower-priority one stepping aside into the nearest cell
/// off the other's route, then waiting there until the other has passed.
/// When neither has room, agents queued behind them step aside first. An
/// agent waiting [`patience`](TrafficOptions::patience) ticks plans a
/// bounded detour around the other agents, and after
/// [`give_up`](TrafficOptions::give_up) ticks without progress it reports
/// [`AgentState::Stuck`] while still trying; see the
/// [navigation guide](https://docs.rs/rayengine/latest/rayengine/guides/navigation/index.html)
/// for the full rules.
///
/// The game animates bodies between the cells reported by
/// [`TrafficEvent::Moved`], and owns door and stair effects. Same inputs and
/// tick sequence give the same moves.
///
/// ```
/// use rayengine_core::glam::{UVec2, Vec2};
/// use rayengine_core::pathfinding::{
///     AgentState, CostGrid, NavPoint, NavTopology, Traffic, TrafficOptions,
/// };
///
/// let floor = [CostGrid::new(UVec2::new(5, 1), 1.0)];
/// let topology = NavTopology::<()>::new();
/// let mut traffic = Traffic::new(TrafficOptions::default());
/// let agent = traffic.add(NavPoint::new(0, UVec2::new(0, 0)), Vec2::ZERO, 0);
/// traffic.set_goal(agent, Some(NavPoint::new(0, UVec2::new(4, 0))));
/// let mut events = Vec::new();
/// for _ in 0..5 {
///     traffic.tick(&floor, &topology, &mut events).unwrap();
/// }
/// assert_eq!(traffic.state(agent), Some(AgentState::Arrived));
/// ```
#[derive(Clone, Debug)]
pub struct Traffic {
    options: TrafficOptions,
    agents: Vec<Agent>,
    free: Vec<u32>,
    finder: NavFinder,
    // Agent whose search is pending in `finder`.
    planning: Option<u32>,
    cursor: usize,
    scratch: Route,
    occupied: HashMap<NavPoint, u32>,
    avoid: HashSet<NavPoint>,
    order: Vec<u32>,
    added: u64,
    passing: HashSet<NavPoint>,
    visited: HashMap<UVec2, UVec2>,
    queue: VecDeque<UVec2>,
    chain: Vec<u32>,
}

impl Traffic {
    /// Creates an empty coordinator. Panics for a zero plan budget or a
    /// `min_cost` that is not finite and positive.
    pub fn new(options: TrafficOptions) -> Self {
        assert!(options.plan_budget > 0, "plan budget must not be zero");
        assert!(
            options.min_cost.is_finite() && options.min_cost > 0.0,
            "invalid minimum cost"
        );
        Self {
            options,
            agents: Vec::new(),
            free: Vec::new(),
            finder: NavFinder::new(),
            planning: None,
            cursor: 0,
            scratch: Route::new(),
            occupied: HashMap::new(),
            avoid: HashSet::new(),
            order: Vec::new(),
            added: 0,
            passing: HashSet::new(),
            visited: HashMap::new(),
            queue: VecDeque::new(),
            chain: Vec::new(),
        }
    }

    /// Settings in use.
    pub fn options(&self) -> &TrafficOptions {
        &self.options
    }

    /// Adds an idle agent holding `position`.
    ///
    /// `clearance` is its half size in cell units (see
    /// [`NavOptions::clearance`]). Higher `priority` moves first and keeps
    /// going when meeting another agent head-on; ties go to the agent added
    /// first. Panics for a clearance that is not finite and non-negative.
    pub fn add(&mut self, position: NavPoint, clearance: Vec2, priority: i32) -> AgentId {
        assert!(
            clearance.is_finite() && clearance.min_element() >= 0.0,
            "invalid clearance"
        );
        let index = self.free.pop().unwrap_or_else(|| {
            self.agents.push(Agent {
                generation: 0,
                live: false,
                position,
                clearance,
                priority,
                added: 0,
                goal: None,
                route: Route::new(),
                at: 0,
                state: AgentState::Idle,
                plan: Plan::None,
                blocked: 0,
                detours: 0,
                hold: 0,
                yielding: None,
                cannot_yield: false,
                trail: HashSet::new(),
                stalled: 0,
                reported: false,
                moved: Moved::Still,
                failed: 0,
            });
            (self.agents.len() - 1) as u32
        });
        let agent = &mut self.agents[index as usize];
        agent.live = true;
        agent.position = position;
        agent.clearance = clearance;
        agent.priority = priority;
        agent.added = self.added;
        self.added += 1;
        agent.goal = None;
        agent.hold = 0;
        agent.reset();
        agent.restart_progress();
        AgentId {
            index,
            generation: agent.generation,
        }
    }

    /// Removes an agent, freeing its cell. Returns `false` for a stale id.
    pub fn remove(&mut self, id: AgentId) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.planning == Some(index) {
            self.finder.cancel();
            self.planning = None;
        }
        let agent = &mut self.agents[index as usize];
        agent.live = false;
        agent.generation = agent.generation.wrapping_add(1);
        agent.route.clear();
        self.free.push(index);
        true
    }

    /// Sends the agent to `goal`, or stops it with `None`, dropping its route
    /// and detour count. Returns `false` for a stale id.
    pub fn set_goal(&mut self, id: AgentId, goal: Option<NavPoint>) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        if self.planning == Some(index) {
            self.finder.cancel();
            self.planning = None;
        }
        let agent = &mut self.agents[index as usize];
        agent.goal = goal;
        agent.reset();
        agent.restart_progress();
        true
    }

    /// Keeps the agent in place, still holding its cell, for the next `ticks`
    /// ticks, for example while it opens a door or climbs stairs. Returns
    /// `false` for a stale id.
    pub fn hold(&mut self, id: AgentId, ticks: u32) -> bool {
        let Some(index) = self.index(id) else {
            return false;
        };
        self.agents[index as usize].hold = ticks;
        true
    }

    /// The point the agent holds.
    pub fn position(&self, id: AgentId) -> Option<NavPoint> {
        self.agent(id).map(|agent| agent.position)
    }

    /// The agent's goal.
    pub fn goal(&self, id: AgentId) -> Option<NavPoint> {
        self.agent(id).and_then(|agent| agent.goal)
    }

    /// The agent's state after the last tick.
    pub fn state(&self, id: AgentId) -> Option<AgentState> {
        self.agent(id).map(|agent| agent.state)
    }

    /// Route steps from the agent's position on: toward its goal, or to the
    /// cell it steps aside into while yielding. Empty without a route.
    pub fn remaining(&self, id: AgentId) -> &[NavStep] {
        self.agent(id).map_or(&[], Agent::remaining)
    }

    /// Live agents in id order.
    pub fn agents(&self) -> impl Iterator<Item = AgentId> + '_ {
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| agent.live)
            .map(|(index, agent)| AgentId {
                index: index as u32,
                generation: agent.generation,
            })
    }

    /// Plans and advances every agent by at most one step, replacing
    /// `events` with what happened.
    ///
    /// Pass the same layers and topology every tick, calling
    /// [`NavTopology::mark_changed`] after editing grid cells; routes broken
    /// by an edit are planned again. Errors when an agent's position, goal or
    /// a link endpoint lies outside the layers, or a cell cost is invalid;
    /// that agent then becomes idle, and no agent moves during that tick.
    pub fn tick<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        events: &mut Vec<TrafficEvent>,
    ) -> Result<(), PathError> {
        events.clear();
        self.occupied.clear();
        for (index, agent) in self.agents.iter().enumerate() {
            if agent.live {
                self.occupied.entry(agent.position).or_insert(index as u32);
            }
        }
        self.invalidate(layers, topology, events);
        self.plan(layers, topology, events)?;
        self.advance(layers, topology, events);
        Ok(())
    }

    fn index(&self, id: AgentId) -> Option<u32> {
        self.agent(id).map(|_| id.index)
    }

    fn agent(&self, id: AgentId) -> Option<&Agent> {
        self.agents
            .get(id.index as usize)
            .filter(|agent| agent.live && agent.generation == id.generation)
    }

    fn id(&self, index: u32) -> AgentId {
        AgentId {
            index,
            generation: self.agents[index as usize].generation,
        }
    }

    fn nav_options(&self, agent: &Agent) -> NavOptions {
        NavOptions {
            neighborhood: self.options.neighborhood,
            min_cost: self.options.min_cost,
            clearance: agent.clearance,
            budget: None,
        }
    }

    /// Replans routes that edits broke and retries unreachable goals.
    fn invalidate<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        events: &mut Vec<TrafficEvent>,
    ) {
        for index in 0..self.agents.len() {
            let options = self.nav_options(&self.agents[index]);
            let agent = &mut self.agents[index];
            if !agent.live {
                continue;
            }
            if agent.state == AgentState::Unreachable && agent.failed != topology.revision() {
                agent.plan = Plan::Fresh;
                agent.state = AgentState::Planning;
            } else if !agent.route.is_empty()
                && !agent.route.revalidate(layers, topology, agent.at, &options)
            {
                let detours = agent.detours;
                agent.reset();
                agent.detours = detours;
                events.push(TrafficEvent::Rerouted(self.id(index as u32)));
            }
        }
    }

    /// Runs searches round-robin until the tick's budget is spent.
    fn plan<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        events: &mut Vec<TrafficEvent>,
    ) -> Result<(), PathError> {
        let mut budget = self.options.plan_budget;
        while budget > 0 {
            // A new search restarts the finder's count.
            let before = self.planning.map_or(0, |_| self.finder.expanded());
            let (index, result) = match self.planning {
                Some(index) => {
                    let avoid = &self.avoid;
                    let result = self.finder.resume_avoiding(
                        layers,
                        topology,
                        Some(budget),
                        &mut self.scratch,
                        &|point| avoid.contains(&point),
                    );
                    if matches!(
                        result,
                        Err(PathError::TopologyChanged | PathError::GridResized)
                    ) {
                        // Start over on the new topology.
                        self.planning = None;
                        continue;
                    }
                    (index, result)
                }
                None => {
                    let Some(index) = self.next_to_plan() else {
                        break;
                    };
                    let agent = &self.agents[index as usize];
                    let Some(goal) = agent.goal else {
                        self.agents[index as usize].plan = Plan::None;
                        continue;
                    };
                    let options = NavOptions {
                        budget: Some(budget),
                        ..self.nav_options(agent)
                    };
                    let (start, detour) = (agent.position, agent.plan == Plan::Detour);
                    self.avoid.clear();
                    if detour {
                        self.avoid.extend(
                            self.occupied
                                .keys()
                                .filter(|&&point| point != start && point != goal),
                        );
                    }
                    self.planning = Some(index);
                    let avoid = &self.avoid;
                    let result = self.finder.find_route_avoiding(
                        layers,
                        topology,
                        start,
                        goal,
                        &options,
                        &mut self.scratch,
                        &|point| avoid.contains(&point),
                    );
                    (index, result)
                }
            };
            let spent = (self.finder.expanded() - before).min(u64::from(budget)) as u32;
            budget -= spent;
            if let Ok(PathStatus::Pending) = result {
                break;
            }
            self.planning = None;
            let id = self.id(index);
            let agent = &mut self.agents[index as usize];
            let detour = agent.plan == Plan::Detour;
            agent.plan = Plan::None;
            match result {
                Ok(PathStatus::Found { .. }) => {
                    std::mem::swap(&mut agent.route, &mut self.scratch);
                    agent.at = 0;
                    if agent.route.steps().len() == 1 {
                        agent.route.clear();
                        agent.state = AgentState::Arrived;
                        events.push(TrafficEvent::Arrived(id));
                    } else {
                        agent.state = AgentState::Moving;
                    }
                }
                // A failed detour keeps the agent waiting on its route.
                Ok(_) if detour => {}
                Ok(_) => {
                    agent.route.clear();
                    agent.state = AgentState::Unreachable;
                    agent.failed = topology.revision();
                    events.push(TrafficEvent::Unreachable(id));
                }
                Err(error) => {
                    agent.goal = None;
                    agent.reset();
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// The next agent needing a search, round-robin from the last one.
    fn next_to_plan(&mut self) -> Option<u32> {
        let count = self.agents.len();
        let found = (0..count)
            .map(|offset| (self.cursor + offset) % count)
            .find(|&index| {
                let agent = &self.agents[index];
                agent.live && agent.plan != Plan::None
            })?;
        self.cursor = found + 1;
        Some(found as u32)
    }

    /// Moves agents one step each, highest priority first.
    fn advance<G: NavGrid, T>(
        &mut self,
        layers: &[G],
        topology: &NavTopology<T>,
        events: &mut Vec<TrafficEvent>,
    ) {
        self.order.clear();
        self.order.extend(
            (0..self.agents.len() as u32).filter(|&index| self.agents[index as usize].live),
        );
        let agents = &self.agents;
        self.order
            .sort_by_key(|&index| agents[index as usize].rank());
        for position in 0..self.order.len() {
            let index = self.order[position];
            let agent = &mut self.agents[index as usize];
            agent.moved = Moved::Still;
            if agent.hold > 0 {
                agent.hold -= 1;
                agent.moved = Moved::Held;
                continue;
            }
            let Some(next) = agent.next_step() else {
                if agent.yielding.is_some() {
                    self.wait_aside(index, layers, topology);
                }
                continue;
            };
            match self.occupied.get(&next.point).copied() {
                Some(other) if other != index => {
                    self.blocked(index, other, layers, topology, events);
                }
                _ => self.step(index, next, events),
            }
        }
        for position in 0..self.order.len() {
            let index = self.order[position];
            self.track_progress(index, events);
        }
    }

    fn step(&mut self, index: u32, next: NavStep, events: &mut Vec<TrafficEvent>) {
        let id = self.id(index);
        // The way cleared; a detour from the old cell is no longer needed.
        self.drop_detour(index);
        let agent = &mut self.agents[index as usize];
        let from = agent.position;
        if self.occupied.get(&from) == Some(&index) {
            self.occupied.remove(&from);
        }
        self.occupied.insert(next.point, index);
        agent.position = next.point;
        agent.at += 1;
        agent.blocked = 0;
        agent.cannot_yield = false;
        agent.moved = if agent.trail.insert(next.point) {
            Moved::Ahead
        } else {
            Moved::Back
        };
        events.push(TrafficEvent::Moved {
            agent: id,
            from,
            to: next.point,
            link: next.link,
        });
        if agent.next_step().is_some() {
            agent.state = if agent.yielding.is_some() {
                AgentState::Yielding
            } else {
                AgentState::Moving
            };
        } else if agent.yielding.is_some() {
            agent.state = AgentState::Yielding;
        } else {
            agent.route.clear();
            agent.at = 0;
            agent.state = AgentState::Arrived;
            events.push(TrafficEvent::Arrived(id));
        }
    }

    /// Forgets a detour the agent queued or is searching for.
    fn drop_detour(&mut self, index: u32) {
        let agent = &mut self.agents[index as usize];
        if agent.plan == Plan::Detour {
            agent.plan = Plan::None;
            if self.planning == Some(index) {
                self.finder.cancel();
                self.planning = None;
            }
        }
    }

    /// Waits at the side cell until the agent it yields to has passed it.
    fn wait_aside<G: NavGrid, T>(&mut self, index: u32, layers: &[G], topology: &NavTopology<T>) {
        let agent = &self.agents[index as usize];
        let Some(yielding) = agent.yielding else {
            return;
        };
        let passing = self.agent(yielding.to).map_or(&[][..], Agent::remaining);
        // Stop waiting here once the other agent is not going anywhere (it
        // arrived, or waits aside itself) or waits on this one, directly or
        // through others (its route now runs through the side cell, or a
        // third agent queues behind this one).
        let blocking = self
            .index(yielding.to)
            .is_some_and(|to| self.waits_on(to, index));
        let in_way = !blocking
            && passing.len() > 1
            && passing.iter().any(|step| {
                agent
                    .route
                    .steps()
                    .iter()
                    .any(|own| own.point == step.point)
            });
        let agent = &mut self.agents[index as usize];
        if !in_way {
            agent.yielding = None;
            agent.route.clear();
            agent.at = 0;
            agent.plan = Plan::Fresh;
            agent.state = AgentState::Planning;
            return;
        }
        // The other agent is jammed with no room to step aside, perhaps for
        // want of this cell: move further aside.
        if let Some(to) = self.index(yielding.to)
            && self.agents[to as usize].cannot_yield
        {
            self.find_escape(index, to, layers, topology);
        }
    }

    fn blocked<G: NavGrid, T>(
        &mut self,
        index: u32,
        other: u32,
        layers: &[G],
        topology: &NavTopology<T>,
        events: &mut Vec<TrafficEvent>,
    ) {
        let options = self.options;
        // Something stands on the way aside: yield to it if needed, step
        // aside elsewhere, or stop yielding and plan again.
        let stale = self.agents[index as usize].yielding.take();
        let (me, them) = (&self.agents[index as usize], &self.agents[other as usize]);
        let head_on = them
            .next_step()
            .is_some_and(|step| step.point == me.position);
        let lower = me.rank() > them.rank();
        let should_yield = head_on && ((lower && !me.cannot_yield) || them.cannot_yield);
        // The blocker is caught in a jam it has no room to step out of: make
        // room by stepping off the route of the agent at the far end.
        let jammed = !head_on && them.cannot_yield;
        let far = if should_yield {
            Some(other)
        } else if jammed {
            self.jam_end(other)
        } else {
            None
        };
        if let Some(far) = far {
            if far != index && self.find_escape(index, far, layers, topology) {
                // Queued detours would replace the escape route, or send the
                // other agent around instead of through the room just made.
                self.drop_detour(index);
                self.drop_detour(far);
                let (id, to) = (self.id(index), self.id(far));
                let agent = &mut self.agents[index as usize];
                agent.yielding = Some(Yield { to });
                agent.state = AgentState::Yielding;
                agent.blocked = 0;
                events.push(TrafficEvent::Yielding { agent: id, to });
                return;
            }
            self.agents[index as usize].cannot_yield = true;
        }
        if let Some(stale) = stale {
            let agent = &mut self.agents[index as usize];
            agent.blocked += 1;
            if let Some(to) = self.index(stale.to)
                && self.find_escape(index, to, layers, topology)
            {
                let agent = &mut self.agents[index as usize];
                agent.yielding = Some(stale);
                agent.state = AgentState::Yielding;
            } else {
                let agent = &mut self.agents[index as usize];
                agent.route.clear();
                agent.at = 0;
                agent.plan = Plan::Fresh;
                agent.state = AgentState::Planning;
            }
            return;
        }
        let id = self.id(index);
        // An agent stepping aside for this one is about to clear the way.
        let making_room = self.agents[other as usize]
            .yielding
            .is_some_and(|yielding| yielding.to == id);
        let agent = &mut self.agents[index as usize];
        agent.blocked += 1;
        // No detour helps while another agent stands on the goal itself.
        let on_goal = agent.next_step().map(|step| step.point) == agent.goal;
        if !making_room
            && !on_goal
            && agent.plan == Plan::None
            && agent.blocked >= options.patience
            // Past the cap, retry once every `give_up` ticks in case a jam
            // has opened up elsewhere.
            && (agent.detours < options.max_detours || agent.blocked.is_multiple_of(options.give_up))
        {
            agent.detours += 1;
            agent.plan = Plan::Detour;
        }
        agent.state = AgentState::Waiting;
    }

    /// Counts ticks without progress and reports `Stuck` once they reach
    /// `give_up`. Steps back while yielding, holds, planning and standing on
    /// the goal do not count.
    fn track_progress(&mut self, index: u32, events: &mut Vec<TrafficEvent>) {
        let give_up = self.options.give_up;
        let id = self.id(index);
        let agent = &mut self.agents[index as usize];
        let moved = std::mem::replace(&mut agent.moved, Moved::Still);
        let waiting = matches!(
            agent.state,
            AgentState::Moving | AgentState::Waiting | AgentState::Yielding | AgentState::Stuck
        );
        match moved {
            Moved::Ahead => {
                agent.stalled = 0;
                agent.reported = false;
                return;
            }
            Moved::Held => return,
            Moved::Back if agent.yielding.is_some() => return,
            Moved::Back | Moved::Still if !waiting => return,
            Moved::Back | Moved::Still => agent.stalled += 1,
        }
        if agent.stalled >= give_up {
            agent.state = AgentState::Stuck;
            if !agent.reported {
                agent.reported = true;
                events.push(TrafficEvent::Stuck(id));
            }
        }
    }

    /// The far end of a jam: follows the agents holding each next cell from
    /// `from` until they wait on one already passed, and returns the last
    /// agent before that. `None` if the chain ends at a free or final cell.
    fn jam_end(&mut self, from: u32) -> Option<u32> {
        self.chain.clear();
        self.chain.push(from);
        let mut current = from;
        loop {
            let next = self.agents[current as usize].next_step()?;
            let holder = self.occupied.get(&next.point).copied()?;
            if self.chain.contains(&holder) {
                return (current != from).then_some(current);
            }
            self.chain.push(holder);
            current = holder;
        }
    }

    /// Whether the agent at `from` waits on `target`: follows the agents
    /// holding each next cell, at most once around all agents.
    fn waits_on(&self, from: u32, target: u32) -> bool {
        let mut current = from;
        for _ in 0..self.agents.len() {
            let Some(next) = self.agents[current as usize].next_step() else {
                return false;
            };
            match self.occupied.get(&next.point).copied() {
                Some(holder) if holder == target => return true,
                Some(holder) if holder != current => current = holder,
                _ => return false,
            }
        }
        false
    }

    /// Routes the agent at `index` to the nearest free cell on its layer that
    /// is off `other`'s remaining route, visiting at most
    /// `(2 × yield_radius + 1)²` cells: every cell within `yield_radius` steps
    /// in the open, and further back along a hallway.
    fn find_escape<G: NavGrid, T>(
        &mut self,
        index: u32,
        other: u32,
        layers: &[G],
        topology: &NavTopology<T>,
    ) -> bool {
        let agent = &self.agents[index as usize];
        let start = agent.position;
        let Some(layer) = layers.get(start.layer as usize) else {
            return false;
        };
        let grid = ClearanceGrid::new(layer, agent.clearance);
        let neighborhood = self.options.neighborhood;
        let rule = neighborhood.corner_rule();
        self.passing.clear();
        self.passing.extend(
            self.agents[other as usize]
                .remaining()
                .iter()
                .map(|step| step.point),
        );
        self.visited.clear();
        self.queue.clear();
        self.visited.insert(start.cell, start.cell);
        self.queue.push_back(start.cell);
        let side = 2 * u64::from(self.options.yield_radius) + 1;
        let cap = usize::try_from(side * side).unwrap_or(usize::MAX);
        let mut found = None;
        while let Some(cell) = self.queue.pop_front() {
            if cell != start.cell && !self.passing.contains(&NavPoint::new(start.layer, cell)) {
                found = Some(cell);
                break;
            }
            for &step in neighborhood.steps() {
                if self.visited.len() >= cap {
                    break;
                }
                let Some(next) = offset(grid.size(), cell, step) else {
                    continue;
                };
                let point = NavPoint::new(start.layer, next);
                if self.visited.contains_key(&next)
                    || self.occupied.contains_key(&point)
                    || !grid.walkable(next)
                    || !corner_open(&grid, cell, step, rule)
                {
                    continue;
                }
                self.visited.insert(next, cell);
                self.queue.push_back(next);
            }
        }
        let Some(mut cell) = found else {
            return false;
        };
        let agent = &mut self.agents[index as usize];
        agent.route.clear();
        let steps = agent.route.steps_mut();
        loop {
            steps.push(NavStep {
                point: NavPoint::new(start.layer, cell),
                link: None,
            });
            if cell == start.cell {
                break;
            }
            cell = self.visited[&cell];
        }
        steps.reverse();
        agent.route.set_revision(topology.revision());
        agent.at = 0;
        true
    }
}
