use super::geometry::{Shape, add, dot, overlap, scale, sub, sweep};
use super::grid::Grid;
use super::{
    BodyId, BodyKind, CollisionFilter, Contact, Events, StepReport, TriggerEvent, TriggerPhase,
};
use std::collections::BTreeSet;

pub(super) struct Node<const N: usize> {
    pub id: BodyId,
    pub position: [f32; N],
    pub shape: Shape<N>,
    pub velocity: [f32; N],
    pub kind: BodyKind,
    pub filter: CollisionFilter,
    pub trigger: bool,
    pub mass: f32,
    pub gravity: Option<[f32; N]>,
    pub restitution: f32,
    pub friction: f32,
    pub drag: f32,
    pub max_speed: Option<f32>,
    pub grounded: bool,
    pub was_grounded: bool,
    pub touched_ground: bool,
    pub carry: [f32; N],
}
impl<const N: usize> Node<N> {
    fn inverse_mass(&self) -> f64 {
        if self.kind == BodyKind::Dynamic && !self.trigger {
            1.0 / f64::from(self.mass)
        } else {
            0.0
        }
    }
    fn motion(&self) -> [f32; N] {
        if self.kind == BodyKind::Static {
            [0.0; N]
        } else {
            add(self.velocity, self.carry)
        }
    }
    fn cap_speed(&mut self) {
        if let Some(max) = self.max_speed {
            let speed = dot(self.velocity, self.velocity).sqrt();
            if speed > max {
                self.velocity = scale(self.velocity, max / speed);
            }
        }
    }
    fn validate(&self, dt: f32) {
        assert!(
            self.position
                .iter()
                .chain(self.velocity.iter())
                .all(|v| v.is_finite())
                && self.shape.valid()
                && self.mass.is_finite()
                && self.mass > 0.0
                && self.gravity.is_none_or(|g| g.iter().all(|v| v.is_finite()))
                && self.restitution.is_finite()
                && (0.0..=1.0).contains(&self.restitution)
                && self.friction.is_finite()
                && (0.0..=1.0).contains(&self.friction)
                && self.drag.is_finite()
                && self.drag >= 0.0
                && self.max_speed.is_none_or(|v| v.is_finite() && v >= 0.0),
            "invalid physics body {:?}",
            self.id
        );
        let end = add(self.position, scale(self.motion(), dt));
        let (min, max) = self.shape.bounds(end);
        assert!(
            min.iter().chain(max.iter()).all(|v| v.is_finite()),
            "physics movement overflow"
        );
    }
}

