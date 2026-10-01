//! Bounded CPU streaming. The game keeps ownership of the world and save policy.
use crate::{
    BlockRegistry, Chunk, ChunkMesh, ChunkPos, ChunkStamp, MeshDependencies, MeshInput,
    MeshingError, MeshingOptions, VoxelError, VoxelWorld,
};
use rayengine_core::jobs::{Cancellation, JobHandle, JobOutcome, JobPool, JobPoolError};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

/// Bounds for a cubic X/Z focus region with independent vertical extent.
#[derive(Clone, Copy, Debug)]
pub struct StreamConfig {
    /// Horizontal radius in chunks; `(2*r+1)^2 * (2*y+1)` must fit max_resident.
    pub radius: u32,
    /// Vertical radius in chunks.
    pub vertical_radius: u32,
    /// Upper bound on the supplied world's capacity, including dirty pinned chunks.
    pub max_resident: usize,
    /// Fixed worker count; generation must cooperate with cancellation.
    pub workers: usize,
    /// Queued, running and unconsumed CPU jobs combined.
    pub max_jobs: usize,
    /// Mesh jobs, ready results and externally held results combined.
    pub max_meshes: usize,
    /// Retained vertex/index byte allowance per mesh; meshing limits must fit it.
    /// Metadata and temporary quad/snapshot buffers are bounded separately by job count.
    pub mesh_bytes: usize,
    /// Immutable geometry policy, including missing-neighbor behavior.
    pub meshing: MeshingOptions,
}
impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            radius: 2,
            vertical_radius: 0,
            max_resident: 32,
            workers: 2,
            max_jobs: 4,
            max_meshes: 4,
            mesh_bytes: 4 * 1024 * 1024,
            meshing: MeshingOptions::default(),
        }
    }
}
/// Persistence decision for an out-of-range dirty chunk. Clean chunks evict directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eviction {
    /// Retain data and apply backpressure. Also use for an in-progress/failed save.
    Keep,
    /// The exact supplied stamp has been durably saved (or is reproducible terrain
    /// explicitly exempted by the game's policy). Acknowledges it before removal.
    Saved,
}
/// Invalid settings, incompatible world, or worker creation failure.
#[derive(Debug)]
pub enum StreamError {
    /// Invalid region, geometry limits, or nonpositive/overflowing bounds.
    Config,
    /// Invalid focus coordinate.
    Position(VoxelError),
    /// World capacity exceeds the bound or a streamer was reused with another world.
    World,
    /// CPU worker creation failed.
    Jobs(JobPoolError),
    /// The bounded focus buffer could not be allocated.
    Allocation,
}
impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config => f.write_str("invalid chunk streaming limits"),
            Self::Position(e) => e.fmt(f),
            Self::World => f.write_str("incompatible streaming world"),
            Self::Jobs(e) => e.fmt(f),
            Self::Allocation => f.write_str("chunk focus allocation failed"),
        }
    }
}
impl std::error::Error for StreamError {}
/// A failed request is paused until its dependencies change or retry is called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamFailure {
    /// Game loader or storage admission rejected data.
    Load(VoxelError),
    /// Mesh generation or retained-byte admission failed.
    Mesh(MeshingError),
    /// The worker panicked; other jobs remain operational.
    Panicked,
    /// The render consumer rejected an otherwise completed result.
    Upload,
}
/// Per-tick counters plus current bounded queue sizes. Timing includes callbacks.
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamReport {
    /// Wanted chunks in nearest-first order.
    pub desired: usize,
    /// Resident world chunks, including pinned dirty chunks.
    pub resident: usize,
    /// Queued/running/unconsumed jobs, including cancelled work not yet drained.
    pub jobs: usize,
    /// Completed meshes waiting to be taken.
    pub ready: usize,
    /// Mesh slots used by jobs, ready results and render consumers together.
    pub mesh_slots: usize,
    /// Actual retained ready vertex/index capacities (worker/consumer bytes excluded).
    pub ready_bytes: usize,
    /// Chunks admitted this tick.
    pub loaded: usize,
    /// Chunks removed after clean/saved admission.
    pub evicted: usize,
    /// Out-of-range dirty chunks whose save was deferred.
    pub pinned: usize,
    /// Cancelled/stale completions or ready meshes discarded this tick.
    pub discarded: usize,
    /// New paused failures this tick.
    pub failed: usize,
    /// Submitted CPU requests this tick.
    pub submitted: usize,
    /// Tick wall time, including save callback work.
    pub elapsed: Duration,
}
type Loader = dyn Fn(ChunkPos, Arc<BlockRegistry>, Cancellation) -> Result<Chunk, VoxelError>
    + Send
    + Sync
    + 'static;
