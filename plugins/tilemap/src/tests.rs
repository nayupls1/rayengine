use super::*;
use rayengine_core::{
    camera::Camera2D,
    collision::Body2D,
    spatial::{Frustum2D, Ray2, SpatialIndex2D},
    viewport::{ScaleMode, Viewport},
};
fn map(width: u32, height: u32) -> Tilemap {
    Tilemap::new(
        width,
        height,
        Vec2::new(-8.0, -4.0),
        Vec2::new(2.0, 4.0),
        vec![
            TileDefinition {
                region: SpriteRegion::new(0, 0, 8, 8).unwrap(),
                collision: CollisionFlags {
                    solid: true,
                    ..Default::default()
                },
            },
            TileDefinition {
                region: SpriteRegion::new(8, 0, 8, 8).unwrap(),
                collision: CollisionFlags {
                    one_way: true,
                    trigger: 2,
                    custom: 4,
                    ..Default::default()
                },
            },
        ],
        vec!["ground".into(), "overlay".into()],
    )
    .unwrap()
}
const LEVEL: &str = include_str!("../examples/assets/level.toml");
#[test]
fn format_loads_layers_flags_and_unicode() {
    let map = Tilemap::from_toml(LEVEL).unwrap();
    assert_eq!(map.dimensions(), (20, 10));
    assert_eq!(map.layer_name(1), Some("triggers"));
    assert_eq!(map.atlas_asset(), Some("tiles.png"));
    let tile = map.tile(1, 17, 7).unwrap();
    assert_eq!(map.palette()[tile.0 as usize].collision.trigger, 1);
    assert!(Tilemap::from_toml(&LEVEL.replace('#', "界")).is_ok());
    let loaded = Tilemap::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/level.toml"),
    )
    .unwrap();
    assert_eq!(loaded.tile(0, 0, 8), map.tile(0, 0, 8));
}
#[test]
fn format_rejects_invalid_data() {
    for text in [
        LEVEL.replace("version = 1", "version = 2"),
        LEVEL.replace("version = 1", "version = 1\nextra = 3"),
        LEVEL.replace("tiles.png", "../tiles.png"),
        LEVEL.replace("tiles.png", "/tiles.png"),
        LEVEL.replace("16, 16]", "0, 16]"),
        LEVEL.replace("name = \"triggers\"", "name = \"ground\""),
        LEVEL.replace("....................", "?..................."),
        LEVEL.replacen("....................", "..", 1),
        LEVEL.replace("tile_size = [32.0, 32.0]", "tile_size = [nan, 32.0]"),
        LEVEL.replace("[tiles.\"#\"]", "[tiles.\"##\"]"),
        LEVEL.replace("[tiles.\"#\"]", "[tiles.\".\"]"),
        LEVEL.replace(
            "region = [0, 0, 16, 16]",
            "region = [4294967295, 0, 16, 16]",
        ),
    ] {
        assert!(Tilemap::from_toml(&text).is_err(), "accepted {text}");
    }
    assert!(Tilemap::load("/nonexistent/tilemap/level.toml").is_err());
}
#[test]
fn coordinates_boundaries_and_chunk_edges() {
    let mut map = map(33, 17);
    for y in 0..17 {
        for x in 0..33 {
            let world = map.tile_to_world(x, y).unwrap();
            assert_eq!(map.world_to_tile(world), Some((x, y)));
            assert_eq!(
                map.world_to_tile(world + map.tile_size() * 0.5),
                Some((x, y))
            );
        }
    }
    assert_eq!(map.world_to_tile(Vec2::new(-8.01, -4.0)), None);
    assert_eq!(map.world_to_tile(map.bounds().max), None);
    assert_eq!(map.world_to_tile(Vec2::splat(f32::NAN)), None);
    for (x, y) in [(0, 0), (15, 15), (16, 16), (32, 16)] {
        map.set_tile(1, x, y, Some(TileId(0))).unwrap();
        assert_eq!(map.tile(1, x, y), Some(TileId(0)));
    }
    assert!(map.set_tile(0, 33, 0, Some(TileId(0))).is_err());
    assert!(map.set_tile(2, 0, 0, None).is_err());
    assert!(map.set_tile(1, 16, 16, Some(TileId(99))).is_err());
    assert_eq!(map.tile(1, 16, 16), Some(TileId(0)));
}
#[test]
fn geometry_validation_and_fractional_coordinates() {
    let palette = map(1, 1).palette().to_vec();
    for (w, h, origin, size, names) in [
        (0, 1, Vec2::ZERO, Vec2::ONE, vec!["a".into()]),
        (u32::MAX, 1, Vec2::ZERO, Vec2::ONE, vec!["a".into()]),
        (
            4096,
            4096,
            Vec2::ZERO,
            Vec2::ONE,
            vec!["a".into(), "b".into()],
        ),
        (1, 1, Vec2::splat(1e30), Vec2::ONE, vec!["a".into()]),
        (1, 1, Vec2::ZERO, Vec2::ZERO, vec!["a".into()]),
        (1, 1, Vec2::ZERO, Vec2::ONE, vec![" ".into()]),
    ] {
        assert!(Tilemap::new(w, h, origin, size, palette.clone(), names).is_err());
    }
    let map = Tilemap::new(
        100,
        100,
        Vec2::new(-10.0, -0.7),
        Vec2::new(0.7, 1.3),
        palette,
        vec!["a".into()],
    )
    .unwrap();
    for y in 0..100 {
        for x in 0..100 {
            assert_eq!(
                map.world_to_tile(map.tile_to_world(x, y).unwrap()),
                Some((x, y))
            );
        }
    }
}
#[test]
fn region_and_edits_agree_with_collision_and_spatial_snapshot() {
    let mut map = map(33, 17);
    map.set_tile(0, 16, 16, Some(TileId(0))).unwrap();
    map.set_tile(1, 16, 16, Some(TileId(1))).unwrap();
    let area = map.tile_bounds(16, 16).unwrap();
    let mut found = Vec::new();
    map.visit_region(area, |tile| found.push(tile));
    assert_eq!(found.len(), 2);
    assert_eq!(found[1].flags.custom, 4);
    assert_eq!(map.solid_geometry(area), vec![area]);
    let mut index = SpatialIndex2D::new();
    assert_eq!(map.rebuild_solid_index(&mut index).unwrap(), 1);
    assert_eq!(index.overlapping(area).unwrap(), vec![(0, 16, 16)]);
    map.set_tile(0, 16, 16, None).unwrap();
    assert!(map.solid_geometry(area).is_empty());
    assert_eq!(map.rebuild_solid_index(&mut index).unwrap(), 0);
    let mut count = 0;
    map.visit_region(
        Aabb2 {
            min: Vec2::splat(f32::NAN),
            max: Vec2::ONE,
        },
        |_| count += 1,
    );
    assert_eq!(count, 0);
}
#[test]
fn swept_movement_and_one_way_platforms() {
    let mut map = map(32, 10);
    for x in 0..32 {
        map.set_tile(0, x, 5, Some(TileId(1))).unwrap();
    }
    let top = map.tile_bounds(3, 5).unwrap().min.y;
    let mut body = Body2D::new(Vec2::new(0.0, 0.0), Vec2::splat(1.0));
    body.velocity = Vec2::new(4.0, 1000.0);
    map.move_body(&mut body, 1.0, false);
    assert_eq!(body.position, Vec2::new(4.0, top - 0.5));
    assert!(body.grounded);
    body.velocity.y = 100.0;
    map.move_body(&mut body, 0.1, true);
    assert!(body.position.y > top);
    body.velocity.y = -1000.0;
    map.move_body(&mut body, 0.1, false);
    assert!(body.position.y < top);
    assert!(!body.grounded);
    // X crosses the platform's side without blocking.
    body.position = Vec2::new(-20.0, top + 1.0);
    body.velocity = Vec2::new(100.0, 0.0);
    map.move_body(&mut body, 0.2, false);
    assert_eq!(body.position.x, 0.0);
    // Solid walls clip X but keep Y movement.
    map.set_tile(0, 6, 4, Some(TileId(0))).unwrap();
    body.position = Vec2::new(0.0, 13.0);
    body.velocity = Vec2::new(1000.0, 2.0);
    map.move_body(&mut body, 0.1, false);
    assert_eq!(body.position.x, 3.5);
    assert_eq!(body.velocity.x, 0.0);
}
#[test]
fn tile_movement_matches_core_solids_for_diagonal_sweeps() {
    let mut map = map(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            if (x * 13 + y * 7) % 9 == 0 {
                map.set_tile(0, x, y, Some(TileId(0))).unwrap();
            }
        }
    }
    let geometry = map.solid_geometry(map.bounds());
    for position in [
        Vec2::new(-20.0, -20.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(30.0, 60.0),
    ] {
        for velocity in [
            Vec2::new(1000.0, 1000.0),
            Vec2::new(-300.0, 500.0),
            Vec2::new(500.0, -400.0),
        ] {
            let mut a = Body2D::new(position, Vec2::splat(1.0));
            a.velocity = velocity;
            let mut b = a;
            a.move_and_slide(0.1, &geometry);
            map.move_body(&mut b, 0.1, false);
            assert_eq!(a.position, b.position);
            assert_eq!(a.velocity, b.velocity);
            assert_eq!(a.grounded, b.grounded);
        }
    }
}
#[test]
fn solid_raycast_boundaries_distance_ties_and_edits() {
    let mut map = map(33, 17);
    map.set_tile(0, 16, 0, Some(TileId(0))).unwrap();
    map.set_tile(1, 16, 0, Some(TileId(0))).unwrap();
    let ray = Ray2::new(Vec2::new(-10.0, -2.0), Vec2::X).unwrap();
    let hit = map.raycast(ray, f32::INFINITY).unwrap().unwrap();
    assert_eq!(hit.tile.coordinate, (16, 0));
    assert_eq!(hit.tile.layer, 0);
    assert_eq!(hit.hit.distance, 34.0);
    assert_eq!(hit.hit.normal, -Vec2::X);
    assert!(map.raycast(ray, 33.99).unwrap().is_none());
    assert!(map.raycast(ray, -1.0).is_err());
    assert!(map.raycast(ray, f32::NAN).is_err());
    let inside = Ray2::new(Vec2::new(25.0, -2.0), Vec2::Y).unwrap();
    assert_eq!(map.raycast(inside, 0.0).unwrap().unwrap().hit.distance, 0.0);
    map.set_tile(0, 16, 0, None).unwrap();
    map.set_tile(1, 16, 0, Some(TileId(1))).unwrap();
    assert!(map.raycast(ray, f32::INFINITY).unwrap().is_none());
}
#[test]
fn culling_matches_brute_force_for_rotated_viewport_policies() {
    let mut map = map(65, 33);
    for layer in 0..2 {
        for y in 0..33 {
            for x in 0..65 {
                if (x + y) % 3 != 0 {
                    map.set_tile(layer, x, y, Some(TileId(0))).unwrap();
                }
            }
        }
    }
    for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
        for size in [Vec2::new(1920.0, 1080.0), Vec2::new(600.0, 900.0)] {
            for rotation in [0.0, 0.7, 1.57] {
                let camera = Camera2D {
                    target: Vec2::new(60.0, 60.0),
                    rotation,
                    view_height: 35.0,
                };
                let viewport = Viewport::new(size, Vec2::new(960.0, 540.0), mode).unwrap();
                let frustum = Frustum2D::from_camera(&camera, &viewport).unwrap();
                let mut expected = Vec::new();
                map.visit_region(map.bounds(), |tile| {
                    if frustum.intersects(tile.bounds) {
                        expected.push((tile.layer, tile.coordinate));
                    }
                });
                let mut actual = Vec::new();
                let stats = map
                    .visit_visible(&camera, &viewport, |tile| {
                        actual.push((tile.layer, tile.coordinate))
                    })
                    .unwrap();
                assert_eq!(stats.tiles, actual.len());
                assert!(actual.windows(2).all(|w| w[0].0 <= w[1].0));
                expected.sort();
                actual.sort();
                assert_eq!(actual, expected);
            }
        }
    }
    let camera = Camera2D {
        target: Vec2::splat(10000.0),
        ..Default::default()
    };
    let viewport = Viewport::new(Vec2::ONE, Vec2::ONE, ScaleMode::Fit).unwrap();
    assert_eq!(
        map.visit_visible(&camera, &viewport, |_| {}).unwrap(),
        SubmissionStats::default()
    );
    assert!(
        map.visit_visible(
            &Camera2D {
                view_height: 0.0,
                ..camera
            },
            &viewport,
            |_| {}
        )
        .is_err()
    );
}
#[test]
fn large_map_visits_only_visible_chunks_and_runtime_edits() {
    let mut map = map(512, 512);
    for y in 0..512 {
        for x in 0..512 {
            map.set_tile(0, x, y, Some(TileId(0))).unwrap();
        }
    }
    let camera = Camera2D {
        target: map.tile_to_world(256, 256).unwrap(),
        view_height: 40.0,
        ..Default::default()
    };
    let view = Viewport::new(
        Vec2::new(1920.0, 1080.0),
        Vec2::new(960.0, 540.0),
        ScaleMode::Fit,
    )
    .unwrap();
    let before = map.visit_visible(&camera, &view, |_| {}).unwrap();
    assert!(before.visible_chunks <= 8);
    assert!(before.tiles < 1024);
    map.set_tile(0, 256, 256, None).unwrap();
    let after = map.visit_visible(&camera, &view, |_| {}).unwrap();
    assert_eq!(before.tiles, after.tiles + 1);
}
