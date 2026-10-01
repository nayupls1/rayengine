//! Fixed schema-one dense-history workloads; no timed disk/window/GPU work.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use rayengine_minecraft::{
    gameplay::Player,
    persistence::Snapshot,
    survival::Survival,
    terrain::{Terrain, TerrainSettings, chunk_fingerprint},
};
use rayengine_voxel::prelude::*;
use std::{hint::black_box, sync::Arc, time::Duration};
fn dirty_world(terrain: &Terrain, offset: usize, count: usize) -> VoxelWorld {
    let mut world = VoxelWorld::new(terrain.registry(), 160);
    for i in offset..offset + count {
        let cells = (0..CHUNK_VOLUME)
            .map(|i| BlockId::from_raw((i % 9) as u16))
            .collect();
        let chunk = Chunk::from_blocks(terrain.registry(), cells).unwrap();
        assert!(chunk.is_dirty());
        // Freeze IDs and cell ordering independently of terrain generation.
        assert_eq!(chunk_fingerprint(&chunk), 0x2731d4deb7933cdd);
        world
            .insert_chunk(ChunkPos::new(i as i32 - 512, 10, -8), chunk)
            .unwrap();
    }
    world
}
fn workloads(c: &mut Criterion) {
    assert_eq!(rayengine_minecraft::persistence::SCHEMA_VERSION, 1);
    assert_eq!(rayengine_minecraft::terrain::GENERATOR_VERSION, 1);
    let terrain = Arc::new(Terrain::new(42, TerrainSettings::default()).unwrap());
    let spawn = terrain.find_spawn(0, 0, 16, 1089).unwrap();
    let player = Player::new(spawn.feet()).unwrap();
    let survival = Survival::default();
    let base = Snapshot::new(terrain.clone(), spawn.support, &player, &survival).unwrap();
    let mut histories = Vec::new();
    for total in [1, 128, 1024] {
        let mut history = base.clone();
        for offset in (0..total).step_by(160) {
            let world = dirty_world(&terrain, offset, (total - offset).min(160));
            history = history.capture(&world, &player, &survival).unwrap().0;
        }
        assert_eq!(history.edited_chunks(), total);
        let bytes = history.encode().unwrap();
        let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        let expected = match total {
            1 => (8929, 0x6e67cfe3ed27f119),
            128 => (1053885, 0x6fba90c860b516ab),
            1024 => (8425443, 0x01bcbb29f2a8421e),
            _ => unreachable!(),
        };
        assert_eq!((bytes.len(), hash), expected);
        eprintln!(
            "minecraft_persistence_v1/history_{total}: payload_bytes={} fingerprint={hash:016x}",
            bytes.len()
        );
        let restored = Snapshot::decode(&bytes).unwrap();
        assert_eq!(restored.edited_chunks(), total);
        assert_eq!(restored.encode().unwrap(), bytes);
        histories.push((total, history, bytes));
    }
    let one = dirty_world(&terrain, 0, 1);
    let many = dirty_world(&terrain, 0, 160);
    let mut group = c.benchmark_group("minecraft_persistence_v1");
    for (id, checkpoint, world) in [
        ("capture_dirty_1", &base, &one),
        ("capture_dirty_160", &base, &many),
        ("capture_dirty_1_history_1024", &histories[2].1, &one),
    ] {
        group.bench_function(id, |b| {
            b.iter_batched(
                || (),
                |()| {
                    black_box(
                        checkpoint
                            .capture(black_box(world), black_box(&player), black_box(&survival))
                            .unwrap(),
                    )
                },
                BatchSize::LargeInput,
            )
        });
    }
    for (count, checkpoint, bytes) in &histories {
        group.bench_function(format!("encode_history_{count}"), |b| {
            b.iter_batched(
                || (),
                |()| black_box(black_box(checkpoint).encode().unwrap()),
                BatchSize::LargeInput,
            )
        });
        group.bench_function(format!("decode_history_{count}"), |b| {
            b.iter_batched(
                || (),
                |()| black_box(Snapshot::decode(black_box(bytes)).unwrap()),
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}
criterion_group! { name=benches; config=Criterion::default().warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2)); targets=workloads }
criterion_main!(benches);
