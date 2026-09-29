# Background work and mesh uploads

`JobPool<R>` runs optional CPU jobs on fixed workers. `MeshUploadQueue<K>` stages
owned `MeshData` until `Frame::upload_meshes` performs GPU work on the render
thread. Synchronous `context.mesh`/`frame.mesh`/`frame.replace_mesh` remain useful
for small games and short operations.

The two stages have independent bounds. Job capacity includes queued, running,
and unconsumed completed jobs. A full pool returns the original closure through
`SubmitError::into_inner`, so captured inputs can be retried without losing them.
The upload queue bounds both request count and retained mesh vector capacities.
Result payloads held in game state and job inputs/outputs need game-defined size
limits; a generic worker pool cannot infer their heap size.

## A complete asynchronous mesh game

This game generates CPU geometry on a worker, receives it without blocking a
frame, and creates the GPU mesh through an explicit upload budget. The pending
request slot permits one retry when the staging queue is full. Other admission
failures are reported rather than retried forever.

```no_run
use rayengine::prelude::*;
use rayengine::upload::MeshQueueError;

struct Generated {
    jobs: JobPool<MeshData>,
    current: Option<JobHandle>,
    revision: u64,
    uploads: MeshUploadQueue<()>,
    waiting: Option<MeshUpload<()>>,
    mesh: Option<MeshId>,
    error: Option<String>,
}

impl Generated {
    fn new() -> Result<Self, Error> {
        Ok(Self {
            jobs: JobPool::new(1, 4).map_err(|e| Error::Config(e.to_string()))?,
            current: None,
            revision: 1,
            uploads: MeshUploadQueue::new(4, 1024 * 1024)?,
            waiting: None,
            mesh: None,
            error: None,
        })
    }
}

impl Game for Generated {
    fn init(&mut self, _: &mut InitContext<'_, '_>) -> Result<(), Error> {
        self.current = Some(self.jobs.try_submit(|cancel| {
            // Larger generators should check this periodically inside their loop.
            if cancel.is_cancelled() { return MeshData::default(); }
            MeshData::new(vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ])
        }).map_err(|e| Error::Config(e.to_string()))?);
        Ok(())
    }

    fn fixed_update(&mut self, _: &mut Update<'_, '_>) {}

    fn draw(&mut self, frame: &mut Frame<'_, '_>) {
        if self.waiting.is_none() {
            if let Some(completion) = self.jobs.try_recv() {
                if self.current.as_ref().is_some_and(|job| job.accepts(&completion)) {
                    match completion.outcome {
                        JobOutcome::Ready(data) => self.waiting = Some(MeshUpload {
                            tag: (),
                            revision: self.revision,
                            target: self.mesh.map_or(MeshUploadTarget::Create, MeshUploadTarget::Replace),
                            data,
                        }),
                        JobOutcome::Panicked => self.error = Some("mesh generation panicked".into()),
                        JobOutcome::Cancelled => {},
                    }
                }
            }
        }
        if let Some(request) = self.waiting.take() {
            if let Err(rejected) = self.uploads.try_push(request) {
                if rejected.reason == MeshQueueError::Full {
                    self.waiting = Some(rejected.request);
                } else {
                    self.error = Some(rejected.reason.to_string());
                }
            }
        }
        let revision = self.revision;
        frame.upload_meshes(
            &mut self.uploads,
            UploadBudget { max_requests: 1, max_bytes: 64 * 1024, ..UploadBudget::default() },
            |_, candidate_revision| candidate_revision == revision,
            |result| match result.outcome {
                MeshUploadOutcome::Uploaded(id) => self.mesh = Some(id),
                MeshUploadOutcome::Failed(e) => self.error = Some(e.to_string()),
                MeshUploadOutcome::Stale => {},
            },
        );
        frame.clear(Color::BLACK);
        frame.world_3d(Camera3D {
            position: Vec3::new(0.0, 0.0, 6.0), target: Vec3::ZERO,
            ..Camera3D::default()
        }, |canvas| {
            if let Some(mesh) = self.mesh {
                canvas.mesh(mesh, Transform3D::default(), Color::GREEN);
            }
        });
        if let Some(error) = &self.error {
            frame.ui(|ui| ui.text(error, Vec2::splat(20.0), 20.0, Color::RED));
        }
    }
}

fn main() -> Result<(), Error> {
    App::new(Config::new("Generated geometry")).run(Generated::new()?)?;
    Ok(())
}
```

