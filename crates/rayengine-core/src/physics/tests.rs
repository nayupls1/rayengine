use super::*;
use crate::collision::{Body2D, Body3D, Circle, Sphere};
fn tick(dt: f32) -> Tick {
    Tick { index: 0, dt }
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.003, "{a} != {b}");
}

macro_rules! scenarios {
    ($module:ident,$v:ident,$shape:ident,$body:ident,$world:ident,$n:literal,$up:expr) => {
        mod $module {
            use super::*;
            fn up() -> f32 {
                $up
            }
            fn x(value: f32) -> $v {
                let mut v = $v::ZERO;
                v.x = value;
                v
            }
            fn y(value: f32) -> $v {
                let mut v = $v::ZERO;
                v.y = value;
                v
            }
            fn shape(round: bool) -> $shape {
                if round {
                    $shape::round(1.0)
                } else {
                    $shape::box_shape($v::splat(2.0))
                }
            }
            fn body(p: $v, round: bool) -> $body {
                $body::new(p, shape(round))
            }
            #[test]
            fn every_shape_pair_reports_separating_penetration_and_tangency() {
                for a in [false, true] {
                    for b in [false, true] {
                        let c = shape(a).overlap(x(1.5), shape(b), $v::ZERO).unwrap();
                        near(c.depth, 0.5);
                        near(c.normal.x, 1.0);
                        assert!(
                            shape(a)
                                .overlap(x(1.5) + c.normal * (c.depth + 0.001), shape(b), $v::ZERO)
                                .is_none()
                        );
                        assert!(shape(a).overlap(x(2.0), shape(b), $v::ZERO).is_none());
                        let opposite = shape(b).overlap($v::ZERO, shape(a), x(1.5)).unwrap();
                        assert_eq!(opposite.normal, -c.normal);
                        assert_eq!(opposite.depth, c.depth);
                    }
                }
                // Closest point outside a box is its corner, not the expanded AABB.
                assert!(
                    shape(true)
                        .overlap($v::splat(1.8), shape(false), $v::ZERO)
                        .is_none()
                );
                let inside = shape(true)
                    .overlap($v::ZERO, shape(false), $v::ZERO)
                    .unwrap();
                near(inside.depth, 2.0);
                assert_eq!(inside.normal, x(1.0));
            }
            #[test]
            fn every_shape_pair_stops_at_high_speed_and_bounces() {
                for a in [false, true] {
                    for b in [false, true] {
                        let mut world = $world::new(4.0);
                        let mut wall = body(x(100.0), b);
                        wall.kind = BodyKind::Static;
                        let wall = world.insert(wall);
                        let mut projectile = body($v::ZERO, a);
                        projectile.velocity = x(100_000.0);
                        let id = world.insert(projectile);
                        let report = world.step(tick(1.0), &mut Events::default());
                        assert_eq!(report.dropped_time, 0.0);
                        near(world.body(id).unwrap().position.x, 98.0);
                        assert!(
                            world
                                .body(id)
                                .unwrap()
                                .shape
                                .overlap(
                                    world.body(id).unwrap().position,
                                    world.body(wall).unwrap().shape,
                                    world.body(wall).unwrap().position
                                )
                                .is_none()
                        );
                        assert_eq!(world.body(id).unwrap().velocity, $v::ZERO);
                        world.body_mut(id).unwrap().restitution = 1.0;
                        world.body_mut(id).unwrap().velocity = x(10.0);
                        world.step(tick(0.1), &mut Events::default());
                        near(world.body(id).unwrap().position.x, 97.0);
                        near(world.body(id).unwrap().velocity.x, -10.0);
                    }
                }
            }
            #[test]
            fn relative_motion_prevents_dynamic_tunneling() {
                for a in [false, true] {
                    for b in [false, true] {
                        let mut world = $world::new(4.0);
                        let mut left = body(x(-50.0), a);
                        left.velocity = x(1000.0);
                        let mut right = body(x(50.0), b);
                        right.velocity = x(-1000.0);
                        let left = world.insert(left);
                        let right = world.insert(right);
                        let report = world.step(tick(1.0), &mut Events::default());
                        assert_eq!(report.dropped_time, 0.0);
                        near(world.body(left).unwrap().position.x, -1.0);
                        near(world.body(right).unwrap().position.x, 1.0);
                        near(world.body(left).unwrap().velocity.x, 0.0);
                        near(world.body(right).unwrap().velocity.x, 0.0);
                    }
                }
            }
            #[test]
            fn depenetration_and_mass_weighted_push_apart() {
                let mut world = $world::new(4.0);
                let mut a = body($v::ZERO, false);
                a.mass = 3.0;
                let a = world.insert(a);
                let b = world.insert(body(x(1.0), false));
                let report = world.step(tick(0.0), &mut Events::default());
                assert!(!report.unresolved_overlaps);
                near(world.body(a).unwrap().position.x, -0.25);
                near(world.body(b).unwrap().position.x, 1.75);
                let mut wall = body(world.body(a).unwrap().position, true);
                wall.kind = BodyKind::Static;
                let wall = world.insert(wall);
                world.step(tick(0.0), &mut Events::default());
                let (a, b) = (world.body(a).unwrap(), world.body(wall).unwrap());
                assert!(a.shape.overlap(a.position, b.shape, b.position).is_none());
            }
            #[test]
            fn layers_filter_solids_and_triggers_symmetrically() {
                let mut world = $world::new(4.0);
                let mut wall = body(x(10.0), false);
                wall.kind = BodyKind::Static;
                wall.filter = CollisionFilter { layers: 2, mask: 1 };
                let wall = world.insert(wall);
                let mut mover = body($v::ZERO, false);
                mover.velocity = x(20.0);
                mover.filter = CollisionFilter { layers: 1, mask: 1 };
                let mover = world.insert(mover);
                world.step(tick(1.0), &mut Events::default());
                near(world.body(mover).unwrap().position.x, 20.0);
                world.body_mut(mover).unwrap().position = $v::ZERO;
                world.body_mut(mover).unwrap().filter.mask = 2;
                world.step(tick(1.0), &mut Events::default());
                near(world.body(mover).unwrap().position.x, 8.0);
                world.body_mut(wall).unwrap().is_trigger = true;
                world.body_mut(wall).unwrap().filter.mask = 0;
                world.body_mut(mover).unwrap().velocity = x(20.0);
                let mut events = Events::default();
                world.step(tick(1.0), &mut events);
                assert!(events.read().is_empty());
            }
            #[test]
            fn triggers_enter_stay_exit_remove_and_swept_pass() {
                let mut world = $world::new(4.0);
                let mut trigger = body($v::ZERO, false);
                trigger.is_trigger = true;
                trigger.kind = BodyKind::Static;
                let trigger = world.insert(trigger);
                let other = world.insert(body(x(-5.0), true));
                let mut events = Events::default();
                world.body_mut(other).unwrap().velocity = x(5.0);
                world.step(tick(1.0), &mut events);
                assert_eq!(
                    events.drain().map(|e| e.phase).collect::<Vec<_>>(),
                    [TriggerPhase::Enter]
                );
                world.body_mut(other).unwrap().velocity = $v::ZERO;
                world.step(tick(1.0), &mut events);
                assert_eq!(
                    events.drain().map(|e| e.phase).collect::<Vec<_>>(),
                    [TriggerPhase::Stay]
                );
                world.body_mut(other).unwrap().filter.mask = 0;
                world.step(tick(0.0), &mut events);
                assert_eq!(
                    events.drain().map(|e| e.phase).collect::<Vec<_>>(),
                    [TriggerPhase::Exit]
                );
                world.body_mut(other).unwrap().filter = CollisionFilter::default();
                world.step(tick(0.0), &mut events);
                events.clear();
                world.remove(other);
                world.step(tick(0.0), &mut events);
                assert_eq!(
                    events.drain().collect::<Vec<_>>(),
                    [TriggerEvent {
                        trigger,
                        other,
                        phase: TriggerPhase::Exit
                    }]
                );
                let mut fast = body(x(-100.0), true);
                fast.velocity = x(10000.0);
                let id = world.insert(fast);
                world.step(tick(0.02), &mut events);
                assert_eq!(
                    events.read(),
                    &[
                        TriggerEvent {
                            trigger,
                            other: id,
                            phase: TriggerPhase::Enter
                        },
                        TriggerEvent {
                            trigger,
                            other: id,
                            phase: TriggerPhase::Exit
                        }
                    ]
                );
                near(world.body(id).unwrap().position.x, 100.0);
            }
            #[test]
            fn platforms_carry_horizontally_vertically_and_release_jumps() {
                for velocity in [x(3.0), y(2.0 * up()), y(-2.0 * up())] {
                    let mut world = $world::new(4.0);
                    let mut platform = $body::new(
                        $v::ZERO,
                        $shape::box_shape({
                            let mut v = $v::splat(10.0);
                            v.y = 2.0;
                            v
                        }),
                    );
                    platform.kind = BodyKind::Kinematic;
                    platform.velocity = velocity;
                    let platform = world.insert(platform);
                    let mut rider = body(y(2.0 * up()), false);
                    rider.gravity = Some(y(-20.0 * up()));
                    let rider = world.insert(rider);
                    for _ in 0..30 {
                        world.step(tick(1.0 / 60.0), &mut Events::default());
                    }
                    let r = world.body(rider).unwrap();
                    let p = world.body(platform).unwrap();
                    near(r.position.x, p.position.x);
                    near(r.position.y, p.position.y + 2.0 * up());
                    assert!(r.grounded);
                    near(r.velocity.x, 0.0);
                    world.body_mut(rider).unwrap().velocity = y(10.0 * up());
                    world.step(tick(0.1), &mut Events::default());
                    assert!(
                        world.body(rider).unwrap().position.y * up()
                            > (world.body(platform).unwrap().position.y + 2.0 * up()) * up()
                    );
                }
            }
            #[test]
            fn grazing_a_box_corner_does_not_collide_or_enter_a_trigger() {
                for trigger in [false, true] {
                    let mut world = $world::new(4.0);
                    let mut wall = body($v::ZERO, false);
                    wall.kind = BodyKind::Static;
                    wall.is_trigger = trigger;
                    world.insert(wall);
                    let mut mover = body(x(-3.0) + y(-1.0), false);
                    mover.velocity = x(4.0) + y(-4.0);
                    let id = world.insert(mover);
                    let mut events = Events::default();
                    let report = world.step(tick(1.0), &mut events);
                    assert_eq!(report.contacts, 0);
                    assert!(events.read().is_empty());
                    assert_eq!(world.body(id).unwrap().position, x(1.0) + y(-5.0));
                    assert_eq!(world.body(id).unwrap().velocity, x(4.0) + y(-4.0));
                }
                // Entering a face exactly at the end of a tick still responds.
                let mut world = $world::new(4.0);
                let mut wall = body($v::ZERO, false);
                wall.kind = BodyKind::Static;
                world.insert(wall);
                let mut mover = body(x(-3.0), false);
                mover.velocity = x(1.0);
                let id = world.insert(mover);
                world.step(tick(1.0), &mut Events::default());
                near(world.body(id).unwrap().position.x, -2.0);
                assert_eq!(world.body(id).unwrap().velocity, $v::ZERO);
            }
            #[test]
            fn landing_on_a_diagonally_rising_platform_transfers_velocity_to_carry() {
                for round in [false, true] {
                    let mut world = $world::new(4.0);
                    let mut p = $body::new(
                        $v::ZERO,
                        $shape::box_shape({
                            let mut v = $v::splat(20.0);
                            v.y = 2.0;
                            v
                        }),
                    );
                    p.kind = BodyKind::Kinematic;
                    p.velocity = x(3.0) + y(2.0 * up());
                    let p = world.insert(p);
                    let mut r = body(y(3.0 * up()), round);
                    r.velocity = y(-10.0 * up());
                    r.gravity = Some(y(-20.0 * up()));
                    let r = world.insert(r);
                    for _ in 0..3 {
                        world.step(tick(0.05), &mut Events::default());
                    }
                    let offset =
                        world.body(r).unwrap().position.x - world.body(p).unwrap().position.x;
                    for _ in 0..30 {
                        let report = world.step(tick(0.05), &mut Events::default());
                        assert_eq!(report.dropped_time, 0.0);
                        let rider = world.body(r).unwrap();
                        let platform = world.body(p).unwrap();
                        near(rider.position.x - platform.position.x, offset);
                        near(rider.position.y - platform.position.y, 2.0 * up());
                        near(rider.velocity.y, 0.0);
                        assert!(rider.grounded);
                    }
                    world.body_mut(r).unwrap().velocity = y(10.0 * up());
                    world.step(tick(0.05), &mut Events::default());
                    assert!(!world.body(r).unwrap().grounded);
                }
            }
            #[test]
            fn forces_speed_cap_and_contact_friction() {
                let mut world = $world::new(4.0);
                let mut b = body($v::ZERO, false);
                b.gravity = Some(x(20.0));
                b.drag = 1.0;
                b.max_speed = Some(4.0);
                let id = world.insert(b);
                world.step(tick(1.0), &mut Events::default());
                near(world.body(id).unwrap().velocity.x, 4.0);
                near(world.body(id).unwrap().position.x, 4.0);
                let mut floor = $body::new(
                    y(-4.0 * up()),
                    $shape::box_shape({
                        let mut v = $v::splat(100.0);
                        v.y = 2.0;
                        v
                    }),
                );
                floor.kind = BodyKind::Static;
                floor.friction = 1.0;
                world.insert(floor);
                let b = world.body_mut(id).unwrap();
                b.position = $v::ZERO;
                b.velocity = x(5.0) + y(-10.0 * up());
                b.gravity = None;
                b.max_speed = None;
                b.drag = 0.0;
                world.step(tick(1.0), &mut Events::default());
                near(world.body(id).unwrap().velocity.x, 0.0);
                assert!(world.body(id).unwrap().grounded);
            }
            #[test]
            fn simultaneous_floor_contacts_remain_outside_fractional_geometry() {
                let mut world = $world::new(4.0);
                let mut floor = $body::new(
                    y(-6.0 * up()),
                    $shape::box_shape({
                        let mut size = $v::splat(100.0);
                        size.y = 1.0;
                        size
                    }),
                );
                floor.kind = BodyKind::Static;
                let floor = world.insert(floor);
                let ids: Vec<_> = (0..6)
                    .map(|i| {
                        let mut b = $body::new(
                            x(i as f32 * 3.0) + y(-4.5 * up()),
                            $shape::box_shape($v::splat(1.4)),
                        );
                        b.gravity = Some(y(-22.0 * up()));
                        b.drag = 0.2;
                        world.insert(b)
                    })
                    .collect();
                for _ in 0..120 {
                    let report = world.step(tick(1.0 / 120.0), &mut Events::default());
                    assert_eq!(report.dropped_time, 0.0);
                    assert!(!report.unresolved_overlaps);
                    for &id in &ids {
                        let b = world.body(id).unwrap();
                        let f = world.body(floor).unwrap();
                        assert!(b.shape.overlap(b.position, f.shape, f.position).is_none());
                    }
                }
            }
            #[test]
            fn small_masses_do_not_overflow_separation_weights() {
                let mut world = $world::new(4.0);
                let mut a = body($v::ZERO, false);
                a.mass = f32::MIN_POSITIVE;
                let mut b = body(x(1.0), false);
                b.mass = f32::MIN_POSITIVE;
                let a = world.insert(a);
                let b = world.insert(b);
                let report = world.step(tick(0.0), &mut Events::default());
                assert!(!report.unresolved_overlaps);
                near(world.body(a).unwrap().position.x, -0.5);
                near(world.body(b).unwrap().position.x, 1.5);
            }
            #[test]
            fn deterministic_hundreds_of_bodies() {
                fn run() -> Vec<$body> {
                    let mut world = $world::new(4.0);
                    for i in 0..300 {
                        let mut b = body(
                            x((i % 30) as f32 * 3.0) + y((i / 30) as f32 * 3.0),
                            i % 2 == 0,
                        );
                        b.velocity = x(if i % 2 == 0 { 1.0 } else { -1.0 });
                        b.gravity = Some(y(2.0));
                        world.insert(b);
                    }
                    for _ in 0..10 {
                        world.step(tick(1.0 / 60.0), &mut Events::default());
                    }
                    world.iter().map(|(_, b)| *b).collect()
                }
                assert_eq!(run(), run());
            }
        }
    };
}
scenarios!(two, Vec2, Shape2D, PhysicsBody2D, PhysicsWorld2D, 2, -1.0);
scenarios!(three, Vec3, Shape3D, PhysicsBody3D, PhysicsWorld3D, 3, 1.0);

