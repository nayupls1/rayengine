use super::*;
use crate::{
    gameplay::Player,
    survival::Survival,
    terrain::{Terrain, TerrainSettings},
};
use rayengine_core::jobs::{JobHandle, JobOutcome, JobPool};
use std::sync::RwLock;
/// Current disk-saving state. Failed writes require an explicit retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    /// New session; no checkpoint is installed yet.
    Idle,
    /// A request awaits the single available worker slot.
    Requested,
    /// One owned snapshot is being encoded/written.
    Writing,
    /// A complete checkpoint is installed; gameplay may have advanced since it.
    Saved,
    /// Write/admission failed; dirty chunks remain pinned.
    Failed,
}
/// Per-interaction admission keeps bounded history saveable before world mutation.
pub struct EditAdmission {
    full: bool,
    edits: Arc<schema::Edits>,
    dirty: Vec<[i32; 3]>,
}
impl EditAdmission {
    /// Existing edited chunks remain editable when historical capacity is full.
    pub fn allows(&self, position: BlockPos) -> bool {
        let p = position.split().0;
        let key = [p.x, p.y, p.z];
        !self.full || self.edits.contains_key(&key) || self.dirty.contains(&key)
    }
    /// Further new chunk histories are blocked; existing ones still accept edits.
    pub fn full(&self) -> bool {
        self.full
    }
}
/// Cheap immutable-history reader cloned by chunk generation jobs.
#[derive(Clone)]
pub struct SavedTerrain {
    checkpoint: Arc<RwLock<Snapshot>>,
}
impl SavedTerrain {
    /// Apply a committed edited chunk, otherwise run the original cancellable
    /// recipe. Reads retain their chosen checkpoint without holding its lock.
    pub fn load(
        &self,
        position: ChunkPos,
        registry: Arc<BlockRegistry>,
        cancel: rayengine_core::jobs::Cancellation,
    ) -> Result<Chunk, VoxelError> {
        let checkpoint = self
            .checkpoint
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if !Arc::ptr_eq(&registry, &checkpoint.terrain.registry()) {
            return Err(VoxelError::RegistryMismatch);
        }
        if let Some(cells) = checkpoint.edits.get(&[position.x, position.y, position.z]) {
            let mut chunk = Chunk::from_blocks(
                registry,
                cells.0.iter().map(|&id| BlockId::from_raw(id)).collect(),
            )?;
            chunk.mark_saved(chunk.revision());
            Ok(chunk)
        } else {
            generate_chunk(
                checkpoint.terrain.as_ref(),
                position,
                &GenerationContext::new(registry, &|| cancel.is_cancelled()),
            )
        }
    }
}
struct Written {
    snapshot: Snapshot,
    result: Result<(), Error>,
}
struct Pending {
    handle: JobHandle,
    stamps: Vec<(ChunkPos, ChunkStamp)>,
}
type Writer = dyn Fn(&Store, &Snapshot) -> Result<(), Error> + Send + Sync;
/// One worker and one outstanding snapshot/result. Requests while busy coalesce
/// into a flag; the latest live state is captured only once room is available.
/// Production callers keep one instance bound to its original VoxelWorld.
pub struct Saving {
    store: Store,
    checkpoint: Snapshot,
    loader: SavedTerrain,
    pool: JobPool<Written>,
    writer: Arc<Writer>,
    pending: Option<Pending>,
    requested: bool,
    has_file: bool,
    error: Option<Error>,
}
impl Saving {
    /// Lock/load an existing world, or create a fresh seeded checkpoint only for
    /// a genuinely missing slot. An explicit conflicting seed is an error.
    pub fn open(store: Store, seed: Option<u64>) -> Result<Self, Error> {
        let loaded = store.load()?;
        let has_file = loaded.is_some();
        let checkpoint = if let Some(checkpoint) = loaded {
            if seed.is_some_and(|seed| seed != checkpoint.terrain.info().seed) {
                return Err(Error::Invalid(
                    "requested seed differs from saved world; choose a different save path".into(),
                ));
            }
            checkpoint
        } else {
            let terrain = Arc::new(
                Terrain::new(seed.unwrap_or(42), TerrainSettings::default())
                    .map_err(|e| Error::Invalid(e.to_string()))?,
            );
            let spawn = terrain
                .find_spawn(0, 0, 16, 1089)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            Snapshot::new(
                terrain,
                spawn.support,
                &Player::new(spawn.feet()).map_err(|e| Error::Invalid(e.to_string()))?,
                &Survival::default(),
            )?
        };
        Self::with_writer(store, checkpoint, has_file, Arc::new(Store::write))
    }
    pub(super) fn with_writer(
        store: Store,
        checkpoint: Snapshot,
        has_file: bool,
        writer: Arc<Writer>,
    ) -> Result<Self, Error> {
        Ok(Self {
            store,
            loader: SavedTerrain {
                checkpoint: Arc::new(RwLock::new(checkpoint.clone())),
            },
            checkpoint,
            pool: JobPool::new(1, 1).map_err(Error::Jobs)?,
            writer,
            pending: None,
            requested: false,
            has_file,
            error: None,
        })
    }
    /// Last committed checkpoint, also supplying initial generator/player state.
    pub fn checkpoint(&self) -> &Snapshot {
        &self.checkpoint
    }
    /// Thread-safe chunk-loading adapter; updates precede clean-chunk eviction.
    pub fn loader(&self) -> SavedTerrain {
        self.loader.clone()
    }
    /// Current typed status for UI/tests.
    pub fn status(&self) -> SaveStatus {
        if self.error.is_some() {
            SaveStatus::Failed
        } else if self.pending.is_some() {
            SaveStatus::Writing
        } else if self.requested {
            SaveStatus::Requested
        } else if self.has_file {
            SaveStatus::Saved
        } else {
            SaveStatus::Idle
        }
    }
    /// Detailed failure, preserving container commit-state information.
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }
    /// Queue/coalesce an automatic request; a failure is held until explicit retry.
    pub fn request(&mut self) {
        if self.error.is_none() {
            self.requested = true;
        }
    }
    /// Explicit F5/retry clears the failure and queues the newest live state.
    pub fn retry(&mut self) {
        self.error = None;
        self.requested = true;
    }
    /// Whether all coalesced work has completed successfully (use before game quit).
    pub fn settled(&self) -> bool {
        self.pending.is_none() && !self.requested && self.error.is_none() && self.has_file
    }
    /// Whether modified history is full, without allocating a per-edit admission view.
    pub fn history_full(&self, world: &VoxelWorld) -> bool {
        let extra = world
            .chunks()
            .filter(|(p, c)| c.is_dirty() && !self.checkpoint.edits.contains_key(&[p.x, p.y, p.z]))
            .count();
        self.checkpoint.edits.len() + extra >= MAX_EDITED_CHUNKS
    }
    /// Build an admission view. With room, no dirty-position allocation is needed.
    pub fn admission(&self, world: &VoxelWorld) -> EditAdmission {
        let full = self.history_full(world);
        let dirty = if full {
            world
                .chunks()
                .filter(|(_, c)| c.is_dirty())
                .map(|(p, _)| [p.x, p.y, p.z])
                .collect()
        } else {
            Vec::new()
        };
        EditAdmission {
            full,
            edits: self.checkpoint.edits.clone(),
            dirty,
        }
    }
    /// Capture only when requested and idle. Encoding/checksumming/flush/rename
    /// execute on the worker, never in the streaming eviction callback.
    pub fn start(&mut self, world: &mut VoxelWorld, player: &Player, survival: &Survival) {
        if !self.requested || self.pending.is_some() || self.error.is_some() {
            return;
        }
        self.requested = false;
        let (snapshot, stamps) = match self.checkpoint.capture(world, player, survival) {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        if self.has_file && self.checkpoint.same_state(&snapshot) {
            self.install(snapshot, stamps, world);
            return;
        }
        let store = self.store.clone();
        let writer = self.writer.clone();
        match self.pool.try_submit(move |_| {
            let result = writer(&store, &snapshot);
            Written { snapshot, result }
        }) {
            Ok(handle) => self.pending = Some(Pending { handle, stamps }),
            Err(_) => self.error = Some(Error::Worker),
        }
    }
    /// Drain the single completion and acknowledge exact installation/revision
    /// stamps only after successful replacement. A newer edit/reinstallation stays dirty.
    pub fn poll(&mut self, world: &mut VoxelWorld) -> bool {
        let Some(completion) = self.pool.try_recv() else {
            return false;
        };
        let pending = self
            .pending
            .take()
            .expect("one completion per submitted snapshot");
        if completion.id != pending.handle.id() {
            self.error = Some(Error::Worker);
            self.requested = false;
            return false;
        }
        match completion.outcome {
            JobOutcome::Ready(Written {
                snapshot,
                result: Ok(()),
            }) => {
                self.install(snapshot, pending.stamps, world);
                true
            }
            JobOutcome::Ready(Written { result: Err(e), .. }) => {
                self.error = Some(e);
                self.requested = false;
                false
            }
            _ => {
                self.error = Some(Error::Worker);
                self.requested = false;
                false
            }
        }
    }
    fn install(
        &mut self,
        snapshot: Snapshot,
        stamps: Vec<(ChunkPos, ChunkStamp)>,
        world: &mut VoxelWorld,
    ) {
        // Updating loaders first guarantees a just-evicted saved chunk cannot
        // reload its original unedited terrain after successful acknowledgement.
        *self
            .loader
            .checkpoint
            .write()
            .unwrap_or_else(|e| e.into_inner()) = snapshot.clone();
        self.checkpoint = snapshot;
        self.has_file = true;
        if Arc::ptr_eq(
            &world.shared_registry(),
            &self.checkpoint.terrain.registry(),
        ) {
            for (position, stamp) in stamps {
                world.mark_saved(position, stamp);
            }
        }
    }
    /// Graceful native-close fallback: drain a running write, capture the latest
    /// state, and wait for that final checkpoint. OS fsync can block; work/result
    /// counts and payload memory remain bounded. Failures return to the CLI.
    pub fn finish(
        &mut self,
        world: &mut VoxelWorld,
        player: &Player,
        survival: &Survival,
    ) -> Result<(), Error> {
        self.wait(world);
        self.retry();
        self.start(world, player, survival);
        self.wait(world);
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        Ok(())
    }
    fn wait(&mut self, world: &mut VoxelWorld) {
        while self.pending.is_some() {
            self.poll(world);
            if self.pending.is_some() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}
