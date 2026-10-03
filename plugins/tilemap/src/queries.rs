use crate::*;
use rayengine_core::{
    camera::Camera2D,
    collision::Body2D,
    glam::Mat2,
    spatial::{Frustum2D, Ray2, RayHit2, SpatialError, SpatialIndex2D},
    viewport::Viewport,
};

/// Work performed by a culled submission. Empty chunks produce no tile visits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubmissionStats {
    /// Nonempty chunks admitted by the conservative camera AABB, across layers.
    pub visible_chunks: usize,
    /// Occupied visible cells emitted to the visitor.
    pub tiles: usize,
}
/// Collision metadata and geometry for one occupied cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionTile {
    /// Layer index.
    pub layer: usize,
    /// Cell coordinate.
    pub coordinate: (u32, u32),
    /// Definition ID.
    pub tile: TileId,
    /// World collision box.
    pub bounds: Aabb2,
    /// Game collision metadata.
    pub flags: CollisionFlags,
}
/// First solid-tile ray contact. Equal distances prefer layer, then row, then column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileHit {
    /// Tile geometry and metadata.
    pub tile: CollisionTile,
    /// Exact world contact.
    pub hit: RayHit2,
}

fn valid_area(area: Aabb2) -> bool {
    area.min.is_finite() && area.max.is_finite() && area.min.cmple(area.max).all()
}
fn touches(a: Aabb2, b: Aabb2) -> bool {
    a.min.cmple(b.max).all() && a.max.cmpge(b.min).all()
}