#[test]
fn grids_match_linear_bounds_with_negative_cells_updates_and_huge_queries() {
    let mut grid2 = UniformGrid2D::new(3.0);
    let mut grid3 = UniformGrid3D::new(3.0);
    let entries: Vec<_> = (0..500)
        .map(|i| {
            (
                i,
                Vec3::new(
                    (i % 23) as f32 * 2.7 - 30.0,
                    (i % 17) as f32 * 3.1 - 25.0,
                    (i % 11) as f32 - 5.0,
                ),
            )
        })
        .collect();
    for &(id, p) in &entries {
        grid2.insert(id, Aabb2::from_center(p.truncate(), Vec2::splat(2.0)));
        grid3.insert(id, Aabb3::from_center(p, Vec3::splat(2.0)));
    }
    for center in [Vec3::ZERO, Vec3::splat(-20.0), Vec3::splat(10.0)] {
        for size in [3.0, 30.0, 1e10] {
            let q2 = Aabb2::from_center(center.truncate(), Vec2::splat(size));
            let q3 = Aabb3::from_center(center, Vec3::splat(size));
            let expected2: Vec<_> = entries
                .iter()
                .filter(|(_, p)| {
                    let b = Aabb2::from_center(p.truncate(), Vec2::splat(2.0));
                    q2.min.cmple(b.max).all() && q2.max.cmpge(b.min).all()
                })
                .map(|(id, _)| *id)
                .collect();
            let expected3: Vec<_> = entries
                .iter()
                .filter(|(_, p)| {
                    let b = Aabb3::from_center(*p, Vec3::splat(2.0));
                    q3.min.cmple(b.max).all() && q3.max.cmpge(b.min).all()
                })
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(grid2.query(q2), expected2);
            assert_eq!(grid3.query(q3), expected3);
        }
    }
    grid2.insert(999, Aabb2::from_center(Vec2::ZERO, Vec2::splat(1e9)));
    assert!(
        grid2
            .query(Aabb2::from_center(Vec2::ZERO, Vec2::ONE))
            .contains(&999)
    );
    grid2.insert(999, Aabb2::from_center(Vec2::splat(1e10), Vec2::ONE));
    assert!(
        !grid2
            .query(Aabb2::from_center(Vec2::ZERO, Vec2::ONE))
            .contains(&999)
    );
    grid2.remove(999);
    grid2.clear();
    assert!(
        grid2
            .query(Aabb2::from_center(Vec2::ZERO, Vec2::ONE))
            .is_empty()
    );
}

