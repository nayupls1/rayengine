use super::*;
use crate::pathfinding::{CostGrid, PathFinder, PathOptions, PathStatus};

fn pose(x: i32, y: i32, rotation: QuarterTurn) -> PlacementPose {
    PlacementPose {
        anchor: IVec2::new(x, y),
        rotation,
    }
}
fn single() -> Footprint {
    Footprint::new([IVec2::ZERO], []).unwrap()
}

#[test]
fn failed_edits_preserve_everything_and_moves_can_overlap_their_old_cells() {
    let mut grid = PlacementGrid::new(UVec2::splat(8));
    let shape = Footprint::new([IVec2::ZERO, IVec2::X], []).unwrap();
    let id = PlacementId(1);
    grid.place(id, &shape, pose(1, 1, QuarterTurn::Zero))
        .unwrap();
    grid.place(PlacementId(2), &single(), pose(4, 1, QuarterTurn::Zero))
        .unwrap();
    let before = grid.clone();
    assert!(matches!(
        grid.place(PlacementId(3), &shape, pose(0, 1, QuarterTurn::Zero)),
        Err(PlacementError::Occupied { .. })
    ));
    assert!(matches!(
        grid.move_object(id, pose(3, 1, QuarterTurn::Zero)),
        Err(PlacementError::Occupied { .. })
    ));
    assert!(grid.move_object(id, pose(7, 1, QuarterTurn::Zero)).is_err());
    assert!(
        grid.place(id, &shape, pose(0, 0, QuarterTurn::Zero))
            .is_err()
    );
    assert!(
        grid.move_object(PlacementId(99), pose(0, 0, QuarterTurn::Zero))
            .is_err()
    );
    assert!(grid.remove(PlacementId(99)).is_err());
    assert_eq!(grid, before);
    let change = grid.move_object(id, pose(2, 1, QuarterTurn::Zero)).unwrap();
    assert_eq!(
        change.before.as_ref().unwrap().occupied(),
        &[UVec2::new(1, 1), UVec2::new(2, 1)]
    );
    assert_eq!(change.after, grid.object(id).cloned());
    assert!(grid.is_free(UVec2::new(1, 1)));
    assert_eq!(grid.occupant(UVec2::new(2, 1)), Some(id));
    let change = grid.remove(id).unwrap();
    assert!(change.after.is_none());
    assert_eq!(change.before.unwrap().pose().anchor, IVec2::new(2, 1));
    assert!(grid.is_free(UVec2::new(2, 1)));
    assert!(grid.is_free(UVec2::new(3, 1)));
}

#[test]
fn irregular_footprints_and_access_rotate_about_anchor_in_all_four_turns() {
    let shape = Footprint::new(
        [IVec2::ZERO, IVec2::new(2, -1), IVec2::ZERO],
        [IVec2::new(-1, 0)],
    )
    .unwrap();
    let grid = PlacementGrid::new(UVec2::splat(10));
    for (rotation, occupied, access) in [
        (QuarterTurn::Zero, (6, 3), (3, 4)),
        (QuarterTurn::One, (5, 6), (4, 3)),
        (QuarterTurn::Two, (2, 5), (5, 4)),
        (QuarterTurn::Three, (3, 2), (4, 5)),
    ] {
        let preview = grid
            .validate_place(PlacementId(1), &shape, pose(4, 4, rotation))
            .unwrap();
        assert_eq!(
            preview.occupied(),
            &[UVec2::new(4, 4), UVec2::new(occupied.0, occupied.1)]
        );
        assert_eq!(preview.access(), &[UVec2::new(access.0, access.1)]);
    }
    assert_eq!(
        QuarterTurn::Zero.next().next().next().next(),
        QuarterTurn::Zero
    );
    assert!(Footprint::new([], []).is_err());
    assert!(Footprint::new([IVec2::ZERO], [IVec2::ZERO]).is_err());
}

