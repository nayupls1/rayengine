//! Two agents walk to a shared workstation, work and release it.
//! Run with `cargo run -p rayengine-actions --example interaction -- --cancel`.
use rayengine_actions::{
    Action, ActionContext, ActionPoll, ActionState, ActionSystem, CancelReason, Reservation,
    ReserveError, TargetId,
};
use std::{
    io::{self, Write},
    num::{NonZeroU32, NonZeroUsize},
};

#[derive(Default)]
struct World {
    // Positions and activity effects belong to the game.
    positions: [u32; 2],
    products: u32,
}

struct Work {
    worker: usize,
    target: TargetId,
    claim: Option<Reservation>,
    work_ticks: u32,
}
impl Action<World> for Work {
    fn tick(&mut self, world: &mut World, context: &mut ActionContext<'_>) -> ActionPoll {
        if self.claim.is_none() {
            match context.reserve(self.target) {
                Ok(claim) => {
                    println!("worker {} claimed workstation", self.worker);
                    self.claim = Some(claim);
                }
                Err(ReserveError::Busy) => return ActionPoll::Continue,
                Err(error) => return ActionPoll::Failed(format!("reservation: {error:?}")),
            }
        }
        if world.positions[self.worker] < 3 {
            world.positions[self.worker] += 1;
            println!("worker {} walking", self.worker);
            return ActionPoll::Continue;
        }
        self.work_ticks += 1;
        println!("worker {} working", self.worker);
        if self.work_ticks == 2 {
            world.products += 1;
            ActionPoll::Succeeded
        } else {
            ActionPoll::Continue
        }
    }

    fn finish(&mut self, world: &mut World, context: &mut ActionContext<'_>, state: &ActionState) {
        // Game cleanup runs while the claim is still valid. The scheduler also
        // releases all claims after this hook, even if the game omits release.
        if let Some(claim) = self.claim.take() {
            assert!(context.release(claim));
            println!("worker {} released workstation", self.worker);
        }
        world.positions[self.worker] = 0;
        println!("worker {} finished: {state:?}", self.worker);
    }
}

fn main() {
    let cancel = std::env::args().any(|arg| arg == "--cancel");
    let interactive = std::env::args().any(|arg| arg == "--interactive");
    let mut world = World::default();
    let mut actions = ActionSystem::new();
    let workstation = actions.add_target(NonZeroUsize::new(1).unwrap());
    let agents = [actions.add_agent(), actions.add_agent()];
    let mut jobs = Vec::new();
    for (worker, agent) in agents.into_iter().enumerate() {
        jobs.push(
            actions
                .enqueue(
                    agent,
                    Work {
                        worker,
                        target: workstation,
                        claim: None,
                        work_ticks: 0,
                    },
                    NonZeroU32::new(20).unwrap(),
                )
                .unwrap(),
        );
    }
    for tick in 0..20 {
        let mut player_cancel = false;
        if interactive {
            print!("tick {tick}: Enter = advance, c = cancel worker 0, q = quit: ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).unwrap() == 0 || input.trim() == "q" {
                break;
            }
            player_cancel = input.trim() == "c";
        }
        if player_cancel || (cancel && tick == 3) {
            // A game's input handler calls this when the player presses cancel.
            actions.cancel(jobs[0], CancelReason::User, &mut world);
        }
        actions.step(&mut world);
        assert!(actions.reserved_count(workstation).unwrap() <= 1);
    }
    if !interactive {
        assert_eq!(actions.reserved_count(workstation), Some(0));
    }
    actions.shutdown(&mut world);
    let outcomes: Vec<_> = actions.drain_finished().collect();
    if interactive {
        println!(
            "produced {} items; {} actions finished",
            world.products,
            outcomes.len()
        );
        return;
    }
    assert_eq!(outcomes.len(), 2);
    assert_eq!(world.products, if cancel { 1 } else { 2 });
    assert_eq!(actions.reserved_count(workstation), None);
    if cancel {
        assert_eq!(
            outcomes[0].state,
            ActionState::Cancelled(CancelReason::User)
        );
    } else {
        assert!(outcomes.iter().all(|o| o.state == ActionState::Succeeded));
    }
}
