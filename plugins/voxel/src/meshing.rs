//! Owned worker inputs and deterministic, bounded face meshing.
use crate::{
    BlockId, BlockRegistry, ChunkPos, ChunkStamp, Face, RenderKind, TileId, VoxelError, VoxelWorld,
};
use rayengine_core::{
    collision::Aabb3,
    glam::{Vec2, Vec3},
    mesh::{MAX_INDEXED_VERTICES, MeshData},
    spatial::Frustum3D,
};
use std::{fmt, sync::Arc};

const PAD: usize = 18;
/// Maximum visible unit faces in a 16³ chunk.
pub const MAX_CHUNK_FACES: usize = 4096 * 6;

/// Opaque and alpha-tested surfaces are drawn with depth writes enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MeshLayer {
    /// Fully opaque surface.
    Opaque,
    /// Alpha-cutout surface; it does not occlude neighboring faces.
    Cutout,
}
/// One texture and alpha policy; merges never cross this boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceKey {
    /// Game-owned texture key.
    pub tile: TileId,
    /// Required material alpha policy.
    pub layer: MeshLayer,
}
/// Comparison mode for identical input data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshingMode {
    /// Emit one quad per visible block face.
    Culled,
    /// Merge coplanar rectangles sharing tile, layer and face shading.
    #[default]
    Greedy,
}
/// Behavior at unavailable chunk borders (including the finite grid edge).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissingFaces {
    /// Draw exposed boundary faces until neighboring data arrives.
    #[default]
    Expose,
    /// Suppress boundary faces until neighboring data arrives.
    Hide,
}
/// Per-face directional sunlight/ambient intensity in Face::ALL order.
/// No propagated light, column shadows, ambient occlusion or torches are baked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceShading(pub [u8; 6]);
impl Default for FaceShading {
    fn default() -> Self {
        Self([204, 204, 140, 255, 178, 178])
    }
}
/// Hard output budgets, checked before constructing vertex buffers.
#[derive(Clone, Copy, Debug)]
pub struct MeshLimits {
    /// Maximum emitted quads, 1..=MAX_CHUNK_FACES.
    pub max_quads: usize,
    /// Maximum independent GPU batches, 1..=MAX_CHUNK_FACES.
    pub max_batches: usize,
    /// Indexed vertices per batch, a multiple of four in 4..=65532.
    pub max_vertices_per_batch: usize,
}
impl Default for MeshLimits {
    fn default() -> Self {
        Self {
            max_quads: MAX_CHUNK_FACES,
            max_batches: MAX_CHUNK_FACES,
            max_vertices_per_batch: MAX_INDEXED_VERTICES / 4 * 4,
        }
    }
}
/// Geometry policy; immutable input and options produce deterministic output.
#[derive(Clone, Copy, Debug, Default)]
pub struct MeshingOptions {
    /// Face merging policy.
    pub mode: MeshingMode,
    /// Missing-neighbor face policy.
    pub missing: MissingFaces,
    /// Baked directional intensities.
    pub shading: FaceShading,
    /// Explicit output budgets.
    pub limits: MeshLimits,
}
/// Meshing failure; no uploaded geometry is changed by CPU generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshingError {
    /// Snapshot admission failed.
    Storage(VoxelError),
    /// Blended transparent blocks require a separate sorting/rendering policy.
    UnsupportedTransparent(BlockId),
    /// A configured output limit is outside its supported range.
    InvalidLimits,
    /// Output exceeds a configured face or batch budget.
    LimitExceeded,
    /// A CPU allocation could not be admitted.
    Allocation,
}
impl fmt::Display for MeshingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(e) => e.fmt(f),
            Self::UnsupportedTransparent(id) => {
                write!(f, "block {} needs blended meshing", id.raw())
            }
            Self::InvalidLimits => f.write_str("invalid chunk mesh limits"),
            Self::LimitExceeded => f.write_str("chunk mesh output budget exceeded"),
            Self::Allocation => f.write_str("chunk mesh allocation failed"),
        }
    }
}
impl std::error::Error for MeshingError {}
impl From<VoxelError> for MeshingError {
    fn from(e: VoxelError) -> Self {
        Self::Storage(e)
    }
}

