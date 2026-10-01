//! Dense validated chunks and a capacity-limited sparse resident world.
use crate::{BlockId, BlockPos, BlockRegistry, CHUNK_VOLUME, ChunkPos, Face, LocalPos, VoxelError};
use std::{collections::HashMap, sync::Arc};

/// One dense chunk: exactly 4096 16-bit IDs (8192 bytes of cell payload).
/// Construction validates all IDs. Dirty state is separate from revisions.
/// New/imported chunks are dirty until explicitly acknowledged with mark_saved.
pub struct Chunk {
    registry: Arc<BlockRegistry>,
    blocks: Box<[BlockId]>,
    revision: u64,
    saved_revision: Option<u64>,
}
impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chunk")
            .field("revision", &self.revision)
            .field("saved_revision", &self.saved_revision)
            .field("cell_count", &self.blocks.len())
            .finish_non_exhaustive()
    }
}

/// Failed admission with the incoming chunk retained for recovery or saving.
#[derive(Debug)]
pub struct ChunkInsertError {
    /// Admission failure; existing resident state is unchanged.
    pub error: VoxelError,
    /// Incoming data, including its dirty/save state.
    pub chunk: Chunk,
}
impl std::fmt::Display for ChunkInsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for ChunkInsertError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
impl Chunk {
    /// Fallibly allocates a uniform chunk using a registered block.
    pub fn filled(registry: Arc<BlockRegistry>, block: BlockId) -> Result<Self, VoxelError> {
        if registry.get(block).is_none() {
            return Err(VoxelError::UnknownBlock(block));
        }
        let mut blocks = Vec::new();
        blocks
            .try_reserve_exact(CHUNK_VOLUME)
            .map_err(|_| VoxelError::Allocation)?;
        blocks.resize(CHUNK_VOLUME, block);
        // Every cell uses the single ID already validated above. Imported
        // arbitrary buffers still go through from_blocks' complete scan.
        Ok(Self {
            registry,
            blocks: blocks.into_boxed_slice(),
            revision: 0,
            saved_revision: None,
        })
    }
    /// Consumes a dense buffer in X/Z/Y order after validating length and IDs.
    /// Excess Vec capacity is discarded when boxing; exact-capacity buffers
    /// transfer without a cell copy. Invalid data is rejected.
    pub fn from_blocks(
        registry: Arc<BlockRegistry>,
        blocks: Vec<BlockId>,
    ) -> Result<Self, VoxelError> {
        if blocks.len() != CHUNK_VOLUME {
            return Err(VoxelError::InvalidChunkLength);
        }
        if let Some(&id) = blocks.iter().find(|&&id| registry.get(id).is_none()) {
            return Err(VoxelError::UnknownBlock(id));
        }
        Ok(Self {
            registry,
            blocks: blocks.into_boxed_slice(),
            revision: 0,
            saved_revision: None,
        })
    }
    /// Immutable registry shared by the world and all its resident chunks.
    pub fn registry(&self) -> &BlockRegistry {
        &self.registry
    }
    /// Read one validated local coordinate.
    pub fn get(&self, local: LocalPos) -> BlockId {
        self.blocks[local.index()]
    }
    /// Read-only dense data for snapshots, generation, and future meshing.
    pub fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }
    /// Monotonic content revision. No-op edits do not advance it.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Whether the current content lacks an explicit save acknowledgement.
    pub fn is_dirty(&self) -> bool {
        self.saved_revision != Some(self.revision)
    }
    /// Acknowledges only an exactly matching snapshot; stale completions do not
    /// clear newer edits. The caller must know that the save actually succeeded.
    pub fn mark_saved(&mut self, revision: u64) -> bool {
        if revision != self.revision {
            return false;
        }
        self.saved_revision = Some(revision);
        true
    }
    /// Edits an offline chunk, returning the previous block only on a change.
    /// Unknown IDs/overflow leave content and metadata unchanged. Resident edits
    /// should go through [`VoxelWorld::set_block`] to obtain border invalidation data.
    pub fn set(&mut self, local: LocalPos, block: BlockId) -> Result<Option<BlockId>, VoxelError> {
        if self.registry.get(block).is_none() {
            return Err(VoxelError::UnknownBlock(block));
        }
        let old = self.get(local);
        if old == block {
            return Ok(None);
        }
        let next = self
            .revision
            .checked_add(1)
            .ok_or(VoxelError::RevisionExhausted)?;
        self.blocks[local.index()] = block;
        self.revision = next;
        Ok(Some(old))
    }
}

