//! Version-one seeded fixtures: native graphics, setup, and output drops are untimed.
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use rayengine_minecraft::terrain::{
    GENERATOR_VERSION, Terrain, TerrainSettings, chunk_fingerprint,
};
use rayengine_voxel::prelude::*;
use std::{collections::BTreeMap, hint::black_box, time::Duration};
fn workloads(c: &mut Criterion) {
    assert_eq!(GENERATOR_VERSION, 1);
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let mut group = c.benchmark_group("minecraft_generation_v1");
    group.throughput(Throughput::Elements(4096));
    for (name, position, fingerprint) in [
        ("sky", ChunkPos::new(0, 8, 0), 0xb9d103fd6854a325),
        (
            "bedrock_caves_ores",
            ChunkPos::new(0, 0, 0),
            0x090aafbca6ad9c74,
        ),
        ("underground", ChunkPos::new(2, 1, -2), 0xff984ffeeca60982),
        ("surface", ChunkPos::new(0, 3, 0), 0x0b77011da62e050d),
        ("canopy", ChunkPos::new(0, 4, 0), 0x69e9e5f97b14c6aa),
        (
            "negative_surface",
            ChunkPos::new(-1, 3, -2),
            0xfa327acc76a82f57,
        ),
        (
            "world_edge",
            ChunkPos::new(134217727, 2, -134217728),
            0xce92446ac541fe1e,
        ),
    ] {
        let chunk = terrain.chunk(position).unwrap();
        assert_eq!(chunk_fingerprint(&chunk), fingerprint);
        let mut counts = BTreeMap::new();
        for &id in chunk.blocks() {
            *counts.entry(id.raw()).or_insert(0) += 1;
        }
        eprintln!(
            "minecraft_generation_v1/{name}: seed=42 position={position:?} fingerprint={fingerprint:016x} blocks={counts:?}"
        );
        group.bench_function(name, |b| {
            b.iter_batched(
                || (),
                |()| black_box(terrain.chunk(black_box(position)).unwrap()),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
    c.bench_function("minecraft_spawn_v1/origin", |b| {
        b.iter(|| {
            black_box(
                terrain
                    .find_spawn(black_box(0), black_box(0), 16, 1089)
                    .unwrap(),
            )
        })
    });
    c.bench_function("minecraft_spawn_v1/negative", |b| {
        b.iter(|| {
            black_box(
                terrain
                    .find_spawn(black_box(-17), black_box(-1), 16, 1089)
                    .unwrap(),
            )
        })
    });
}
criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2));
    targets = workloads
}
criterion_main!(benches);
