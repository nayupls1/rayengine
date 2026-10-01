//! Render-thread upload staging and whole-chunk commit for the CPU streamer.
use super::*;
use crate::ChunkStreamer;
use rayengine::upload::{
    MeshUpload, MeshUploadOutcome, MeshUploadQueue, MeshUploadTarget, UploadBudget, UploadReport,
};

/// Hard limits on this renderer's chunks, live GPU payload, and CPU staging.
/// Native allocator/driver overhead and unrelated game assets are excluded.
#[derive(Clone, Copy, Debug)]
pub struct StreamRenderConfig {
    /// Installed chunks (including valid empty chunks).
    pub max_chunks: usize,
    /// Live meshes including both old and partially uploaded replacement batches.
    pub max_meshes: usize,
    /// Logical GPU vertex/index bytes, including temporary replacements.
    pub max_buffer_bytes: usize,
    /// Maximum staged requests in a single whole-chunk transaction.
    pub max_staging_requests: usize,
    /// Retained vertex/index capacities in the SDK upload queue.
    pub max_staging_bytes: usize,
}
impl Default for StreamRenderConfig {
    fn default() -> Self {
        Self {
            max_chunks: 32,
            max_meshes: 1024,
            max_buffer_bytes: 64 * 1024 * 1024,
            max_staging_requests: 256,
            max_staging_bytes: 4 * 1024 * 1024,
        }
    }
}
struct Pending {
    dependencies: MeshDependencies,
    stats: MeshStats,
    queue: MeshUploadQueue<usize>,
    materials: Vec<(MaterialId, usize)>,
    uploaded_bytes: usize,
    uploaded: Vec<GpuBatch>,
}
/// Live resource counters for the streamer, including partial replacements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamResources {
    /// Installed chunks.
    pub chunks: usize,
    /// Installed plus partial GPU mesh handles.
    pub meshes: usize,
    /// Installed plus partial logical GPU buffer bytes.
    pub buffer_bytes: usize,
    /// Staged CPU requests not yet uploaded.
    pub staged_requests: usize,
    /// Retained CPU staging capacities.
    pub staged_bytes: usize,
}
impl StreamResources {
    fn maximize(&mut self, other: Self) {
        self.chunks = self.chunks.max(other.chunks);
        self.meshes = self.meshes.max(other.meshes);
        self.buffer_bytes = self.buffer_bytes.max(other.buffer_bytes);
        self.staged_requests = self.staged_requests.max(other.staged_requests);
        self.staged_bytes = self.staged_bytes.max(other.staged_bytes);
    }
}
/// One pump result. Rejections pause the CPU receipt until retry/dependency change.
#[derive(Debug, Default)]
pub struct StreamRenderReport {
    /// SDK request/byte/time budget accounting for this call.
    pub uploads: UploadReport,
    /// Whole chunks committed after all batches succeeded.
    pub committed: usize,
    /// Installed chunks unloaded after leaving focus/removal.
    pub unloaded: usize,
    /// Stale partial transactions rolled back.
    pub discarded: usize,
    /// Admission, material, or native upload failure; old geometry is preserved.
    pub error: Option<Error>,
    /// Maximum ownership/staging counts observed inside this call, including
    /// the moment before old replacement buffers are released.
    pub peak_resources: StreamResources,
    /// Current resources (including partial uploads).
    pub resources: StreamResources,
}
/// Single bounded whole-chunk staging transaction with atomic replacement.
///
/// Pump before drawing, after ChunkStreamer::tick. At most one CPU result is taken
/// per call. Large batches stay queued when they exceed the frame byte budget;
/// split them with MeshLimits::max_vertices_per_batch or increase that budget.
/// A whole result that cannot fit staging or old+new GPU bounds is rejected and
/// paused until ChunkStreamer::retry. Old geometry remains drawable during work.
/// Drop leaves native resources in Assets until run end; call unload for early teardown.
/// Do not externally unload the renderer's mesh handles. Materials are borrowed.
pub struct StreamRenderer {
    config: StreamRenderConfig,
    chunks: HashMap<ChunkPos, RenderedChunk>,
    pending: Option<Pending>,
}
impl StreamRenderer {
    /// Creates an empty renderer; zero bounds are rejected.
    pub fn new(config: StreamRenderConfig) -> Result<Self, Error> {
        if config.max_chunks == 0
            || config.max_meshes == 0
            || config.max_buffer_bytes == 0
            || config.max_staging_requests == 0
            || config.max_staging_bytes == 0
        {
            return Err(Error::Config(
                "voxel render streaming limits must be positive".into(),
            ));
        }
        Ok(Self {
            config,
            chunks: HashMap::new(),
            pending: None,
        })
    }
    /// Installed chunk by position, available for drawing/culling.
    pub fn chunk(&self, position: ChunkPos) -> Option<&RenderedChunk> {
        self.chunks.get(&position)
    }
    /// Installed chunks, in unspecified order.
    pub fn chunks(&self) -> impl Iterator<Item = &RenderedChunk> {
        self.chunks.values()
    }
    /// Live ownership and staging counters. GPU bytes count actual successful uploads.
    pub fn resources(&self) -> StreamResources {
        let mut r = StreamResources {
            chunks: self.chunks.len(),
            ..Default::default()
        };
        for c in self.chunks.values() {
            r.meshes += c.batches.len();
            r.buffer_bytes += c.stats.buffer_bytes;
        }
        if let Some(p) = &self.pending {
            r.meshes += p.uploaded.len();
            // All emitted attributes are present, so GPU and logical payload agree.
            r.buffer_bytes += p.uploaded_bytes;
            r.staged_requests = p.queue.len();
            r.staged_bytes = p.queue.pending_bytes();
        }
        r
    }
    fn stage(
        &mut self,
        mesh: ChunkMesh,
        materials: &VoxelMaterials,
        assets: &Assets<'_>,
    ) -> Result<(), Error> {
        let stats = mesh.stats();
        let p = mesh.dependencies().position();
        let used = self.resources();
        if (!self.chunks.contains_key(&p) && used.chunks >= self.config.max_chunks)
            || stats.batches > self.config.max_meshes.saturating_sub(used.meshes)
            || stats.buffer_bytes
                > self
                    .config
                    .max_buffer_bytes
                    .saturating_sub(used.buffer_bytes)
            || stats.batches > self.config.max_staging_requests
            || mesh.retained_bytes() > self.config.max_staging_bytes
        {
            return Err(Error::Asset(
                "voxel transaction exceeds staging or old+new GPU limits".into(),
            ));
        }
        self.chunks.try_reserve(1).map_err(|_| allocation())?;
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(stats.batches)
            .map_err(|_| allocation())?;
        for batch in mesh.batches() {
            let d = batch.data();
            let bytes = d.positions.len() * 36 + d.indices.as_ref().unwrap().len() * 2;
            bindings.push((validate_surface(batch.surface(), materials, assets)?, bytes));
        }
        let dependencies = mesh.dependencies().clone();
        let mut queue = MeshUploadQueue::new(
            self.config.max_staging_requests,
            self.config.max_staging_bytes,
        )?;
        let mut uploaded = Vec::new();
        uploaded
            .try_reserve_exact(stats.batches)
            .map_err(|_| allocation())?;
        for (index, batch) in mesh.into_batches().into_iter().enumerate() {
            queue
                .try_push(MeshUpload {
                    tag: index,
                    revision: 0,
                    target: MeshUploadTarget::Create,
                    data: batch.into_data(),
                })
                .map_err(|e| Error::Asset(e.reason.to_string()))?;
        }
        self.pending = Some(Pending {
            dependencies,
            stats,
            queue,
            materials: bindings,
            uploaded,
            uploaded_bytes: 0,
        });
        Ok(())
    }
    fn rollback(&mut self, streamer: &mut ChunkStreamer, assets: &mut Assets<'_>) {
        if let Some(p) = self.pending.take() {
            for b in p.uploaded {
                assets.unload_mesh(b.mesh);
            }
            streamer.finish_mesh(&p.dependencies, false);
        }
    }
    /// Unloads out-of-range chunks, rejects stale work, stages a nearby result,
    /// and uses the SDK queue to apply the supplied per-frame budget. A failed or
    /// stale batch rolls back every partial new upload. Empty results clear old geometry.
    pub fn pump(
        &mut self,
        world: &VoxelWorld,
        streamer: &mut ChunkStreamer,
        materials: &VoxelMaterials,
        frame: &mut Frame<'_, '_>,
        budget: UploadBudget,
    ) -> StreamRenderReport {
        let mut report = StreamRenderReport {
            peak_resources: self.resources(),
            ..Default::default()
        };
        self.chunks.retain(|p, chunk| {
            if streamer.wants(*p) && world.chunk(*p).is_some() {
                true
            } else {
                chunk.unload(frame.assets);
                streamer.forget_mesh(*p);
                report.unloaded += 1;
                false
            }
        });
        if self.pending.as_ref().is_some_and(|p| {
            !streamer.wants(p.dependencies.position()) || !p.dependencies.is_current(world)
        }) {
            self.rollback(streamer, frame.assets);
            report.discarded += 1;
        }
        if self.pending.is_none()
            && report.discarded == 0
            && let Some(mesh) = streamer.take_mesh(world)
        {
            let dependencies = mesh.dependencies().clone();
            if let Err(e) = self.stage(mesh, materials, frame.assets) {
                streamer.finish_mesh(&dependencies, false);
                report.error = Some(e);
            }
        }
        report.peak_resources.maximize(self.resources());
        if let Some(pending) = &mut self.pending {
            let current = pending.dependencies.is_current(world)
                && streamer.wants(pending.dependencies.position());
            report.uploads = frame.upload_meshes(
                &mut pending.queue,
                budget,
                |_, _| current,
                |result| match result.outcome {
                    MeshUploadOutcome::Uploaded(mesh) => {
                        pending.uploaded_bytes += pending.materials[result.tag].1;
                        pending.uploaded.push(GpuBatch {
                            mesh,
                            material: pending.materials[result.tag].0,
                        });
                    }
                    MeshUploadOutcome::Failed(e) => report.error = Some(e),
                    MeshUploadOutcome::Stale => report.discarded += 1,
                },
            );
            if report.error.is_none() && report.discarded == 0 && pending.queue.is_empty() {
                // Revalidate borrowed material dependencies after multi-frame staging.
                for &(id, _) in &pending.materials {
                    let validation = frame
                        .assets
                        .material(id)
                        .ok_or_else(|| {
                            Error::Asset("voxel material unloaded during staging".into())
                        })
                        .and_then(|desc| frame.assets.validate_material(desc));
                    if let Err(e) = validation {
                        report.error = Some(e);
                        break;
                    }
                }
            }
            let complete = pending.queue.is_empty();
            report.peak_resources.maximize(self.resources());
            if report.error.is_some() || report.discarded > 0 {
                self.rollback(streamer, frame.assets);
            } else if complete {
                let p = self.pending.take().unwrap();
                let position = p.dependencies.position();
                let chunk = self
                    .chunks
                    .entry(position)
                    .or_insert_with(|| RenderedChunk::new(position).unwrap());
                for old in std::mem::replace(&mut chunk.batches, p.uploaded) {
                    frame.assets.unload_mesh(old.mesh);
                }
                chunk.stats = p.stats;
                chunk.dependencies = Some(p.dependencies.clone());
                streamer.finish_mesh(&p.dependencies, true);
                report.committed = 1;
            }
        }
        report.resources = self.resources();
        report.peak_resources.maximize(report.resources);
        report
    }
    /// Releases partial/installed GPU geometry and consumer reservations. World
    /// data, materials and textures stay owned by the game. Idempotent.
    pub fn unload(&mut self, streamer: &mut ChunkStreamer, assets: &mut Assets<'_>) {
        let pending = self.pending.as_ref().map(|p| p.dependencies.position());
        self.rollback(streamer, assets);
        if let Some(position) = pending {
            streamer.retry(position);
        }
        for (p, mut chunk) in self.chunks.drain() {
            chunk.unload(assets);
            streamer.forget_mesh(p);
        }
    }
}
fn validate_surface(
    key: SurfaceKey,
    materials: &VoxelMaterials,
    assets: &Assets<'_>,
) -> Result<MaterialId, Error> {
    let id = materials
        .surface(key)
        .ok_or_else(|| Error::Asset(format!("undefined voxel surface {key:?}")))?;
    let desc = assets
        .material(id)
        .ok_or_else(|| Error::Asset("voxel material is unloaded".into()))?;
    if !matches!(
        (key.layer, desc.alpha),
        (MeshLayer::Opaque, AlphaMode::Opaque) | (MeshLayer::Cutout, AlphaMode::Cutout(_))
    ) {
        return Err(Error::Asset(
            "voxel material alpha policy does not match mesh layer".into(),
        ));
    }
    assets.validate_material(desc)?;
    Ok(id)
}

#[cfg(test)]
mod tests;
