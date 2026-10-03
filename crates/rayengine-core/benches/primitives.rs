//! Display-independent benchmarks with stable IDs for historical comparisons.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayengine_core::{prelude::*, transform::Parent};
use std::{hint::black_box, time::Duration};

#[path = "support/first_person.rs"]
mod first_person;
#[path = "support/pathfinding.rs"]
mod pathfinding;
#[path = "support/saves.rs"]
mod saves;

struct Velocity(Vec3);

fn primitives(c: &mut Criterion) {
    let mut group = c.benchmark_group("mesh_validation");
    for triangles in [1_000, 10_000] {
        let positions = vec![Vec3::ZERO; triangles * 3];
        let plain = MeshData::new(positions.clone());
        let indexed = MeshData {
            positions,
            normals: Some(vec![Vec3::Y; triangles * 3]),
            texcoords: Some(vec![Vec2::ZERO; triangles * 3]),
            colors: Some(vec![[255; 4]; triangles * 3]),
            indices: Some((0..triangles * 3).map(|i| i as u16).collect()),
        };
        group.throughput(Throughput::Elements(triangles as u64));
        for (name, mesh) in [("positions", plain), ("indexed_attributes", indexed)] {
            group.bench_with_input(BenchmarkId::new(name, triangles), &mesh, |b, mesh| {
                b.iter(|| black_box(black_box(mesh).validate().unwrap()));
            });
        }
    }
    group.finish();

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
    let mut tweens = vec![
        Tween::new(Vec2::ZERO, Vec2::ONE, Duration::from_millis(700))
            .with_ease(Ease::BackOut)
            .with_mode(TweenMode::PingPong);
        1_000
    ];
    c.bench_function("tween_ping_pong_1000", |b| {
        b.iter(|| {
            for tween in &mut tweens {
                tween.advance(black_box(Duration::from_millis(8)));
                black_box(tween.value());
            }
        })
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

fn spatial_queries(c: &mut Criterion) {
    let viewport = Viewport::new(
        Vec2::new(960.0, 540.0),
        Vec2::new(960.0, 540.0),
        ScaleMode::Fit,
    )
    .unwrap();
    let view2 = Frustum2D::from_camera(
        &Camera2D {
            target: Vec2::ZERO,
            rotation: 0.0,
            view_height: 12.0,
        },
        &viewport,
    )
    .unwrap();
    let view3 = Frustum3D::from_camera(
        &Camera3D {
            position: Vec3::new(-10.0, 0.0, 0.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            vertical_fov: 60.0,
        },
        &viewport,
        0.1,
        100.0,
    )
    .unwrap();
    let ray2 = Ray2::new(Vec2::new(-10.0, 0.0), Vec2::X).unwrap();
    let ray3 = Ray3::new(Vec3::new(-10.0, 0.0, 0.0), Vec3::X).unwrap();
    let area2 = Aabb2::from_center(Vec2::ZERO, Vec2::splat(8.0));
    let area3 = Aabb3::from_center(Vec3::ZERO, Vec3::splat(8.0));

    for count in [128, 4_096, 32_768] {
        // Reverse insertion keeps the closest collider last for the linear
        // reference, while BVH results are independent of source order.
        let entries2: Vec<_> = (0..count)
            .rev()
            .map(|i| {
                (
                    i,
                    Aabb2::from_center(Vec2::new(i as f32 * 4.0, 0.0), Vec2::splat(2.0)),
                )
            })
            .collect();
        let entries3: Vec<_> = (0..count)
            .rev()
            .map(|i| {
                (
                    i,
                    Aabb3::from_center(Vec3::new(i as f32 * 4.0, 0.0, 0.0), Vec3::splat(2.0)),
                )
            })
            .collect();
        let mut index2 = SpatialIndex2D::new();
        index2.rebuild(entries2.iter().copied()).unwrap();
        let mut index3 = SpatialIndex3D::new();
        index3.rebuild(entries3.iter().copied()).unwrap();

        let mut rays = c.benchmark_group("spatial_ray");
        rays.bench_function(BenchmarkId::new("index_2d", count), |b| {
            b.iter(|| black_box(index2.nearest(black_box(ray2), f32::INFINITY).unwrap()))
        });
        rays.bench_function(BenchmarkId::new("index_3d", count), |b| {
            b.iter(|| black_box(index3.nearest(black_box(ray3), f32::INFINITY).unwrap()))
        });
        rays.bench_function(BenchmarkId::new("linear_2d", count), |b| {
            b.iter(|| {
                let mut nearest = f32::INFINITY;
                for (_, bounds) in black_box(&entries2) {
                    if let Some(hit) = ray2.cast(*bounds, nearest).unwrap() {
                        nearest = hit.distance;
                    }
                }
                black_box(nearest)
            })
        });
        rays.bench_function(BenchmarkId::new("linear_3d", count), |b| {
            b.iter(|| {
                let mut nearest = f32::INFINITY;
                for (_, bounds) in black_box(&entries3) {
                    if let Some(hit) = ray3.cast(*bounds, nearest).unwrap() {
                        nearest = hit.distance;
                    }
                }
                black_box(nearest)
            })
        });
        rays.finish();

        let mut proximity = c.benchmark_group("spatial_nearby");
        proximity.bench_function(BenchmarkId::new("2d", count), |b| {
            b.iter(|| {
                index2
                    .visit_overlapping(black_box(area2), |id| {
                        black_box(id);
                    })
                    .unwrap();
            })
        });
        proximity.bench_function(BenchmarkId::new("3d", count), |b| {
            b.iter(|| {
                index3
                    .visit_overlapping(black_box(area3), |id| {
                        black_box(id);
                    })
                    .unwrap();
            })
        });
        proximity.finish();

        let mut visible = c.benchmark_group("spatial_visible");
        visible.bench_function(BenchmarkId::new("2d", count), |b| {
            b.iter(|| {
                index2.visit_visible(black_box(&view2), |id| {
                    black_box(id);
                })
            })
        });
        visible.bench_function(BenchmarkId::new("3d", count), |b| {
            b.iter(|| {
                index3.visit_visible(black_box(&view3), |id| {
                    black_box(id);
                })
            })
        });
        visible.finish();

        let mut rebuild = c.benchmark_group("spatial_rebuild");
        rebuild.throughput(Throughput::Elements(count as u64));
        rebuild.bench_function(BenchmarkId::new("2d", count), |b| {
            b.iter(|| {
                black_box(
                    index2
                        .rebuild(black_box(&entries2).iter().copied())
                        .unwrap(),
                )
            })
        });
        rebuild.bench_function(BenchmarkId::new("3d", count), |b| {
            b.iter(|| {
                black_box(
                    index3
                        .rebuild(black_box(&entries3).iter().copied())
                        .unwrap(),
                )
            })
        });
        rebuild.finish();
    }
}

fn background_jobs(c: &mut Criterion) {
    fn generate(size: usize) -> Vec<u8> {
        let mut data = vec![0; size];
        for (i, value) in data.iter_mut().enumerate() {
            *value = (i as u8).wrapping_mul(13);
        }
        data
    }
    let pool = JobPool::new(1, 8).unwrap();
    let mut latency = c.benchmark_group("jobs_latency");
    for size in [0, 1_024, 65_536] {
        latency.bench_function(BenchmarkId::new("synchronous", size), |b| {
            b.iter(|| black_box(generate(black_box(size))))
        });
        latency.bench_function(BenchmarkId::new("worker_roundtrip", size), |b| {
            b.iter(|| {
                pool.try_submit(move |_| generate(size)).unwrap();
                loop {
                    if let Some(completion) = pool.try_recv() {
                        let JobOutcome::Ready(data) = completion.outcome else {
                            panic!("benchmark job failed")
                        };
                        black_box(data);
                        break;
                    }
                    std::thread::yield_now();
                }
            })
        });
    }
    latency.finish();
    c.bench_function("jobs_batch_8x1024", |b| {
        b.iter(|| {
            for _ in 0..8 {
                pool.try_submit(|_| generate(1024)).unwrap();
            }
            let mut received = 0;
            while received < 8 {
                if let Some(completion) = pool.try_recv() {
                    assert!(matches!(completion.outcome, JobOutcome::Ready(_)));
                    black_box(completion);
                    received += 1;
                } else {
                    std::thread::yield_now();
                }
            }
        })
    });
    c.bench_function("jobs_empty_poll", |b| b.iter(|| black_box(pool.try_recv())));
    let full = JobPool::new(1, 1).unwrap();
    full.try_submit(|_| 0_u32).unwrap();
    c.bench_function("jobs_full_rejection", |b| {
        b.iter(|| black_box(full.try_submit(|_| 1_u32).is_err()))
    });
}

fn ui_interaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("ui_interaction");
    for count in [1, 32] {
        let mut regions: Vec<_> = (0..count)
            .map(|i| {
                UiRegion::new(
                    UiId(i as u64),
                    Aabb2::from_center(Vec2::new(i as f32 * 40.0, 20.0), Vec2::splat(30.0)),
                )
            })
            .collect();
        let mut ui = UiState::with_capacity(count);
        let idle = UiInput {
            pointer: Some(regions[count - 1].bounds.center()),
            window_focused: true,
            ..UiInput::default()
        };
        group.bench_function(BenchmarkId::new("hover", count), |b| {
            b.iter(|| black_box(ui.update(black_box(&regions), black_box(idle))))
        });
        group.bench_function(BenchmarkId::new("click_pair", count), |b| {
            b.iter(|| {
                black_box(ui.update(
                    &regions,
                    UiInput {
                        primary: UiButton {
                            down: true,
                            pressed: true,
                            released: false,
                        },
                        ..idle
                    },
                ));
                black_box(ui.update(
                    &regions,
                    UiInput {
                        primary: UiButton {
                            released: true,
                            ..UiButton::default()
                        },
                        ..idle
                    },
                ));
            })
        });
        group.bench_function(BenchmarkId::new("keyboard_next", count), |b| {
            b.iter(|| {
                black_box(ui.update(
                    &regions,
                    UiInput {
                        next: true,
                        window_focused: true,
                        ..UiInput::default()
                    },
                ))
            })
        });
        regions[count - 1].draggable = true;
        group.bench_function(BenchmarkId::new("drag_triplet", count), |b| {
            b.iter(|| {
                black_box(ui.update(
                    &regions,
                    UiInput {
                        primary: UiButton {
                            down: true,
                            pressed: true,
                            released: false,
                        },
                        ..idle
                    },
                ));
                black_box(ui.update(
                    &regions,
                    UiInput {
                        pointer: idle.pointer.map(|p| p + Vec2::splat(5.0)),
                        primary: UiButton {
                            down: true,
                            ..UiButton::default()
                        },
                        ..idle
                    },
                ));
                black_box(ui.update(
                    &regions,
                    UiInput {
                        primary: UiButton {
                            released: true,
                            ..UiButton::default()
                        },
                        ..idle
                    },
                ));
            })
        });
    }
    group.finish();
    let mut raw = Input::with_capacity(8);
    let actions: Vec<_> = (0..8).map(Action).collect();
    for &action in &actions {
        raw.set(action, true);
    }
    raw.add_pointer_delta(Vec2::ONE);
    let mut routing = c.benchmark_group("ui_routing");
    for masked in [false, true] {
        routing.bench_function(if masked { "masked_8" } else { "unmasked_8" }, |b| {
            b.iter(|| {
                let view = raw.routed(black_box(if masked { &actions } else { &[] }), masked);
                for &action in &actions {
                    black_box(view.down(black_box(action)));
                    black_box(view.pressed(black_box(action)));
                }
                black_box(view.pointer_delta());
            })
        });
    }
    routing.finish();
}

criterion_group! { name = benches; config = Criterion::default().sample_size(30).warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(2)); targets = primitives, spatial_queries, background_jobs, ui_interaction, saves::containers, saves::files, first_person::controller, pathfinding::queries }
criterion_main!(benches);
