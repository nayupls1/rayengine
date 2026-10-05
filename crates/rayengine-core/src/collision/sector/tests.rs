use super::*;
use crate::spatial::SpatialIndex2D;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

fn sector(angle: f32) -> Sector2 {
    Sector2::new(Vec2::ZERO, Vec2::X, 5.0, angle).unwrap()
}

fn hit(area: Sector2, center: Vec2, radius: f32) -> bool {
    area.intersects_circle(&Circle::new(center, radius))
        .unwrap()
}

#[test]
fn validates_constructor_and_query_inputs() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            Sector2::new(Vec2::new(bad, 0.0), Vec2::X, 5.0, PI),
            Err(SectorError::InvalidOrigin)
        );
        assert_eq!(
            Sector2::new(Vec2::ZERO, Vec2::new(bad, 1.0), 5.0, PI),
            Err(SectorError::InvalidDirection)
        );
        assert_eq!(
            Sector2::new(Vec2::ZERO, Vec2::X, bad, PI),
            Err(SectorError::InvalidRange)
        );
        assert_eq!(
            Sector2::new(Vec2::ZERO, Vec2::X, 5.0, bad),
            Err(SectorError::InvalidAngle)
        );
        assert_eq!(
            sector(PI).contains(Vec2::new(bad, 0.0)),
            Err(SectorError::InvalidPoint)
        );
        assert_eq!(
            sector(PI).intersects_circle(&Circle {
                center: Vec2::new(bad, 0.0),
                radius: 1.0
            }),
            Err(SectorError::InvalidCircle)
        );
    }
    assert_eq!(
        Sector2::new(Vec2::ZERO, Vec2::ZERO, 5.0, PI),
        Err(SectorError::InvalidDirection)
    );
    assert_eq!(
        Sector2::new(Vec2::ZERO, Vec2::X, -1.0, PI),
        Err(SectorError::InvalidRange)
    );
    for angle in [-0.1, TAU.next_up()] {
        assert_eq!(
            Sector2::new(Vec2::ZERO, Vec2::X, 5.0, angle),
            Err(SectorError::InvalidAngle)
        );
    }
    for radius in [-1.0, 0.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            sector(PI).intersects_circle(&Circle {
                center: Vec2::ZERO,
                radius
            }),
            Err(SectorError::InvalidCircle)
        );
    }
    assert_eq!(
        Sector2::new(Vec2::splat(f32::MAX), Vec2::X, 1.0, PI),
        Err(SectorError::InvalidBounds)
    );
    assert_eq!(
        Sector2::new(Vec2::splat(f32::MAX * 0.75), Vec2::X, f32::MAX * 0.5, PI),
        Err(SectorError::InvalidBounds)
    );
}

#[test]
fn normalizes_extreme_directions_and_preserves_parameters() {
    for length in [f32::from_bits(1), 1e-30, 3.0, f32::MAX] {
        let area = Sector2::new(Vec2::new(2.0, -3.0), Vec2::new(length, length), 4.0, PI).unwrap();
        assert!((area.direction().length() - 1.0).abs() < 1e-6);
        assert_eq!(area.origin(), Vec2::new(2.0, -3.0));
        assert_eq!(area.range(), 4.0);
        assert_eq!(area.angle(), PI);
        assert!(area.contains(area.origin()).unwrap());
        assert!(area.contains(Vec2::new(3.0, -2.0)).unwrap());
        assert!(!area.contains(Vec2::new(1.0, -4.0)).unwrap());
    }
}

#[test]
fn angle_wrap_and_both_sides_of_direction() {
    for facing in [179.0_f32, -179.0] {
        let area = Sector2::new(
            Vec2::new(10.0, -7.0),
            Vec2::from_angle(facing.to_radians()),
            5.0,
            10.0_f32.to_radians(),
        )
        .unwrap();
        for angle in [178.0_f32, -178.0] {
            let p = area.origin() + Vec2::from_angle(angle.to_radians()) * 3.0;
            assert!(area.contains(p).unwrap());
            assert!(hit(area, p, 0.1));
        }
        for angle in [165.0_f32, -165.0, 0.0] {
            let p = area.origin() + Vec2::from_angle(angle.to_radians()) * 3.0;
            assert!(!area.contains(p).unwrap());
            assert!(!hit(area, p, 0.1));
        }
    }
}