/// Owner plus six face-neighbor dependency stamps, including missing data.
/// Captures world identity too; stamps cannot be transferred between worlds.
#[derive(Clone)]
pub struct MeshDependencies {
    identity: Arc<()>,
    position: ChunkPos,
    stamps: [Option<ChunkStamp>; 7],
}
impl MeshDependencies {
    /// Owner chunk.
    pub fn position(&self) -> ChunkPos {
        self.position
    }
    /// Owner stamp followed by Face::ALL neighbors; None records missing/out-of-grid data.
    pub fn stamps(&self) -> &[Option<ChunkStamp>; 7] {
        &self.stamps
    }
    /// Rejects edits, replacement, removal, neighbor arrivals and another world.
    /// Conservatively rejects any neighbor edit, even outside the sampled border.
    pub fn is_current(&self, world: &VoxelWorld) -> bool {
        Arc::ptr_eq(&self.identity, &world.identity)
            && self.stamps == dependency_stamps(world, self.position)
    }
}
fn dependency_stamps(world: &VoxelWorld, position: ChunkPos) -> [Option<ChunkStamp>; 7] {
    let mut stamps = [None; 7];
    stamps[0] = world.stamp(position);
    for face in Face::ALL {
        stamps[face.index() + 1] = position.neighbor(face).and_then(|p| world.stamp(p));
    }
    stamps
}

