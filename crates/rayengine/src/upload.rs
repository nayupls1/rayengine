//! Bounded CPU mesh staging and explicit render-thread upload budgets.
//!
//! No uploads occur until [`crate::render::Frame::upload_meshes`] is called.
//! Counts/bytes are hard limits; wall time is checked between requests and
//! cannot preempt an individual raylib/driver call. FIFO requests too large
//! for the remaining byte budget stay queued. Use [`MeshUploadQueue::pop`] to
//! remove a permanently oversized request or adjust the next frame's budget.

use crate::{Error, assets::MeshId};
use rayengine_core::{
    glam::{Vec2, Vec3},
    mesh::{MeshData, MeshError},
};
use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

/// GPU operation performed for a current staged mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshUploadTarget {
    /// Create a new owned generated mesh.
    Create,
    /// Replace an existing mesh atomically, preserving it if upload fails.
    Replace(MeshId),
}

/// CPU request identified by a game-owned tag and revision.
#[derive(Debug)]
pub struct MeshUpload<K> {
    /// Game-owned identity, such as a chunk coordinate or entity ID.
    pub tag: K,
    /// Revision compared with current game state immediately before upload.
    pub revision: u64,
    /// Create or replace operation.
    pub target: MeshUploadTarget,
    /// Owned CPU geometry, optionally produced by a worker.
    pub data: MeshData,
}

/// Why the queue rejected a CPU mesh, without consuming its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshQueueError {
    /// The configured request count is full.
    Full,
    /// Retained vector capacities would exceed the CPU byte bound.
    MemoryLimit,
    /// Geometry validation failed before admission.
    InvalidMesh(MeshError),
    /// Byte accounting overflowed the platform's usize.
    TooLarge,
}

impl fmt::Display for MeshQueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => f.write_str("mesh upload queue is full"),
            Self::MemoryLimit => f.write_str("mesh upload queue CPU byte limit exceeded"),
            Self::InvalidMesh(error) => fmt::Display::fmt(error, f),
            Self::TooLarge => f.write_str("mesh byte count exceeds platform limits"),
        }
    }
}
impl std::error::Error for MeshQueueError {}

/// Rejected request. Recover it to retry, shrink capacities, or handle the error.
#[derive(Debug)]
pub struct MeshEnqueueError<K> {
    /// Original request, including its CPU mesh.
    pub request: MeshUpload<K>,
    /// Admission failure.
    pub reason: MeshQueueError,
}

/// Per-call upload limits. Zero requests/bytes/time can intentionally pause work.
#[derive(Clone, Copy, Debug)]
pub struct UploadBudget {
    /// Maximum processed requests, including stale discards and failed attempts.
    pub max_requests: usize,
    /// Maximum GPU buffer bytes attempted, including failed uploads and fallback UVs.
    pub max_bytes: usize,
    /// Wall-time threshold checked before each request; individual calls can overrun it.
    pub max_time: Duration,
}

impl Default for UploadBudget {
    fn default() -> Self {
        Self {
            max_requests: 4,
            max_bytes: 4 * 1024 * 1024,
            max_time: Duration::from_millis(2),
        }
    }
}

/// Outcome for a request removed from the upload queue.
#[derive(Debug)]
pub enum MeshUploadOutcome {
    /// Created/replaced resource. Replacement returns its original live handle.
    Uploaded(MeshId),
    /// GPU operation failed; existing replacement geometry remains intact.
    Failed(Error),
    /// The game's current-revision predicate rejected the request before GPU work.
    Stale,
}

/// Upload result carrying the original game identity and revision.
#[derive(Debug)]
pub struct MeshUploadResult<K> {
    /// Original request's tag.
    pub tag: K,
    /// Original request's revision.
    pub revision: u64,
    /// Upload success/failure or stale discard.
    pub outcome: MeshUploadOutcome,
}

/// Counters for a single queue processing call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadReport {
    /// Processed requests, including stale discards.
    pub processed: usize,
    /// GPU upload attempts, whether successful or failed.
    pub attempted: usize,
    /// Successfully uploaded meshes.
    pub uploaded: usize,
    /// Failed attempts.
    pub failed: usize,
    /// Stale requests discarded before GPU work.
    pub discarded: usize,
    /// GPU buffer bytes charged to attempts.
    pub bytes: usize,
    /// Processing time including callbacks.
    pub elapsed: Duration,
    /// Requests left after processing.
    pub remaining: usize,
    /// Next current request's full byte cost when it did not fit the remaining budget.
    pub blocked_upload_bytes: Option<usize>,
}

struct Queued<K> {
    request: MeshUpload<K>,
    cpu_bytes: usize,
    upload_bytes: usize,
}

/// Owned FIFO mesh staging, bounded by request count and retained CPU capacities.
///
/// The CPU byte count includes mesh vector capacities, not just lengths. Queue
/// metadata and allocator overhead are additional and bounded by request count.
/// Capacity does not bound results still held in a job pool; bound both stages.
pub struct MeshUploadQueue<K> {
    pending: VecDeque<Queued<K>>,
    max_requests: usize,
    max_pending_bytes: usize,
    pending_bytes: usize,
}

impl<K> MeshUploadQueue<K> {
    /// Creates a queue with positive request and retained CPU-byte bounds.
    pub fn new(max_requests: usize, max_pending_bytes: usize) -> Result<Self, Error> {
        if max_requests == 0 || max_pending_bytes == 0 {
            return Err(Error::Config("mesh queue limits must be positive".into()));
        }
        Ok(Self {
            pending: VecDeque::new(),
            max_requests,
            max_pending_bytes,
            pending_bytes: 0,
        })
    }

