use super::*;
use crate::{
    camera::{Camera2D, Camera3D},
    collision::{Aabb2, Aabb3},
    viewport::{ScaleMode, Viewport},
};
use glam::{Vec2, Vec3};

fn box2(center: Vec2) -> Aabb2 {
    Aabb2::from_center(center, Vec2::splat(2.0))
}

fn box3(center: Vec3) -> Aabb3 {
    Aabb3::from_center(center, Vec3::splat(2.0))
}

fn view(size: Vec2, mode: ScaleMode) -> Viewport {
    Viewport::new(size, Vec2::new(960.0, 540.0), mode).unwrap()
}

#[test]
fn rays_normalize_and_report_world_distance_and_face_normals() {
    let ray2 = Ray2::new(Vec2::new(-5.0, 0.0), Vec2::new(10.0, 0.0)).unwrap();
    assert_eq!(ray2.direction(), Vec2::X);
    assert_eq!(ray2.origin(), Vec2::new(-5.0, 0.0));
    let hit2 = ray2.cast(box2(Vec2::ZERO), 4.0).unwrap().unwrap();
    assert_eq!(hit2.distance, 4.0);
    assert_eq!(hit2.point, Vec2::new(-1.0, 0.0));
    assert_eq!(hit2.normal, Vec2::NEG_X);
    assert!(ray2.cast(box2(Vec2::ZERO), 3.99).unwrap().is_none());

    let ray3 = Ray3::new(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -20.0)).unwrap();
    let hit3 = ray3.cast(box3(Vec3::ZERO), f32::INFINITY).unwrap().unwrap();
    assert_eq!(hit3.distance, 4.0);
    assert_eq!(hit3.point, Vec3::new(0.0, 0.0, 1.0));
    assert_eq!(hit3.normal, Vec3::Z);
}

#[test]
fn inside_parallel_miss_and_boundary_contacts_match_in_both_dimensions() {
    assert_eq!(
        Ray2::new(Vec2::ZERO, Vec2::X)
            .unwrap()
            .cast(box2(Vec2::ZERO), 0.0)
            .unwrap()
            .unwrap()
            .normal,
        Vec2::ZERO
    );
    assert_eq!(
        Ray3::new(Vec3::ZERO, Vec3::X)
            .unwrap()
            .cast(box3(Vec3::ZERO), 0.0)
            .unwrap()
            .unwrap()
            .normal,
        Vec3::ZERO
    );
    assert!(
        Ray2::new(Vec2::new(-5.0, 2.0), Vec2::X)
            .unwrap()
            .cast(box2(Vec2::ZERO), 100.0)
            .unwrap()
            .is_none()
    );
    assert!(
        Ray3::new(Vec3::new(-5.0, 2.0, 0.0), Vec3::X)
            .unwrap()
            .cast(box3(Vec3::ZERO), 100.0)
            .unwrap()
            .is_none()
    );
    let on_edge = Ray2::new(Vec2::new(-5.0, 1.0), Vec2::X)
        .unwrap()
        .cast(box2(Vec2::ZERO), 4.0)
        .unwrap()
        .unwrap();
    assert_eq!((on_edge.distance, on_edge.normal), (4.0, Vec2::NEG_X));
    let on_corner = Ray3::new(Vec3::new(-5.0, 1.0, 1.0), Vec3::X)
        .unwrap()
        .cast(box3(Vec3::ZERO), 4.0)
        .unwrap()
        .unwrap();
    assert_eq!((on_corner.distance, on_corner.normal), (4.0, Vec3::NEG_X));
    assert_eq!(
        Ray2::new(Vec2::new(-1.0, 0.0), Vec2::X)
            .unwrap()
            .cast(box2(Vec2::ZERO), 0.0)
            .unwrap()
            .unwrap()
            .normal,
        Vec2::NEG_X
    );
    assert_eq!(
        Ray2::new(Vec2::new(-1.0, 0.0), Vec2::NEG_X)
            .unwrap()
            .cast(box2(Vec2::ZERO), 0.0)
            .unwrap()
            .unwrap()
            .normal,
        Vec2::ZERO
    );
}

