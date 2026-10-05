#![doc = include_str!("../README.md")]

use std::{
    collections::{BTreeMap, VecDeque},
    num::{NonZeroU32, NonZeroUsize},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::atomic::{AtomicU64, Ordering},
};

// Process-wide IDs prevent stale tokens, including tokens from another system,
// from ever addressing a new owner. Exhaustion fails before reusing an ID.
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut id = NEXT.load(Ordering::Relaxed);
    loop {
        let next = id
            .checked_add(1)
            .expect("action identifier space exhausted");
        match NEXT.compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return id,
            Err(current) => id = current,
        }
    }
}

macro_rules! identifier {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(u64);
    };
}
identifier!(
    AgentId,
    "Opaque agent identity; never reused within this process."
);
identifier!(
    ActionId,
    "Opaque action identity; never reused within this process."
);
identifier!(
    TargetId,
    "Opaque resource identity; never reused within this process."
);

/// Copyable proof of one action's ownership of one target slot.
/// Copies do not create more claims. Tokens are runtime-only, not save data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reservation {
    id: u64,
    agent: AgentId,
    action: ActionId,
    target: TargetId,
}
impl Reservation {
    /// Agent that owns this claim.
    pub fn agent(self) -> AgentId {
        self.agent
    }
    /// Action whose terminal transition releases this claim.
    pub fn action(self) -> ActionId {
        self.action
    }
    /// Reserved resource.
    pub fn target(self) -> TargetId {
        self.target
    }
}

/// Immediate reservation result. Busy calls allocate nothing and never block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReserveError {
    /// All slots belong to other actions; retry within the action's tick budget.
    Busy,
    /// The target was removed or belongs to a different system.
    UnknownTarget,
    /// Finish hooks can release existing claims but cannot acquire new ones.
    Finishing,
}

/// Reason for game-requested or structural cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelReason {
    /// Player or game explicitly cancelled the activity.
    User,
    /// A replacement action interrupted this one.
    Interrupted,
    /// The owning agent was removed.
    AgentRemoved,
    /// The game shut down the scheduler.
    Shutdown,
}

/// Reason an action failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// A game hook returned an error message.
    Game(String),
    /// The action exhausted its start/tick call budget.
    TickBudgetExceeded,
    /// A resource claimed by this action was removed.
    TargetRemoved(TargetId),
    /// A game hook panicked; cleanup ran before the panic resumed.
    HookPanicked,
}

/// Observable lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionState {
    /// Queued; start has not run yet.
    Pending,
    /// Start ran; the action is advancing or waiting for a slot.
    Running,
    /// The game reported successful completion.
    Succeeded,
    /// The game or scheduler reported failure.
    Failed(Failure),
    /// Explicitly cancelled, including pending activities.
    Cancelled(CancelReason),
}

/// Result of a start or tick hook.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionPoll {
    /// Remain running until a later scheduler step.
    Continue,
    /// Complete successfully now.
    Succeeded,
    /// Complete with a game-owned failure message now.
    Failed(String),
}