fn candidates<const N: usize>(
    nodes: &[Node<N>],
    grid: &mut Grid<N>,
    dt: f32,
    padding: f32,
) -> Vec<(usize, usize)> {
    grid.clear();
    let mut bounds = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let (lo, hi) = node.shape.bounds(node.position);
        let delta = scale(node.motion(), dt);
        let min = std::array::from_fn(|axis| lo[axis].min(lo[axis] + delta[axis]) - padding);
        let max = std::array::from_fn(|axis| hi[axis].max(hi[axis] + delta[axis]) + padding);
        grid.insert(i as u64, min, max);
        bounds.push((min, max));
    }
    let mut pairs = Vec::new();
    for (i, (min, max)) in bounds.into_iter().enumerate() {
        for j in grid.query(min, max) {
            let j = j as usize;
            if i < j && nodes[i].filter.allows(nodes[j].filter) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}
fn both_mut<T>(slice: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    let (left, right) = slice.split_at_mut(j);
    (&mut left[i], &mut right[0])
}
fn is_solid<const N: usize>(a: &Node<N>, b: &Node<N>) -> bool {
    !a.trigger && !b.trigger && (a.kind == BodyKind::Dynamic || b.kind == BodyKind::Dynamic)
}
fn up<const N: usize>() -> f32 {
    if N == 2 { -1.0 } else { 1.0 }
}
fn ground<const N: usize>(a: &mut Node<N>, b: &mut Node<N>, normal: [f32; N]) {
    a.touched_ground |= a.kind == BodyKind::Dynamic && normal[1] * up::<N>() > 0.5;
    b.touched_ground |= b.kind == BodyKind::Dynamic && normal[1] * up::<N>() < -0.5;
}
fn respond<const N: usize>(a: &mut Node<N>, b: &mut Node<N>, normal: [f32; N]) {
    ground(a, b, normal);
    let wa = a.inverse_mass();
    let wb = b.inverse_mass();
    let relative = sub(a.motion(), b.motion());
    let vn = dot(relative, normal);
    if vn >= 0.0 || wa + wb == 0.0 {
        return;
    }
    let ratio_a = (wa / (wa + wb)) as f32;
    let ratio_b = (wb / (wa + wb)) as f32;
    let impulse = -(1.0 + a.restitution.max(b.restitution)) * vn;
    a.velocity = add(a.velocity, scale(normal, impulse * ratio_a));
    b.velocity = sub(b.velocity, scale(normal, impulse * ratio_b));
    land_on_platform(a, b, normal);
    land_on_platform(b, a, scale(normal, -1.0));
    let relative = sub(a.motion(), b.motion());
    let tangent = sub(relative, scale(normal, dot(relative, normal)));
    let speed = dot(tangent, tangent).sqrt();
    if speed > 0.0 {
        let friction = speed.min(impulse * a.friction.max(b.friction));
        a.velocity = sub(a.velocity, scale(tangent, friction * ratio_a / speed));
        b.velocity = add(b.velocity, scale(tangent, friction * ratio_b / speed));
    }
}
// On landing, the normal impulse has supplied the platform's normal speed.
// Transfer that contribution to carry before storing intrinsic velocity, and
// add tangential carry immediately. A separating bounce remains detached.
fn land_on_platform<const N: usize>(rider: &mut Node<N>, platform: &Node<N>, normal: [f32; N]) {
    if rider.kind != BodyKind::Dynamic
        || platform.kind != BodyKind::Kinematic
        || normal[1] * up::<N>() <= 0.5
    {
        return;
    }
    if dot(sub(rider.motion(), platform.motion()), normal) > 0.00001 {
        // A rebound is airborne now. Preserve its world velocity across the
        // boundary instead of carrying this tick and losing momentum next tick.
        detach_carry(rider);
        return;
    }
    let carry = platform.motion();
    let transferred = dot(sub(carry, rider.carry), normal);
    rider.velocity = sub(rider.velocity, scale(normal, transferred));
    rider.carry = carry;
}

fn detach_carry<const N: usize>(node: &mut Node<N>) {
    node.velocity = add(node.velocity, node.carry);
    node.carry = [0.0; N];
}

fn separate<const N: usize>(a: &mut Node<N>, b: &mut Node<N>, contact: Contact<N>) {
    let wa = a.inverse_mass();
    let wb = b.inverse_mass();
    if wa + wb == 0.0 {
        return;
    }
    // Tiny outward skin protects subsequent sweeps from contact rounding.
    let amount = contact.depth + 1e-5;
    a.position =
        super::geometry::outward(a.position, contact.normal, amount * (wa / (wa + wb)) as f32);
    b.position = super::geometry::outward(
        b.position,
        scale(contact.normal, -1.0),
        amount * (wb / (wa + wb)) as f32,
    );
    respond(a, b, contact.normal);
}

// Propagate resting normal constraints through short touching chains. This is
// bounded arcade push-apart, not a stacking solver. Without it, repeatedly
// pushing three boxes against a wall creates hundreds of zero-time impacts.
fn settle_contacts<const N: usize>(nodes: &mut [Node<N>], grid: &mut Grid<N>) {
    let pairs = candidates(nodes, grid, 0.0, 0.00003);
    for _ in 0..32 {
        let mut changed = false;
        for &(i, j) in &pairs {
            let (a, b) = both_mut(nodes, i, j);
            if !is_solid(a, b) || a.restitution.max(b.restitution) > 0.0 {
                continue;
            }
            let relative = sub(a.motion(), b.motion());
            let speed = dot(relative, relative).sqrt();
            if speed <= 0.00001 {
                continue;
            }
            let probe = scale(relative, 0.00003 / speed);
            let normal = overlap(a.shape, a.position, b.shape, b.position)
                .map(|c| c.normal)
                .or_else(|| sweep(a.shape, a.position, b.shape, b.position, probe).map(|(_, n)| n));
            let Some(normal) = normal else {
                continue;
            };
            let inward = dot(relative, normal);
            if inward >= -0.00001 {
                continue;
            }
            respond(a, b, normal);
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

fn trigger_pair<const N: usize>(a: &Node<N>, b: &Node<N>, set: &mut BTreeSet<(BodyId, BodyId)>) {
    if a.trigger {
        set.insert((a.id, b.id));
    }
    if b.trigger {
        set.insert((b.id, a.id));
    }
}
fn record_triggers<const N: usize>(
    nodes: &[Node<N>],
    grid: &mut Grid<N>,
    dt: f32,
    crossed: &mut BTreeSet<(BodyId, BodyId)>,
) {
    for (i, j) in candidates(nodes, grid, dt, 0.0) {
        let a = &nodes[i];
        let b = &nodes[j];
        if !(a.trigger || b.trigger) {
            continue;
        }
        if overlap(a.shape, a.position, b.shape, b.position).is_some()
            || sweep(
                a.shape,
                a.position,
                b.shape,
                b.position,
                scale(sub(a.motion(), b.motion()), dt),
            )
            .is_some_and(|(t, _)| t < 1.0)
        {
            trigger_pair(a, b, crossed);
        }
    }
}

fn depenetrate<const N: usize>(nodes: &mut [Node<N>], grid: &mut Grid<N>) {
    for _ in 0..16 {
        let mut changed = false;
        for (i, j) in candidates(nodes, grid, 0.0, 0.0) {
            let (a, b) = both_mut(nodes, i, j);
            if is_solid(a, b)
                && let Some(contact) = overlap(a.shape, a.position, b.shape, b.position)
            {
                separate(a, b, contact);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

pub(super) fn step<const N: usize>(
    nodes: &mut [Node<N>],
    dt: f32,
    grid: &mut Grid<N>,
    previous: &mut BTreeSet<(BodyId, BodyId)>,
    events: &mut Events<TriggerEvent>,
) -> StepReport {
    assert!(dt.is_finite() && dt >= 0.0, "invalid physics timestep");
    for node in nodes.iter() {
        node.validate(dt);
    }
    let mut report = StepReport::default();
    let mut crossed = BTreeSet::new();
    record_triggers(nodes, grid, 0.0, &mut crossed);
    depenetrate(nodes, grid);
    // Refresh supports each tick from geometry, so removal, layer changes,
    // horizontal walk-off and jumping never leave a stale platform attachment.
    for (i, j) in candidates(nodes, grid, 0.0, support_padding(nodes)) {
        let (a, b) = both_mut(nodes, i, j);
        if !is_solid(a, b) {
            continue;
        }
        set_support(a, b);
        set_support(b, a);
    }
    for node in nodes.iter_mut() {
        if node.kind == BodyKind::Dynamic {
            if let Some(g) = node.gravity {
                node.velocity = add(node.velocity, scale(g, dt));
            }
            node.velocity = scale(node.velocity, 1.0 / (1.0 + node.drag * dt));
            node.cap_speed();
        }
        node.validate(dt);
    }
    settle_contacts(nodes, grid);
    let mut remaining = dt;
    while remaining > 0.0 && report.contacts < 256 {
        let mut earliest: Option<(f32, usize, usize, [f32; N])> = None;
        let mut repaired = false;
        for (i, j) in candidates(nodes, grid, remaining, 0.0) {
            let (a, b) = both_mut(nodes, i, j);
            if is_solid(a, b)
                && let Some(c) = overlap(a.shape, a.position, b.shape, b.position)
            {
                separate(a, b, c);
                repaired = true;
                report.contacts += 1;
                if report.contacts == 256 {
                    break;
                }
                continue;
            }
            if !is_solid(a, b) {
                continue;
            }
            let delta = scale(sub(a.motion(), b.motion()), remaining);
            if let Some((time, normal)) = sweep(a.shape, a.position, b.shape, b.position, delta)
                && earliest.is_none_or(|(best, _, _, _)| time < best)
            {
                earliest = Some((time, i, j, normal));
            }
        }
        // Simultaneous contacts can round another pair just inside its boundary.
        // Repair those before rebuilding sweeps from the new velocities/bounds.
        if repaired {
            continue;
        }
        let fraction = earliest.map_or(1.0, |(t, _, _, _)| t);
        let advance = remaining * fraction;
        record_triggers(nodes, grid, advance, &mut crossed);
        for node in nodes.iter_mut() {
            node.position = add(node.position, scale(node.motion(), advance));
        }
        remaining = (remaining - advance).max(0.0);
        let Some((_, i, j, normal)) = earliest else {
            break;
        };
        let (a, b) = both_mut(nodes, i, j);
        // Remove floating-point overlap at the swept boundary before response.
        if let Some(c) = overlap(a.shape, a.position, b.shape, b.position) {
            separate(a, b, c);
        } else {
            separate(a, b, Contact { normal, depth: 0.0 });
        }
        report.contacts += 1;
        settle_contacts(nodes, grid);
    }
    if report.contacts == 256 {
        report.dropped_time = remaining;
    }
    // Clean up contact rounding at the final boundary as well as at spawn.
    record_triggers(nodes, grid, 0.0, &mut crossed);
    depenetrate(nodes, grid);
    for node in nodes.iter_mut() {
        node.grounded = false;
    }
    let mut current = BTreeSet::new();
    let mut supported = BTreeSet::new();
    for (i, j) in candidates(nodes, grid, 0.0, support_padding(nodes)) {
        let (a, b) = both_mut(nodes, i, j);
        let contact = overlap(a.shape, a.position, b.shape, b.position);
        if a.trigger || b.trigger {
            if contact.is_some() {
                trigger_pair(a, b, &mut current);
            }
        } else if is_solid(a, b) {
            report.unresolved_overlaps |= contact.is_some();
            mark_support(a, b, &mut supported);
            mark_support(b, a, &mut supported);
        }
    }
    // Carry is an integration contribution only while supported. A wall/floor
    // can cancel it through the normal impulse, leaving an opposing intrinsic
    // velocity. Convert to world velocity on detachment before the next tick
    // initializes carry to zero, avoiding a spurious rebound on static landings.
    for node in nodes.iter_mut() {
        if !supported.contains(&node.id) {
            detach_carry(node);
        }
    }
    let all: BTreeSet<_> = previous
        .union(&current)
        .copied()
        .chain(crossed.iter().copied())
        .collect();
    for pair @ (trigger, other) in all {
        let before = previous.contains(&pair);
        let after = current.contains(&pair);
        if before {
            events.send(TriggerEvent {
                trigger,
                other,
                phase: if after {
                    TriggerPhase::Stay
                } else {
                    TriggerPhase::Exit
                },
            });
        } else if after || crossed.contains(&pair) {
            events.send(TriggerEvent {
                trigger,
                other,
                phase: TriggerPhase::Enter,
            });
            if !after {
                events.send(TriggerEvent {
                    trigger,
                    other,
                    phase: TriggerPhase::Exit,
                });
            }
        }
    }
    *previous = current;
    report
}

// Use the actual contact skin, enlarged only to cover coordinate precision.
// A wide fixed proximity probe can mistake a tiny bounce's apex for resting
// support and repeatedly add carry that was already inherited on rebound.
fn coordinate_ulp(value: f32) -> f32 {
    let value = value.abs();
    let upper = value.next_up();
    if upper.is_finite() {
        upper - value
    } else {
        value - value.next_down()
    }
}
fn support_padding<const N: usize>(nodes: &[Node<N>]) -> f32 {
    nodes
        .iter()
        .map(|n| coordinate_ulp(n.position[1]) * 2.0)
        .fold(0.00003, f32::max)
}
fn support_probe<const N: usize>(a: &Node<N>, b: &Node<N>) -> [f32; N] {
    let margin = 0.00003_f32
        .max(coordinate_ulp(a.position[1]) * 2.0)
        .max(coordinate_ulp(b.position[1]) * 2.0);
    std::array::from_fn(|axis| if axis == 1 { -up::<N>() * margin } else { 0.0 })
}

fn mark_support<const N: usize>(
    rider: &mut Node<N>,
    other: &Node<N>,
    supported: &mut BTreeSet<BodyId>,
) {
    if rider.kind != BodyKind::Dynamic {
        return;
    }
    if let Some((time, normal)) = sweep(
        rider.shape,
        rider.position,
        other.shape,
        other.position,
        support_probe(rider, other),
    ) && normal[1] * up::<N>() > 0.5
        && (rider.was_grounded || rider.touched_ground || time == 0.0)
        && dot(sub(rider.motion(), other.motion()), normal) <= 0.001
    {
        rider.grounded = true;
        if other.kind == BodyKind::Kinematic {
            supported.insert(rider.id);
        }
    }
}

fn set_support<const N: usize>(rider: &mut Node<N>, platform: &Node<N>) {
    if rider.kind != BodyKind::Dynamic || platform.kind != BodyKind::Kinematic {
        return;
    }
    // Only resting intrinsic normal motion retains preliminary carry. Incoming
    // airborne bodies land through CCD, and jumps/rebounds remain detached.
    if rider.velocity[1].abs() > 0.001 {
        return;
    }
    let probe = support_probe(rider, platform);
    if sweep(
        rider.shape,
        rider.position,
        platform.shape,
        platform.position,
        probe,
    )
    .is_some_and(|(time, n)| n[1] * up::<N>() > 0.5 && (rider.was_grounded || time == 0.0))
    {
        rider.carry = platform.motion();
        rider.touched_ground = true;
    }
}
