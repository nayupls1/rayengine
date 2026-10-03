use criterion::{BenchmarkId, Criterion, Throughput};
use rayengine_core::pathfinding::smooth_path;
use rayengine_core::prelude::*;
use std::hint::black_box;

/// Serpentine walls every eight columns force long corner-to-corner routes,
/// with scattered rough cells (cost 3) so costs matter.
fn maze(size: u32) -> CostGrid {
    let mut grid = CostGrid::new(UVec2::splat(size), 1.0);
    for y in 0..size {
        for x in 0..size {
            if (x * 7 + y * 13) % 11 == 0 {
                grid.set(UVec2::new(x, y), Some(3.0));
            }
        }
    }
    for (wall, x) in (4..size).step_by(8).enumerate() {
        // Alternate a two-cell gap between the bottom and top rows.
        let (min, max) = if wall % 2 == 0 {
            (0, size - 3)
        } else {
            (2, size - 1)
        };
        grid.block_rect(UVec2::new(x, min), UVec2::new(x, max));
    }
    grid
}

pub fn queries(c: &mut Criterion) {
    let four = PathOptions {
        neighborhood: Neighborhood::Four,
        ..PathOptions::default()
    };
    let eight = PathOptions::default();
    for size in [64, 256, 512] {
        let grid = maze(size);
        let open = CostGrid::new(UVec2::splat(size), 1.0);
        let (start, goal) = (UVec2::ZERO, UVec2::splat(size - 1));
        let cells = u64::from(size * size);
        let mut finder = PathFinder::new();
        let mut path = Vec::new();
        // Warm buffers so measured queries reuse them, as in a running game.
        for options in [&four, &eight] {
            let status = finder
                .find_path(&grid, start, goal, options, &mut path)
                .unwrap();
            assert!(matches!(status, PathStatus::Found { .. }));
        }

        let mut group = c.benchmark_group("pathfinding_astar");
        for (name, grid, options) in [
            ("maze_four", &grid, &four),
            ("maze_eight", &grid, &eight),
            ("open_eight", &open, &eight),
        ] {
            // Throughput counts expanded cells: an open map expands only the
            // diagonal, a maze most of the grid.
            finder
                .find_path(grid, start, goal, options, &mut path)
                .unwrap();
            group.throughput(Throughput::Elements(finder.expanded()));
            group.bench_function(BenchmarkId::new(name, size), |b| {
                b.iter(|| {
                    black_box(
                        finder
                            .find_path(grid, black_box(start), goal, options, &mut path)
                            .unwrap(),
                    )
                })
            });
        }
        group.finish();

        finder
            .find_path(&grid, start, goal, &eight, &mut path)
            .unwrap();
        let fixture = path.clone();
        let mut group = c.benchmark_group("pathfinding_smooth");
        group.throughput(Throughput::Elements(fixture.len() as u64));
        group.bench_function(BenchmarkId::new("maze_eight", size), |b| {
            b.iter(|| {
                path.clone_from(&fixture);
                smooth_path(&grid, &mut path, eight.neighborhood, Vec2::splat(0.3)).unwrap();
                black_box(path.len())
            })
        });
        group.finish();

        let mut field = DistanceField::new();
        field.compute(&grid, [goal], eight.neighborhood).unwrap();
        let mut group = c.benchmark_group("pathfinding_field");
        group.throughput(Throughput::Elements(cells));
        group.bench_function(BenchmarkId::new("maze_eight", size), |b| {
            b.iter(|| {
                field
                    .compute(&grid, [black_box(goal)], eight.neighborhood)
                    .unwrap();
                black_box(field.distance(start))
            })
        });
        group.finish();
    }
}
