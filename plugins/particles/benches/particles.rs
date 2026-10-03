//! Fixed seeded workloads; setup/storage allocation and drop are outside timing.
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use rayengine_particles::{Emitter, EmitterConfig};
use std::{hint::black_box, time::Duration};

fn particles(c: &mut Criterion) {
    let mut group = c.benchmark_group("particles_cpu_v1");
    for capacity in [128, 4_096, 32_768] {
        for workload in [
            "idle",
            "full_tick",
            "saturated_240_ticks",
            "oversized_burst",
            "admitted_burst",
        ] {
            group.bench_with_input(
                BenchmarkId::new(workload, capacity),
                &capacity,
                |b, &capacity| {
                    b.iter_batched_ref(
                        || {
                            let mut emitter = Emitter::new(EmitterConfig {
                                capacity,
                                max_spawn: capacity / 4,
                                lifetime: [10.0, 10.0],
                                seed: 42,
                                rate: if workload == "saturated_240_ticks" {
                                    1_000_000.0
                                } else {
                                    0.0
                                },
                                ..Default::default()
                            })
                            .unwrap();
                            if workload != "idle" && workload != "admitted_burst" {
                                for _ in 0..4 {
                                    emitter.burst(capacity);
                                }
                            }
                            emitter
                        },
                        |emitter| {
                            if workload == "oversized_burst" || workload == "admitted_burst" {
                                black_box(emitter.burst(black_box(usize::MAX)));
                            } else {
                                for _ in 0..if workload == "saturated_240_ticks" {
                                    240
                                } else {
                                    1
                                } {
                                    black_box(emitter.step(black_box(1.0 / 120.0)).unwrap());
                                }
                            }
                            black_box(emitter.particles());
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }
    }
    group.finish();
}
criterion_group! { name = benches; config = Criterion::default().sample_size(10).warm_up_time(Duration::from_millis(100)).measurement_time(Duration::from_millis(300)); targets = particles }
criterion_main!(benches);
