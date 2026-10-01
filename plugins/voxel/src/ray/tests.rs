use super::*;
use crate::{BlockRegistry, Chunk, ChunkPos, CollisionKind, RenderKind};
use std::sync::Arc;

const SOLID: BlockId = BlockId::from_raw(1);
fn options(distance: f64) -> RaycastOptions {
    RaycastOptions {
        max_distance: distance,
        max_cells: 256,
        missing: MissingPolicy::Stop,
    }
}
fn hit(result: Raycast) -> VoxelHit {
    match result.outcome {
        RaycastOutcome::Hit(hit) => hit,
        other => panic!("expected hit, got {other:?}"),
    }
}

#[test]
fn six_face_contacts_are_normalized_and_return_adjacent_cells() {
    for face in Face::ALL {
        let normal = DVec3::from_array(face.normal().map(f64::from));
        let ray = GridRay::new(DVec3::splat(0.5) + normal * 2.0, -normal * 19.0).unwrap();
        let result = ray
            .cast(options(4.0), |p| {
                if p == BlockPos::default() {
                    RayCell::Hit(SOLID)
                } else {
                    RayCell::Empty
                }
            })
            .unwrap();
        assert_eq!(result.visited_cells, 3);
        let hit = hit(result);
        assert_eq!(hit.position, BlockPos::default());
        assert_eq!(hit.face, Some(face));
        assert_eq!(hit.adjacent, BlockPos::default().neighbor(face));
        assert_eq!(hit.distance, 1.5);
        assert!((hit.point - (DVec3::splat(0.5) + normal * 0.5)).length() < 1e-12);
    }
}

#[test]
fn exact_boundary_start_selects_only_the_forward_cell_in_each_direction() {
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut origin = DVec3::splat(0.5);
            origin[axis] = 0.0;
            let mut direction = DVec3::ZERO;
            direction[axis] = sign;
            let result = GridRay::new(origin, direction)
                .unwrap()
                .cast(options(0.0), |_| RayCell::Hit(SOLID))
                .unwrap();
            let contact = hit(result);
            let expected = if sign < 0.0 { -1 } else { 0 };
            assert_eq!(
                [contact.position.x, contact.position.y, contact.position.z][axis],
                expected
            );
            assert_eq!(contact.face, Some(Face::entering(axis, sign as i32)));
            assert_eq!(contact.distance, 0.0);
        }
    }
    let contact = hit(GridRay::new(DVec3::ZERO, -DVec3::ONE)
        .unwrap()
        .cast(options(0.0), |_| RayCell::Hit(SOLID))
        .unwrap());
    assert_eq!(contact.position, BlockPos::new(-1, -1, -1));
    assert_eq!(contact.face, Some(Face::PosX));
    // Parallel coordinates on a boundary follow half-open ownership.
    let contact = hit(GridRay::new(DVec3::new(0.0, 0.5, 0.0), DVec3::Y)
        .unwrap()
        .cast(options(0.0), |_| RayCell::Hit(SOLID))
        .unwrap());
    assert_eq!(contact.position, BlockPos::default());
    assert_eq!(contact.face, None);
    assert_eq!(contact.adjacent, None);
}

#[test]
fn strict_interior_and_zero_reach_have_no_invented_placement_face() {
    let origin = DVec3::new(-0.25, 1.25, 2.75);
    let contact = hit(GridRay::new(origin, DVec3::X)
        .unwrap()
        .cast(options(0.0), |_| RayCell::Hit(SOLID))
        .unwrap());
    assert_eq!(contact.position, BlockPos::new(-1, 1, 2));
    assert_eq!(contact.face, None);
    assert_eq!(contact.adjacent, None);
    assert_eq!(contact.point, origin);
    assert_eq!(contact.distance, 0.0);
    let result = GridRay::new(origin, DVec3::X)
        .unwrap()
        .cast(options(0.0), |_| RayCell::Empty)
        .unwrap();
    assert_eq!(result.outcome, RaycastOutcome::Miss);
    assert_eq!(result.visited_cells, 1);
}