/// Game-defined activity. All hooks execute synchronously on the caller's thread.
/// Finish runs exactly once on every terminal path, even for never-started jobs.
pub trait Action<W> {
    /// Called once at the queue head. This call consumes one budget tick.
    fn start(&mut self, _world: &mut W, _context: &mut ActionContext<'_>) -> ActionPoll {
        ActionPoll::Continue
    }
    /// Called on subsequent steps while running; one call consumes one budget tick.
    fn tick(&mut self, world: &mut W, context: &mut ActionContext<'_>) -> ActionPoll;
    /// Undo game-owned transient effects. Claims remain valid until this returns.
    /// Pending cancellations also call this hook; inspect `state` and game state.
    fn finish(&mut self, _world: &mut W, _context: &mut ActionContext<'_>, _state: &ActionState) {}
}

struct Target {
    capacity: usize,
    claims: BTreeMap<ActionId, Reservation>,
}
#[derive(Default)]
struct Reservations {
    targets: BTreeMap<TargetId, Target>,
}
impl Reservations {
    fn contains(&self, token: Reservation) -> bool {
        self.targets
            .get(&token.target)
            .and_then(|target| target.claims.get(&token.action))
            == Some(&token)
    }
    fn release(&mut self, token: Reservation) -> bool {
        if !self.contains(token) {
            return false;
        }
        self.targets
            .get_mut(&token.target)
            .unwrap()
            .claims
            .remove(&token.action);
        true
    }
    fn release_action(&mut self, action: ActionId) {
        for target in self.targets.values_mut() {
            target.claims.remove(&action);
        }
    }
}

/// Action-scoped reservation access supplied to game hooks.
/// Ownership is assigned by the scheduler, never supplied by game code.
pub struct ActionContext<'a> {
    agent: AgentId,
    action: ActionId,
    reservations: &'a mut Reservations,
    finishing: bool,
}
impl ActionContext<'_> {
    /// Agent being advanced or cleaned up.
    pub fn agent(&self) -> AgentId {
        self.agent
    }
    /// Action being advanced or cleaned up.
    pub fn action(&self) -> ActionId {
        self.action
    }
    /// Immediately claims one slot, or returns a nonblocking error.
    /// Repeated calls for the same action/target return the same token.
    pub fn reserve(&mut self, target: TargetId) -> Result<Reservation, ReserveError> {
        if self.finishing {
            return Err(ReserveError::Finishing);
        }
        let resource = self
            .reservations
            .targets
            .get_mut(&target)
            .ok_or(ReserveError::UnknownTarget)?;
        if let Some(token) = resource.claims.get(&self.action) {
            return Ok(*token);
        }
        if resource.claims.len() == resource.capacity {
            return Err(ReserveError::Busy);
        }
        let token = Reservation {
            id: next_id(),
            agent: self.agent,
            action: self.action,
            target,
        };
        resource.claims.insert(self.action, token);
        Ok(token)
    }
    /// Releases only this action's live token; stale/foreign copies are harmless.
    pub fn release(&mut self, token: Reservation) -> bool {
        token.action == self.action && self.reservations.release(token)
    }
    /// Whether a token still identifies a live claim in this system.
    pub fn is_reserved(&self, token: Reservation) -> bool {
        self.reservations.contains(token)
    }
}

struct Entry<W> {
    agent: AgentId,
    action: Box<dyn Action<W>>,
    state: ActionState,
    remaining: u32,
}

/// Retained terminal result. Drain results regularly to bound history memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionOutcome {
    /// Finished action.
    pub action: ActionId,
    /// Its owning agent, possibly already removed.
    pub agent: AgentId,
    /// Succeeded, failed or cancelled.
    pub state: ActionState,
}

/// An enqueue/interrupt used an agent removed or created by another system.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownAgent;

