//! Uniform-grid broadphase and fixed world tick scaling.
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayengine_core::prelude::*;
use std::hint::black_box;

fn broadphase(c: &mut Criterion) {
    for count in [128, 512, 2048] {
        let boxes2: Vec<_> = (0..count)
            .map(|i| {
                Aabb2::from_center(
                    Vec2::new((i % 32) as f32 * 3.0, (i / 32) as f32 * 3.0),
                    Vec2::splat(2.0),
                )
            })
            .collect();
        let boxes3: Vec<_> = (0..count)
            .map(|i| {
                Aabb3::from_center(
                    Vec3::new(
                        (i % 16) as f32 * 3.0,
                        ((i / 16) % 16) as f32 * 3.0,
                        (i / 256) as f32 * 3.0,
                    ),
                    Vec3::splat(2.0),
                )
            })
            .collect();
        let mut grid2 = UniformGrid2D::new(4.0);
        let mut grid3 = UniformGrid3D::new(4.0);
        for (i, &b) in boxes2.iter().enumerate() {
            grid2.insert(i as u64, b);
        }
        for (i, &b) in boxes3.iter().enumerate() {
            grid3.insert(i as u64, b);
        }
        let mut group = c.benchmark_group("physics_broadphase_query_all");
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::new("grid_2d", count), &count, |b, _| {
            b.iter(|| {
                for &bounds in &boxes2 {
                    black_box(grid2.query(black_box(bounds)));
                }
            })
        });
        group.bench_with_input(BenchmarkId::new("grid_3d", count), &count, |b, _| {
            b.iter(|| {
                for &bounds in &boxes3 {
                    black_box(grid3.query(black_box(bounds)));
                }
            })
        });
        group.bench_with_input(BenchmarkId::new("linear_2d", count), &count, |b, _| {
            b.iter(|| {
                for bounds in &boxes2 {
                    black_box(
                        boxes2
                            .iter()
                            .filter(|other| bounds.intersects(other))
                            .count(),
                    );
                }
            })
        });
        group.bench_with_input(BenchmarkId::new("linear_3d", count), &count, |b, _| {
            b.iter(|| {
                for bounds in &boxes3 {
                    black_box(
                        boxes3
                            .iter()
                            .filter(|other| bounds.intersects(other))
                            .count(),
                    );
                }
            })
        });
        group.finish();
        let mut rebuild = c.benchmark_group("physics_broadphase_rebuild");
        rebuild.throughput(Throughput::Elements(count as u64));
        rebuild.bench_function(BenchmarkId::new("2d", count), |b| {
            b.iter(|| {
                grid2.clear();
                for (i, &bounds) in boxes2.iter().enumerate() {
                    grid2.insert(i as u64, black_box(bounds));
                }
            })
        });
        rebuild.bench_function(BenchmarkId::new("3d", count), |b| {
            b.iter(|| {
                grid3.clear();
                for (i, &bounds) in boxes3.iter().enumerate() {
                    grid3.insert(i as u64, black_box(bounds));
                }
            })
        });
        rebuild.finish();
        let mut world2 = PhysicsWorld2D::new(4.0);
        let mut world3 = PhysicsWorld3D::new(4.0);
        for &bounds in &boxes2 {
            world2.insert(PhysicsBody2D::new(
                bounds.center(),
                Shape2D::box_shape(bounds.size()),
            ));
        }
        for &bounds in &boxes3 {
            world3.insert(PhysicsBody3D::new(
                bounds.center(),
                Shape3D::box_shape(bounds.size()),
            ));
        }
        let mut events = Events::default();
        let mut step = c.benchmark_group("physics_world_fixed_tick");
        step.throughput(Throughput::Elements(count as u64));
        step.bench_function(BenchmarkId::new("2d", count), |b| {
            b.iter(|| {
                black_box(world2.step(
                    Tick {
                        index: 0,
                        dt: 1.0 / 120.0,
                    },
                    &mut events,
                ))
            })
        });
        step.bench_function(BenchmarkId::new("3d", count), |b| {
            b.iter(|| {
                black_box(world3.step(
                    Tick {
                        index: 0,
                        dt: 1.0 / 120.0,
                    },
                    &mut events,
                ))
            })
        });
        step.finish();
    }
}
criterion_group!(benches, broadphase);
criterion_main!(benches);