#[test]
fn standalone_queries_and_character_depenetration() {
    let a = Aabb2::from_center(Vec2::ZERO, Vec2::splat(2.0));
    let c = Circle::new(Vec2::new(1.5, 0.0), 1.0);
    assert_eq!(
        a.overlap_circle(&c).unwrap().normal,
        -c.overlap_box(&a).unwrap().normal
    );
    near(
        c.overlap(&Circle::new(Vec2::new(3.0, 0.0), 1.0))
            .unwrap()
            .depth,
        0.5,
    );
    let b = Aabb3::from_center(Vec3::ZERO, Vec3::splat(2.0));
    let s = Sphere::new(Vec3::new(1.5, 0.0, 0.0), 1.0);
    assert_eq!(
        b.overlap_sphere(&s).unwrap().normal,
        -s.overlap_box(&b).unwrap().normal
    );
    near(
        s.overlap(&Sphere::new(Vec3::new(3.0, 0.0, 0.0), 1.0))
            .unwrap()
            .depth,
        0.5,
    );
    let mut body = Body2D::new(Vec2::ZERO, Vec2::ONE);
    body.move_and_slide(0.0, &[a]);
    assert!(!body.bounds().intersects(&a));
    let mut body = Body3D::new(Vec3::ZERO, Vec3::ONE);
    body.move_and_slide(0.0, &[b]);
    assert!(!body.bounds().intersects(&b));
}

#[test]
fn exact_round_box_corner_ccd_does_not_hit_empty_aabb_corner() {
    let mut world = PhysicsWorld2D::new(2.0);
    let mut wall = PhysicsBody2D::new(Vec2::ZERO, Shape2D::box_shape(Vec2::splat(2.0)));
    wall.kind = BodyKind::Static;
    world.insert(wall);
    let mut ball = PhysicsBody2D::new(Vec2::new(1.8, -5.0), Shape2D::round(1.0));
    ball.velocity = Vec2::new(0.0, 10.0);
    let id = world.insert(ball);
    let report = world.step(tick(1.0), &mut Events::default());
    assert!(report.contacts > 0);
    assert!(world.body(id).unwrap().position.x > 1.8); // curved normal slides outward
    assert!(world.body(id).unwrap().position.y > -2.0);
}