/// Resident identity plus content revision, suitable for rejecting stale jobs.
/// Generations are never reused within one VoxelWorld, including remove/reinsert.
/// A stamp has meaning only in its originating VoxelWorld; adjacent mesh dependencies
/// need their own stamps too. Save completions must match the whole stamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkStamp {
    /// Installation generation within this world.
    pub generation: u64,
    /// Chunk content revision within this installation.
    pub revision: u64,
}

/// Owner and up to three face-border neighbors affected by an edit.
/// Includes unloaded neighbors; a later mesher/streamer owns invalidation policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyChunks {
    positions: [ChunkPos; 4],
    len: u8,
}
impl DirtyChunks {
    fn for_edit(pos: ChunkPos, local: LocalPos) -> Self {
        let mut dirty = Self {
            positions: [pos; 4],
            len: 1,
        };
        let borders = [
            local.x() == 0,
            local.x() == 15,
            local.y() == 0,
            local.y() == 15,
            local.z() == 0,
            local.z() == 15,
        ];
        for (face, touches) in Face::ALL.into_iter().zip(borders) {
            if touches && let Some(neighbor) = pos.neighbor(face) {
                dirty.positions[usize::from(dirty.len)] = neighbor;
                dirty.len += 1;
            }
        }
        dirty
    }
    /// Owner first, then representable neighbors in [`Face::ALL`] order.
    pub fn as_slice(&self) -> &[ChunkPos] {
        &self.positions[..usize::from(self.len)]
    }
}

/// One successful resident edit; absent for unchanged values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockEdit {
    /// Edited world cell.
    pub position: BlockPos,
    /// Replaced ID.
    pub previous: BlockId,
    /// Newly installed ID.
    pub current: BlockId,
    /// Owner's new identity/content stamp.
    pub stamp: ChunkStamp,
    /// Mesh invalidation candidates without per-edit heap allocation.
    pub affected_chunks: DirtyChunks,
}
struct Resident {
    chunk: Chunk,
    generation: u64,
}