#[test]
fn exact_edge_and_corner_ties_skip_zero_length_side_cells() {
    for direction in [DVec3::new(1.0, 1.0, 0.0), DVec3::ONE] {
        let ray = GridRay::new(DVec3::splat(0.5), direction).unwrap();
        let expected = if direction.z == 0.0 {
            BlockPos::new(1, 1, 0)
        } else {
            BlockPos::new(1, 1, 1)
        };
        let mut visited = Vec::new();
        let result = ray
            .cast(options(1.0), |cell| {
                visited.push(cell);
                if cell == expected || cell == BlockPos::new(1, 0, 0) {
                    RayCell::Hit(SOLID)
                } else {
                    RayCell::Empty
                }
            })
            .unwrap();
        assert_eq!(visited, vec![BlockPos::default(), expected]);
        let contact = hit(result);
        assert_eq!(contact.face, Some(Face::NegX));
        assert_eq!(contact.adjacent, expected.neighbor(Face::NegX));
        assert!((contact.distance - direction.length() * 0.5).abs() < 1e-12);
    }
    let ray = GridRay::new(DVec3::splat(0.5), DVec3::new(-1.0, -1.0, 0.0)).unwrap();
    let result = ray
        .cast(options(1.0), |cell| {
            if cell == BlockPos::new(-1, -1, 0) {
                RayCell::Hit(SOLID)
            } else {
                RayCell::Empty
            }
        })
        .unwrap();
    assert_eq!(hit(result).face, Some(Face::PosX));
}

#[test]
fn reach_is_inclusive_and_budget_exhaustion_is_not_a_miss() {
    let ray = GridRay::new(DVec3::new(0.25, 0.5, 0.5), DVec3::X).unwrap();
    let source = |cell: BlockPos| {
        if cell.x == 2 {
            RayCell::Hit(SOLID)
        } else {
            RayCell::Empty
        }
    };
    assert_eq!(hit(ray.cast(options(1.75), source).unwrap()).distance, 1.75);
    assert_eq!(
        ray.cast(options(1.749999), source).unwrap().outcome,
        RaycastOutcome::Miss
    );
    let result = ray
        .cast(
            RaycastOptions {
                max_cells: 2,
                ..options(1.75)
            },
            source,
        )
        .unwrap();
    assert_eq!(result.visited_cells, 2);
    assert_eq!(
        result.outcome,
        RaycastOutcome::BudgetExhausted {
            next_position: BlockPos::new(2, 0, 0),
            distance: 1.75
        }
    );
    let result = ray
        .cast(
            RaycastOptions {
                max_cells: 1,
                ..options(0.5)
            },
            source,
        )
        .unwrap();
    assert_eq!(result.outcome, RaycastOutcome::Miss); // Reach ended before another cell was needed.
}

#[test]
fn unloaded_policy_and_game_predicate_remain_explicit() {
    let mut registry = BlockRegistry::new();
    let mut invisible = BlockDef::new("barrier");
    invisible.render = RenderKind::Invisible;
    let barrier = registry.register(invisible).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 1);
    world
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(registry, barrier).unwrap(),
        )
        .unwrap();
    let ray = GridRay::new(DVec3::new(-0.5, 0.5, 0.5), DVec3::X).unwrap();
    let mut calls = 0;
    let result = world
        .raycast(ray, options(2.0), |_, _| {
            calls += 1;
            true
        })
        .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(
        result.outcome,
        RaycastOutcome::Unloaded {
            position: BlockPos::new(-1, 0, 0),
            distance: 0.0
        }
    );
    let result = world
        .raycast(
            ray,
            RaycastOptions {
                missing: MissingPolicy::Skip,
                ..options(2.0)
            },
            |_, block| block.collision == CollisionKind::Solid,
        )
        .unwrap();
    assert_eq!(hit(result).distance, 0.5);
    let result = world
        .raycast(
            ray,
            RaycastOptions {
                missing: MissingPolicy::Skip,
                ..options(2.0)
            },
            |_, block| block.render != RenderKind::Invisible,
        )
        .unwrap();
    assert_eq!(result.outcome, RaycastOutcome::Miss);
}

