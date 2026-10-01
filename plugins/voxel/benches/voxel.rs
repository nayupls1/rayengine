//! Stable CPU workloads: bounded fixture sizes, no graphics or filesystem I/O.
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayengine_voxel::{glam::DVec3, prelude::*};
use std::{hint::black_box, sync::Arc, time::Duration};
mod collision;
mod meshing;
mod streaming;

fn fixture() -> (Arc<BlockRegistry>, BlockId) {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("bench:stone")).unwrap();
    (Arc::new(registry), stone)
}
fn workloads(c: &mut Criterion) {
    let (registry, stone) = fixture();
    let mut storage = c.benchmark_group("voxel_storage");
    storage.throughput(Throughput::Bytes(8192));
    storage.bench_function("allocate_filled_4096", |b| {
        b.iter(|| black_box(Chunk::filled(black_box(registry.clone()), black_box(stone)).unwrap()));
    });
    let cells = vec![stone; CHUNK_VOLUME];
    storage.bench_function("validate_import_4096", |b| {
        // Clone input outside the timed region; measure validation/adoption; output drop is outside timing.
        b.iter_batched(
            || cells.clone(),
            |data| black_box(Chunk::from_blocks(registry.clone(), data).unwrap()),
            criterion::BatchSize::LargeInput,
        );
    });
    storage.finish();

    let chunk = Chunk::filled(registry.clone(), stone).unwrap();
    let mut access = c.benchmark_group("voxel_access");
    access.throughput(Throughput::Elements(1));
    access.bench_function("local", |b| {
        b.iter(|| black_box(black_box(&chunk).get(black_box(LocalPos::new(7, 8, 9).unwrap()))));
    });
    for count in [1, 64] {
        let mut world = VoxelWorld::new(registry.clone(), count);
        for x in 0..count {
            world
                .insert_chunk(
                    ChunkPos::new(x as i32, 0, 0),
                    Chunk::filled(registry.clone(), stone).unwrap(),
                )
                .unwrap();
        }
        let positions: Vec<_> = (0..1024)
            .map(|i| {
                BlockPos::new(
                    (i % count) as i32 * 16 + (i % 16) as i32,
                    (i / 16 % 16) as i32,
                    (i / 256 % 16) as i32,
                )
            })
            .collect();
        access.throughput(Throughput::Elements(positions.len() as u64));
        access.bench_function(BenchmarkId::new("world_reads_1024", count), |b| {
            b.iter(|| {
                for &position in black_box(&positions) {
                    black_box(world.block(position));
                }
            });
        });
    }
    access.finish();

    let mut world = VoxelWorld::new(registry.clone(), 1);
    world
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
        )
        .unwrap();
    let mut edits = c.benchmark_group("voxel_edits");
    edits.throughput(Throughput::Elements(2));
    for (name, position) in [
        ("interior_pair", BlockPos::new(7, 8, 9)),
        ("border_pair", BlockPos::new(0, 0, 0)),
    ] {
        edits.bench_function(name, |b| {
            b.iter(|| {
                black_box(
                    world
                        .set_block(black_box(position), black_box(stone))
                        .unwrap(),
                );
                black_box(
                    world
                        .set_block(black_box(position), black_box(BlockId::AIR))
                        .unwrap(),
                );
            });
        });
    }
    edits.finish();

    let mut rays = c.benchmark_group("voxel_raycast");
    for cells in [16, 256] {
        let ray = GridRay::new(DVec3::splat(0.5), DVec3::X).unwrap();
        let options = RaycastOptions {
            max_distance: cells as f64 - 1.0,
            max_cells: cells,
            ..Default::default()
        };
        assert_eq!(
            ray.cast(options, |_| RayCell::Empty).unwrap().visited_cells,
            cells
        );
        rays.throughput(Throughput::Elements(u64::from(cells)));
        rays.bench_function(BenchmarkId::new("empty_axis", cells), |b| {
            b.iter(|| {
                black_box(
                    black_box(ray)
                        .cast(black_box(options), |_| RayCell::Empty)
                        .unwrap(),
                )
            });
        });
    }
    // Hash lookup and predicate included: 16 air chunks, hit at x=255.
    let mut world = VoxelWorld::new(registry.clone(), 16);
    for x in 0..16 {
        world
            .insert_chunk(
                ChunkPos::new(x, 0, 0),
                Chunk::filled(registry.clone(), BlockId::AIR).unwrap(),
            )
            .unwrap();
    }
    world.set_block(BlockPos::new(255, 0, 0), stone).unwrap();
    let ray = GridRay::new(DVec3::splat(0.5), DVec3::X).unwrap();
    let options = RaycastOptions {
        max_distance: 256.0,
        max_cells: 512,
        ..Default::default()
    };
    assert_eq!(
        world
            .raycast(ray, options, |id, _| id != BlockId::AIR)
            .unwrap()
            .visited_cells,
        256
    );
    rays.throughput(Throughput::Elements(256));
    rays.bench_function("resident_hit_256", |b| {
        b.iter(|| {
            black_box(
                world
                    .raycast(black_box(ray), black_box(options), |id, _| {
                        id != BlockId::AIR
                    })
                    .unwrap(),
            )
        });
    });
    let diagonal = GridRay::new(DVec3::splat(0.5), DVec3::ONE).unwrap();
    let options = RaycastOptions {
        max_distance: 255.0 * 3.0_f64.sqrt(),
        max_cells: 512,
        ..Default::default()
    };
    assert_eq!(
        diagonal
            .cast(options, |_| RayCell::Empty)
            .unwrap()
            .visited_cells,
        256
    );
    rays.bench_function("empty_corner_256", |b| {
        b.iter(|| {
            black_box(
                black_box(diagonal)
                    .cast(black_box(options), |_| RayCell::Empty)
                    .unwrap(),
            )
        });
    });
    rays.finish();
}
criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
    targets = workloads, meshing::workloads, streaming::workloads, collision::workloads
}
criterion_main!(benches);
