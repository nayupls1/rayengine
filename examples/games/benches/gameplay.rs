//! Full fixed-update workloads; no window or GPU is initialized.

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use rayengine::core::input::Input;
use rayengine_demos::{
    arena::{ATTACK, ArenaSimulation, JUMP, RIGHT},
    meadow::{FORWARD, MeadowSimulation, SPRINT},
};
use std::{hint::black_box, time::Duration};

fn gameplay(c: &mut Criterion) {
    let mut group = c.benchmark_group("gameplay");
    group.throughput(Throughput::Elements(240));
    // Reset fixtures outside the measured region, so every sample executes the
    // same two-second script rather than an arbitrarily aged simulation.
    group.bench_function("arena_scripted_240_ticks", |b| {
        b.iter_batched(
            || {
                let mut arena = ArenaSimulation::new();
                arena.enemy_ai = true;
                (arena, Input::with_capacity(6))
            },
            |(mut arena, mut input)| {
                for tick in 0u64..240 {
                    input.set(RIGHT, tick % 240 < 120);
                    input.set(JUMP, tick.is_multiple_of(100));
                    input.set(ATTACK, tick.is_multiple_of(60));
                    arena.step(black_box(&input), black_box(1.0 / 120.0));
                    input.consume_edges();
                }
                black_box((arena, input))
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("meadow_scripted_240_ticks", |b| {
        b.iter_batched(
            || (MeadowSimulation::new(), Input::with_capacity(9)),
            |(mut meadow, mut input)| {
                for tick in 0u64..240 {
                    input.set(FORWARD, true);
                    input.set(SPRINT, true);
                    input.set(rayengine_demos::meadow::JUMP, tick.is_multiple_of(70));
                    meadow.step(black_box(&input), black_box(1.0 / 120.0));
                    input.consume_edges();
                }
                black_box((meadow, input))
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group! { name = benches; config = Criterion::default().sample_size(30).warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2)); targets = gameplay }
criterion_main!(benches);
