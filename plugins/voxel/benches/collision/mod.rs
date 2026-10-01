//! Versioned player-local fixtures. Setup and correctness checks are untimed.
use criterion::{BenchmarkId, Criterion, Throughput};
use rayengine_core::collision::Aabb3;
use rayengine_voxel::{
    glam::{DVec3, Vec3},
    prelude::*,
};
use std::{hint::black_box, sync::Arc};
pub fn workloads(c: &mut Criterion) {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("bench:stone")).unwrap();
    let registry = Arc::new(registry);
    let mut group = c.benchmark_group("voxel_colliders_v1");
    for residents in [8, 512] {
        let mut world = VoxelWorld::new(registry.clone(), residents);
        for x in -1..=0 {
            for y in -1..=0 {
                for z in -1..=0 {
                    world
                        .insert_chunk(
                            ChunkPos::new(x, y, z),
                            Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
                        )
                        .unwrap();
                }
            }
        }
        for x in -3..=3 {
            for z in -3..=3 {
                world.set_block(BlockPos::new(x, 0, z), stone).unwrap();
            }
        }
        for x in 0..residents - 8 {
            world
                .insert_chunk(
                    ChunkPos::new(x as i32 + 10, 0, 0),
                    Chunk::filled(registry.clone(), stone).unwrap(),
                )
                .unwrap();
        }
        for (name, bounds, visited, solids) in [
            (
                "standing_negative",
                Aabb3::from_center(Vec3::new(-1.0, 2.15, -1.0), Vec3::new(0.6, 2.7, 0.6)),
                16,
                4,
            ),
            (
                "swept_negative",
                Aabb3::from_center(Vec3::new(-0.5, 2.5, -0.5), Vec3::new(4.0, 6.0, 4.0)),
                175,
                25,
            ),
        ] {
            let mut output = Vec::new();
            let report = world
                .collect_colliders(
                    bounds,
                    BlockPos::default(),
                    4096,
                    MissingColliders::Reject,
                    &mut output,
                )
                .unwrap();
            assert_eq!((report.visited, report.solids), (visited, solids));
            group.throughput(Throughput::Elements(visited as u64));
            group.bench_function(BenchmarkId::new(name, residents), |b| {
                b.iter(|| {
                    black_box(
                        world
                            .collect_colliders(
                                black_box(bounds),
                                BlockPos::default(),
                                4096,
                                MissingColliders::Reject,
                                &mut output,
                            )
                            .unwrap(),
                    )
                })
            });
        }
    }
    group.finish();
    let mut group = c.benchmark_group("voxel_interaction_v1");
    let mut world = VoxelWorld::new(registry.clone(), 2);
    for z in -1..=0 {
        world
            .insert_chunk(
                ChunkPos::new(-1, 0, z),
                Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
            )
            .unwrap();
    }
    world.set_block(BlockPos::new(-1, 2, -5), stone).unwrap();
    let options = RaycastOptions {
        max_distance: 5.0,
        max_cells: 64,
        missing: MissingPolicy::Stop,
    };
    for (name, direction, visited) in [
        ("hit_negative_boundary", -DVec3::Z, 6),
        ("miss_five_blocks", DVec3::Z, 6),
        ("unloaded_boundary", DVec3::X, 2),
    ] {
        let ray = GridRay::new(DVec3::new(-0.5, 2.6, 0.5), direction).unwrap();
        let query = || {
            world
                .raycast(ray, options, |id, def| {
                    id != BlockId::AIR && def.render != RenderKind::Invisible
                })
                .unwrap()
        };
        let report = query();
        assert_eq!(report.visited_cells, visited);
        match name {
            "hit_negative_boundary" => assert!(matches!(report.outcome, RaycastOutcome::Hit(_))),
            "miss_five_blocks" => assert_eq!(report.outcome, RaycastOutcome::Miss),
            _ => assert!(matches!(report.outcome, RaycastOutcome::Unloaded { .. })),
        }
        group.throughput(Throughput::Elements(u64::from(visited)));
        group.bench_function(name, |b| b.iter(|| black_box(query())));
    }
    group.finish();
}