#[test]
fn invalid_rays_distances_and_bounds_are_errors() {
    assert_eq!(
        Ray2::new(Vec2::ZERO, Vec2::ZERO),
        Err(SpatialError::InvalidRay)
    );
    assert_eq!(
        Ray3::new(Vec3::splat(f32::NAN), Vec3::X),
        Err(SpatialError::InvalidRay)
    );
    for magnitude in [f32::MAX, f32::MIN_POSITIVE * 0.001] {
        let direction = Ray3::new(Vec3::ZERO, Vec3::splat(magnitude))
            .unwrap()
            .direction();
        assert!((direction.length() - 1.0).abs() < 0.0001);
    }
    let ray = Ray2::new(Vec2::ZERO, Vec2::X).unwrap();
    assert_eq!(
        ray.cast(box2(Vec2::ZERO), f32::NAN),
        Err(SpatialError::InvalidDistance)
    );
    assert_eq!(
        ray.cast(box2(Vec2::ZERO), -1.0),
        Err(SpatialError::InvalidDistance)
    );
    assert_eq!(
        ray.cast(
            Aabb2 {
                min: Vec2::ONE,
                max: Vec2::ZERO
            },
            10.0
        ),
        Err(SpatialError::InvalidBounds(0))
    );
    let index = SpatialIndex2D::<u32>::new();
    assert!(matches!(
        index.nearest(ray, -1.0),
        Err(SpatialError::InvalidDistance)
    ));
    assert!(matches!(
        index.overlapping(Aabb2 {
            min: Vec2::ONE,
            max: Vec2::ZERO
        }),
        Err(SpatialError::InvalidBounds(0))
    ));
}

#[test]
fn nearest_is_independent_of_input_order_and_breaks_ties_by_input_position() {
    let mut index2 = SpatialIndex2D::new();
    let entries: Vec<_> = (0..64)
        .rev()
        .map(|i| (i, box2(Vec2::new(i as f32 * 4.0, 0.0))))
        .collect();
    index2.rebuild(entries).unwrap();
    let ray2 = Ray2::new(Vec2::new(-10.0, 0.0), Vec2::X).unwrap();
    assert_eq!(index2.nearest(ray2, 9.0_f32.next_down()).unwrap(), None);
    let hit = index2.nearest(ray2, 9.0).unwrap().unwrap();
    assert_eq!(
        (hit.id, hit.hit.distance, hit.hit.normal),
        (0, 9.0, Vec2::NEG_X)
    );

    let mut index3 = SpatialIndex3D::new();
    index3
        .rebuild(
            (0..64)
                .rev()
                .map(|i| (i, box3(Vec3::new(i as f32 * 4.0, 0.0, 0.0)))),
        )
        .unwrap();
    let ray3 = Ray3::new(Vec3::new(-10.0, 0.0, 0.0), Vec3::X).unwrap();
    assert_eq!(index3.nearest(ray3, f32::INFINITY).unwrap().unwrap().id, 0);

    let same = box2(Vec2::new(3.0, 0.0));
    index2.rebuild([(7, same), (5, same), (11, same)]).unwrap();
    assert_eq!(index2.nearest(ray2, f32::INFINITY).unwrap().unwrap().id, 7);
}