#[test]
fn storage_queries_cross_negative_chunk_boundaries_and_do_not_allocate_missing_chunks() {
    let mut registry = BlockRegistry::new();
    let solid = registry.register(BlockDef::new("stone")).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 3);
    for x in -2..=0 {
        world
            .insert_chunk(
                ChunkPos::new(x, 0, 0),
                Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
            )
            .unwrap();
    }
    world.set_block(BlockPos::new(0, 1, 1), solid).unwrap();
    let ray = GridRay::new(DVec3::new(-16.25, 1.5, 1.5), DVec3::X).unwrap();
    let contact = hit(world
        .raycast(ray, options(20.0), |id, _| id != BlockId::AIR)
        .unwrap());
    assert_eq!(contact.position, BlockPos::new(0, 1, 1));
    assert_eq!(contact.distance, 16.25);
    assert_eq!(contact.adjacent, Some(BlockPos::new(-1, 1, 1)));
    assert_eq!(world.len(), 3);
}

#[test]
fn invalid_queries_are_rejected_before_calling_the_source() {
    for (origin, direction) in [
        (DVec3::splat(f64::NAN), DVec3::X),
        (DVec3::splat(f64::MAX), DVec3::X),
        (DVec3::ZERO, DVec3::ZERO),
        (DVec3::ZERO, DVec3::splat(f64::INFINITY)),
    ] {
        assert_eq!(GridRay::new(origin, direction), Err(VoxelError::InvalidRay));
    }
    let ray = GridRay::new(DVec3::splat(0.25), DVec3::X).unwrap();
    let bad = [
        RaycastOptions {
            max_distance: -1.0,
            ..Default::default()
        },
        RaycastOptions {
            max_distance: f64::NAN,
            ..Default::default()
        },
        RaycastOptions {
            max_distance: f64::INFINITY,
            ..Default::default()
        },
        RaycastOptions {
            max_cells: 0,
            ..Default::default()
        },
        RaycastOptions {
            max_cells: MAX_RAY_CELLS + 1,
            ..Default::default()
        },
    ];
    for options in bad {
        assert_eq!(
            ray.cast(options, |_| panic!("must validate before traversal")),
            Err(VoxelError::InvalidRayOptions)
        );
    }
    for direction in [
        DVec3::splat(f64::MAX),
        DVec3::splat(f64::from_bits(1)),
        DVec3::new(1.0, 1e-320, 0.0),
    ] {
        let ray = GridRay::new(DVec3::splat(0.25), direction).unwrap();
        assert!((ray.direction().length() - 1.0).abs() < 1e-12);
        assert_eq!(
            ray.cast(options(1.0), |_| RayCell::Empty).unwrap().outcome,
            RaycastOutcome::Miss
        );
    }
    let original = Ray3::new(
        rayengine_core::glam::Vec3::splat(0.25),
        rayengine_core::glam::Vec3::X,
    )
    .unwrap();
    assert_eq!(GridRay::try_from(original).unwrap(), ray);
}

#[test]
fn full_integer_grid_precision_and_outer_boundaries_are_checked() {
    let upper = f64::from(i32::MAX);
    let lower = f64::from(i32::MIN);
    for (origin, direction) in [(upper + 0.5, DVec3::X), (lower + 0.5, -DVec3::X)] {
        let ray = GridRay::new(DVec3::new(origin, 0.5, 0.5), direction).unwrap();
        let result = ray.cast(options(1.0), |_| RayCell::Empty).unwrap();
        assert_eq!(
            result.outcome,
            RaycastOutcome::OutOfBounds { distance: 0.5 }
        );
        assert_eq!(result.visited_cells, 1);
    }
    let ray = GridRay::new(DVec3::new(upper + 1.0, 0.5, 0.5), -DVec3::X).unwrap();
    let contact = hit(ray.cast(options(0.0), |_| RayCell::Hit(SOLID)).unwrap());
    assert_eq!(contact.position.x, i32::MAX);
    assert_eq!(contact.adjacent, None);
    assert_eq!(contact.face, Some(Face::PosX));
    assert!(GridRay::new(DVec3::new(upper + 1.0, 0.5, 0.5), DVec3::X).is_err());
    assert!(GridRay::new(DVec3::new(lower, 0.5, 0.5), -DVec3::X).is_err());
    let ray = GridRay::new(DVec3::new(upper - 1.75, 0.5, 0.5), DVec3::X).unwrap();
    let contact = hit(ray
        .cast(options(4.0), |cell| {
            if cell.x == i32::MAX {
                RayCell::Hit(SOLID)
            } else {
                RayCell::Empty
            }
        })
        .unwrap());
    assert_eq!(contact.distance, 1.75);
}