impl Tilemap {
    fn cell_range(&self, area: Aabb2) -> Option<(u32, u32, u32, u32)> {
        if !valid_area(area) || !touches(area, self.bounds()) {
            return None;
        }
        // Include neighbors touching the boundary, as required by sweeps/rays.
        let min = ((area.min - self.origin) / self.tile_size).floor() - Vec2::ONE;
        let max = ((area.max - self.origin) / self.tile_size).floor() + Vec2::ONE;
        Some((
            min.x.clamp(0.0, (self.width - 1) as f32) as u32,
            min.y.clamp(0.0, (self.height - 1) as f32) as u32,
            max.x.clamp(0.0, (self.width - 1) as f32) as u32,
            max.y.clamp(0.0, (self.height - 1) as f32) as u32,
        ))
    }
    /// Visits occupied cells whose boxes touch a world region, in layer/chunk order.
    /// Invalid or disjoint regions emit nothing. Boundary contacts are included.
    /// Only chunks intersecting the region are traversed; no allocation occurs.
    pub fn visit_region(&self, area: Aabb2, mut emit: impl FnMut(CollisionTile)) {
        let Some((x0, y0, x1, y1)) = self.cell_range(area) else {
            return;
        };
        for (layer, data) in self.layers.iter().enumerate() {
            for cy in y0 / CHUNK_SIZE..=y1 / CHUNK_SIZE {
                for cx in x0 / CHUNK_SIZE..=x1 / CHUNK_SIZE {
                    let chunk = &data.chunks[(cy * self.chunks_x + cx) as usize];
                    if chunk.occupied == 0 {
                        continue;
                    }
                    for y in y0.max(cy * CHUNK_SIZE)..=y1.min((cy + 1) * CHUNK_SIZE - 1) {
                        for x in x0.max(cx * CHUNK_SIZE)..=x1.min((cx + 1) * CHUNK_SIZE - 1) {
                            let Some(tile) = chunk.tiles
                                [(y % CHUNK_SIZE * CHUNK_SIZE + x % CHUNK_SIZE) as usize]
                            else {
                                continue;
                            };
                            let bounds = self.tile_bounds(x, y).unwrap();
                            if touches(bounds, area) {
                                emit(CollisionTile {
                                    layer,
                                    coordinate: (x, y),
                                    tile,
                                    bounds,
                                    flags: self.palette[tile.0 as usize].collision,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    /// Submits only occupied visible cells, preserving layer order. Uses the
    /// rotated camera rectangle and the current fitted/expanded viewport.
    /// Work is bounded by chunks in the camera's world AABB, then exact tests.
    pub fn visit_visible(
        &self,
        camera: &Camera2D,
        viewport: &Viewport,
        mut emit: impl FnMut(CollisionTile),
    ) -> Result<SubmissionStats, SpatialError> {
        let frustum = Frustum2D::from_camera(camera, viewport)?;
        let half = Vec2::new(
            camera.view_height * viewport.aspect() * 0.5,
            camera.view_height * 0.5,
        );
        let rotation = Mat2::from_angle(-camera.rotation);
        let extent =
            (rotation * Vec2::new(half.x, 0.0)).abs() + (rotation * Vec2::new(0.0, half.y)).abs();
        // Bound rounding in the camera/tile SAT arithmetic. At large world
        // coordinates even a cell center can round by half an ulp. Keep the
        // chunk broadphase conservative and leave oriented tests to the cells:
        // f32 SAT is not monotone between a chunk and its contained tiles.
        let padding = camera.target.abs().max(extent) * (4.0 * f32::EPSILON);
        let area = Aabb2 {
            min: camera.target - extent - padding,
            max: camera.target + extent + padding,
        };
        if !valid_area(area) {
            return Err(SpatialError::InvalidCamera);
        }
        let mut stats = SubmissionStats::default();
        let Some((x0, y0, x1, y1)) = self.cell_range(area) else {
            return Ok(stats);
        };
        for (layer, data) in self.layers.iter().enumerate() {
            for cy in y0 / CHUNK_SIZE..=y1 / CHUNK_SIZE {
                for cx in x0 / CHUNK_SIZE..=x1 / CHUNK_SIZE {
                    let chunk = &data.chunks[(cy * self.chunks_x + cx) as usize];
                    if chunk.occupied == 0 || !touches(self.chunk_bounds(cx, cy), area) {
                        continue;
                    }
                    stats.visible_chunks += 1;
                    for y in cy * CHUNK_SIZE..((cy + 1) * CHUNK_SIZE).min(self.height) {
                        for x in cx * CHUNK_SIZE..((cx + 1) * CHUNK_SIZE).min(self.width) {
                            let Some(tile) = chunk.tiles
                                [(y % CHUNK_SIZE * CHUNK_SIZE + x % CHUNK_SIZE) as usize]
                            else {
                                continue;
                            };
                            let bounds = self.tile_bounds(x, y).unwrap();
                            if frustum.intersects(bounds) {
                                stats.tiles += 1;
                                emit(CollisionTile {
                                    layer,
                                    coordinate: (x, y),
                                    tile,
                                    bounds,
                                    flags: self.palette[tile.0 as usize].collision,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(stats)
    }
    /// Collects solid boxes touching a region for `Body2D::move_and_slide` or
    /// an external physics broadphase. One-way/trigger/custom cells are omitted.
    pub fn solid_geometry(&self, area: Aabb2) -> Vec<Aabb2> {
        let mut boxes = Vec::new();
        self.visit_region(area, |tile| {
            if tile.flags.solid {
                boxes.push(tile.bounds);
            }
        });
        boxes
    }
    /// Rebuilds an engine spatial snapshot of solid boxes with layer/cell IDs.
    /// Call again after edits if the external snapshot is retained.
    pub fn rebuild_solid_index(
        &self,
        index: &mut SpatialIndex2D<(usize, u32, u32)>,
    ) -> Result<usize, SpatialError> {
        let mut entries = Vec::new();
        self.visit_region(self.bounds(), |tile| {
            if tile.flags.solid {
                entries.push((
                    (tile.layer, tile.coordinate.0, tile.coordinate.1),
                    tile.bounds,
                ));
            }
        });
        index.rebuild(entries)
    }
    /// Sweeps X against solids, then Y against solids and eligible top faces.
    /// One-way cells only block downward movement starting above their top.
    /// `drop_through` disables one-way cells for this step. Same initial-overlap
    /// and axis-order contract as `Body2D::move_and_slide`; dt must be finite
    /// and nonnegative, body geometry and displacement must be finite.
    pub fn move_body(&self, body: &mut Body2D, dt: f32, drop_through: bool) {
        assert!(dt.is_finite() && dt >= 0.0);
        assert!(
            body.position.is_finite()
                && body.velocity.is_finite()
                && body.half_size.is_finite()
                && body.half_size.min_element() >= 0.0
        );
        let step = body.velocity * dt;
        let bounds = body.bounds();
        let area = Aabb2 {
            min: bounds.min.min(bounds.min + step),
            max: bounds.max.max(bounds.max + step),
        };
        assert!(valid_area(area));
        let mut boxes = self.solid_geometry(area);
        let velocity_y = body.velocity.y;
        body.velocity.y = 0.0;
        body.move_and_slide(dt, &boxes);
        body.velocity.y = velocity_y;
        let velocity_x = body.velocity.x;
        body.velocity.x = 0.0;
        if velocity_y > 0.0 && !drop_through {
            let bottom = body.bounds().max.y;
            self.visit_region(area, |tile| {
                if !tile.flags.solid && tile.flags.one_way && bottom <= tile.bounds.min.y {
                    boxes.push(tile.bounds);
                }
            });
        }
        body.move_and_slide(dt, &boxes);
        body.velocity.x = velocity_x;
    }
    /// Casts against solids only, using chunk box rejection before cell tests.
    /// Infinity is accepted; negative/NaN distances are rejected. Boundary
    /// contacts are inclusive and starts inside solids hit at distance zero.
    pub fn raycast(&self, ray: Ray2, max_distance: f32) -> Result<Option<TileHit>, SpatialError> {
        // Also validate max_distance for empty maps.
        if ray.cast(self.bounds(), max_distance)?.is_none() {
            return Ok(None);
        }
        let mut closest: Option<TileHit> = None;
        for (layer, data) in self.layers.iter().enumerate() {
            for cy in 0..self.chunks_y {
                for cx in 0..self.chunks_x {
                    let chunk = &data.chunks[(cy * self.chunks_x + cx) as usize];
                    let limit = closest.map_or(max_distance, |hit| hit.hit.distance);
                    if chunk.occupied == 0 || ray.cast(self.chunk_bounds(cx, cy), limit)?.is_none()
                    {
                        continue;
                    }
                    for y in cy * CHUNK_SIZE..((cy + 1) * CHUNK_SIZE).min(self.height) {
                        for x in cx * CHUNK_SIZE..((cx + 1) * CHUNK_SIZE).min(self.width) {
                            let Some(tile) = self.tile(layer, x, y) else {
                                continue;
                            };
                            let flags = self.palette[tile.0 as usize].collision;
                            if !flags.solid {
                                continue;
                            }
                            let bounds = self.tile_bounds(x, y).unwrap();
                            let limit = closest.map_or(max_distance, |hit| hit.hit.distance);
                            let Some(hit) = ray.cast(bounds, limit)? else {
                                continue;
                            };
                            let replace = closest.is_none_or(|old| {
                                hit.distance < old.hit.distance
                                    || (hit.distance == old.hit.distance
                                        && (layer, y, x)
                                            < (
                                                old.tile.layer,
                                                old.tile.coordinate.1,
                                                old.tile.coordinate.0,
                                            ))
                            });
                            if replace {
                                closest = Some(TileHit {
                                    tile: CollisionTile {
                                        layer,
                                        coordinate: (x, y),
                                        tile,
                                        bounds,
                                        flags,
                                    },
                                    hit,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(closest)
    }
}
