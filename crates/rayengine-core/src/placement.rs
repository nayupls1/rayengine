//! Optional grid placement with atomic edits and game-defined interaction access.
//!
//! Footprints and access cells rotate together around an integer anchor. Access
//! cells are kept unoccupied (and may be shared), but this does **not** prove they
//! are reachable. Use navigation on a preview of the proposed layout when the
//! game requires reachability. Prices, terrain restrictions and room rules remain
//! game-owned; validate those before calling an edit operation.

use crate::pathfinding::NavGrid;
use glam::{IVec2, UVec2};
use std::{collections::BTreeMap, fmt};

/// Game-owned stable object identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlacementId(pub u64);

/// Quarter turns around the anchor in grid coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuarterTurn {
    /// `(x, y)`.
    #[default]
    Zero,
    /// `(-y, x)`.
    One,
    /// `(-x, -y)`.
    Two,
    /// `(y, -x)`.
    Three,
}

impl QuarterTurn {
    /// Advances by one quarter turn.
    pub fn next(self) -> Self {
        match self {
            Self::Zero => Self::One,
            Self::One => Self::Two,
            Self::Two => Self::Three,
            Self::Three => Self::Zero,
        }
    }
}

/// Local occupied and interaction cells; arbitrary shapes are supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Footprint {
    occupied: Vec<IVec2>,
    access: Vec<IVec2>,
}

impl Footprint {
    /// Creates a footprint, deduplicating offsets. Occupied cells must be
    /// nonempty and disjoint from access cells. Negative offsets are supported.
    pub fn new(
        occupied: impl IntoIterator<Item = IVec2>,
        access: impl IntoIterator<Item = IVec2>,
    ) -> Result<Self, PlacementError> {
        fn cells(input: impl IntoIterator<Item = IVec2>) -> Vec<IVec2> {
            let mut cells: Vec<_> = input.into_iter().collect();
            cells.sort_by_key(|cell| (cell.x, cell.y));
            cells.dedup();
            cells
        }
        let occupied = cells(occupied);
        let access = cells(access);
        if occupied.is_empty() || occupied.iter().any(|cell| access.contains(cell)) {
            return Err(PlacementError::InvalidFootprint);
        }
        Ok(Self { occupied, access })
    }

    /// Occupied offsets relative to the anchor before rotation.
    pub fn occupied(&self) -> &[IVec2] {
        &self.occupied
    }

    /// Required free offsets relative to the anchor before rotation.
    pub fn access(&self) -> &[IVec2] {
        &self.access
    }
}

/// Anchor and rotation of an object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlacementPose {
    /// Integer anchor, which need not itself be inside the grid.
    pub anchor: IVec2,
    /// Rotation of occupied and access offsets about the anchor.
    pub rotation: QuarterTurn,
}

/// Immutable placement snapshot, also returned by validation for previews.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedObject {
    id: PlacementId,
    footprint: Footprint,
    pose: PlacementPose,
    occupied: Vec<UVec2>,
    access: Vec<UVec2>,
}

impl PlacedObject {
    /// Game-owned identifier.
    pub fn id(&self) -> PlacementId {
        self.id
    }
    /// Original local footprint.
    pub fn footprint(&self) -> &Footprint {
        &self.footprint
    }
    /// Committed or previewed transform.
    pub fn pose(&self) -> PlacementPose {
        self.pose
    }
    /// Absolute occupied cells, in deterministic order.
    pub fn occupied(&self) -> &[UVec2] {
        &self.occupied
    }
    /// Absolute interaction access cells, in deterministic order.
    pub fn access(&self) -> &[UVec2] {
        &self.access
    }
}

/// Successful edit snapshots for updating game collision and navigation.
///
/// Remove collision for `before`, then create collision for `after`. Navigation
/// can read [`PlacementGrid::navigation`] directly, or refresh the union of both
/// snapshots' occupied cells against the final occupancy and game terrain costs.
/// Access cells remain walkable. Invalidate pending searches after an edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementChange {
    /// Previous object; `None` when placing.
    pub before: Option<PlacedObject>,
    /// New object; `None` when removing.
    pub after: Option<PlacedObject>,
}

