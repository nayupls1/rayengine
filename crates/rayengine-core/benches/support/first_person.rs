use criterion::{BenchmarkId, Criterion};
use rayengine_core::prelude::*;
use std::hint::black_box;

pub fn controller(c: &mut Criterion) {
    let mut group = c.benchmark_group("first_person");
    let mut fixture = FirstPersonController::new(
        Vec3::new(0.0, 0.9, 0.0),
        Vec3::new(0.8, 1.8, 0.8),
        FirstPersonConfig::default(),
    )
    .unwrap();
    let input = FirstPersonInput {
        movement: Vec2::new(1.0, -1.0),
        look_delta: Vec2::new(2.0, -0.5),
        sprint: true,
        ..Default::default()
    };
    for count in [1, 100] {
        let solids: Vec<_> = (0..count)
            .map(|i| {
                Aabb3::from_center(
                    Vec3::new(i as f32 * 20.0, -0.5, 0.0),
                    Vec3::new(10.0, 1.0, 10.0),
                )
            })
            .collect();
        fixture.step(Default::default(), 1.0 / 120.0, &solids);
        group.bench_function(BenchmarkId::new("tick", count), |b| {
            b.iter(|| {
                // Copy a settled, fixed fixture each sample. No construction or allocation.
                let mut player = fixture;
                player.step(black_box(input), black_box(1.0 / 120.0), black_box(&solids));
                black_box(player);
            });
        });
    }
    fixture.step(
        FirstPersonInput {
            jump_pressed: true,
            ..Default::default()
        },
        1.0 / 120.0,
        &[Aabb3::from_center(
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(10.0, 1.0, 10.0),
        )],
    );
    group.bench_function("camera_interpolated", |b| {
        b.iter(|| black_box(black_box(&fixture).camera(black_box(0.5))));
    });
    group.finish();
}