#[test]
fn narrow_sector_radial_arc_and_endpoint_contacts() {
    let area = sector(FRAC_PI_2);
    // Center outside the angular interval, but the circle crosses a radial edge.
    assert!(!area.contains(Vec2::new(2.0, 2.5)).unwrap());
    assert!(hit(area, Vec2::new(2.0, 2.5), 0.4));
    assert!(!hit(area, Vec2::new(2.0, 2.5), 0.3));
    assert!(hit(area, Vec2::new(2.0, -2.5), 0.4));
    // A circle behind the origin only intersects if its radius reaches it.
    assert!(hit(area, Vec2::new(-1.0, 0.0), 1.0));
    assert!(!hit(area, Vec2::new(-1.0, 0.0), 0.99));
    assert!(area.contains(Vec2::new(5.0, 0.0)).unwrap());
    assert!(!area.contains(Vec2::new(5.0_f32.next_up(), 0.0)).unwrap());
    assert!(hit(area, Vec2::new(6.0, 0.0), 1.0));
    assert!(!hit(area, Vec2::new(6.0_f32.next_up(), 0.0), 1.0));
    // Outside both angle and range: nearest point is an arc endpoint.
    assert!(hit(area, Vec2::new(4.0, 4.5), 1.1));
    assert!(!hit(area, Vec2::new(4.0, 4.5), 1.0));
    // Passing separate expanded range/angle tests is insufficient at a corner.
    assert!(!hit(sector(0.0), Vec2::new(5.8, 0.8), 1.0));
    let tiny = sector(0.001);
    assert!(hit(tiny, Vec2::new(3.0, 0.1), 0.1));
    assert!(!hit(tiny, Vec2::new(3.0, 0.1), 0.09));
}

#[test]
fn wide_sector_includes_back_quadrants_but_excludes_its_notch() {
    let area = sector(PI * 1.5);
    assert!(area.contains(Vec2::new(-2.0, 3.0)).unwrap());
    assert!(area.contains(Vec2::new(-2.0, -3.0)).unwrap());
    assert!(!area.contains(Vec2::new(-3.0, 0.0)).unwrap());
    assert!(!hit(area, Vec2::new(-3.0, 0.0), 2.0));
    assert!(hit(area, Vec2::new(-3.0, 0.0), 2.2));
    assert!(hit(area, Vec2::new(-2.5, 2.0), 0.4));
    assert!(!hit(area, Vec2::new(-2.5, 2.0), 0.3));
    assert!(hit(area, Vec2::new(-2.5, -2.0), 0.4));
    assert!(hit(area, Vec2::ZERO, 0.01));
}

#[test]
fn zero_angle_preserves_collinearity_for_non_axis_directions() {
    for x in -100..=100 {
        for y in -100..=100 {
            let direction = Vec2::new(x as f32, y as f32);
            if direction == Vec2::ZERO {
                continue;
            }
            let area = Sector2::new(Vec2::ZERO, direction, 300.0, 0.0).unwrap();
            assert!(area.contains(direction).unwrap(), "direction={direction}");
            assert!(hit(area, direction, 1e-20), "direction={direction}");
            assert!(area.contains(direction * 2.0).unwrap());
            assert!(!area.contains(-direction).unwrap());
            assert!(!hit(area, direction + direction.perp() * 0.01, 1e-20));
        }
    }
    for scale in [f32::from_bits(1), 1e-30, 1e30] {
        let direction = Vec2::new(3.0, 9.0) * scale;
        let area = Sector2::new(Vec2::ZERO, direction, 20.0 * scale, 0.0).unwrap();
        assert!(area.contains(direction).unwrap());
        assert!(hit(area, direction, scale));
    }
    let origin = Vec2::new(100.0, -200.0);
    let direction = Vec2::new(3.0, 9.0);
    let area = Sector2::new(origin, direction, 20.0, 0.0).unwrap();
    assert!(area.contains(origin + direction).unwrap());
    assert!(hit(area, origin + direction, 1e-20));
}

