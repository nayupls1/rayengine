//! Generation fixtures; geometry checks/setup/output drops are outside timings.
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput};
use rayengine_voxel::prelude::*;
use std::{hint::black_box, sync::Arc};

pub fn workloads(c: &mut Criterion) {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("bench:stone")).unwrap();
    let mut dirt = BlockDef::new("bench:dirt");
    dirt.textures = [TileId(1); 6];
    let dirt = registry.register(dirt).unwrap();
    let mut leaves = BlockDef::new("bench:leaves");
    leaves.render = RenderKind::Cutout;
    let leaves = registry.register(leaves).unwrap();
    let registry = Arc::new(registry);
    let mut group = c.benchmark_group("voxel_meshing");
    group.throughput(Throughput::Elements(4096));
    for name in ["solid", "terrain", "checkerboard", "mixed_tiles", "cutout"] {
        let cells = (0..4096)
            .map(|i| {
                let local = LocalPos::from_index(i).unwrap();
                let (x, y, z) = (local.x(), local.y(), local.z());
                match name {
                    "solid" => stone,
                    "terrain" if y < 4 + x / 4 + z / 4 => stone,
                    "checkerboard" if (x + y + z).is_multiple_of(2) => stone,
                    "mixed_tiles" => {
                        if (x / 2 + y / 2 + z / 2).is_multiple_of(2) {
                            stone
                        } else {
                            dirt
                        }
                    }
                    "cutout" => leaves,
                    _ => BlockId::AIR,
                }
            })
            .collect();
        let mut world = VoxelWorld::new(registry.clone(), 1);
        world
            .insert_chunk(
                ChunkPos::default(),
                Chunk::from_blocks(registry.clone(), cells).unwrap(),
            )
            .unwrap();
        let input = MeshInput::capture(&world, ChunkPos::default()).unwrap();
        for (label, mode) in [
            ("culled", MeshingMode::Culled),
            ("greedy", MeshingMode::Greedy),
        ] {
            let options = MeshingOptions {
                mode,
                ..Default::default()
            };
            let stats = input.build(options).unwrap().stats();
            eprintln!("voxel_meshing/{name}/{label}: {stats:?}");
            group.bench_function(BenchmarkId::new(name, label), |b| {
                b.iter_batched(
                    || (),
                    |()| black_box(black_box(&input).build(black_box(options)).unwrap()),
                    BatchSize::LargeInput,
                );
            });
        }
    }
    group.finish();
    let mut world = VoxelWorld::new(registry.clone(), 7);
    for pos in std::iter::once(ChunkPos::default())
        .chain(Face::ALL.map(|face| ChunkPos::default().neighbor(face).unwrap()))
    {
        world
            .insert_chunk(pos, Chunk::filled(registry.clone(), stone).unwrap())
            .unwrap();
    }
    c.bench_function("voxel_snapshot/capture_7_chunks", |b| {
        b.iter_batched(
            || (),
            |()| black_box(MeshInput::capture(black_box(&world), ChunkPos::default()).unwrap()),
            BatchSize::LargeInput,
        );
    });
}
