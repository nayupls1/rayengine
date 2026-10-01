//! Versioned multi-pump native transaction: six old meshes, twelve new meshes.
use criterion::{Criterion, Throughput};
use rayengine::{prelude::*, upload::UploadBudget};
use rayengine_voxel::prelude::*;
use std::{
    hint::black_box,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
fn ready(cpu: &mut ChunkStreamer, world: &mut VoxelWorld) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while cpu
        .tick(world, ChunkPos::default(), |_, _, _| Eviction::Keep)
        .unwrap()
        .ready
        == 0
    {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}
struct Fixture {
    cpu: ChunkStreamer,
    world: VoxelWorld,
    gpu: StreamRenderer,
}
fn setup(frame: &mut Frame<'_, '_>, materials: &VoxelMaterials) -> Fixture {
    let mut registry = BlockRegistry::new();
    let id = registry.register(BlockDef::new("bench:solid")).unwrap();
    let mut world = VoxelWorld::new(Arc::new(registry), 1);
    world
        .insert_chunk(
            ChunkPos::default(),
            Chunk::filled(world.shared_registry(), id).unwrap(),
        )
        .unwrap();
    let mut options = MeshingOptions::default();
    options.limits.max_vertices_per_batch = 4;
    let mut cpu = ChunkStreamer::new(
        StreamConfig {
            radius: 0,
            max_resident: 1,
            workers: 1,
            max_jobs: 1,
            max_meshes: 1,
            meshing: options,
            ..Default::default()
        },
        |_, r, _| Chunk::filled(r, BlockId::AIR),
    )
    .unwrap();
    let mut gpu = StreamRenderer::new(StreamRenderConfig {
        max_chunks: 1,
        max_meshes: 32,
        ..Default::default()
    })
    .unwrap();
    ready(&mut cpu, &mut world);
    let r = gpu.pump(
        &world,
        &mut cpu,
        materials,
        frame,
        UploadBudget {
            max_requests: 32,
            max_bytes: 4096,
            max_time: Duration::from_secs(1),
        },
    );
    assert_eq!(r.committed, 1);
    assert_eq!(r.resources.meshes, 6);
    world
        .set_block(BlockPos::new(8, 8, 8), BlockId::AIR)
        .unwrap();
    ready(&mut cpu, &mut world);
    Fixture { cpu, world, gpu }
}
fn replace(
    s: &mut Fixture,
    frame: &mut Frame<'_, '_>,
    materials: &VoxelMaterials,
) -> (usize, usize, usize) {
    let mut peak_meshes = 0;
    let mut peak_bytes = 0;
    let mut peak_staging = 0;
    for i in 0..12 {
        let r = s.gpu.pump(
            &s.world,
            &mut s.cpu,
            materials,
            frame,
            UploadBudget {
                max_requests: 1,
                max_bytes: 156,
                max_time: Duration::from_secs(1),
            },
        );
        assert!(r.error.is_none());
        assert_eq!(r.uploads.attempted, 1);
        assert_eq!(r.uploads.bytes, 156);
        assert_eq!(r.committed, usize::from(i == 11));
        peak_meshes = peak_meshes.max(r.peak_resources.meshes);
        peak_bytes = peak_bytes.max(r.peak_resources.buffer_bytes);
        peak_staging = peak_staging.max(r.peak_resources.staged_bytes);
        black_box(r);
    }
    assert_eq!((peak_meshes, peak_bytes, peak_staging), (18, 2808, 1872));
    (peak_meshes, peak_bytes, peak_staging)
}
pub fn workloads(c: &mut Criterion, frame: &mut Frame<'_, '_>, materials: &VoxelMaterials) {
    let mut probe = setup(frame, materials);
    eprintln!(
        "voxel_stream_upload_v1/replace_12_pumps peaks (GPU meshes/bytes, CPU staging bytes): {:?}",
        replace(&mut probe, frame, materials)
    );
    probe.gpu.unload(&mut probe.cpu, frame.assets);
    probe.cpu.shutdown();
    let mut group = c.benchmark_group("voxel_stream_upload_v1");
    group.throughput(Throughput::Elements(12));
    group.bench_function("replace_12_pumps", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iterations {
                let mut s = setup(frame, materials); // Worker start/generation/initial upload untimed.
                let start = Instant::now();
                black_box(replace(&mut s, frame, materials));
                elapsed += start.elapsed();
                s.gpu.unload(&mut s.cpu, frame.assets);
                s.cpu.shutdown(); // Untimed teardown.
            }
            elapsed
        })
    });
    group.finish();
}