#[test]
fn rebuild_is_atomic_and_proximity_reads_the_last_snapshot() {
    let mut index2 = SpatialIndex2D::new();
    assert_eq!(
        index2.rebuild([(1, box2(Vec2::ZERO)), (2, box2(Vec2::new(10.0, 0.0)))]),
        Ok(2)
    );
    let area = box2(Vec2::new(1.0, 0.0));
    assert_eq!(index2.overlapping(area).unwrap(), vec![1]);
    let touching = box2(Vec2::new(2.0, 0.0));
    assert!(index2.overlapping(touching).unwrap().is_empty());
    let invalid = Aabb2 {
        min: Vec2::new(f32::NAN, 0.0),
        max: Vec2::ONE,
    };
    assert_eq!(
        index2.rebuild([(9, box2(Vec2::new(100.0, 0.0))), (10, invalid)]),
        Err(SpatialError::InvalidBounds(1))
    );
    assert_eq!(index2.len(), 2);
    assert_eq!(index2.overlapping(area).unwrap(), vec![1]);
    assert_eq!(index2.rebuild([(3, box2(Vec2::new(100.0, 0.0)))]), Ok(1));
    assert!(index2.overlapping(area).unwrap().is_empty());
    let mut visited = Vec::new();
    index2
        .visit_overlapping(box2(Vec2::new(100.0, 0.0)), |id| visited.push(id))
        .unwrap();
    assert_eq!(visited, [3]);
    assert_eq!(index2.rebuild(std::iter::empty()), Ok(0));
    assert!(index2.is_empty());

    let mut index3 = SpatialIndex3D::new();
    index3
        .rebuild([(1, box3(Vec3::ZERO)), (2, box3(Vec3::new(10.0, 0.0, 0.0)))])
        .unwrap();
    assert_eq!(index3.overlapping(box3(Vec3::ZERO)).unwrap(), vec![1]);
    let invalid = Aabb3 {
        min: Vec3::ONE,
        max: Vec3::ZERO,
    };
    assert_eq!(
        index3.rebuild([(3, box3(Vec3::ZERO)), (4, invalid)]),
        Err(SpatialError::InvalidBounds(1))
    );
    assert_eq!(
        index3
            .nearest(Ray3::new(Vec3::new(-5.0, 0.0, 0.0), Vec3::X).unwrap(), 4.0)
            .unwrap()
            .unwrap()
            .id,
        1
    );
}

#[test]
fn bvh_nearest_matches_brute_force_across_many_layouts() {
    let mut seed = 12_345_u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1_u32 << 24) as f32
    };
    let mut index = SpatialIndex3D::new();
    let mut entries = Vec::new();
    for id in 0..256 {
        let center = Vec3::new(
            next() * 200.0 - 100.0,
            next() * 20.0 - 10.0,
            next() * 200.0 - 100.0,
        );
        entries.push((
            id,
            Aabb3::from_center(center, Vec3::splat(1.0 + next() * 3.0)),
        ));
    }
    index.rebuild(entries.iter().copied()).unwrap();
    let mut seen_hits = 0;
    for _ in 0..100 {
        let origin = Vec3::new(
            next() * 200.0 - 100.0,
            next() * 20.0 - 10.0,
            next() * 200.0 - 100.0,
        );
        let direction = Vec3::new(next() - 0.5, next() - 0.5, next() - 0.5);
        let ray = Ray3::new(origin, direction).unwrap();
        let mut expected: Option<(usize, f32)> = None;
        for &(id, bounds) in &entries {
            if let Some(hit) = ray.cast(bounds, 80.0).unwrap()
                && expected.is_none_or(|current| hit.distance < current.1)
            {
                expected = Some((id, hit.distance));
            }
        }
        let actual = index
            .nearest(ray, 80.0)
            .unwrap()
            .map(|hit| (hit.id, hit.hit.distance));
        assert_eq!(actual, expected);
        seen_hits += usize::from(actual.is_some());
    }
    assert!(seen_hits > 3, "random workload missed too many boxes");
}

