//! Identical submission callback; CPU traversal/culling only, no GPU timing.
use criterion::{Criterion, criterion_group, criterion_main};
use rayengine_core::{
    camera::Camera2D,
    glam::Vec2,
    sprite::SpriteRegion,
    viewport::{ScaleMode, Viewport},
};
use rayengine_tilemap::{CollisionFlags, TileDefinition, TileId, Tilemap};
use std::{hint::black_box, time::Duration};
fn tilemap(c: &mut Criterion) {
    let mut map = Tilemap::new(
        512,
        512,
        Vec2::ZERO,
        Vec2::splat(32.0),
        vec![TileDefinition {
            region: SpriteRegion::new(0, 0, 16, 16).unwrap(),
            collision: CollisionFlags::default(),
        }],
        vec!["ground".into()],
    )
    .unwrap();
    for y in 0..512 {
        for x in 0..512 {
            map.set_tile(0, x, y, Some(TileId(0))).unwrap();
        }
    }
    let camera = Camera2D {
        target: Vec2::splat(8192.0),
        view_height: 540.0,
        ..Default::default()
    };
    let view = Viewport::new(
        Vec2::new(1920.0, 1080.0),
        Vec2::new(960.0, 540.0),
        ScaleMode::Fit,
    )
    .unwrap();
    let stats = map.visit_visible(&camera, &view, |_| {}).unwrap();
    assert!(stats.visible_chunks < 20 && stats.tiles < 1024);
    let mut group = c.benchmark_group("tilemap_submission_512x512");
    group.bench_function("naive_full_map", |b| {
        b.iter(|| {
            for y in 0..512 {
                for x in 0..512 {
                    if let Some(id) = black_box(&map).tile(0, x, y) {
                        black_box((id, map.tile_bounds(x, y).unwrap()));
                    }
                }
            }
        })
    });
    group.bench_function("culled_chunks", |b| {
        b.iter(|| {
            black_box(
                map.visit_visible(black_box(&camera), black_box(&view), |tile| {
                    black_box((tile.tile, tile.bounds));
                })
                .unwrap(),
            );
        })
    });
    group.finish();
}
criterion_group! { name = benches; config = Criterion::default().sample_size(20).warm_up_time(Duration::from_millis(300)).measurement_time(Duration::from_secs(1)); targets = tilemap }
criterion_main!(benches);
