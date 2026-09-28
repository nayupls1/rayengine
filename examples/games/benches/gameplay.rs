//! Full fixed-update workloads; no window or GPU is initialized.

use criterion::{Criterion, criterion_group, criterion_main};
use rayengine::core::input::Input;
use rayengine_demos::{
    arena::{ATTACK, ArenaSimulation, JUMP, RIGHT},
    meadow::{FORWARD, MeadowSimulation, SPRINT},
};
use std::{hint::black_box, time::Duration};

fn gameplay(c: &mut Criterion) {
    let mut arena = ArenaSimulation::new();
    arena.enemy_ai = true;
    let mut arena_input = Input::with_capacity(6);
    let mut tick = 0u64;
    c.bench_function("arena_fixed_update", |b| {
        b.iter(|| {
            tick += 1;
            arena_input.set(RIGHT, tick % 240 < 120);
            arena_input.set(JUMP, tick.is_multiple_of(100));
            arena_input.set(ATTACK, tick.is_multiple_of(60));
            arena.step(black_box(&arena_input), black_box(1.0 / 120.0));
            arena_input.consume_edges();
            black_box(arena.fighter(0));
        })
    });
    let mut meadow = MeadowSimulation::new();
    let mut meadow_input = Input::with_capacity(9);
    meadow_input.set(FORWARD, true);
    meadow_input.set(SPRINT, true);
    c.bench_function("meadow_fixed_update", |b| {
        b.iter(|| {
            meadow.step(black_box(&meadow_input), black_box(1.0 / 120.0));
            black_box(meadow.explorer());
        })
    });
}

criterion_group! { name = benches; config = Criterion::default().sample_size(30).warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2)); targets = gameplay }
criterion_main!(benches);
