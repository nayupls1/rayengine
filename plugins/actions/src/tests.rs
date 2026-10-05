use super::*;

fn budget(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}
fn capacity(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[derive(Default)]
struct World {
    events: Vec<(&'static str, ActionId)>,
    claims: Vec<Reservation>,
    finishes: Vec<(ActionId, ActionState)>,
}

struct Job {
    targets: Vec<TargetId>,
    start_result: ActionPoll,
    tick_result: ActionPoll,
}
impl Job {
    fn holding(targets: Vec<TargetId>) -> Self {
        Self {
            targets,
            start_result: ActionPoll::Continue,
            tick_result: ActionPoll::Continue,
        }
    }
    fn complete() -> Self {
        Self {
            targets: vec![],
            start_result: ActionPoll::Continue,
            tick_result: ActionPoll::Succeeded,
        }
    }
    fn acquire(&self, world: &mut World, context: &mut ActionContext<'_>) -> bool {
        for &target in &self.targets {
            match context.reserve(target) {
                Ok(token) => {
                    assert_eq!(token.agent(), context.agent());
                    assert_eq!(token.action(), context.action());
                    assert_eq!(token.target(), target);
                    world.claims.push(token);
                }
                Err(ReserveError::Busy) => return false,
                error => panic!("unexpected reserve result {error:?}"),
            }
        }
        true
    }
}
impl Action<World> for Job {
    fn start(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
        world.events.push(("start", context.action()));
        if self.acquire(world, context) {
            self.start_result.clone()
        } else {
            ActionPoll::Continue
        }
    }
    fn tick(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
        world.events.push(("tick", context.action()));
        if self.acquire(world, context) {
            self.tick_result.clone()
        } else {
            ActionPoll::Continue
        }
    }
    fn finish(&mut self, world: &mut World, context: &mut ActionContext<'_>, state: &ActionState) {
        world.events.push(("finish", context.action()));
        let action_id = context.action();
        for token in world.claims.iter().filter(|t| t.action() == action_id) {
            assert!(context.is_reserved(*token));
            assert_eq!(
                context.reserve(token.target()),
                Err(ReserveError::Finishing)
            );
        }
        // Deliberately omit release to test scheduler-owned cleanup.
        world.finishes.push((context.action(), state.clone()));
    }
}

#[test]
fn fifo_lifecycle_finishes_once_and_never_advances_the_next_job_in_the_same_step() {
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let agent = system.add_agent();
    let a = system.enqueue(agent, Job::complete(), budget(2)).unwrap();
    let b = system.enqueue(agent, Job::complete(), budget(2)).unwrap();
    assert_eq!(system.state(a), Some(&ActionState::Pending));
    assert_eq!(system.state(b), Some(&ActionState::Pending));
    system.step(&mut world);
    assert_eq!(system.state(a), Some(&ActionState::Running));
    assert_eq!(system.state(b), Some(&ActionState::Pending));
    system.step(&mut world);
    assert_eq!(system.state(a), Some(&ActionState::Succeeded));
    assert_eq!(system.state(b), Some(&ActionState::Pending));
    assert_eq!(system.queued_count(agent), Some(1));
    system.step(&mut world);
    system.step(&mut world);
    system.step(&mut world);
    assert_eq!(
        world.events,
        vec![
            ("start", a),
            ("tick", a),
            ("finish", a),
            ("start", b),
            ("tick", b),
            ("finish", b)
        ]
    );
    assert!(!system.cancel(a, CancelReason::User, &mut world));
    let outcomes: Vec<_> = system.drain_finished().collect();
    assert_eq!(
        outcomes.iter().map(|o| o.action).collect::<Vec<_>>(),
        vec![a, b]
    );
    assert!(
        outcomes
            .iter()
            .all(|o| o.agent == agent && o.state == ActionState::Succeeded)
    );
    assert_eq!(system.state(a), None);
    assert_eq!(system.queued_count(agent), Some(0));
}

#[test]
fn success_and_game_failure_release_every_claim_from_start_and_tick() {
    for at_start in [true, false] {
        for poll in [
            ActionPoll::Succeeded,
            ActionPoll::Failed("game failure".into()),
        ] {
            let expected = match &poll {
                ActionPoll::Succeeded => ActionState::Succeeded,
                ActionPoll::Failed(msg) => ActionState::Failed(Failure::Game(msg.clone())),
                _ => unreachable!(),
            };
            let mut system = ActionSystem::new();
            let mut world = World::default();
            let agent = system.add_agent();
            let targets = [
                system.add_target(capacity(1)),
                system.add_target(capacity(1)),
            ];
            let mut job = Job::holding(targets.to_vec());
            if at_start {
                job.start_result = poll;
            } else {
                job.tick_result = poll;
            }
            let id = system.enqueue(agent, job, budget(2)).unwrap();
            system.step(&mut world);
            if !at_start {
                system.step(&mut world);
            }
            assert_eq!(system.state(id), Some(&expected));
            assert_eq!(world.finishes, vec![(id, expected)]);
            assert!(targets.iter().all(|t| system.reserved_count(*t) == Some(0)));
            assert!(world.claims.iter().all(|t| !system.is_reserved(*t)));
        }
    }
}

#[test]
fn pending_and_running_cancellation_cleanup_is_immediate_and_idempotent() {
    for running in [true, false] {
        let mut system = ActionSystem::new();
        let mut world = World::default();
        let agent = system.add_agent();
        let target = system.add_target(capacity(1));
        let id = system
            .enqueue(agent, Job::holding(vec![target]), budget(10))
            .unwrap();
        if running {
            system.step(&mut world);
        }
        assert!(system.cancel(id, CancelReason::User, &mut world));
        assert!(!system.cancel(id, CancelReason::User, &mut world));
        assert_eq!(
            system.state(id),
            Some(&ActionState::Cancelled(CancelReason::User))
        );
        assert_eq!(world.finishes.len(), 1);
        assert_eq!(system.reserved_count(target), Some(0));
        assert_eq!(system.queued_count(agent), Some(0));
        if !running {
            assert_eq!(world.events, vec![("finish", id)]);
        }
    }
}

#[test]
fn interrupt_releases_current_claim_and_preserves_the_remaining_queue() {
    for running in [true, false] {
        let mut system = ActionSystem::new();
        let mut world = World::default();
        let agent = system.add_agent();
        let target = system.add_target(capacity(1));
        let a = system
            .enqueue(agent, Job::holding(vec![target]), budget(10))
            .unwrap();
        let b = system.enqueue(agent, Job::complete(), budget(2)).unwrap();
        if running {
            system.step(&mut world);
        }
        let replacement = system
            .interrupt(agent, Job::complete(), budget(2), &mut world)
            .unwrap();
        assert_eq!(
            system.state(a),
            Some(&ActionState::Cancelled(CancelReason::Interrupted))
        );
        assert_eq!(system.reserved_count(target), Some(0));
        assert_eq!(system.state(replacement), Some(&ActionState::Pending));
        for _ in 0..4 {
            system.step(&mut world);
        }
        assert_eq!(
            world.finishes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![a, replacement, b]
        );
        let c = system
            .interrupt(agent, Job::complete(), budget(2), &mut world)
            .unwrap();
        system.step(&mut world);
        assert_eq!(system.state(c), Some(&ActionState::Running));
    }
}

#[test]
fn two_agents_compete_without_overlap_and_waiter_acquires_on_release() {
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let target = system.add_target(capacity(1));
    let a = system.add_agent();
    let b = system.add_agent();
    let first = system
        .enqueue(a, Job::holding(vec![target]), budget(10))
        .unwrap();
    let second = system
        .enqueue(b, Job::holding(vec![target]), budget(10))
        .unwrap();
    for _ in 0..3 {
        system.step(&mut world);
        assert_eq!(system.reserved_count(target), Some(1));
        assert!(world.claims.iter().all(|token| token.action() == first));
    }
    let old_claim = world.claims[0];
    assert!(world.claims.iter().all(|token| *token == old_claim)); // Idempotent reserve.
    system.cancel(first, CancelReason::User, &mut world);
    assert!(!system.is_reserved(old_claim));
    system.step(&mut world);
    assert_eq!(system.reserved_count(target), Some(1));
    let new_claim = *world.claims.last().unwrap();
    assert_eq!(new_claim.action(), second);
    assert!(system.is_reserved(new_claim));
    system.cancel(second, CancelReason::User, &mut world);
    assert_eq!(system.reserved_count(target), Some(0));
}

#[test]
fn busy_wait_and_unproductive_actions_fail_at_the_required_budget() {
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let target = system.add_target(capacity(1));
    let a = system.add_agent();
    let b = system.add_agent();
    let owner = system
        .enqueue(a, Job::holding(vec![target]), budget(3))
        .unwrap();
    let waiter = system
        .enqueue(b, Job::holding(vec![target]), budget(2))
        .unwrap();
    system.step(&mut world);
    system.step(&mut world);
    assert_eq!(
        system.state(waiter),
        Some(&ActionState::Failed(Failure::TickBudgetExceeded))
    );
    assert_eq!(system.reserved_count(target), Some(1));
    system.step(&mut world);
    assert_eq!(
        system.state(owner),
        Some(&ActionState::Failed(Failure::TickBudgetExceeded))
    );
    assert_eq!(system.reserved_count(target), Some(0));
    let one_tick = system
        .enqueue(a, Job::holding(vec![target]), budget(1))
        .unwrap();
    system.step(&mut world);
    assert_eq!(
        system.state(one_tick),
        Some(&ActionState::Failed(Failure::TickBudgetExceeded))
    );
    assert_eq!(system.reserved_count(target), Some(0));
}

#[test]
fn capacity_removal_fails_all_owners_and_releases_their_other_targets() {
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let shared = system.add_target(capacity(2));
    let other = system.add_target(capacity(2));
    let agents = [system.add_agent(), system.add_agent(), system.add_agent()];
    let jobs: Vec<_> = agents
        .into_iter()
        .map(|agent| {
            system
                .enqueue(agent, Job::holding(vec![shared, other]), budget(10))
                .unwrap()
        })
        .collect();
    system.step(&mut world);
    assert_eq!(system.reserved_count(shared), Some(2));
    assert_eq!(system.reserved_count(other), Some(2));
    assert_eq!(world.claims.len(), 4);
    assert!(system.remove_target(shared, &mut world));
    assert!(!system.remove_target(shared, &mut world));
    assert_eq!(system.reserved_count(shared), None);
    assert_eq!(system.reserved_count(other), Some(0));
    assert_eq!(
        system.state(jobs[0]),
        Some(&ActionState::Failed(Failure::TargetRemoved(shared)))
    );
    assert_eq!(
        system.state(jobs[1]),
        Some(&ActionState::Failed(Failure::TargetRemoved(shared)))
    );
    // Non-owning waiters remain running; their game code handles UnknownTarget.
    assert_eq!(system.state(jobs[2]), Some(&ActionState::Running));
    system.cancel(jobs[2], CancelReason::User, &mut world);
    assert!(world.claims.iter().all(|t| !system.is_reserved(*t)));
}

#[test]
fn agent_removal_cancels_entire_queue_without_affecting_other_agents() {
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let target = system.add_target(capacity(1));
    let a = system.add_agent();
    let b = system.add_agent();
    let running = system
        .enqueue(a, Job::holding(vec![target]), budget(10))
        .unwrap();
    let pending = system.enqueue(a, Job::complete(), budget(2)).unwrap();
    let other = system
        .enqueue(b, Job::holding(vec![target]), budget(10))
        .unwrap();
    system.step(&mut world);
    assert!(system.remove_agent(a, &mut world));
    assert!(!system.remove_agent(a, &mut world));
    assert_eq!(system.queued_count(a), None);
    for id in [running, pending] {
        assert_eq!(
            system.state(id),
            Some(&ActionState::Cancelled(CancelReason::AgentRemoved))
        );
    }
    assert_eq!(system.queued_count(b), Some(1));
    system.step(&mut world);
    assert_eq!(world.claims.last().unwrap().action(), other);
    assert_eq!(
        system.enqueue(a, Job::complete(), budget(2)),
        Err(UnknownAgent)
    );
    assert_eq!(
        system.interrupt(a, Job::complete(), budget(2), &mut world),
        Err(UnknownAgent)
    );
    system.shutdown(&mut world);
}

#[test]
fn shutdown_cleans_running_and_pending_jobs_and_system_can_be_reused() {
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let agent = system.add_agent();
    let target = system.add_target(capacity(1));
    let ids = [
        system
            .enqueue(agent, Job::holding(vec![target]), budget(10))
            .unwrap(),
        system.enqueue(agent, Job::complete(), budget(2)).unwrap(),
    ];
    system.step(&mut world);
    system.shutdown(&mut world);
    system.shutdown(&mut world);
    assert_eq!(world.finishes.len(), 2);
    for id in ids {
        assert_eq!(
            system.state(id),
            Some(&ActionState::Cancelled(CancelReason::Shutdown))
        );
    }
    assert_eq!(system.reserved_count(target), None);
    assert_eq!(system.queued_count(agent), None);
    let new_agent = system.add_agent();
    let new_target = system.add_target(capacity(1));
    assert_ne!(agent, new_agent);
    assert_ne!(target, new_target);
    system
        .enqueue(new_agent, Job::holding(vec![new_target]), budget(1))
        .unwrap();
    system.step(&mut world);
    assert_eq!(system.reserved_count(new_target), Some(0));
}

#[test]
fn stale_copies_cannot_release_reacquired_claims_and_foreign_action_cannot_release() {
    struct Reacquire(TargetId);
    impl Action<World> for Reacquire {
        fn tick(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
            let original = context.reserve(self.0).unwrap();
            let copy = original;
            assert!(context.release(original));
            assert!(!context.release(copy));
            let new = context.reserve(self.0).unwrap();
            assert_ne!(original, new);
            assert!(!context.release(copy));
            assert!(context.is_reserved(new));
            world.claims.push(new);
            ActionPoll::Continue
        }
    }
    struct Foreign(TargetId);
    impl Action<World> for Foreign {
        fn tick(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
            let foreign = world.claims[0];
            assert!(!context.release(foreign));
            assert!(context.is_reserved(foreign));
            assert_eq!(context.reserve(self.0), Err(ReserveError::Busy));
            ActionPoll::Succeeded
        }
    }
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let target = system.add_target(capacity(1));
    let a = system.add_agent();
    let b = system.add_agent();
    system.enqueue(a, Reacquire(target), budget(3)).unwrap();
    system.enqueue(b, Foreign(target), budget(2)).unwrap();
    system.step(&mut world);
    system.step(&mut world);
    assert_eq!(system.reserved_count(target), Some(1));
    system.shutdown(&mut world);
    assert!(!system.is_reserved(world.claims[0]));
}

#[test]
fn runtime_ids_and_tokens_are_isolated_between_systems() {
    struct Check(TargetId, Reservation);
    impl Action<World> for Check {
        fn start(&mut self, _world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
            assert_eq!(context.reserve(self.0), Err(ReserveError::UnknownTarget));
            assert!(!context.is_reserved(self.1));
            assert!(!context.release(self.1));
            ActionPoll::Succeeded
        }
        fn tick(&mut self, _: &mut World, _: &mut ActionContext<'_>) -> ActionPoll {
            unreachable!()
        }
    }
    let mut world = World::default();
    let mut first = ActionSystem::new();
    let a = first.add_agent();
    let t = first.add_target(capacity(1));
    first.enqueue(a, Job::holding(vec![t]), budget(5)).unwrap();
    first.step(&mut world);
    let token = world.claims[0];
    let mut second = ActionSystem::new();
    assert_eq!(
        second.enqueue(a, Job::complete(), budget(2)),
        Err(UnknownAgent)
    );
    let b = second.add_agent();
    let u = second.add_target(capacity(1));
    assert_ne!(a, b);
    assert_ne!(t, u);
    second.enqueue(b, Check(t, token), budget(1)).unwrap();
    second.step(&mut world);
    assert!(first.is_reserved(token));
    first.shutdown(&mut world);
}

struct PanicJob {
    target: TargetId,
    panic_start: bool,
    panic_tick: bool,
    panic_finish: bool,
}
impl Action<World> for PanicJob {
    fn start(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
        world.claims.push(context.reserve(self.target).unwrap());
        assert!(!self.panic_start, "start panic");
        ActionPoll::Continue
    }
    fn tick(&mut self, _: &mut World, _: &mut ActionContext<'_>) -> ActionPoll {
        assert!(!self.panic_tick, "tick panic");
        ActionPoll::Succeeded
    }
    fn finish(&mut self, world: &mut World, context: &mut ActionContext<'_>, state: &ActionState) {
        world.finishes.push((context.action(), state.clone()));
        assert!(!self.panic_finish, "finish panic");
    }
}

#[test]
fn hook_panics_release_claims_record_failure_and_resume_original_panic() {
    for (start, tick, finish) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, false, true),
    ] {
        let mut system = ActionSystem::new();
        let mut world = World::default();
        let agent = system.add_agent();
        let target = system.add_target(capacity(1));
        let id = system
            .enqueue(
                agent,
                PanicJob {
                    target,
                    panic_start: start,
                    panic_tick: tick,
                    panic_finish: finish,
                },
                budget(10),
            )
            .unwrap();
        if !start {
            system.step(&mut world);
        }
        let panic = catch_unwind(AssertUnwindSafe(|| system.step(&mut world))).unwrap_err();
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap();
        assert_eq!(
            message,
            if start {
                "start panic"
            } else if tick {
                "tick panic"
            } else {
                "finish panic"
            }
        );
        assert_eq!(system.reserved_count(target), Some(0));
        assert_eq!(system.queued_count(agent), Some(0));
        assert_eq!(world.finishes.len(), 1);
        assert_eq!(
            system.state(id),
            Some(&ActionState::Failed(Failure::HookPanicked))
        );
        system.step(&mut world);
    }
}

