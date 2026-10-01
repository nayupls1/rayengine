use criterion::{BatchSize, Criterion, Throughput};
use rayengine_voxel::prelude::*;
use std::hint::black_box;
#[path = "../../examples/support/streaming.rs"]
mod fixture;
pub fn workloads(c: &mut Criterion) {
    let mut group = c.benchmark_group("voxel_stream_v1");
    let mut s = fixture::fixture(1);
    let mut peaks = fixture::Peaks::default();
    fixture::settle(&mut s, ChunkPos::default(), &mut peaks);
    eprintln!("stream idle_9 peaks: {peaks:?}");
    group.throughput(Throughput::Elements(9));
    group.bench_function("idle_9", |b| {
        b.iter(|| {
            black_box(
                s.cpu
                    .tick(&mut s.world, black_box(ChunkPos::default()), |_, _, _| {
                        Eviction::Keep
                    })
                    .unwrap(),
            )
        })
    });
    let mut probe = fixture::fixture(0);
    eprintln!(
        "stream travel_16 peaks: {:?}",
        fixture::travel_16(&mut probe)
    );
    probe.cpu.shutdown();
    group.throughput(Throughput::Elements(16));
    group.bench_function("travel_16", |b| {
        b.iter_batched(
            || fixture::fixture(0),
            |mut s| {
                black_box(fixture::travel_16(&mut s));
                s
            },
            BatchSize::PerIteration,
        )
    });
    group.finish();
}