/// Owned 18³ padded block buffer: owner cells plus six neighbor border slabs.
/// Only face slabs are populated; diagonal padding stays air and is not sampled.
/// Capture on the world-owning thread, then move to an existing CPU job queue.
pub struct MeshInput {
    registry: Arc<BlockRegistry>,
    blocks: Vec<BlockId>,
    dependencies: MeshDependencies,
}
impl MeshInput {
    /// Copies 4096 owner cells and up to 1536 neighboring cells, with at most seven map
    /// lookups. Missing owner is an error; neighboring absence is recorded.
    pub fn capture(world: &VoxelWorld, position: ChunkPos) -> Result<Self, MeshingError> {
        position.origin()?;
        let (owner, stamp) = world
            .chunk_with_stamp(position)
            .ok_or(VoxelError::MissingChunk(position))?;
        let mut stamps = [None; 7];
        stamps[0] = Some(stamp);
        let mut blocks = reserved(PAD * PAD * PAD)?;
        blocks.resize(PAD * PAD * PAD, BlockId::AIR);
        for y in 0..16 {
            for z in 0..16 {
                let source = y * 256 + z * 16;
                let dest = padded([1, y + 1, z + 1]);
                blocks[dest..dest + 16].copy_from_slice(&owner.blocks()[source..source + 16]);
            }
        }
        for face in Face::ALL {
            let Some((neighbor, stamp)) = position
                .neighbor(face)
                .and_then(|p| world.chunk_with_stamp(p))
            else {
                continue;
            };
            stamps[face.index() + 1] = Some(stamp);
            let (axis, u, v, positive) = axes(face);
            for b in 0..16 {
                for a in 0..16 {
                    let mut local = [0; 3];
                    local[axis] = if positive { 0 } else { 15 };
                    local[u] = a;
                    local[v] = b;
                    let source = local[0] + local[2] * 16 + local[1] * 256;
                    let mut dest = local.map(|c| c + 1);
                    dest[axis] = if positive { 17 } else { 0 };
                    blocks[padded(dest)] = neighbor.blocks()[source];
                }
            }
        }
        Ok(Self {
            registry: world.shared_registry(),
            blocks,
            dependencies: MeshDependencies {
                identity: world.identity.clone(),
                position,
                stamps,
            },
        })
    }
    /// Dependency receipt for acceptance/invalidation checks.
    pub fn dependencies(&self) -> &MeshDependencies {
        &self.dependencies
    }
    /// Owned block payload (registry and metadata excluded).
    pub fn block_bytes(&self) -> usize {
        self.blocks.len() * size_of::<BlockId>()
    }
    /// Generates local-space indexed meshes. UVs repeat once per block; a merged
    /// face is never stretched. Use the repeating renderer or an equivalent shader.
    pub fn build(&self, options: MeshingOptions) -> Result<ChunkMesh, MeshingError> {
        let limits = options.limits;
        if !(1..=MAX_CHUNK_FACES).contains(&limits.max_quads)
            || !(1..=MAX_CHUNK_FACES).contains(&limits.max_batches)
            || !(4..=MAX_INDEXED_VERTICES / 4 * 4).contains(&limits.max_vertices_per_batch)
            || !limits.max_vertices_per_batch.is_multiple_of(4)
        {
            return Err(MeshingError::InvalidLimits);
        }
        let mut quads = Vec::new();
        let mut visible_faces = 0;
        for face in Face::ALL {
            let (axis, u, v, positive) = axes(face);
            for slice in 0..16 {
                let mut mask = [None; 256];
                for b in 0..16 {
                    for a in 0..16 {
                        let mut p = [1; 3];
                        p[axis] = slice + 1;
                        p[u] = a + 1;
                        p[v] = b + 1;
                        let id = self.blocks[padded(p)];
                        let def = self
                            .registry
                            .get(id)
                            .expect("snapshot IDs validated by storage");
                        let layer = match def.render {
                            RenderKind::Invisible => continue,
                            RenderKind::Opaque => MeshLayer::Opaque,
                            RenderKind::Cutout => MeshLayer::Cutout,
                            RenderKind::Transparent => {
                                return Err(MeshingError::UnsupportedTransparent(id));
                            }
                        };
                        let border = if positive { slice == 15 } else { slice == 0 };
                        if border
                            && self.dependencies.stamps[face.index() + 1].is_none()
                            && options.missing == MissingFaces::Hide
                        {
                            continue;
                        }
                        p[axis] = if positive { p[axis] + 1 } else { p[axis] - 1 };
                        let neighbor = self.registry.get(self.blocks[padded(p)]).unwrap();
                        // Alpha holes must show geometry behind them, including internal foliage faces.
                        if neighbor.render == RenderKind::Opaque {
                            continue;
                        }
                        mask[b * 16 + a] = Some(SurfaceKey {
                            tile: def.texture(face),
                            layer,
                        });
                        visible_faces += 1;
                    }
                }
                for b in 0..16 {
                    for a in 0..16 {
                        let Some(key) = mask[b * 16 + a] else {
                            continue;
                        };
                        let mut width = 1;
                        let mut height = 1;
                        if options.mode == MeshingMode::Greedy {
                            while a + width < 16 && mask[b * 16 + a + width] == Some(key) {
                                width += 1;
                            }
                            while b + height < 16
                                && (a..a + width).all(|x| mask[(b + height) * 16 + x] == Some(key))
                            {
                                height += 1;
                            }
                        }
                        for row in b..b + height {
                            mask[row * 16 + a..row * 16 + a + width].fill(None);
                        }
                        if quads.len() == limits.max_quads {
                            return Err(MeshingError::LimitExceeded);
                        }
                        quads.try_reserve(1).map_err(|_| MeshingError::Allocation)?;
                        quads.push(Quad {
                            key,
                            face,
                            slice: slice as u8,
                            a: a as u8,
                            b: b as u8,
                            width: width as u8,
                            height: height as u8,
                        });
                    }
                }
            }
        }
        // Include all fields to make equal-surface ordering reproducible too.
        quads.sort_unstable_by_key(|q| {
            (q.key.layer, q.key.tile.0, q.face.index(), q.slice, q.b, q.a)
        });
        let per_batch = limits.max_vertices_per_batch / 4;
        let mut batches = Vec::new();
        let mut cursor = 0;
        let mut stats = MeshStats {
            visible_faces,
            quads: quads.len(),
            ..Default::default()
        };
        while cursor < quads.len() {
            if batches.len() == limits.max_batches {
                return Err(MeshingError::LimitExceeded);
            }
            let key = quads[cursor].key;
            let count = quads[cursor..]
                .iter()
                .take(per_batch)
                .take_while(|q| q.key == key)
                .count();
            let mut data = MeshData {
                positions: reserved(count * 4)?,
                normals: Some(reserved(count * 4)?),
                texcoords: Some(reserved(count * 4)?),
                colors: Some(reserved(count * 4)?),
                indices: Some(reserved(count * 6)?),
            };
            for q in &quads[cursor..cursor + count] {
                q.emit(&mut data, options.shading);
            }
            stats.vertices += count * 4;
            stats.triangles += count * 2;
            stats.buffer_bytes += count * (4 * (12 + 12 + 8 + 4) + 6 * 2);
            batches
                .try_reserve(1)
                .map_err(|_| MeshingError::Allocation)?;
            batches.push(MeshBatch { key, data });
            cursor += count;
        }
        stats.batches = batches.len();
        Ok(ChunkMesh {
            dependencies: self.dependencies.clone(),
            batches,
            stats,
        })
    }
}
fn reserved<T>(count: usize) -> Result<Vec<T>, MeshingError> {
    let mut v = Vec::new();
    v.try_reserve_exact(count)
        .map_err(|_| MeshingError::Allocation)?;
    Ok(v)
}
fn padded(p: [usize; 3]) -> usize {
    p[0] + PAD * p[2] + PAD * PAD * p[1]
}
// Choose positive grid tangents; emission adjusts winding and side UV orientation.
fn axes(face: Face) -> (usize, usize, usize, bool) {
    match face {
        Face::NegX => (0, 2, 1, false),
        Face::PosX => (0, 2, 1, true),
        Face::NegY => (1, 0, 2, false),
        Face::PosY => (1, 0, 2, true),
        Face::NegZ => (2, 0, 1, false),
        Face::PosZ => (2, 0, 1, true),
    }
}
struct Quad {
    key: SurfaceKey,
    face: Face,
    slice: u8,
    a: u8,
    b: u8,
    width: u8,
    height: u8,
}
impl Quad {
    fn emit(&self, data: &mut MeshData, shading: FaceShading) {
        let (axis, u, v, positive) = axes(self.face);
        let mut origin = [0.0; 3];
        origin[axis] = f32::from(self.slice) + if positive { 1.0 } else { 0.0 };
        origin[u] = f32::from(self.a);
        origin[v] = f32::from(self.b);
        let mut du = [0.0; 3];
        du[u] = f32::from(self.width);
        let mut dv = [0.0; 3];
        dv[v] = f32::from(self.height);
        let o = Vec3::from_array(origin);
        let du = Vec3::from_array(du);
        let dv = Vec3::from_array(dv);
        let normal = Vec3::from_array(self.face.normal().map(|c| c as f32));
        let flip = du.cross(dv).dot(normal) < 0.0;
        let base = data.positions.len() as u16;
        data.positions.extend([o, o + du, o + du + dv, o + dv]);
        data.normals.as_mut().unwrap().extend([normal; 4]);
        // +Y has no upright side convention; on side faces, V=0 is at the top.
        let w = f32::from(self.width);
        let h = f32::from(self.height);
        let uv = if axis == 1 {
            [
                Vec2::ZERO,
                Vec2::new(w, 0.0),
                Vec2::new(w, h),
                Vec2::new(0.0, h),
            ]
        } else if positive == (axis == 2) {
            [
                Vec2::new(0.0, h),
                Vec2::new(w, h),
                Vec2::new(w, 0.0),
                Vec2::ZERO,
            ]
        } else {
            [
                Vec2::new(w, h),
                Vec2::new(0.0, h),
                Vec2::ZERO,
                Vec2::new(w, 0.0),
            ]
        };
        data.texcoords.as_mut().unwrap().extend(uv);
        let c = shading.0[self.face.index()];
        data.colors.as_mut().unwrap().extend([[c, c, c, 255]; 4]);
        let indices = if flip {
            [0, 2, 1, 0, 3, 2]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        data.indices
            .as_mut()
            .unwrap()
            .extend(indices.map(|i| base + i));
    }
}

/// Counts and exact buffer payload, independent of capacities/driver overhead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshStats {
    /// Exposed unit faces before merging.
    pub visible_faces: usize,
    /// Rectangles after merging.
    pub quads: usize,
    /// Indexed vertices.
    pub vertices: usize,
    /// Complete triangles.
    pub triangles: usize,
    /// Material/16-bit-limit partitions.
    pub batches: usize,
    /// Positions, normals, UVs, colors and indices; excludes Vec/driver overhead.
    pub buffer_bytes: usize,
}
/// One valid indexed mesh using a single tile/layer.
pub struct MeshBatch {
    key: SurfaceKey,
    data: MeshData,
}
impl MeshBatch {
    /// Required material.
    pub fn surface(&self) -> SurfaceKey {
        self.key
    }
    /// Immutable SDK upload data.
    pub fn data(&self) -> &MeshData {
        &self.data
    }
}
/// Completed CPU result; an empty chunk has no batches and is a valid result.
pub struct ChunkMesh {
    dependencies: MeshDependencies,
    batches: Vec<MeshBatch>,
    stats: MeshStats,
}
impl ChunkMesh {
    /// Owner and neighbor receipts used to reject stale worker results.
    pub fn dependencies(&self) -> &MeshDependencies {
        &self.dependencies
    }
    /// Ordered material batches (opaque, then cutout; each ordered by tile).
    pub fn batches(&self) -> &[MeshBatch] {
        &self.batches
    }
    /// Exact geometry/payload counts.
    pub fn stats(&self) -> MeshStats {
        self.stats
    }
    /// Conservative camera visibility without allocating. Local vertex data and
    /// camera-relative translation keep precision near distant signed coordinates.
    /// View's camera must use the same render origin as the translation.
    pub fn visible(&self, view: &Frustum3D, render_origin: crate::BlockPos) -> bool {
        view.intersects(chunk_bounds(self.dependencies.position, render_origin))
    }
}
/// Conservative camera-relative chunk bounds. Compute differences as i64 before
/// converting to f32; absolute i32 grid positions lose unit precision in f32.
pub fn chunk_bounds(position: ChunkPos, render_origin: crate::BlockPos) -> Aabb3 {
    let translation = chunk_translation(position, render_origin);
    Aabb3 {
        min: translation,
        max: translation + Vec3::splat(16.0),
    }
}
/// Translation for local chunk vertices into a camera-relative render space.
/// Position must be a validated resident/snapshot coordinate.
pub fn chunk_translation(position: ChunkPos, origin: crate::BlockPos) -> Vec3 {
    Vec3::new(
        (i64::from(position.x) * 16 - i64::from(origin.x)) as f32,
        (i64::from(position.y) * 16 - i64::from(origin.y)) as f32,
        (i64::from(position.z) * 16 - i64::from(origin.z)) as f32,
    )
}

#[cfg(test)]
mod tests;
