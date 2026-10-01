use super::*;
use crate::{BlockDef, BlockId, BlockRegistry, Chunk, ChunkPos};
use rayengine_core::first_person::{FirstPersonConfig, FirstPersonInput};
use std::sync::Arc;
fn fixture() -> (VoxelWorld, BlockId) {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("stone")).unwrap();
    let registry = Arc::new(registry);
    let mut world = VoxelWorld::new(registry.clone(), 64);
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                world
                    .insert_chunk(
                        ChunkPos::new(x, y, z),
                        Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    (world, stone)
}
#[test]
fn local_queries_cross_negative_boundaries_and_ignore_distant_chunks() {
    let (mut world, stone) = fixture();
    let p = BlockPos::new(-1, 0, -1);
    world.set_block(p, stone).unwrap();
    let bounds = Aabb3::from_center(Vec3::new(-0.5, 0.5, -0.5), Vec3::splat(0.8));
    let mut out = Vec::new();
    let report = world
        .collect_colliders(
            bounds,
            BlockPos::default(),
            8,
            MissingColliders::Reject,
            &mut out,
        )
        .unwrap();
    assert_eq!(
        report,
        ColliderReport {
            visited: 1,
            missing: 0,
            solids: 1
        }
    );
    assert_eq!(out, vec![block_bounds(p, BlockPos::default()).unwrap()]);
    world
        .insert_chunk(
            ChunkPos::new(500, 500, 500),
            Chunk::filled(world.shared_registry(), stone).unwrap(),
        )
        .unwrap();
    assert_eq!(
        world
            .collect_colliders(
                bounds,
                BlockPos::default(),
                8,
                MissingColliders::Reject,
                &mut out
            )
            .unwrap(),
        report
    );
    // A noncolliding block can still have visible geometry.
    let mut r = BlockRegistry::new();
    let mut definition = BlockDef::new("flower");
    definition.collision = CollisionKind::None;
    let flower = r.register(definition).unwrap();
    let r = Arc::new(r);
    let mut w = VoxelWorld::new(r.clone(), 1);
    w.insert_chunk(ChunkPos::default(), Chunk::filled(r, flower).unwrap())
        .unwrap();
    assert_eq!(
        w.collect_colliders(
            Aabb3::from_center(Vec3::splat(0.5), Vec3::splat(0.8)),
            BlockPos::default(),
            8,
            MissingColliders::Reject,
            &mut out
        )
        .unwrap()
        .solids,
        0
    );
}
#[test]
fn touching_cells_limits_and_failure_clear_partial_output() {
    let (world, _) = fixture();
    let mut out = vec![Aabb3::from_center(Vec3::ZERO, Vec3::ONE)];
    let bounds = Aabb3 {
        min: Vec3::ZERO,
        max: Vec3::ONE,
    };
    assert_eq!(
        world.collect_colliders(
            bounds,
            BlockPos::default(),
            26,
            MissingColliders::Reject,
            &mut out
        ),
        Err(ColliderError::BudgetExceeded)
    );
    assert!(out.is_empty());
    assert_eq!(
        world
            .collect_colliders(
                bounds,
                BlockPos::default(),
                27,
                MissingColliders::Reject,
                &mut out
            )
            .unwrap()
            .visited,
        27
    );
    let bounds = Aabb3::from_center(Vec3::new(15.5, 0.5, 0.5), Vec3::new(40.0, 0.5, 0.5));
    assert!(matches!(
        world.collect_colliders(
            bounds,
            BlockPos::default(),
            100,
            MissingColliders::Reject,
            &mut out
        ),
        Err(ColliderError::Unloaded(_))
    ));
    assert!(out.is_empty());
    let r = world
        .collect_colliders(
            bounds,
            BlockPos::default(),
            100,
            MissingColliders::Solid,
            &mut out,
        )
        .unwrap();
    assert!(r.missing > 0);
    assert_eq!(r.solids, r.missing);
    let r = world
        .collect_colliders(
            bounds,
            BlockPos::default(),
            100,
            MissingColliders::Skip,
            &mut out,
        )
        .unwrap();
    assert!(r.missing > 0);
    assert!(out.is_empty());
    for bounds in [
        Aabb3 {
            min: Vec3::splat(f32::NAN),
            max: Vec3::ONE,
        },
        Aabb3 {
            min: Vec3::ONE,
            max: Vec3::ZERO,
        },
        Aabb3 {
            min: Vec3::splat(2_000_000.0),
            max: Vec3::splat(2_000_001.0),
        },
    ] {
        assert_eq!(
            world.collect_colliders(
                bounds,
                BlockPos::default(),
                100,
                MissingColliders::Skip,
                &mut out
            ),
            Err(ColliderError::InvalidQuery)
        );
    }
}
#[test]
fn origin_subtraction_preserves_cells_at_both_grid_edges() {
    for x in [i32::MIN, i32::MAX - 1] {
        let origin = BlockPos::new(x, 0, x);
        let bounds = block_bounds(BlockPos::new(x + 1, 0, x), origin).unwrap();
        assert_eq!(bounds.min, Vec3::X);
        assert_eq!(bounds.size(), Vec3::ONE);
    }
    assert_eq!(
        block_bounds(BlockPos::new(i32::MAX, 0, 0), BlockPos::new(i32::MIN, 0, 0)),
        Err(ColliderError::InvalidQuery)
    );
}
#[test]
fn controller_jumps_lands_and_sweeps_wall_across_chunk_boundary() {
    let (mut world, stone) = fixture();
    for x in -5..=5 {
        for z in -5..=5 {
            world.set_block(BlockPos::new(x, 0, z), stone).unwrap();
        }
    }
    for y in 1..=5 {
        for z in -5..=5 {
            world.set_block(BlockPos::new(0, y, z), stone).unwrap();
        }
    }
    let mut player = FirstPersonController::new(
        Vec3::new(-1.0, 1.900001, 0.5),
        Vec3::new(0.6, 1.8, 0.6),
        FirstPersonConfig::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    let mut highest = player.body.position.y;
    for tick in 0..180 {
        let bounds = controller_bounds(&player, 1.0 / 60.0).unwrap();
        world
            .collect_colliders(
                bounds,
                BlockPos::default(),
                4096,
                MissingColliders::Reject,
                &mut out,
            )
            .unwrap();
        player.step(
            FirstPersonInput {
                movement: crate::glam::Vec2::X,
                jump_pressed: tick == 2,
                ..Default::default()
            },
            1.0 / 60.0,
            &out,
        );
        highest = highest.max(player.body.position.y);
        assert!(player.body.bounds().max.x <= 0.0);
    }
    assert!(highest > 3.0);
    assert!(player.body.grounded);
    assert!((player.body.bounds().min.y - 1.0).abs() < 0.0001);
    // Removing the wall is immediately reflected in collision, before remeshing.
    for y in 1..=5 {
        for z in -5..=5 {
            world
                .set_block(BlockPos::new(0, y, z), BlockId::AIR)
                .unwrap();
        }
    }
    for _ in 0..20 {
        world
            .collect_colliders(
                controller_bounds(&player, 1.0 / 60.0).unwrap(),
                BlockPos::default(),
                4096,
                MissingColliders::Reject,
                &mut out,
            )
            .unwrap();
        player.step(
            FirstPersonInput {
                movement: crate::glam::Vec2::X,
                ..Default::default()
            },
            1.0 / 60.0,
            &out,
        );
    }
    assert!(player.body.position.x > 0.0);
}