/// A rejected edit; occupancy and objects are unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementError {
    /// Empty occupied footprint or an occupied/access offset conflict.
    InvalidFootprint,
    /// Identifier is already placed.
    DuplicateId(PlacementId),
    /// Identifier is not placed.
    UnknownId(PlacementId),
    /// A rotated occupied or access cell is outside the grid.
    OutOfBounds,
    /// An occupied cell overlaps another object.
    Occupied {
        /// Conflicting absolute cell.
        cell: UVec2,
        /// Object already occupying the cell.
        owner: PlacementId,
    },
    /// A proposed access cell is occupied, or proposed occupancy blocks existing access.
    BlockedAccess {
        /// Conflicting absolute access cell.
        cell: UVec2,
        /// Existing object whose occupancy or access conflicts with the edit.
        owner: PlacementId,
    },
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "placement rejected: {self:?}")
    }
}
impl std::error::Error for PlacementError {}

/// Sparse bounded occupancy. No allocation is made for empty grid cells.
///
/// Only edit methods mutate the grid. Every edit validates all occupied and
/// access cells before committing; failed edits leave the entire state intact.
/// Validation of occupied cells against existing access is linear in object count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementGrid {
    size: UVec2,
    objects: BTreeMap<PlacementId, PlacedObject>,
    occupancy: BTreeMap<(u32, u32), PlacementId>,
}

impl PlacementGrid {
    /// Creates an empty grid, including support for zero-size grids.
    pub fn new(size: UVec2) -> Self {
        Self {
            size,
            objects: BTreeMap::new(),
            occupancy: BTreeMap::new(),
        }
    }
    /// Grid dimensions in cells.
    pub fn size(&self) -> UVec2 {
        self.size
    }
    /// Owner of a cell; `None` for empty or out-of-bounds cells.
    pub fn occupant(&self, cell: UVec2) -> Option<PlacementId> {
        self.occupancy.get(&(cell.x, cell.y)).copied()
    }
    /// Whether a cell is inside the grid and unoccupied (including access cells).
    pub fn is_free(&self, cell: UVec2) -> bool {
        cell.cmplt(self.size).all() && self.occupant(cell).is_none()
    }
    /// Finds a committed object.
    pub fn object(&self, id: PlacementId) -> Option<&PlacedObject> {
        self.objects.get(&id)
    }
    /// Committed objects ordered by identifier.
    pub fn objects(&self) -> impl Iterator<Item = &PlacedObject> {
        self.objects.values()
    }

    /// Validates a new placement without changing the grid.
    pub fn validate_place(
        &self,
        id: PlacementId,
        footprint: &Footprint,
        pose: PlacementPose,
    ) -> Result<PlacedObject, PlacementError> {
        if self.objects.contains_key(&id) {
            return Err(PlacementError::DuplicateId(id));
        }
        self.validate(id, footprint, pose)
    }
    /// Validates a move/rotation, ignoring only the object's own old cells/access.
    pub fn validate_move(
        &self,
        id: PlacementId,
        pose: PlacementPose,
    ) -> Result<PlacedObject, PlacementError> {
        let object = self.objects.get(&id).ok_or(PlacementError::UnknownId(id))?;
        self.validate(id, &object.footprint, pose)
    }
    /// Validates and atomically places an object. Preview snapshots cannot be
    /// committed directly: this always revalidates against the current grid.
    pub fn place(
        &mut self,
        id: PlacementId,
        footprint: &Footprint,
        pose: PlacementPose,
    ) -> Result<PlacementChange, PlacementError> {
        let after = self.validate_place(id, footprint, pose)?;
        Ok(self.commit(after))
    }
    /// Validates and atomically moves/rotates an existing object.
    pub fn move_object(
        &mut self,
        id: PlacementId,
        pose: PlacementPose,
    ) -> Result<PlacementChange, PlacementError> {
        let after = self.validate_move(id, pose)?;
        Ok(self.commit(after))
    }
    /// Removes an object and its access requirements.
    pub fn remove(&mut self, id: PlacementId) -> Result<PlacementChange, PlacementError> {
        let before = self
            .objects
            .remove(&id)
            .ok_or(PlacementError::UnknownId(id))?;
        for cell in &before.occupied {
            self.occupancy.remove(&(cell.x, cell.y));
        }
        Ok(PlacementChange {
            before: Some(before),
            after: None,
        })
    }
    /// Borrows terrain navigation and overlays committed occupied cells as blocked.
    /// Terrain dimensions must equal this grid's dimensions.
    pub fn navigation<'a, G: NavGrid + ?Sized>(&'a self, terrain: &'a G) -> PlacementNav<'a, G> {
        assert_eq!(
            self.size,
            terrain.size(),
            "placement and terrain sizes differ"
        );
        PlacementNav {
            placement: self,
            terrain,
        }
    }