// Independent analytic slab oracle over every selected unit box. No grid stepping.
fn slab(ray: GridRay, position: BlockPos, reach: f64) -> Option<f64> {
    let min = DVec3::new(
        f64::from(position.x),
        f64::from(position.y),
        f64::from(position.z),
    );
    let mut enter = 0.0_f64;
    let mut exit = f64::INFINITY;
    for axis in 0..3 {
        let origin = ray.origin()[axis];
        let direction = ray.direction()[axis];
        if direction == 0.0 {
            if origin < min[axis] || origin >= min[axis] + 1.0 {
                return None;
            }
        } else {
            let a = (min[axis] - origin) / direction;
            let b = (min[axis] + 1.0 - origin) / direction;
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    (exit > enter && enter <= reach).then_some(enter)
}
fn selected(p: BlockPos) -> bool {
    [-4..=4, -4..=4, -4..=4]
        .into_iter()
        .zip([p.x, p.y, p.z])
        .all(|(r, v)| r.contains(&v))
        && (p.x * 17 + p.y * 13 + p.z * 7).rem_euclid(5) == 0
}
#[test]
fn seeded_random_rays_match_exhaustive_box_intersections() {
    let mut seed = 0x127b_32dd_39a5_21f1_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as f64 / f64::from(u32::MAX)
    };
    for _ in 0..1200 {
        let origin = DVec3::new(
            random() * 6.0 - 3.0,
            random() * 6.0 - 3.0,
            random() * 6.0 - 3.0,
        );
        let direction = DVec3::new(
            random() * 2.0 - 1.0,
            random() * 2.0 - 1.0,
            random() * 2.0 - 1.0,
        );
        let ray = GridRay::new(origin, direction).unwrap();
        let reach = 12.0;
        let mut expected: Option<(BlockPos, f64)> = None;
        for x in -4..=4 {
            for y in -4..=4 {
                for z in -4..=4 {
                    let position = BlockPos::new(x, y, z);
                    if selected(position)
                        && let Some(distance) = slab(ray, position, reach)
                        && expected.is_none_or(|(_, old)| distance < old)
                    {
                        expected = Some((position, distance));
                    }
                }
            }
        }
        let actual = ray
            .cast(options(reach), |position| {
                if selected(position) {
                    RayCell::Hit(SOLID)
                } else {
                    RayCell::Empty
                }
            })
            .unwrap();
        match (expected, actual.outcome) {
            (None, RaycastOutcome::Miss) => {}
            (Some((position, distance)), RaycastOutcome::Hit(contact)) => {
                assert_eq!(contact.position, position, "ray {ray:?}");
                assert!(
                    (contact.distance - distance).abs() < 1e-10,
                    "ray {ray:?}: {contact:?} vs {distance}"
                );
            }
            other => panic!("oracle mismatch for {ray:?}: {other:?}"),
        }
    }
}

#[test]
fn resident_cache_matches_uncached_queries_with_missing_and_cutout_cells() {
    let mut definitions = BlockRegistry::new();
    let stone = definitions.register(BlockDef::new("stone")).unwrap();
    let mut foliage = BlockDef::new("foliage");
    foliage.render = RenderKind::Cutout;
    foliage.collision = CollisionKind::None;
    let foliage = definitions.register(foliage).unwrap();
    let definitions = Arc::new(definitions);
    let mut world = VoxelWorld::new(definitions.clone(), 8);
    for x in -1..=0 {
        for y in -1..=0 {
            for z in -1..=0 {
                let pos = ChunkPos::new(x, y, z);
                let mut blocks = vec![BlockId::AIR; crate::CHUNK_VOLUME];
                for (index, block) in blocks.iter_mut().enumerate() {
                    let cell = pos
                        .block(crate::LocalPos::from_index(index).unwrap())
                        .unwrap();
                    let key = (cell.x * 17 + cell.y * 13 + cell.z * 7).rem_euclid(47);
                    *block = match key {
                        0 => stone,
                        1 => foliage,
                        _ => BlockId::AIR,
                    };
                }
                world
                    .insert_chunk(
                        pos,
                        Chunk::from_blocks(definitions.clone(), blocks).unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    let mut seed = 0x75b4_74a3_b5f1_0929_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as f64 / f64::from(u32::MAX)
    };
    for _ in 0..300 {
        let ray = GridRay::new(
            DVec3::new(
                random() * 40.0 - 20.0,
                random() * 40.0 - 20.0,
                random() * 40.0 - 20.0,
            ),
            DVec3::new(
                random() * 2.0 - 1.0,
                random() * 2.0 - 1.0,
                random() * 2.0 - 1.0,
            ),
        )
        .unwrap();
        for missing in [MissingPolicy::Stop, MissingPolicy::Skip] {
            for solid_only in [false, true] {
                let options = RaycastOptions {
                    max_distance: 50.0,
                    max_cells: 256,
                    missing,
                };
                let select = |_: BlockId, block: &BlockDef| {
                    if solid_only {
                        block.collision == CollisionKind::Solid
                    } else {
                        block.render != RenderKind::Invisible
                    }
                };
                let uncached = ray
                    .cast(options, |position| match world.block(position) {
                        Some(id) if select(id, world.registry().get(id).unwrap()) => {
                            RayCell::Hit(id)
                        }
                        Some(_) => RayCell::Empty,
                        None => RayCell::Missing,
                    })
                    .unwrap();
                assert_eq!(world.raycast(ray, options, select).unwrap(), uncached);
            }
        }
    }
}

#[test]
fn repeated_rational_corner_crossings_preserve_inclusive_reach() {
    let ray = GridRay::new(DVec3::new(0.0, 0.0, 0.5), DVec3::new(3.0, 4.0, 0.0)).unwrap();
    for scale in [1, 10, 100, 1000] {
        let target = BlockPos::new(3 * scale, 4 * scale, 0);
        let reach = f64::from(5 * scale);
        let options = RaycastOptions {
            max_distance: reach,
            max_cells: 16384,
            ..Default::default()
        };
        let result = ray
            .cast(options, |cell| {
                if cell == target {
                    RayCell::Hit(SOLID)
                } else {
                    RayCell::Empty
                }
            })
            .unwrap();
        let contact = hit(result);
        assert_eq!(contact.position, target);
        assert_eq!(contact.distance, reach);
        assert_eq!(contact.face, Some(Face::NegX));
    }
}

#[test]
fn tiny_coordinate_gap_can_cross_an_axis_with_an_infinite_reciprocal() {
    let gap = f64::from_bits(1);
    let direction = 1e-320;
    let ray = GridRay::new(DVec3::new(0.5, -gap, 0.5), DVec3::new(1.0, direction, 0.0)).unwrap();
    let contact = hit(ray
        .cast(options(0.01), |cell| {
            if cell.y == 0 {
                RayCell::Hit(SOLID)
            } else {
                RayCell::Empty
            }
        })
        .unwrap());
    assert_eq!(contact.position, BlockPos::default());
    assert_eq!(contact.face, Some(Face::NegY));
    assert_eq!(contact.distance, gap / direction);
}

#[test]
fn inclusive_reach_uses_the_same_crossing_calculation_after_the_first_cell() {
    let ray = GridRay::new(DVec3::new(0.0, 0.0, 0.5), DVec3::new(2.0, 3.0, 0.0)).unwrap();
    let reach = 3.0 / ray.direction().x;
    let contact = hit(ray
        .cast(options(reach), |cell| {
            if cell.x == 3 {
                RayCell::Hit(SOLID)
            } else {
                RayCell::Empty
            }
        })
        .unwrap());
    assert_eq!(contact.position.x, 3);
    assert_eq!(contact.distance, reach);
    assert_eq!(contact.face, Some(Face::NegX));
}

#[test]
fn repeated_asymmetric_corner_crossings_skip_side_cells() {
    let ray = GridRay::new(DVec3::new(0.0, 0.0, 0.5), DVec3::new(1.0, 3.0, 0.0)).unwrap();
    let target = BlockPos::new(15, 45, 0);
    let reach = 15.0 / ray.direction().x;
    assert_eq!(reach, 45.0 / ray.direction().y);
    let contact = hit(ray
        .cast(options(reach), |cell| {
            // This side cell is touched only at the corner and must be skipped.
            assert_ne!(cell, BlockPos::new(14, 45, 0));
            if cell == target {
                RayCell::Hit(SOLID)
            } else {
                RayCell::Empty
            }
        })
        .unwrap());
    assert_eq!(contact.position, target);
    assert_eq!(contact.distance, reach);
    assert_eq!(contact.face, Some(Face::NegX));
}