// The bounded pool holds only max_jobs entries; keeping results inline avoids
// a separate allocation for every completed mesh.
#[allow(clippy::large_enum_variant)]
enum Output {
    Load(Result<Chunk, VoxelError>),
    Mesh(Result<ChunkMesh, MeshingError>),
}
struct Active {
    position: ChunkPos,
    handle: JobHandle,
    dependencies: Option<MeshDependencies>,
}
struct Failure {
    reason: StreamFailure,
    dependencies: Option<MeshDependencies>,
}

/// CPU scheduler using the core bounded job pool. No native resources or implicit saves.
///
/// The world stays game-owned, including after shutdown/Drop. A loader must bound its
/// own scratch/I/O and check Cancellation during lengthy work. Loaded chunks retain
/// their dirty status: explicitly mark reproducible fresh terrain saved if desired.
/// Mesh inputs are 11,664 cell bytes each; output is capped by mesh_bytes and geometry
/// limits; temporary quads are bounded by max_quads. Metadata/allocator overhead is
/// additional. Cancelled jobs occupy slots until their completions are drained.
pub struct ChunkStreamer {
    config: StreamConfig,
    loader: Arc<Loader>,
    pool: JobPool<Output>,
    identity: Option<Arc<()>>,
    desired: Vec<ChunkPos>,
    focus: Option<ChunkPos>,
    evictions: Vec<ChunkPos>,
    active: Vec<Active>,
    ready: VecDeque<ChunkMesh>,
    held: HashMap<ChunkPos, MeshDependencies>,
    installed: HashMap<ChunkPos, MeshDependencies>,
    failures: HashMap<ChunkPos, Failure>,
    stopped: bool,
}
impl ChunkStreamer {
    /// Starts fixed workers. No world loads occur until tick is called.
    pub fn new(
        config: StreamConfig,
        loader: impl Fn(ChunkPos, Arc<BlockRegistry>, Cancellation) -> Result<Chunk, VoxelError>
        + Send
        + Sync
        + 'static,
    ) -> Result<Self, StreamError> {
        let width = usize::try_from(config.radius)
            .ok()
            .and_then(|r| r.checked_mul(2))
            .and_then(|r| r.checked_add(1))
            .ok_or(StreamError::Config)?;
        let height = usize::try_from(config.vertical_radius)
            .ok()
            .and_then(|r| r.checked_mul(2))
            .and_then(|r| r.checked_add(1))
            .ok_or(StreamError::Config)?;
        let count = width
            .checked_mul(width)
            .and_then(|v| v.checked_mul(height))
            .ok_or(StreamError::Config)?;
        let limits = config.meshing.limits;
        if count > config.max_resident
            || config.workers == 0
            || config.max_jobs == 0
            || config.max_meshes == 0
            || config.radius > i32::MAX as u32
            || config.vertical_radius > i32::MAX as u32
            || !(1..=crate::meshing::MAX_CHUNK_FACES).contains(&limits.max_quads)
            || !(1..=crate::meshing::MAX_CHUNK_FACES).contains(&limits.max_batches)
            || !(4..=65532).contains(&limits.max_vertices_per_batch)
            || !limits.max_vertices_per_batch.is_multiple_of(4)
            || config.mesh_bytes < limits.max_quads * 156
            || config.mesh_bytes.checked_mul(config.max_meshes).is_none()
        {
            return Err(StreamError::Config);
        }
        let mut desired = Vec::new();
        desired
            .try_reserve_exact(count)
            .map_err(|_| StreamError::Allocation)?;
        let pool = JobPool::new(config.workers, config.max_jobs).map_err(StreamError::Jobs)?;
        Ok(Self {
            config,
            loader: Arc::new(loader),
            pool,
            identity: None,
            desired,
            focus: None,
            evictions: Vec::new(),
            active: Vec::new(),
            ready: VecDeque::new(),
            held: HashMap::new(),
            installed: HashMap::new(),
            failures: HashMap::new(),
            stopped: false,
        })
    }
    /// Current configured bounds.
    pub fn config(&self) -> StreamConfig {
        self.config
    }
    /// Current region, nearest first, deterministic ties ordered X/Y/Z.
    pub fn desired(&self) -> &[ChunkPos] {
        &self.desired
    }
    /// Whether a position belongs to the current focus region.
    pub fn wants(&self, position: ChunkPos) -> bool {
        !self.stopped && self.desired.contains(&position)
    }
    /// Paused failure for this position. The game can inspect before retrying.
    pub fn failure(&self, position: ChunkPos) -> Option<&StreamFailure> {
        self.failures.get(&position).map(|f| &f.reason)
    }
    /// Explicit retry after external repair, such as adding a missing material.
    pub fn retry(&mut self, position: ChunkPos) {
        self.failures.remove(&position);
    }
    fn mesh_slots(&self) -> usize {
        self.active
            .iter()
            .filter(|a| a.dependencies.is_some())
            .count()
            + self.ready.len()
            + self.held.len()
    }
    /// Moves one ready result to a consumer. Its mesh slot stays reserved until
    /// finish_mesh; the consumer must eventually acknowledge success or rejection.
    /// Rechecks receipts and chooses the nearest ready chunk at the current focus.
    pub fn take_mesh(&mut self, world: &VoxelWorld) -> Option<ChunkMesh> {
        self.ready.retain(|m| {
            self.desired.contains(&m.dependencies().position())
                && m.dependencies().is_current(world)
        });
        let index = self
            .ready
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| {
                self.desired
                    .iter()
                    .position(|p| *p == m.dependencies().position())
                    .unwrap()
            })
            .map(|(i, _)| i)?;
        let mesh = self.ready.remove(index)?;
        self.held
            .insert(mesh.dependencies().position(), mesh.dependencies().clone());
        Some(mesh)
    }
    /// Releases a consumer's reservation. Success records the installed receipt;
    /// failure pauses this revision, so persistent upload errors do not spin.
    /// Old acknowledgements cannot overwrite a newer held request.
    pub fn finish_mesh(&mut self, dependencies: &MeshDependencies, installed: bool) -> bool {
        let position = dependencies.position();
        if !self
            .held
            .get(&position)
            .is_some_and(|d| d.same(dependencies))
        {
            return false;
        }
        self.held.remove(&position);
        if !self.wants(position) {
            return true;
        }
        if installed {
            self.installed.insert(position, dependencies.clone());
        } else {
            self.failures.insert(
                position,
                Failure {
                    reason: StreamFailure::Upload,
                    dependencies: Some(dependencies.clone()),
                },
            );
        }
        true
    }
    /// Invalidates an installed receipt, e.g. after explicit renderer teardown.
    pub fn forget_mesh(&mut self, position: ChunkPos) {
        self.installed.remove(&position);
    }
    /// Updates focus, requests saves/evictions, drains results, then submits nearest
    /// missing loads or stale meshes. Edits through VoxelWorld are detected without
    /// notifications; all six neighbors participate, including missing neighbors.
    /// Save callback runs only for out-of-range dirty chunks, on the caller thread.
    pub fn tick(
        &mut self,
        world: &mut VoxelWorld,
        focus: ChunkPos,
        mut eviction: impl FnMut(ChunkPos, &Chunk, ChunkStamp) -> Eviction,
    ) -> Result<StreamReport, StreamError> {
        let start = Instant::now();
        focus.origin().map_err(StreamError::Position)?;
        if world.capacity() > self.config.max_resident
            || self
                .identity
                .as_ref()
                .is_some_and(|id| !Arc::ptr_eq(id, &world.identity))
        {
            return Err(StreamError::World);
        }
        self.identity.get_or_insert_with(|| world.identity.clone());
        if self.focus != Some(focus) {
            self.desired.clear();
            let r = i64::from(self.config.radius);
            let v = i64::from(self.config.vertical_radius);
            for y in -v..=v {
                for z in -r..=r {
                    for x in -r..=r {
                        let (Ok(x), Ok(y), Ok(z)) = (
                            i32::try_from(i64::from(focus.x) + x),
                            i32::try_from(i64::from(focus.y) + y),
                            i32::try_from(i64::from(focus.z) + z),
                        ) else {
                            continue;
                        };
                        let position = ChunkPos::new(x, y, z);
                        if position.origin().is_ok() {
                            self.desired.push(position);
                        }
                    }
                }
            }
            self.desired.sort_unstable_by_key(|p| {
                let dx = i64::from(p.x) - i64::from(focus.x);
                let dy = i64::from(p.y) - i64::from(focus.y);
                let dz = i64::from(p.z) - i64::from(focus.z);
                (dx * dx + dy * dy + dz * dz, p.x, p.y, p.z)
            });
            self.focus = Some(focus);
        }
        let mut report = StreamReport::default();
        self.evictions.clear();
        self.evictions.extend(
            world
                .chunks()
                .map(|(p, _)| p)
                .filter(|p| !self.desired.contains(p)),
        );
        for &p in &self.evictions {
            let (chunk, stamp) = world.chunk_with_stamp(p).unwrap();
            if chunk.is_dirty() {
                if eviction(p, chunk, stamp) == Eviction::Keep {
                    report.pinned += 1;
                    continue;
                }
                world.mark_saved(p, stamp);
            }
            world.remove_chunk(p);
            report.evicted += 1;
        }
        self.installed
            .retain(|p, _| self.desired.contains(p) && world.chunk(*p).is_some());
        self.failures.retain(|p, f| {
            self.desired.contains(p)
                && match &f.dependencies {
                    Some(d) => d.is_current(world),
                    None => world.chunk(*p).is_none(),
                }
        });
        let before = self.ready.len();
        self.ready.retain(|m| {
            self.desired.contains(&m.dependencies().position())
                && m.dependencies().is_current(world)
        });
        report.discarded += before - self.ready.len();
        for active in &self.active {
            if !self.desired.contains(&active.position)
                || match &active.dependencies {
                    Some(d) => !d.is_current(world),
                    None => world.chunk(active.position).is_some(),
                }
            {
                active.handle.cancel();
            }
        }
        while let Some(completion) = self.pool.try_recv() {
            let index = self
                .active
                .iter()
                .position(|a| a.handle.id() == completion.id)
                .unwrap();
            let active = self.active.swap_remove(index);
            let p = active.position;
            let failure = match completion.outcome {
                JobOutcome::Cancelled => {
                    report.discarded += 1;
                    continue;
                }
                JobOutcome::Panicked => Some(StreamFailure::Panicked),
                JobOutcome::Ready(Output::Load(result)) => match result {
                    Ok(chunk) => {
                        if !self.wants(p) || world.chunk(p).is_some() {
                            report.discarded += 1;
                            continue;
                        }
                        match world.insert_chunk(p, chunk) {
                            Ok(_) => {
                                report.loaded += 1;
                                None
                            }
                            Err(e) => Some(StreamFailure::Load(e.error)),
                        }
                    }
                    Err(e) => Some(StreamFailure::Load(e)),
                },
                JobOutcome::Ready(Output::Mesh(result)) => {
                    if !self.wants(p) || !active.dependencies.as_ref().unwrap().is_current(world) {
                        report.discarded += 1;
                        continue;
                    }
                    match result {
                        Ok(mesh) if mesh.retained_bytes() <= self.config.mesh_bytes => {
                            self.ready.push_back(mesh);
                            None
                        }
                        Ok(_) => Some(StreamFailure::Mesh(MeshingError::LimitExceeded)),
                        Err(e) => Some(StreamFailure::Mesh(e)),
                    }
                }
            };
            if let Some(reason) = failure {
                self.failures.insert(
                    p,
                    Failure {
                        reason,
                        dependencies: active.dependencies,
                    },
                );
                report.failed += 1;
            }
        }
        if !self.stopped {
            for &p in &self.desired {
                if self.active.len() == self.config.max_jobs {
                    break;
                }
                if self.failures.contains_key(&p)
                    || self.active.iter().any(|a| a.position == p)
                    || self.held.contains_key(&p)
                    || self.ready.iter().any(|m| m.dependencies().position() == p)
                {
                    continue;
                }
                if world.chunk(p).is_none() {
                    let loads = self
                        .active
                        .iter()
                        .filter(|a| a.dependencies.is_none())
                        .count();
                    if world.len() + loads >= world.capacity() {
                        continue;
                    }
                    let loader = self.loader.clone();
                    let registry = world.shared_registry();
                    if let Ok(handle) = self
                        .pool
                        .try_submit(move |cancel| Output::Load(loader(p, registry, cancel)))
                    {
                        self.active.push(Active {
                            position: p,
                            handle,
                            dependencies: None,
                        });
                        report.submitted += 1;
                    }
                } else if self.mesh_slots() < self.config.max_meshes
                    && !self.installed.get(&p).is_some_and(|d| d.is_current(world))
                {
                    match MeshInput::capture(world, p) {
                        Ok(input) => {
                            let dependencies = Some(input.dependencies().clone());
                            let options = self.config.meshing;
                            if let Ok(handle) = self.pool.try_submit(move |cancel| {
                                Output::Mesh(
                                    input.build_cancellable(options, || cancel.is_cancelled()),
                                )
                            }) {
                                self.active.push(Active {
                                    position: p,
                                    handle,
                                    dependencies,
                                });
                                report.submitted += 1;
                            }
                        }
                        Err(e) => {
                            // Capture errors are transient admission errors, reported for this tick.
                            let _ = e;
                            report.failed += 1;
                        }
                    }
                }
            }
        }
        report.desired = self.desired.len();
        report.resident = world.len();
        report.jobs = self.active.len();
        report.ready = self.ready.len();
        report.mesh_slots = self.mesh_slots();
        report.ready_bytes = self.ready.iter().map(ChunkMesh::retained_bytes).sum();
        report.elapsed = start.elapsed();
        Ok(report)
    }
    /// Cancels and joins CPU work, drops ready results/reservations, and disables
    /// submissions. Idempotent. World data is untouched; unload renderer separately.
    pub fn shutdown(&mut self) {
        self.pool.shutdown();
        self.active.clear();
        self.ready.clear();
        self.held.clear();
        self.installed.clear();
        self.failures.clear();
        self.stopped = true;
    }
}

#[cfg(test)]
mod tests;
