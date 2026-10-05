use super::*;
use crate::physics::*;

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}
macro_rules! scenarios {
    ($module:ident, $v:ident, $shape:ident, $body:ident, $world:ident, $ray:ident, $hit:ident) => {
        mod $module {
            use super::*;
            fn x(x: f32) -> $v {
                let mut v = $v::ZERO;
                v.x = x;
                v
            }
            fn y(y: f32) -> $v {
                let mut v = $v::ZERO;
                v.y = y;
                v
            }
            fn shape(round: bool) -> $shape {
                if round {
                    $shape::round(1.0)
                } else {
                    $shape::box_shape($v::splat(2.0))
                }
            }
            #[test]
            fn all_shape_pairs_and_rays_hit_at_high_speed() {
                for a in [false, true] {
                    for b in [false, true] {
                        let hit = shape(a)
                            .cast(x(-10.0), x(100_000.0), shape(b), $v::ZERO)
                            .unwrap()
                            .unwrap();
                        near(hit.distance, 8.0);
                        near(hit.fraction, 0.00008);
                        near(hit.position.x, -2.0);
                        assert_eq!(hit.normal, x(-1.0));
                        let endpoint = shape(a)
                            .cast(x(-10.0), x(8.0), shape(b), $v::ZERO)
                            .unwrap()
                            .unwrap();
                        assert_eq!(endpoint.fraction, 1.0);
                    }
                    let hit = shape(a)
                        .raycast($v::ZERO, $ray::new(x(-10.0), x(17.0)).unwrap(), 100_000.0)
                        .unwrap()
                        .unwrap();
                    near(hit.distance, 9.0);
                    near(hit.position.x, -1.0);
                    assert_eq!(hit.normal, x(-1.0));
                }
            }
            #[test]
            fn initial_overlap_zero_travel_touching_and_tangency() {
                for a in [false, true] {
                    for b in [false, true] {
                        for delta in [$v::ZERO, x(100.0), x(-100.0)] {
                            let hit = shape(a)
                                .cast($v::ZERO, delta, shape(b), $v::ZERO)
                                .unwrap()
                                .unwrap();
                            assert_eq!(hit.fraction, 0.0);
                            assert_eq!(hit.distance, 0.0);
                            assert_eq!(hit.normal, $v::ZERO);
                        }
                        assert!(
                            shape(a)
                                .cast(x(-2.0), $v::ZERO, shape(b), $v::ZERO)
                                .unwrap()
                                .is_none()
                        );
                        assert!(
                            shape(a)
                                .cast(x(-2.0), x(-1.0), shape(b), $v::ZERO)
                                .unwrap()
                                .is_none()
                        );
                        let entering = shape(a)
                            .cast(x(-2.0), x(1.0), shape(b), $v::ZERO)
                            .unwrap()
                            .unwrap();
                        assert_eq!(entering.fraction, 0.0);
                        assert_eq!(entering.normal, x(-1.0));
                        assert!(
                            shape(a)
                                .cast(x(-5.0) + y(2.0), x(10.0), shape(b), $v::ZERO)
                                .unwrap()
                                .is_none()
                        );
                    }
                    let inside = shape(a)
                        .raycast($v::ZERO, $ray::new($v::ZERO, x(1.0)).unwrap(), 0.0)
                        .unwrap()
                        .unwrap();
                    assert_eq!(inside.normal, $v::ZERO);
                    assert!(
                        shape(a)
                            .raycast($v::ZERO, $ray::new(x(-1.0), x(-1.0)).unwrap(), 1.0)
                            .unwrap()
                            .is_none()
                    );
                    assert!(
                        shape(a)
                            .raycast($v::ZERO, $ray::new(x(-1.0), x(1.0)).unwrap(), 0.0)
                            .unwrap()
                            .is_none()
                    );
                    assert!(
                        shape(a)
                            .raycast($v::ZERO, $ray::new(x(-5.0) + y(1.0), x(1.0)).unwrap(), 10.0)
                            .unwrap()
                            .is_none()
                    );
                }
            }
            #[test]
            fn round_bounding_corner_is_not_a_hit_and_corner_normal_is_exact() {
                let ray = $ray::new(x(-2.0) + y(0.9), x(1.0)).unwrap();
                assert!(shape(true).raycast($v::ZERO, ray, 1.2).unwrap().is_none());
                let hit = shape(true).raycast($v::ZERO, ray, 3.0).unwrap().unwrap();
                near(hit.position.x, -(1.0_f32 - 0.9 * 0.9).sqrt());
                near(hit.normal.y, 0.9);
                for (a, b) in [(true, false), (false, true)] {
                    // Expanded box would hit x=-2; rounded corner hits later.
                    let hit = shape(a)
                        .cast(x(-5.0) + y(1.9), x(10.0), shape(b), $v::ZERO)
                        .unwrap()
                        .unwrap();
                    near(hit.position.x, -1.0 - (1.0_f32 - 0.9 * 0.9).sqrt());
                    near(hit.normal.y, 0.9);
                }
            }
            #[test]
            fn wall_is_first_in_both_insertion_orders_and_ties_use_identity() {
                for wall_first in [false, true] {
                    let mut world = $world::new(4.0);
                    let wall = $body::new(
                        x(5.0),
                        $shape::box_shape(x(0.01) + ($v::ONE - x(1.0)) * 20.0),
                    );
                    let target = $body::new(x(10.0), shape(true));
                    let (wall_id, target_id) = if wall_first {
                        (world.insert(wall), world.insert(target))
                    } else {
                        let t = world.insert(target);
                        (world.insert(wall), t)
                    };
                    let ray = $ray::new($v::ZERO, x(1.0)).unwrap();
                    assert_eq!(
                        world
                            .raycast(ray, 100_000.0, QueryFilter::default())
                            .unwrap()
                            .unwrap()
                            .body,
                        wall_id
                    );
                    assert_eq!(
                        world
                            .cast_shape(shape(true), $v::ZERO, x(100_000.0), QueryFilter::default())
                            .unwrap()
                            .unwrap()
                            .body,
                        wall_id
                    );
                    world.remove(wall_id);
                    let duplicate = world.insert(target);
                    assert!(target_id < duplicate);
                    assert_eq!(
                        world
                            .raycast(ray, 20.0, QueryFilter::default())
                            .unwrap()
                            .unwrap()
                            .body,
                        target_id
                    );
                    assert_eq!(
                        world
                            .cast_shape(shape(false), $v::ZERO, x(20.0), QueryFilter::default())
                            .unwrap()
                            .unwrap()
                            .body,
                        target_id
                    );
                }
            }
            #[test]
            fn filters_exclusions_and_live_edits_apply_to_all_queries() {
                let mut world = $world::new(4.0);
                let mut trigger = $body::new(x(4.0), shape(true));
                trigger.is_trigger = true;
                let trigger_id = world.insert(trigger);
                let solid = world.insert($body::new(x(8.0), shape(false)));
                let ray = $ray::new($v::ZERO, x(1.0)).unwrap();
                let default = QueryFilter::default();
                let include = QueryFilter {
                    include_triggers: true,
                    ..default
                };
                assert_eq!(
                    world.raycast(ray, 20.0, default).unwrap().unwrap().body,
                    solid
                );
                assert_eq!(
                    world.raycast(ray, 20.0, include).unwrap().unwrap().body,
                    trigger_id
                );
                let exclude = QueryFilter {
                    excluded: &[trigger_id, solid],
                    ..include
                };
                assert!(world.raycast(ray, 20.0, exclude).unwrap().is_none());
                assert!(
                    world
                        .cast_shape(shape(false), $v::ZERO, x(20.0), exclude)
                        .unwrap()
                        .is_none()
                );
                let mut ids = vec![];
                world
                    .visit_overlaps($shape::box_shape($v::splat(30.0)), $v::ZERO, exclude, |h| {
                        ids.push(h.body)
                    })
                    .unwrap();
                assert!(ids.is_empty());
                // Check both halves of symmetric filtering.
                for collision in [
                    CollisionFilter { layers: 2, mask: 2 },
                    CollisionFilter { layers: 1, mask: 2 },
                ] {
                    assert!(
                        world
                            .raycast(
                                ray,
                                20.0,
                                QueryFilter {
                                    collision,
                                    ..default
                                }
                            )
                            .unwrap()
                            .is_none()
                    );
                }
                world.body_mut(solid).unwrap().filter.mask = 2;
                assert!(world.raycast(ray, 20.0, default).unwrap().is_none());
                world.body_mut(solid).unwrap().filter.mask = u32::MAX;
                world.body_mut(solid).unwrap().position = x(2.0);
                world.body_mut(solid).unwrap().shape = shape(true);
                near(
                    world
                        .raycast(ray, 20.0, default)
                        .unwrap()
                        .unwrap()
                        .hit
                        .distance,
                    1.0,
                );
                world.body_mut(trigger_id).unwrap().is_trigger = false;
                world
                    .visit_overlaps($shape::box_shape($v::splat(30.0)), $v::ZERO, default, |h| {
                        ids.push(h.body)
                    })
                    .unwrap();
                assert_eq!(ids, vec![trigger_id, solid]);
                world.remove(solid);
                assert_eq!(
                    world.raycast(ray, 20.0, default).unwrap().unwrap().body,
                    trigger_id
                );
            }
            #[test]
            fn overlap_visits_are_exact_sorted_and_exclude_touching() {
                for a in [false, true] {
                    for b in [false, true] {
                        let mut world = $world::new(4.0);
                        let overlap = world.insert($body::new(x(1.5), shape(b)));
                        world.insert($body::new(x(2.0), shape(b)));
                        let mut hits = vec![];
                        world
                            .visit_overlaps(shape(a), $v::ZERO, QueryFilter::default(), |h| {
                                hits.push(h)
                            })
                            .unwrap();
                        assert_eq!(hits.len(), 1);
                        assert_eq!(hits[0].body, overlap);
                        near(hits[0].penetration.depth, 0.5);
                        assert_eq!(hits[0].penetration.normal, x(-1.0));
                    }
                }
                let mut world = $world::new(4.0);
                world.insert($body::new(x(1.8) + y(1.8), shape(true)));
                let mut visits = 0;
                world
                    .visit_overlaps(shape(false), $v::ZERO, QueryFilter::default(), |_| {
                        visits += 1
                    })
                    .unwrap();
                assert_eq!(visits, 0);
            }
            #[test]
            fn snapshot_ignores_velocity_and_relative_cast_is_explicit() {
                let mut world = $world::new(4.0);
                let mut target = $body::new(x(10.0), shape(true));
                target.velocity = x(-10.0);
                world.insert(target);
                assert!(
                    world
                        .cast_shape(shape(true), $v::ZERO, x(5.0), QueryFilter::default())
                        .unwrap()
                        .is_none()
                );
                let relative = shape(true)
                    .cast(
                        $v::ZERO,
                        x(5.0) - target.velocity,
                        target.shape,
                        target.position,
                    )
                    .unwrap()
                    .unwrap();
                near(relative.fraction, 8.0 / 15.0);
                // World-space caster center uses its own displacement.
                near((x(5.0) * relative.fraction).x, 8.0 / 3.0);
            }
            #[test]
            fn invalid_inputs_are_errors_even_in_an_empty_world() {
                let mut world = $world::new(4.0);
                let ray = $ray::new($v::ZERO, x(1.0)).unwrap();
                for distance in [-1.0, f32::NAN, f32::INFINITY] {
                    assert_eq!(
                        world.raycast(ray, distance, QueryFilter::default()),
                        Err(QueryError::InvalidMotion)
                    );
                }
                assert_eq!(
                    world.cast_shape(
                        shape(true),
                        $v::ZERO,
                        x(f32::INFINITY),
                        QueryFilter::default()
                    ),
                    Err(QueryError::InvalidMotion)
                );
                let invalid = $shape::Box {
                    half_size: $v::ZERO,
                };
                assert_eq!(
                    world.cast_shape(invalid, $v::ZERO, x(1.0), QueryFilter::default()),
                    Err(QueryError::InvalidGeometry)
                );
                let id = world.insert($body::new($v::ZERO, shape(true)));
                let bad = world.insert($body::new(x(3.0), shape(true)));
                world.body_mut(bad).unwrap().position = x(f32::NAN);
                assert_eq!(
                    world.raycast(ray, 10.0, QueryFilter::default()),
                    Err(QueryError::InvalidGeometry)
                );
                let mut visits = 0;
                assert_eq!(
                    world.visit_overlaps(shape(false), $v::ZERO, QueryFilter::default(), |_| {
                        visits += 1
                    }),
                    Err(QueryError::InvalidGeometry)
                );
                assert_eq!(visits, 0);
                assert_eq!(
                    world
                        .raycast(
                            ray,
                            10.0,
                            QueryFilter {
                                excluded: &[bad],
                                ..QueryFilter::default()
                            }
                        )
                        .unwrap()
                        .unwrap()
                        .body,
                    id
                );
            }
            #[test]
            fn distant_round_queries_preserve_small_radius() {
                let ray = $ray::new(x(-100_000_000.0), x(1.0)).unwrap();
                let hit = shape(true)
                    .raycast($v::ZERO, ray, 200_000_000.0)
                    .unwrap()
                    .unwrap();
                assert_eq!(hit.normal, x(-1.0));
                let hit = shape(true)
                    .cast(
                        x(-1_000_000_000.0),
                        x(2_000_000_000.0),
                        shape(true),
                        $v::ZERO,
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(hit.normal, x(-1.0));
                near(hit.position.x, -2.0);
                let tangent = $ray::new(x(-100_000_000.0) + y(1.0), x(1.0)).unwrap();
                assert!(
                    shape(true)
                        .raycast($v::ZERO, tangent, 200_000_000.0)
                        .unwrap()
                        .is_none()
                );
            }
            #[test]
            fn oblique_round_touching_and_endpoint_contacts_are_included() {
                let caster = $shape::round(2.0);
                let target = $shape::round(3.0);
                let start = x(3.0) + y(4.0);
                let hit = caster
                    .cast(start, x(-20.0) + y(-12.0), target, $v::ZERO)
                    .unwrap()
                    .unwrap();
                assert_eq!(hit.fraction, 0.0);
                assert_eq!(hit.position, start);
                near(hit.normal.x, 0.6);
                near(hit.normal.y, 0.8);
                let hit = caster
                    .cast(x(23.0) + y(1.0), x(-20.0) + y(3.0), target, $v::ZERO)
                    .unwrap()
                    .unwrap();
                assert_eq!(hit.fraction, 1.0);
                assert_eq!(hit.position, start);
                near(hit.normal.x, 0.6);
                near(hit.normal.y, 0.8);
                assert!(
                    caster
                        .cast(start, x(20.0) + y(12.0), target, $v::ZERO)
                        .unwrap()
                        .is_none()
                );
                // Do not promote actual near misses beyond the end to a hit.
                let before = x((-20.0_f32).next_up()) + y(3.0);
                assert!(
                    caster
                        .cast(x(23.0) + y(1.0), before, target, $v::ZERO)
                        .unwrap()
                        .is_none()
                );
            }
            #[test]
            fn long_world_casts_choose_the_actual_first_hit_before_rounding() {
                for a in [false, true] {
                    for b in [false, true] {
                        let mut world = $world::new(4.0);
                        let farther = world.insert($body::new($v::ZERO, shape(b)));
                        let nearer = world.insert($body::new(x(-2.0), shape(b)));
                        assert!(farther < nearer);
                        let ray = $ray::new(x(-100_000_000.0), x(1.0)).unwrap();
                        let hit = world
                            .raycast(ray, 200_000_000.0, QueryFilter::default())
                            .unwrap()
                            .unwrap();
                        assert_eq!(hit.body, nearer);
                        near(hit.hit.position.x, -3.0);
                        let hit = world
                            .cast_shape(
                                shape(a),
                                x(-1_000_000_000.0),
                                x(2_000_000_000.0),
                                QueryFilter::default(),
                            )
                            .unwrap()
                            .unwrap();
                        assert_eq!(hit.body, nearer);
                        near(hit.hit.position.x, -4.0);
                    }
                }
            }
            #[test]
            fn oblique_tangencies_have_no_entering_hit() {
                let caster = $shape::round(2.0);
                let target = $shape::round(3.0);
                assert!(
                    caster
                        .cast(x(-1.0) + y(7.0), x(364.0) + y(-273.0), target, $v::ZERO)
                        .unwrap()
                        .is_none()
                );
                let box_shape = $shape::box_shape($v::splat(2.0));
                // The same tangent path across the top-right rounded corner.
                assert!(
                    $shape::round(5.0)
                        .cast(y(8.0), x(364.0) + y(-273.0), box_shape, $v::ZERO)
                        .unwrap()
                        .is_none()
                );
            }
            #[test]
            fn long_oblique_casts_use_the_earliest_actual_face_interval() {
                let wall_shape = $shape::box_shape($v::splat(2.0));
                let wall_center = y(1.2);
                for travel in [100.0, 1e8, 1e9, 1e10] {
                    let start = x(-travel) + y(-travel);
                    let delta = x(2.0 * travel) + y(2.0 * travel);
                    for (caster, target) in [(shape(true), wall_shape), (wall_shape, shape(true))] {
                        let hit = caster
                            .cast(start, delta, target, wall_center)
                            .unwrap()
                            .unwrap();
                        near(hit.position.x, -0.8);
                        near(hit.position.y, -0.8);
                        assert_eq!(hit.normal, y(-1.0));
                    }
                    let mut world = $world::new(4.0);
                    let wall = world.insert($body::new(wall_center, wall_shape));
                    let target_coordinate = -0.79 + 1.01 / std::f32::consts::SQRT_2;
                    world.insert($body::new(
                        x(target_coordinate) + y(target_coordinate),
                        $shape::round(0.01),
                    ));
                    let hit = world
                        .cast_shape(shape(true), start, delta, QueryFilter::default())
                        .unwrap()
                        .unwrap();
                    assert_eq!(hit.body, wall);
                    assert_eq!(hit.hit.normal, y(-1.0));
                }
            }
            #[test]
            fn large_and_small_geometry_does_not_overflow_distance_squares() {
                for scale in [1e-25, 1e20] {
                    let round = $shape::round(scale);
                    let overlapping = round
                        .cast(x(1.5 * scale), $v::ZERO, round, $v::ZERO)
                        .unwrap()
                        .unwrap();
                    assert_eq!(overlapping.fraction, 0.0);
                    assert!(
                        round
                            .cast(x(2.5 * scale), $v::ZERO, round, $v::ZERO)
                            .unwrap()
                            .is_none()
                    );
                    let box_shape = $shape::box_shape($v::splat(2.0 * scale));
                    assert!(
                        round
                            .cast(x(2.5 * scale), $v::ZERO, box_shape, $v::ZERO)
                            .unwrap()
                            .is_none()
                    );
                }
                let moving = $shape::box_shape($v::splat(2e37));
                let target = $shape::box_shape($v::splat(3e38));
                let hit = moving
                    .cast(x(2.5e38), x(-3e38), target, $v::ZERO)
                    .unwrap()
                    .unwrap();
                near(hit.fraction, 0.3);
                assert_eq!(hit.normal, x(1.0));
            }
            #[test]
            fn world_results_match_standalone_scan() {
                let mut world = $world::new(4.0);
                for i in 0..200 {
                    let mut p = x(((i * 17) % 53) as f32 - 26.0) + y(((i * 11) % 37) as f32 - 18.0);
                    if p.x == 0.0 && p.y == 0.0 {
                        p.x = 2.0;
                    }
                    let mut b = $body::new(p, shape(i % 2 == 0));
                    b.is_trigger = i % 3 == 0;
                    world.insert(b);
                }
                for i in 0..30 {
                    let start = x(-30.0) + y(i as f32 - 15.0);
                    let delta = x(60.0) + y(2.0);
                    let filter = QueryFilter {
                        include_triggers: i % 2 == 0,
                        ..QueryFilter::default()
                    };
                    let mut expected = None;
                    for (id, body) in world.iter() {
                        if !filter.allows(id, body.filter, body.is_trigger) {
                            continue;
                        }
                        if let Some(hit) = shape(i % 2 == 0)
                            .cast(start, delta, body.shape, body.position)
                            .unwrap()
                        {
                            if expected
                                .is_none_or(|(_, old): (BodyId, $hit)| hit.fraction < old.fraction)
                            {
                                expected = Some((id, hit));
                            }
                        }
                    }
                    let actual = world
                        .cast_shape(shape(i % 2 == 0), start, delta, filter)
                        .unwrap()
                        .map(|h| (h.body, h.hit));
                    assert_eq!(actual, expected);
                }
            }
        }
    };
}
scenarios!(
    d2,
    Vec2,
    Shape2D,
    PhysicsBody2D,
    PhysicsWorld2D,
    Ray2,
    CastHit2D
);
scenarios!(
    d3,
    Vec3,
    Shape3D,
    PhysicsBody3D,
    PhysicsWorld3D,
    Ray3,
    CastHit3D
);

#[test]
fn sphere_casts_resolve_box_edges_corners_and_z_motion() {
    let box_shape = Shape3D::box_shape(Vec3::splat(2.0));
    let sphere = Shape3D::round(1.0);
    for offset in [Vec3::new(0.0, 1.6, 0.0), Vec3::new(0.0, 1.6, 1.6)] {
        let hit = sphere
            .cast(
                Vec3::new(-5.0, 0.0, 0.0) + offset,
                Vec3::X * 10.0,
                box_shape,
                Vec3::ZERO,
            )
            .unwrap()
            .unwrap();
        let squared = (offset.y - 1.0).max(0.0).powi(2) + (offset.z - 1.0).max(0.0).powi(2);
        near(hit.position.x, -1.0 - (1.0 - squared).sqrt());
        near(hit.normal.y, 0.6);
        near(hit.normal.z, if offset.z > 0.0 { 0.6 } else { 0.0 });
    }
    assert!(
        sphere
            .cast(
                Vec3::new(-5.0, 1.8, 1.8),
                Vec3::X * 10.0,
                box_shape,
                Vec3::ZERO
            )
            .unwrap()
            .is_none()
    );
    let hit = sphere
        .cast(Vec3::Z * -5.0, Vec3::Z * 10.0, box_shape, Vec3::ZERO)
        .unwrap()
        .unwrap();
    near(hit.distance, 3.0);
    assert_eq!(hit.normal, -Vec3::Z);
    let ray = Ray3::new(Vec3::new(0.9, 0.9, -5.0), Vec3::Z).unwrap();
    assert!(sphere.raycast(Vec3::ZERO, ray, 10.0).unwrap().is_none());
}