#[test]
fn bulk_cleanup_continues_after_finish_panics() {
    for operation in ["agent", "target", "shutdown"] {
        let mut system = ActionSystem::new();
        let mut world = World::default();
        let agent = system.add_agent();
        let target = system.add_target(capacity(2));
        let a = system
            .enqueue(
                agent,
                PanicJob {
                    target,
                    panic_start: false,
                    panic_tick: false,
                    panic_finish: true,
                },
                budget(10),
            )
            .unwrap();
        let b_agent = if operation == "agent" {
            agent
        } else {
            system.add_agent()
        };
        let b = system
            .enqueue(b_agent, Job::holding(vec![target]), budget(10))
            .unwrap();
        system.step(&mut world);
        assert!(
            catch_unwind(AssertUnwindSafe(|| match operation {
                "agent" => {
                    system.remove_agent(agent, &mut world);
                }
                "target" => {
                    system.remove_target(target, &mut world);
                }
                _ => system.shutdown(&mut world),
            }))
            .is_err()
        );
        assert_eq!(
            system.state(a),
            Some(&ActionState::Failed(Failure::HookPanicked))
        );
        assert_eq!(world.finishes.len(), 2);
        assert!(matches!(
            system.state(b),
            Some(ActionState::Cancelled(_)) | Some(ActionState::Failed(Failure::TargetRemoved(_)))
        ));
        assert!(world.claims.iter().all(|t| !system.is_reserved(*t)));
        assert_eq!(
            system.queued_count(agent),
            if operation == "target" { Some(0) } else { None }
        );
    }
}