## Cancellation, obsolete results, and shutdown

`JobHandle::cancel` signals cooperative cancellation. Queued cancelled jobs skip
their closures. Running closures should check `Cancellation::is_cancelled` in
long loops. A cancelled payload is dropped when its completion is received;
already-received results cannot be revoked. Cancellation does not release a
capacity slot until the completion is consumed. Job panics produce
`JobOutcome::Panicked`, and the worker continues; the ordinary panic hook runs.

Completions arrive in finish order. Store the newest `JobHandle` for a game
request and use `handle.accepts(&completion)` to reject old/cancelled work.
When a world region changes, cancel its old handle, increment its game revision,
and submit a replacement. Check that revision again with the upload queue's
`is_current` predicate immediately before GPU work. Request tags/revisions and
world scheduling policy belong to the game; the engine does not retain an
unbounded map of every region ever visited.

Dropping a handle does not cancel work. Dropping the pool or calling `shutdown`
discards queued/completed work, signals running jobs, and joins workers. There
is no need to drain a full completion queue first. Rust jobs cannot be forcibly
preempted: a closure that ignores cancellation delays shutdown until it returns.
Use bounded work units. Send CPU payloads such as `MeshData`; raylib resources
and its thread token stay on the graphics thread.

## Upload limits and failures

Call `Frame::upload_meshes` before camera passes. Each call has explicit limits:

- `max_requests`: processed requests, including stale discards and failed attempts.
- `max_bytes`: GPU vertex/index bytes attempted, including fallback UVs and failures.
- `max_time`: elapsed CPU/driver/callback time, checked between requests.

An individual upload cannot be interrupted and may exceed the time limit. Byte
and request limits are hard bounds. Stale discards do not charge GPU bytes.
`UploadReport` reports counts, charged bytes, elapsed time, remaining requests,
and `blocked_upload_bytes` when the FIFO head does not fit. Zero request/time
budgets pause processing; a zero byte budget still allows stale discards.

Oversized current requests stay queued. Use `front_upload_bytes`, increase a
later budget, split geometry, or remove the request with `pop`. This avoids an
implicit exception that uploads an arbitrarily large mesh in one frame. Queue
admission validates geometry and accounts for vector **capacities**, so a short
mesh retaining a very large buffer can exceed the CPU limit.

GPU failure returns `MeshUploadOutcome::Failed` and removes the request. Requeue
explicitly if appropriate. Replacement uses the existing atomic mesh API, so
the old mesh/handle survive a failed allocation. Replacement temporarily needs
both old and new GPU buffers; queue limits do not cap total resident GPU memory.
Result callbacks run on the render thread and should remain short.

Each submission allocates a cancellation flag and boxed closure; queue storage
grows to its configured high-water mark and is reused. Polling an idle job pool
takes a mutex. There is no job-pool overhead when a game does not create one.
Small work often costs less synchronously than the scheduling/receipt overhead.

`jobs_latency`, `jobs_batch_8x1024`, `jobs_empty_poll`, and `jobs_full_rejection`
measure scheduling, payload allocation and backpressure on the CPU. Native
`mesh_upload/direct_replace` and `mesh_upload/budgeted_replace` cases compare
one and 1,024 triangles. Staging/cloning is outside the budgeted measured region;
draining includes GPU upload, callbacks, and releasing the staged CPU payload.
Both native paths include validation, allocation and driver work. See
[testing and performance](crate::guides::testing_performance) for saved baselines.