    fn validate(
        &self,
        id: PlacementId,
        footprint: &Footprint,
        pose: PlacementPose,
    ) -> Result<PlacedObject, PlacementError> {
        let transform = |offset: &IVec2| {
            // Widen before negating/adding so even i32::MIN cannot overflow.
            let (x, y) = (i64::from(offset.x), i64::from(offset.y));
            let (x, y) = match pose.rotation {
                QuarterTurn::Zero => (x, y),
                QuarterTurn::One => (-y, x),
                QuarterTurn::Two => (-x, -y),
                QuarterTurn::Three => (y, -x),
            };
            let (x, y) = (x + i64::from(pose.anchor.x), y + i64::from(pose.anchor.y));
            if x < 0 || y < 0 || x >= i64::from(self.size.x) || y >= i64::from(self.size.y) {
                Err(PlacementError::OutOfBounds)
            } else {
                Ok(UVec2::new(x as u32, y as u32))
            }
        };
        let occupied: Vec<_> = footprint
            .occupied
            .iter()
            .map(transform)
            .collect::<Result<_, _>>()?;
        let access: Vec<_> = footprint
            .access
            .iter()
            .map(transform)
            .collect::<Result<_, _>>()?;
        for &cell in &occupied {
            if let Some(owner) = self.occupant(cell).filter(|&owner| owner != id) {
                return Err(PlacementError::Occupied { cell, owner });
            }
            for object in self.objects.values().filter(|object| object.id != id) {
                if object.access.contains(&cell) {
                    return Err(PlacementError::BlockedAccess {
                        cell,
                        owner: object.id,
                    });
                }
            }
        }
        for &cell in &access {
            if let Some(owner) = self.occupant(cell).filter(|&owner| owner != id) {
                return Err(PlacementError::BlockedAccess { cell, owner });
            }
        }
        Ok(PlacedObject {
            id,
            footprint: footprint.clone(),
            pose,
            occupied,
            access,
        })
    }

    fn commit(&mut self, after: PlacedObject) -> PlacementChange {
        let before = self.objects.insert(after.id, after.clone());
        if let Some(before) = &before {
            for cell in &before.occupied {
                self.occupancy.remove(&(cell.x, cell.y));
            }
        }
        for cell in &after.occupied {
            self.occupancy.insert((cell.x, cell.y), after.id);
        }
        PlacementChange {
            before,
            after: Some(after),
        }
    }
}

/// Read-only navigation overlay preserving the game's underlying terrain costs.
#[derive(Clone, Copy, Debug)]
pub struct PlacementNav<'a, G: NavGrid + ?Sized> {
    placement: &'a PlacementGrid,
    terrain: &'a G,
}
impl<G: NavGrid + ?Sized> NavGrid for PlacementNav<'_, G> {
    fn size(&self) -> UVec2 {
        self.placement.size
    }
    fn cost(&self, cell: UVec2) -> Option<f32> {
        if self.placement.is_free(cell) {
            self.terrain.cost(cell)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