/// CPU-only FIFO action scheduler and reservation owner.
/// Games explicitly step, cancel, remove entities and shut down with their world.
/// Dropping releases Rust storage but cannot invoke hooks without the game world.
pub struct ActionSystem<W> {
    agents: BTreeMap<AgentId, VecDeque<ActionId>>,
    actions: BTreeMap<ActionId, Entry<W>>,
    reservations: Reservations,
    finished: VecDeque<ActionOutcome>,
}
impl<W> Default for ActionSystem<W> {
    fn default() -> Self {
        Self {
            agents: BTreeMap::new(),
            actions: BTreeMap::new(),
            reservations: Reservations::default(),
            finished: VecDeque::new(),
        }
    }
}
impl<W> ActionSystem<W> {
    /// Creates an empty scheduler with no engine/backend dependencies.
    pub fn new() -> Self {
        Self::default()
    }
    /// Adds an agent. Games maintain the mapping to their own entity/save IDs.
    pub fn add_agent(&mut self) -> AgentId {
        let id = AgentId(next_id());
        self.agents.insert(id, VecDeque::new());
        id
    }
    /// Adds an exclusive (`capacity = 1`) or capacity-limited interaction target.
    pub fn add_target(&mut self, capacity: NonZeroUsize) -> TargetId {
        let id = TargetId(next_id());
        self.reservations.targets.insert(
            id,
            Target {
                capacity: capacity.get(),
                claims: BTreeMap::new(),
            },
        );
        id
    }
    /// Appends a pending action. The nonzero budget includes start and tick calls,
    /// including busy waits, but excludes time spent pending behind another job.
    pub fn enqueue<A: Action<W> + 'static>(
        &mut self,
        agent: AgentId,
        action: A,
        budget: NonZeroU32,
    ) -> Result<ActionId, UnknownAgent> {
        let queue = self.agents.get_mut(&agent).ok_or(UnknownAgent)?;
        let id = ActionId(next_id());
        queue.push_back(id);
        self.actions.insert(
            id,
            Entry {
                agent,
                action: Box::new(action),
                state: ActionState::Pending,
                remaining: budget.get(),
            },
        );
        Ok(id)
    }
    /// Starts or ticks at most one action per agent, in agent creation order.
    /// Completion never starts the next queued action in this same step.
    pub fn step(&mut self, world: &mut W) {
        let heads: Vec<_> = self
            .agents
            .values()
            .filter_map(|q| q.front().copied())
            .collect();
        for id in heads {
            let mut entry = self.actions.remove(&id).unwrap();
            let starting = entry.state == ActionState::Pending;
            entry.state = ActionState::Running;
            let mut context = ActionContext {
                agent: entry.agent,
                action: id,
                reservations: &mut self.reservations,
                finishing: false,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                if starting {
                    entry.action.start(world, &mut context)
                } else {
                    entry.action.tick(world, &mut context)
                }
            }));
            entry.remaining -= 1;
            let state = match result {
                Ok(ActionPoll::Continue) if entry.remaining > 0 => {
                    self.actions.insert(id, entry);
                    continue;
                }
                Ok(ActionPoll::Continue) => ActionState::Failed(Failure::TickBudgetExceeded),
                Ok(ActionPoll::Succeeded) => ActionState::Succeeded,
                Ok(ActionPoll::Failed(message)) => ActionState::Failed(Failure::Game(message)),
                Err(panic) => {
                    // Retain the original panic even if finish also panics.
                    let _ = self.finish_entry(
                        id,
                        entry,
                        ActionState::Failed(Failure::HookPanicked),
                        world,
                    );
                    resume_unwind(panic);
                }
            };
            if let Some(panic) = self.finish_entry(id, entry, state, world) {
                resume_unwind(panic);
            }
        }
    }
    fn finish_entry(
        &mut self,
        id: ActionId,
        mut entry: Entry<W>,
        state: ActionState,
        world: &mut W,
    ) -> Option<Box<dyn std::any::Any + Send>> {
        let mut context = ActionContext {
            agent: entry.agent,
            action: id,
            reservations: &mut self.reservations,
            finishing: true,
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            entry.action.finish(world, &mut context, &state)
        }));
        self.reservations.release_action(id);
        self.agents
            .get_mut(&entry.agent)
            .unwrap()
            .retain(|queued| *queued != id);
        self.finished.push_back(ActionOutcome {
            action: id,
            agent: entry.agent,
            state: if result.is_err() {
                ActionState::Failed(Failure::HookPanicked)
            } else {
                state
            },
        });
        result.err()
    }
    /// Cancels a pending/running action immediately. Terminal/unknown IDs are no-ops.
    pub fn cancel(&mut self, id: ActionId, reason: CancelReason, world: &mut W) -> bool {
        let Some(entry) = self.actions.remove(&id) else {
            return false;
        };
        if let Some(panic) = self.finish_entry(id, entry, ActionState::Cancelled(reason), world) {
            resume_unwind(panic);
        }
        true
    }
    /// Cancels the queue head and puts a replacement first. Remaining jobs keep
    /// their order. The replacement starts on the next step, never in this call.
    pub fn interrupt<A: Action<W> + 'static>(
        &mut self,
        agent: AgentId,
        action: A,
        budget: NonZeroU32,
        world: &mut W,
    ) -> Result<ActionId, UnknownAgent> {
        let head = self
            .agents
            .get(&agent)
            .ok_or(UnknownAgent)?
            .front()
            .copied();
        if let Some(head) = head {
            self.cancel(head, CancelReason::Interrupted, world);
        }
        let id = self.enqueue(agent, action, budget)?;
        let queue = self.agents.get_mut(&agent).unwrap();
        queue.pop_back();
        queue.push_front(id);
        Ok(id)
    }
    /// Cancels every job and releases every claim for an agent. Call before
    /// removing its game entity so cleanup hooks can still access game data.
    pub fn remove_agent(&mut self, agent: AgentId, world: &mut W) -> bool {
        let Some(queue) = self.agents.get(&agent) else {
            return false;
        };
        let ids: Vec<_> = queue.iter().copied().collect();
        // Complete structural cleanup even if a finish hook panics.
        let mut first_panic = None;
        for id in ids {
            let entry = self.actions.remove(&id).unwrap();
            let panic = self.finish_entry(
                id,
                entry,
                ActionState::Cancelled(CancelReason::AgentRemoved),
                world,
            );
            first_panic = first_panic.or(panic);
        }
        self.agents.remove(&agent);
        if let Some(panic) = first_panic {
            resume_unwind(panic);
        }
        true
    }
    /// Fails every action claiming this target, releasing all of those actions'
    /// claims, then removes the target. Call before deleting its game data.
    /// Non-owning waiters observe UnknownTarget on their next reserve call.
    pub fn remove_target(&mut self, target: TargetId, world: &mut W) -> bool {
        let Some(resource) = self.reservations.targets.get(&target) else {
            return false;
        };
        let owners: Vec<_> = resource.claims.keys().copied().collect();
        let mut first_panic = None;
        for id in owners {
            let entry = self.actions.remove(&id).unwrap();
            let panic = self.finish_entry(
                id,
                entry,
                ActionState::Failed(Failure::TargetRemoved(target)),
                world,
            );
            first_panic = first_panic.or(panic);
        }
        self.reservations.targets.remove(&target);
        if let Some(panic) = first_panic {
            resume_unwind(panic);
        }
        true
    }
    /// Cancels all pending/running jobs and removes all agents/targets. Reusable
    /// afterward; retained outcomes remain available until drained.
    pub fn shutdown(&mut self, world: &mut W) {
        let ids: Vec<_> = self
            .agents
            .values()
            .flat_map(|q| q.iter().copied())
            .collect();
        let mut first_panic = None;
        for id in ids {
            let entry = self.actions.remove(&id).unwrap();
            let panic = self.finish_entry(
                id,
                entry,
                ActionState::Cancelled(CancelReason::Shutdown),
                world,
            );
            first_panic = first_panic.or(panic);
        }
        self.agents.clear();
        self.reservations.targets.clear();
        if let Some(panic) = first_panic {
            resume_unwind(panic);
        }
    }
    /// Live state or retained terminal state. Returns None after its outcome drains.
    pub fn state(&self, id: ActionId) -> Option<&ActionState> {
        self.actions.get(&id).map(|entry| &entry.state).or_else(|| {
            self.finished
                .iter()
                .find(|o| o.action == id)
                .map(|o| &o.state)
        })
    }
    /// Pending plus running jobs for a live agent; None for unknown/removed agents.
    pub fn queued_count(&self, agent: AgentId) -> Option<usize> {
        self.agents.get(&agent).map(VecDeque::len)
    }
    /// Number of occupied slots, or None for an unknown/removed target.
    pub fn reserved_count(&self, target: TargetId) -> Option<usize> {
        self.reservations
            .targets
            .get(&target)
            .map(|target| target.claims.len())
    }
    /// Tests token validity, including after cancellation/removal/reacquisition.
    pub fn is_reserved(&self, token: Reservation) -> bool {
        self.reservations.contains(token)
    }
    /// Drains retained outcomes in termination order. Dropping this iterator
    /// discards its remaining outcomes as with VecDeque::drain.
    pub fn drain_finished(&mut self) -> impl Iterator<Item = ActionOutcome> + '_ {
        self.finished.drain(..)
    }
}

#[cfg(test)]
mod tests;