#[test]
fn access_protection_is_bidirectional_shared_and_released_on_move_remove() {
    let mut grid = PlacementGrid::new(UVec2::splat(8));
    let chair = Footprint::new([IVec2::ZERO], [IVec2::X]).unwrap();
    grid.place(PlacementId(1), &chair, pose(2, 2, QuarterTurn::Zero))
        .unwrap();
    let before = grid.clone();
    assert!(matches!(
        grid.place(PlacementId(2), &single(), pose(3, 2, QuarterTurn::Zero)),
        Err(PlacementError::BlockedAccess { .. })
    ));
    assert!(matches!(
        grid.place(PlacementId(2), &chair, pose(1, 2, QuarterTurn::Zero)),
        Err(PlacementError::BlockedAccess { .. })
    ));
    assert_eq!(grid, before);
    grid.place(PlacementId(2), &chair, pose(4, 2, QuarterTurn::Two))
        .unwrap();
    assert!(grid.is_free(UVec2::new(3, 2))); // shared access
    let before = grid.clone();
    assert!(
        grid.move_object(PlacementId(1), pose(3, 2, QuarterTurn::One))
            .is_err()
    );
    assert_eq!(grid, before);
    grid.remove(PlacementId(2)).unwrap();
    // Own previous occupied cell may become own access.
    grid.move_object(PlacementId(1), pose(1, 2, QuarterTurn::Zero))
        .unwrap();
    grid.place(PlacementId(3), &single(), pose(3, 2, QuarterTurn::Zero))
        .unwrap();
    grid.remove(PlacementId(1)).unwrap();
    grid.place(PlacementId(4), &single(), pose(2, 2, QuarterTurn::Zero))
        .unwrap();
}

#[test]
fn bounds_include_access_and_extreme_offsets_never_overflow() {
    let grid = PlacementGrid::new(UVec2::splat(4));
    let access = Footprint::new([IVec2::ZERO], [IVec2::X]).unwrap();
    for p in [
        pose(-1, 0, QuarterTurn::Zero),
        pose(3, 0, QuarterTurn::Zero),
        pose(0, 0, QuarterTurn::Two),
    ] {
        assert_eq!(
            grid.validate_place(PlacementId(0), &access, p),
            Err(PlacementError::OutOfBounds)
        );
    }
    let extreme = Footprint::new([IVec2::new(i32::MIN, i32::MAX)], []).unwrap();
    for rotation in [
        QuarterTurn::Zero,
        QuarterTurn::One,
        QuarterTurn::Two,
        QuarterTurn::Three,
    ] {
        assert!(
            grid.validate_place(PlacementId(0), &extreme, pose(i32::MAX, i32::MIN, rotation))
                .is_err()
        );
    }
    assert!(!grid.is_free(UVec2::new(4, 0)));
    assert!(
        PlacementGrid::new(UVec2::ZERO)
            .validate_place(PlacementId(0), &single(), PlacementPose::default())
            .is_err()
    );
}

#[test]
fn committed_navigation_and_collision_snapshots_agree_after_edits() {
    let mut grid = PlacementGrid::new(UVec2::new(5, 1));
    let mut terrain = CostGrid::new(grid.size(), 2.0);
    terrain.set(UVec2::new(4, 0), Some(3.0));
    let id = PlacementId(1);
    let change = grid
        .place(id, &single(), pose(2, 0, QuarterTurn::Zero))
        .unwrap();
    assert_eq!(change.after.unwrap().occupied(), &[UVec2::new(2, 0)]);
    let mut finder = PathFinder::new();
    let mut path = Vec::new();
    let search = |grid: &PlacementGrid, finder: &mut PathFinder, path: &mut Vec<UVec2>| {
        finder
            .find_path(
                &grid.navigation(&terrain),
                UVec2::ZERO,
                UVec2::new(4, 0),
                &PathOptions::default(),
                path,
            )
            .unwrap()
    };
    assert_eq!(
        search(&grid, &mut finder, &mut path),
        PathStatus::Unreachable
    );
    let before = grid.clone();
    assert!(grid.move_object(id, pose(5, 0, QuarterTurn::Zero)).is_err());
    assert_eq!(grid, before);
    assert_eq!(
        search(&grid, &mut finder, &mut path),
        PathStatus::Unreachable
    );
    grid.move_object(id, pose(1, 0, QuarterTurn::Zero)).unwrap();
    assert_eq!(grid.navigation(&terrain).cost(UVec2::new(2, 0)), Some(2.0));
    assert_eq!(grid.navigation(&terrain).cost(UVec2::new(1, 0)), None);
    grid.remove(id).unwrap();
    assert!(matches!(
        search(&grid, &mut finder, &mut path),
        PathStatus::Found { .. }
    ));
    assert_eq!(grid.navigation(&terrain).cost(UVec2::new(4, 0)), Some(3.0));
    terrain.set(UVec2::new(3, 0), None);
    assert_eq!(grid.navigation(&terrain).cost(UVec2::new(3, 0)), None);
}