#[test]
fn rotated_2d_visibility_matches_camera_axes_and_aspect() {
    let viewport = view(Vec2::new(960.0, 540.0), ScaleMode::Fit);
    let camera = Camera2D {
        target: Vec2::ZERO,
        rotation: 0.0,
        view_height: 4.0,
    };
    let normal = Frustum2D::from_camera(&camera, &viewport).unwrap();
    let rotated = Frustum2D::from_camera(
        &Camera2D {
            rotation: std::f32::consts::FRAC_PI_2,
            ..camera
        },
        &viewport,
    )
    .unwrap();
    let right = Aabb2::from_center(Vec2::new(3.0, 0.0), Vec2::splat(0.2));
    let down = Aabb2::from_center(Vec2::new(0.0, 3.0), Vec2::splat(0.2));
    assert!(normal.intersects(right));
    assert!(!normal.intersects(down));
    assert!(!rotated.intersects(right));
    assert!(rotated.intersects(down));
    let touching = Aabb2::from_center(Vec2::new(0.0, 2.0), Vec2::ZERO);
    assert!(normal.intersects(touching));
    let mut index = SpatialIndex2D::new();
    index.rebuild([(1, right), (2, down)]).unwrap();
    assert_eq!(index.visible(&normal), vec![1]);
    assert_eq!(index.visible(&rotated), vec![2]);
    assert!(
        Frustum2D::from_camera(
            &Camera2D {
                view_height: 0.0,
                ..camera
            },
            &viewport
        )
        .is_err()
    );
}

#[test]
fn visibility_3d_respects_near_far_and_fit_or_expand() {
    let camera = Camera3D {
        position: Vec3::ZERO,
        target: Vec3::NEG_Z,
        up: Vec3::Y,
        vertical_fov: 90.0,
    };
    let fitted = Frustum3D::from_camera(
        &camera,
        &view(Vec2::new(960.0, 540.0), ScaleMode::Fit),
        1.0,
        10.0,
    )
    .unwrap();
    let portrait_fit = Frustum3D::from_camera(
        &camera,
        &view(Vec2::new(800.0, 1200.0), ScaleMode::Fit),
        1.0,
        10.0,
    )
    .unwrap();
    let portrait_expand = Frustum3D::from_camera(
        &camera,
        &view(Vec2::new(800.0, 1200.0), ScaleMode::Expand),
        1.0,
        10.0,
    )
    .unwrap();
    let center = Aabb3::from_center(Vec3::new(0.0, 0.0, -5.0), Vec3::splat(0.2));
    let side = Aabb3::from_center(Vec3::new(6.0, 0.0, -5.0), Vec3::splat(0.2));
    let behind = Aabb3::from_center(Vec3::new(0.0, 0.0, 5.0), Vec3::splat(0.2));
    let too_close = Aabb3::from_center(Vec3::new(0.0, 0.0, -0.5), Vec3::splat(0.2));
    let too_far = Aabb3::from_center(Vec3::new(0.0, 0.0, -11.0), Vec3::splat(0.2));
    assert!(fitted.intersects(center));
    assert!(fitted.intersects(Aabb3::from_center(Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO,)));
    assert!(fitted.intersects(Aabb3::from_center(Vec3::new(0.0, 0.0, -10.0), Vec3::ZERO,)));
    assert!(fitted.intersects(side));
    assert!(portrait_fit.intersects(side));
    assert!(!portrait_expand.intersects(side));
    assert!(!fitted.intersects(behind));
    assert!(!fitted.intersects(too_close));
    assert!(!fitted.intersects(too_far));
    let mut index = SpatialIndex3D::new();
    index
        .rebuild([
            (1, center),
            (2, side),
            (3, behind),
            (4, too_close),
            (5, too_far),
        ])
        .unwrap();
    let mut visible = index.visible(&fitted);
    visible.sort_unstable();
    assert_eq!(visible, [1, 2]);
    assert!(
        Frustum3D::from_camera(
            &camera,
            &view(Vec2::new(960.0, 540.0), ScaleMode::Fit),
            0.0,
            10.0
        )
        .is_err()
    );
    assert!(
        Frustum3D::from_camera(
            &Camera3D {
                target: Vec3::ZERO,
                ..camera
            },
            &view(Vec2::new(960.0, 540.0), ScaleMode::Fit),
            1.0,
            10.0
        )
        .is_err()
    );
}
