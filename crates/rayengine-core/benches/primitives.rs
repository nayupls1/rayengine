//! Display-independent benchmarks with stable IDs for historical comparisons.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayengine_core::{prelude::*, transform::Parent};
use std::{hint::black_box, time::Duration};

struct Velocity(Vec3);

fn primitives(c: &mut Criterion) {
    let mut group = c.benchmark_group("ecs_update");
    for count in [1_000, 10_000, 100_000] {
        let mut world = World::new();
        world
            .spawn_batch((0..count).map(|i| {
                (
                    Transform3D::at(Vec3::new(i as f32, 0.0, 0.0)),
                    Velocity(Vec3::new(0.5, 0.2, -0.1)),
                )
            }))
            .for_each(drop);
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                for (transform, velocity) in world.query_mut::<(&mut Transform3D, &Velocity)>() {
                    transform.position += velocity.0 * black_box(1.0 / 120.0);
                }
                black_box(&world);
            })
        });
    }
    group.finish();

    let mut group = c.benchmark_group("scene_propagation");
    for parented in [false, true] {
        let mut scene = Scene::new();
        let root = scene.spawn_3d(Transform3D::default(), ());
        let mut parent = root;
        for i in 0..1000 {
            let child = scene.spawn_3d(Transform3D::at(Vec3::ONE), ());
            if parented {
                scene.world.insert_one(child, Parent(parent)).unwrap();
            }
            if i % 8 == 0 {
                parent = child;
            }
        }
        scene.propagate().unwrap();
        group.throughput(Throughput::Elements(1001));
        group.bench_function(
            if parented {
                "parented_1000"
            } else {
                "flat_1000"
            },
            |b| {
                b.iter(|| {
                    scene.propagate().unwrap();
                    black_box(&scene);
                })
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("character_collision");
    for count in [1, 100] {
        let solids2: Vec<_> = (0..count)
            .map(|i| Aabb2::from_center(Vec2::new(i as f32 * 20.0, 10.0), Vec2::new(10.0, 1.0)))
            .collect();
        let solids3: Vec<_> = (0..count)
            .map(|i| {
                Aabb3::from_center(
                    Vec3::new(i as f32 * 20.0, 0.0, 0.0),
                    Vec3::new(10.0, 1.0, 10.0),
                )
            })
            .collect();
        group.bench_with_input(BenchmarkId::new("2d", count), &count, |b, _| {
            b.iter(|| {
                let mut body = Body2D::new(Vec2::ZERO, Vec2::splat(2.0));
                body.velocity = Vec2::new(1.0, 1000.0);
                body.move_and_slide(black_box(1.0 / 120.0), black_box(&solids2));
                black_box(body);
            })
        });
        group.bench_with_input(BenchmarkId::new("3d", count), &count, |b, _| {
            b.iter(|| {
                let mut body = Body3D::new(Vec3::new(0.0, 10.0, 0.0), Vec3::splat(2.0));
                body.velocity = Vec3::new(1.0, -1000.0, 0.5);
                body.move_and_slide(black_box(1.0 / 120.0), black_box(&solids3));
                black_box(body);
            })
        });
    }
    group.finish();

    c.bench_function("viewport_fit_pointer_dpi", |b| {
        b.iter(|| {
            let view = Viewport::new(
                black_box(Vec2::new(2400.0, 900.0)),
                Vec2::new(960.0, 540.0),
                ScaleMode::Fit,
            )
            .unwrap();
            black_box(view.screen_to_ui(Vec2::new(1000.0, 400.0)));
            black_box(view.render_size(Vec2::splat(2.0)));
        })
    });
    let mut clock = FixedClock::new(120, 8);
    c.bench_function("fixed_clock_120hz", |b| {
        b.iter(|| black_box(clock.advance(black_box(Duration::from_millis(10)))))
    });
    let mut input = Input::with_capacity(64);
    c.bench_function("input_64_actions", |b| {
        b.iter(|| {
            for i in 0..64 {
                input.set(Action(i), black_box(i % 2 == 0));
            }
            input.consume_edges();
            black_box(&input);
        })
    });
    let mut timer = Timer::repeating(Duration::from_millis(250));
    c.bench_function("timer_repeating", |b| {
        b.iter(|| black_box(timer.advance(black_box(Duration::from_millis(10)))))
    });
    let mut events = Events::with_capacity(64);
    c.bench_function("event_queue_64", |b| {
        b.iter(|| {
            for i in 0..64 {
                events.send(black_box(i));
            }
            black_box(events.read());
            events.clear();
        })
    });
}

criterion_group! { name = benches; config = Criterion::default().sample_size(30).warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2)); targets = primitives }
criterion_main!(benches);