#[test]
fn pending_and_running_waiters_fail_on_missing_target_without_claiming_its_replacement() {
    struct Seek(TargetId);
    impl Action<World> for Seek {
        fn start(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
            self.tick(world, context)
        }
        fn tick(&mut self, _: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
            match context.reserve(self.0) {
                Ok(_) => ActionPoll::Succeeded,
                Err(ReserveError::Busy) => ActionPoll::Continue,
                Err(ReserveError::UnknownTarget) => ActionPoll::Failed("target missing".into()),
                Err(ReserveError::Finishing) => unreachable!(),
            }
        }
    }
    let mut system = ActionSystem::new();
    let mut world = World::default();
    let owner = system.add_agent();
    let waiter = system.add_agent();
    let pending = system.add_agent();
    let target = system.add_target(capacity(1));
    system
        .enqueue(owner, Job::holding(vec![target]), budget(10))
        .unwrap();
    let running_id = system.enqueue(waiter, Seek(target), budget(10)).unwrap();
    system.step(&mut world);
    let pending_id = system.enqueue(pending, Seek(target), budget(10)).unwrap();
    system.remove_target(target, &mut world);
    let replacement = system.add_target(capacity(1));
    assert_ne!(target, replacement);
    system.step(&mut world);
    for id in [running_id, pending_id] {
        assert_eq!(
            system.state(id),
            Some(&ActionState::Failed(Failure::Game("target missing".into())))
        );
    }
    assert_eq!(system.reserved_count(replacement), Some(0));
}