    /// Requests waiting for upload/discard.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether no requests remain.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Retained CPU mesh vector capacity bytes.
    pub fn pending_bytes(&self) -> usize {
        self.pending_bytes
    }

    /// Full GPU upload cost of the head request, including fallback UVs.
    pub fn front_upload_bytes(&self) -> Option<usize> {
        self.pending.front().map(|q| q.upload_bytes)
    }

    /// Validates and stages CPU data without GPU work. Errors return the request.
    // Backpressure is routine: retain the owned payload directly rather than
    // allocate a boxed error on every rejected/retried request.
    #[allow(clippy::result_large_err)]
    pub fn try_push(&mut self, request: MeshUpload<K>) -> Result<(), MeshEnqueueError<K>> {
        let admission = (|| {
            if self.pending.len() == self.max_requests {
                return Err(MeshQueueError::Full);
            }
            request
                .data
                .validate()
                .map_err(MeshQueueError::InvalidMesh)?;
            let (cpu_bytes, upload_bytes) = byte_counts(&request.data)?;
            if cpu_bytes > self.max_pending_bytes - self.pending_bytes {
                return Err(MeshQueueError::MemoryLimit);
            }
            Ok((cpu_bytes, upload_bytes))
        })();
        match admission {
            Ok((cpu_bytes, upload_bytes)) => {
                self.pending_bytes += cpu_bytes;
                self.pending.push_back(Queued {
                    request,
                    cpu_bytes,
                    upload_bytes,
                });
                Ok(())
            }
            Err(reason) => Err(MeshEnqueueError { request, reason }),
        }
    }

    /// Removes the head request without GPU work, releasing its CPU byte charge.
    pub fn pop(&mut self) -> Option<MeshUpload<K>> {
        let queued = self.pending.pop_front()?;
        self.pending_bytes -= queued.cpu_bytes;
        Some(queued.request)
    }

    pub(crate) fn process(
        &mut self,
        budget: UploadBudget,
        is_current: impl FnMut(&K, u64) -> bool,
        upload: impl FnMut(MeshUploadTarget, &MeshData) -> Result<MeshId, Error>,
        on_result: impl FnMut(MeshUploadResult<K>),
    ) -> UploadReport {
        let start = Instant::now();
        self.process_with_clock(budget, is_current, upload, on_result, || start.elapsed())
    }

    fn process_with_clock(
        &mut self,
        budget: UploadBudget,
        mut is_current: impl FnMut(&K, u64) -> bool,
        mut upload: impl FnMut(MeshUploadTarget, &MeshData) -> Result<MeshId, Error>,
        mut on_result: impl FnMut(MeshUploadResult<K>),
        mut elapsed: impl FnMut() -> Duration,
    ) -> UploadReport {
        let mut report = UploadReport::default();
        while report.processed < budget.max_requests && elapsed() < budget.max_time {
            let Some(front) = self.pending.front() else {
                break;
            };
            let current = is_current(&front.request.tag, front.request.revision);
            let bytes = front.upload_bytes;
            if current && bytes > budget.max_bytes - report.bytes {
                report.blocked_upload_bytes = Some(bytes);
                break;
            }
            let request = self.pop().expect("front is present");
            report.processed += 1;
            let outcome = if current {
                report.attempted += 1;
                report.bytes += bytes;
                match upload(request.target, &request.data) {
                    Ok(id) => {
                        report.uploaded += 1;
                        MeshUploadOutcome::Uploaded(id)
                    }
                    Err(error) => {
                        report.failed += 1;
                        MeshUploadOutcome::Failed(error)
                    }
                }
            } else {
                report.discarded += 1;
                MeshUploadOutcome::Stale
            };
            on_result(MeshUploadResult {
                tag: request.tag,
                revision: request.revision,
                outcome,
            });
        }
        report.remaining = self.pending.len();
        report.elapsed = elapsed();
        report
    }
}

fn byte_counts(data: &MeshData) -> Result<(usize, usize), MeshQueueError> {
    let checked_sum = |terms: &[(usize, usize)]| -> Result<usize, MeshQueueError> {
        terms.iter().try_fold(0_usize, |sum, &(count, size)| {
            count
                .checked_mul(size)
                .and_then(|bytes| sum.checked_add(bytes))
                .ok_or(MeshQueueError::TooLarge)
        })
    };
    let cpu = checked_sum(&[
        (data.positions.capacity(), size_of::<Vec3>()),
        (
            data.normals.as_ref().map_or(0, Vec::capacity),
            size_of::<Vec3>(),
        ),
        (
            data.texcoords.as_ref().map_or(0, Vec::capacity),
            size_of::<Vec2>(),
        ),
        (
            data.colors.as_ref().map_or(0, Vec::capacity),
            size_of::<[u8; 4]>(),
        ),
        (
            data.indices.as_ref().map_or(0, Vec::capacity),
            size_of::<u16>(),
        ),
    ])?;
    let gpu = checked_sum(&[
        (data.positions.len(), size_of::<[f32; 3]>()),
        (data.positions.len(), size_of::<[f32; 2]>()), // UV buffer exists even if omitted.
        (
            data.normals.as_ref().map_or(0, Vec::len),
            size_of::<[f32; 3]>(),
        ),
        (
            data.colors.as_ref().map_or(0, Vec::len),
            size_of::<[u8; 4]>(),
        ),
        (data.indices.as_ref().map_or(0, Vec::len), size_of::<u16>()),
    ])?;
    Ok((cpu, gpu))
}

#[cfg(test)]
mod tests;