#[test]
fn full_disk_zero_angle_and_zero_range_are_closed() {
    let disk = sector(TAU);
    assert!(disk.contains(Vec2::new(-5.0, 0.0)).unwrap());
    assert!(hit(disk, Vec2::new(-6.0, 0.0), 1.0));
    assert!(!hit(disk, Vec2::new(-6.01, 0.0), 1.0));
    let segment = sector(0.0);
    assert!(segment.contains(Vec2::new(3.0, 0.0)).unwrap());
    assert!(!segment.contains(Vec2::new(3.0, f32::from_bits(1))).unwrap());
    assert!(hit(segment, Vec2::new(3.0, 1.0), 1.0));
    assert!(!hit(segment, Vec2::new(3.0, 1.01), 1.0));
    for angle in [0.0, PI, TAU] {
        let point = Sector2::new(Vec2::ONE, Vec2::X, 0.0, angle).unwrap();
        assert!(point.contains(Vec2::ONE).unwrap());
        assert!(!point.contains(Vec2::new(1.0_f32.next_up(), 1.0)).unwrap());
        assert!(hit(point, Vec2::new(2.0, 1.0), 1.0));
        assert!(!hit(point, Vec2::new(2.01, 1.0), 1.0));
    }
}

#[test]
fn extreme_coordinates_do_not_overflow_or_underflow_queries() {
    let large = Sector2::new(Vec2::new(-1e30, 0.0), Vec2::X, 2e30, PI).unwrap();
    assert!(hit(large, Vec2::new(1.5e30, 0.0), 1e30));
    assert!(!hit(large, Vec2::new(2.5e30, 0.0), 1e30));
    assert!(!hit(large, Vec2::new(f32::MAX, f32::MAX), 1.0));
    let tiny = Sector2::new(Vec2::ZERO, Vec2::X, 1e-30, 0.0).unwrap();
    assert!(hit(tiny, Vec2::new(0.5e-30, 0.5e-30), 0.6e-30));
    assert!(!hit(tiny, Vec2::new(0.5e-30, 0.5e-30), 0.4e-30));
}

#[test]
fn broadphase_retains_tangencies_and_degenerate_sectors() {
    for angle in [0.0, 0.01, FRAC_PI_2, PI * 1.5, TAU] {
        let area = sector(angle);
        let mut index = SpatialIndex2D::new();
        let circles = [
            Circle::new(Vec2::new(6.0, 0.0), 1.0),
            Circle::new(Vec2::ZERO, 0.1),
            Circle::new(Vec2::new(2.0, 2.5), 0.4),
            Circle::new(Vec2::new(-6.0, 0.0), 1.0),
        ];
        index
            .rebuild(
                circles
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i, Aabb2::from_center(c.center, Vec2::splat(c.radius * 2.0)))),
            )
            .unwrap();
        let candidates = index.overlapping(area.bounds()).unwrap();
        for (i, circle) in circles.iter().enumerate() {
            if area.intersects_circle(circle).unwrap() {
                assert!(candidates.contains(&i), "lost {circle:?} for angle {angle}");
            }
        }
    }
    let point = Sector2::new(Vec2::ZERO, Vec2::X, 0.0, 0.0).unwrap();
    let mut index = SpatialIndex2D::new();
    index
        .rebuild([(0, Aabb2::from_center(Vec2::X, Vec2::splat(2.0)))])
        .unwrap();
    assert_eq!(index.overlapping(point.bounds()).unwrap(), vec![0]);
    let fractional = Sector2::new(Vec2::new(0.1, -0.7), Vec2::X, 0.2, PI).unwrap();
    for axis in 0..2 {
        assert!(
            f64::from(fractional.bounds().min[axis])
                < f64::from(fractional.origin()[axis]) - f64::from(fractional.range())
        );
        assert!(
            f64::from(fractional.bounds().max[axis])
                > f64::from(fractional.origin()[axis]) + f64::from(fractional.range())
        );
    }
}