/// Game-owned sparse resident chunks, with an explicit maximum count.
/// Missing data is not air. Reads never allocate; edits never create chunks.
/// This is storage, not an automatic streaming/persistence service.
pub struct VoxelWorld {
    // Distinguishes dependency stamps from separate worlds sharing a registry.
    pub(crate) identity: Arc<()>,
    registry: Arc<BlockRegistry>,
    chunks: HashMap<ChunkPos, Resident>,
    max_chunks: usize,
    generation: u64,
}
impl VoxelWorld {
    /// Creates empty storage. Zero capacity is valid; it admits no chunks.
    /// The limit bounds dense payload to `max_chunks * 8192` bytes plus registry,
    /// map/metadata overhead, caller-owned chunks, and snapshots. It is not a
    /// process-memory/VRAM limit. No chunk capacity is allocated in this call.
    pub fn new(registry: Arc<BlockRegistry>, max_chunks: usize) -> Self {
        Self {
            identity: Arc::new(()),
            registry,
            chunks: HashMap::new(),
            max_chunks,
            generation: 0,
        }
    }
    /// Immutable definitions used by every resident chunk.
    pub fn registry(&self) -> &BlockRegistry {
        &self.registry
    }
    /// Clones the same immutable registry allocation for new chunks or CPU jobs.
    /// This does not copy definitions or require a separately retained handle.
    pub fn shared_registry(&self) -> Arc<BlockRegistry> {
        self.registry.clone()
    }
    /// Resident chunk count.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }
    /// Whether no chunks are resident.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
    /// Configured maximum resident count.
    pub fn capacity(&self) -> usize {
        self.max_chunks
    }
    /// Immutable resident chunk, or None for missing data.
    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos).map(|c| &c.chunk)
    }
    /// Current resident generation/revision, or None when missing.
    pub fn stamp(&self, pos: ChunkPos) -> Option<ChunkStamp> {
        self.chunks.get(&pos).map(|c| ChunkStamp {
            generation: c.generation,
            revision: c.chunk.revision(),
        })
    }
    /// Resident chunks in unspecified map order; sort positions for reproducible
    /// serialization or generation recipes that depend on iteration order.
    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPos, &Chunk)> {
        self.chunks.iter().map(|(&pos, c)| (pos, &c.chunk))
    }
    /// Inserts/replaces a validated chunk and returns the previous resident.
    /// Rejects out-of-grid coordinates, registry mismatch, capacity, allocation,
    /// and generation overflow before replacing any existing data.
    /// Replacement is explicit even if the old chunk is dirty; the caller owns
    /// preservation/save policy and receives the displaced chunk.
    pub fn insert_chunk(
        &mut self,
        pos: ChunkPos,
        chunk: Chunk,
    ) -> Result<Option<Chunk>, ChunkInsertError> {
        let admission = (|| {
            pos.origin()?;
            if !Arc::ptr_eq(&self.registry, &chunk.registry) {
                return Err(VoxelError::RegistryMismatch);
            }
            let replacing = self.chunks.contains_key(&pos);
            if !replacing && self.chunks.len() >= self.max_chunks {
                return Err(VoxelError::WorldFull);
            }
            let generation = self
                .generation
                .checked_add(1)
                .ok_or(VoxelError::RevisionExhausted)?;
            if !replacing {
                self.chunks
                    .try_reserve(1)
                    .map_err(|_| VoxelError::Allocation)?;
            }
            Ok(generation)
        })();
        let generation = match admission {
            Ok(generation) => generation,
            Err(error) => return Err(ChunkInsertError { error, chunk }),
        };
        let previous = self.chunks.insert(pos, Resident { chunk, generation });
        self.generation = generation;
        Ok(previous.map(|c| c.chunk))
    }
    /// Explicitly removes a resident, returning its data even if dirty. Never
    /// silently saves/discards edits; streaming must handle the returned chunk.
    /// Map allocation may retain its bounded high-water capacity until VoxelWorld drops.
    pub fn remove_chunk(&mut self, pos: ChunkPos) -> Option<Chunk> {
        self.chunks.remove(&pos).map(|c| c.chunk)
    }
    /// Reads a world cell. Some(AIR) means loaded air; None means missing chunk.
    pub fn block(&self, pos: BlockPos) -> Option<BlockId> {
        let (chunk, local) = pos.split();
        self.chunk(chunk).map(|c| c.get(local))
    }
    /// Edits a resident cell with revision and bounded neighbor notifications.
    pub fn set_block(
        &mut self,
        position: BlockPos,
        block: BlockId,
    ) -> Result<Option<BlockEdit>, VoxelError> {
        let (pos, local) = position.split();
        let resident = self
            .chunks
            .get_mut(&pos)
            .ok_or(VoxelError::MissingChunk(pos))?;
        let Some(previous) = resident.chunk.set(local, block)? else {
            return Ok(None);
        };
        Ok(Some(BlockEdit {
            position,
            previous,
            current: block,
            stamp: ChunkStamp {
                generation: resident.generation,
                revision: resident.chunk.revision(),
            },
            affected_chunks: DirtyChunks::for_edit(pos, local),
        }))
    }
    /// Acknowledges a successfully persisted snapshot only if installation and
    /// content both still match. Removed/replaced chunks and newer edits reject it.
    pub fn mark_saved(&mut self, pos: ChunkPos, stamp: ChunkStamp) -> bool {
        let Some(resident) = self.chunks.get_mut(&pos) else {
            return false;
        };
        resident.generation == stamp.generation && resident.chunk.mark_saved(stamp.revision)
    }
}

#[cfg(test)]
mod tests;
