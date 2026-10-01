#![doc = include_str!("../README.md")]

pub use rayengine_core::glam;

pub mod block;
pub mod coords;
pub mod meshing;
pub mod ray;
#[cfg(feature = "render")]
pub mod render;
pub mod storage;
pub mod streaming;
pub use streaming::{
    ChunkStreamer, Eviction, StreamConfig, StreamError, StreamFailure, StreamReport,
};

pub use meshing::{
    ChunkMesh, FaceShading, MeshBatch, MeshDependencies, MeshInput, MeshLayer, MeshLimits,
    MeshStats, MeshingError, MeshingMode, MeshingOptions, MissingFaces, SurfaceKey,
};

pub use block::{BlockDef, BlockId, BlockRegistry, CollisionKind, RenderKind, TileId};
pub use coords::{BlockPos, CHUNK_SIZE, CHUNK_VOLUME, ChunkPos, Face, LocalPos};
pub use ray::{GridRay, MissingPolicy, RayCell, Raycast, RaycastOptions, RaycastOutcome, VoxelHit};
pub use storage::{BlockEdit, Chunk, ChunkInsertError, ChunkStamp, DirtyChunks, VoxelWorld};

/// Rejected input, capacity exhaustion, or mutation admission failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoxelError {
    /// Invalid block name, hardness, or other definition properties.
    InvalidDefinition,
    /// A block with the same name is already registered.
    DuplicateName,
    /// All 65,536 block IDs (including air) are in use.
    RegistryFull,
    /// The ID is absent from the chunk's registry.
    UnknownBlock(BlockId),
    /// A local coordinate is outside 0..16.
    InvalidLocalPosition,
    /// A chunk origin or extent is outside the i32 block grid.
    InvalidChunkPosition,
    /// Imported block data has a length other than 4096.
    InvalidChunkLength,
    /// Chunk and world must share the same registry allocation.
    RegistryMismatch,
    /// The configured resident chunk limit would be exceeded.
    WorldFull,
    /// An edit or metadata operation requires a resident chunk.
    MissingChunk(ChunkPos),
    /// An allocation could not be admitted.
    Allocation,
    /// A revision/generation would overflow; no state was changed.
    RevisionExhausted,
    /// Ray origin/direction is nonfinite, zero, loses a moving axis during
    /// normalization, or starts outside the grid.
    InvalidRay,
    /// Distance must be finite/nonnegative and cell budget must be supported.
    InvalidRayOptions,
}
impl std::fmt::Display for VoxelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDefinition => f.write_str("block name or hardness is invalid"),
            Self::DuplicateName => f.write_str("block name is already registered"),
            Self::RegistryFull => f.write_str("block registry has exhausted its 16-bit IDs"),
            Self::UnknownBlock(id) => write!(f, "unknown block ID {}", id.raw()),
            Self::InvalidLocalPosition => {
                f.write_str("local position must be in 0..16 on each axis")
            }
            Self::InvalidChunkPosition => f.write_str("chunk must fit inside the i32 block grid"),
            Self::InvalidChunkLength => f.write_str("chunk data must have exactly 4096 cells"),
            Self::RegistryMismatch => f.write_str("chunk and world must share the same registry"),
            Self::WorldFull => f.write_str("resident chunk limit reached"),
            Self::MissingChunk(pos) => write!(f, "chunk {pos:?} is not resident"),
            Self::Allocation => f.write_str("voxel allocation failed"),
            Self::RevisionExhausted => f.write_str("voxel revision or generation exhausted"),
            Self::InvalidRay => f.write_str(
                "ray must be finite, nonzero, preserve moving axes, and start inside the grid",
            ),
            Self::InvalidRayOptions => f.write_str("ray distance or cell budget is invalid"),
        }
    }
}
impl std::error::Error for VoxelError {}

/// Common voxel imports; gameplay content and survival rules remain game-owned.
pub mod prelude {
    pub use crate::meshing::{
        ChunkMesh, FaceShading, MeshBatch, MeshDependencies, MeshInput, MeshLayer, MeshLimits,
        MeshStats, MeshingError, MeshingMode, MeshingOptions, MissingFaces, SurfaceKey,
    };
    #[cfg(feature = "render")]
    pub use crate::render::{
        ChunkDraw, RenderedChunk, StreamRenderConfig, StreamRenderReport, StreamRenderer,
        StreamResources, TileTexture, VoxelMaterials,
    };
    pub use crate::streaming::{
        ChunkStreamer, Eviction, StreamConfig, StreamError, StreamFailure, StreamReport,
    };
    pub use crate::{
        BlockDef, BlockEdit, BlockId, BlockPos, BlockRegistry, CHUNK_SIZE, CHUNK_VOLUME, Chunk,
        ChunkInsertError, ChunkPos, ChunkStamp, CollisionKind, DirtyChunks, Face, GridRay,
        LocalPos, MissingPolicy, RayCell, Raycast, RaycastOptions, RaycastOutcome, RenderKind,
        TileId, VoxelError, VoxelHit, VoxelWorld,
    };
}
