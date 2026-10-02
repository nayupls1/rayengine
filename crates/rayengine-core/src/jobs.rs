//! Optional bounded CPU jobs. Native GPU resources remain on their owning thread.
//!
//! Capacity counts queued, running, and unconsumed completed jobs together.
//! Submissions never wait for room. Cancellation is cooperative; shutdown drops
//! queued work, signals running work, and joins workers without requiring the
//! caller to drain completions. Jobs must check cancellation if they may run
//! for a long time. A job that ignores it can delay shutdown indefinitely.

use std::{
    collections::VecDeque,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

static NEXT_JOB: AtomicU64 = AtomicU64::new(0);

/// Process-unique job identity. IDs are never reused, including across pools.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(u64);

/// Cooperative cancellation signal supplied to a running CPU job.
#[derive(Clone, Debug)]
pub struct Cancellation {
    cancelled: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
}

impl Cancellation {
    /// Whether this job was cancelled or its pool is shutting down.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.stopping.load(Ordering::Acquire)
    }
}

/// Identity and cancellation handle. Dropping a handle does not cancel its job.
#[derive(Clone, Debug)]
pub struct JobHandle {
    id: JobId,
    cancellation: Cancellation,
}

impl JobHandle {
    /// Identity used to match a completion with the current game request.
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Signals cancellation; does not preempt running Rust code.
    pub fn cancel(&self) {
        self.cancellation.cancelled.store(true, Ordering::Release);
    }

    /// Whether cancellation or pool shutdown has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Whether this completion belongs to a still-current, uncancelled request.
    /// Store the newest handle in game state and reject older completions here.
    pub fn accepts<R>(&self, completion: &Completion<R>) -> bool {
        self.id == completion.id && !self.is_cancelled()
    }
}

/// Outcome of one CPU job. Job panics do not terminate a worker.
#[derive(Debug)]
pub enum JobOutcome<R> {
    /// Job returned a CPU payload.
    Ready(R),
    /// Job was cancelled before its completion was received.
    Cancelled,
    /// Job panicked. The normal panic hook still runs.
    Panicked,
}

/// Completion received in finish order, rather than submission order.
#[derive(Debug)]
pub struct Completion<R> {
    /// Identity of the request that produced this completion.
    pub id: JobId,
    /// CPU result or cancellation/panic outcome.
    pub outcome: JobOutcome<R>,
}

/// Rejected submission, preserving the closure and its captured inputs for retry.
pub enum SubmitError<F> {
    /// Queued, running, and completed work already fills the configured capacity.
    Full(F),
    /// The pool has shut down.
    Stopped(F),
    /// The process exhausted the job ID space; identities cannot safely wrap.
    IdExhausted(F),
}

impl<F> SubmitError<F> {
    /// Recovers the unexecuted closure, including all captured inputs.
    pub fn into_inner(self) -> F {
        match self {
            Self::Full(work) | Self::Stopped(work) | Self::IdExhausted(work) => work,
        }
    }
}

impl<F> fmt::Debug for SubmitError<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full(_) => "Full",
            Self::Stopped(_) => "Stopped",
            Self::IdExhausted(_) => "IdExhausted",
        })
    }
}

impl<F> fmt::Display for SubmitError<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl<F> std::error::Error for SubmitError<F> {}

/// Invalid pool settings or a failure to create a worker thread.
#[derive(Debug)]
pub enum JobPoolError {
    /// Worker count and outstanding capacity must both be positive.
    InvalidConfig,
    /// The operating system rejected thread creation.
    Spawn(std::io::Error),
}

impl fmt::Display for JobPoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => f.write_str("workers and capacity must be positive"),
            Self::Spawn(error) => write!(f, "worker creation failed: {error}"),
        }
    }
}

impl std::error::Error for JobPoolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(error) => Some(error),
            _ => None,
        }
    }
}

type Work<R> = Box<dyn FnOnce(Cancellation) -> R + Send + 'static>;
struct Job<R> {
    handle: JobHandle,
    work: Work<R>,
}
struct Finished<R> {
    cancellation: Cancellation,
    completion: Completion<R>,
}
struct State<R> {
    queued: VecDeque<Job<R>>,
    completed: VecDeque<Finished<R>>,
    outstanding: usize,
    closed: bool,
}
struct Shared<R> {
    state: Mutex<State<R>>,
    available: Condvar,
    stopping: Arc<AtomicBool>,
}

impl<R> Shared<R> {
    fn lock(&self) -> MutexGuard<'_, State<R>> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Fixed workers with one bound on all outstanding CPU work/results.
///
/// `R` and closures must be `Send + 'static`. Capacity bounds job/result counts,
/// not payload bytes; game code must bound its own inputs/results too. Polling
/// releases a slot. Unconsumed cancelled completions still occupy slots until
/// received, so cancellation cannot silently grow the completion queue.
pub struct JobPool<R: Send + 'static> {
    shared: Arc<Shared<R>>,
    workers: Vec<JoinHandle<()>>,
    capacity: usize,
}

impl<R: Send + 'static> JobPool<R> {
    /// Starts `workers` threads and permits at most `capacity` outstanding jobs.
    pub fn new(workers: usize, capacity: usize) -> Result<Self, JobPoolError> {
        if workers == 0 || capacity == 0 {
            return Err(JobPoolError::InvalidConfig);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queued: VecDeque::new(),
                completed: VecDeque::new(),
                outstanding: 0,
                closed: false,
            }),
            available: Condvar::new(),
            stopping: Arc::new(AtomicBool::new(false)),
        });
        let mut pool = Self {
            shared,
            workers: Vec::new(),
            capacity,
        };
        for index in 0..workers {
            let shared = Arc::clone(&pool.shared);
            let worker = thread::Builder::new()
                .name(format!("rayengine-job-{index}"))
                .spawn(move || worker_loop(shared))
                .map_err(JobPoolError::Spawn)?;
            pool.workers.push(worker);
        }
        Ok(pool)
    }

    /// Bound shared by queued, running, and unconsumed completed jobs.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Current outstanding count. This is a momentary snapshot with multiple callers.
    pub fn pending(&self) -> usize {
        self.shared.lock().outstanding
    }

    /// Attempts submission without waiting for capacity. Errors retain `work`.
    pub fn try_submit<F>(&self, work: F) -> Result<JobHandle, SubmitError<F>>
    where
        F: FnOnce(Cancellation) -> R + Send + 'static,
    {
        let mut state = self.shared.lock();
        if state.closed {
            return Err(SubmitError::Stopped(work));
        }
        if state.outstanding == self.capacity {
            return Err(SubmitError::Full(work));
        }
        // `try_update` is newer than our Rust 1.89 minimum.
        #[allow(deprecated)]
        let Ok(id) =
            NEXT_JOB.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        else {
            return Err(SubmitError::IdExhausted(work));
        };
        let handle = JobHandle {
            id: JobId(id),
            cancellation: Cancellation {
                cancelled: Arc::new(AtomicBool::new(false)),
                stopping: Arc::clone(&self.shared.stopping),
            },
        };
        state.queued.push_back(Job {
            handle: handle.clone(),
            work: Box::new(work),
        });
        state.outstanding += 1;
        drop(state);
        self.shared.available.notify_one();
        Ok(handle)
    }

    /// Receives one completion without waiting, releasing one capacity slot.
    /// Cancelled payloads are dropped before returning, including cancellation
    /// after the worker finished. Cancellation after receipt cannot revoke data.
    pub fn try_recv(&self) -> Option<Completion<R>> {
        let finished = {
            let mut state = self.shared.lock();
            let finished = state.completed.pop_front()?;
            state.outstanding -= 1;
            finished
        };
        let mut completion = finished.completion;
        if finished.cancellation.is_cancelled() {
            completion.outcome = JobOutcome::Cancelled;
        }
        Some(completion)
    }

    /// Drops queued/completed work, signals running jobs, and joins workers.
    /// Idempotent. Running jobs must cooperate or finish before this returns.
    /// If the pool's last owner is dropped by its own job, that worker exits
    /// after the job returns instead of attempting to join itself.
    pub fn shutdown(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        let discarded = {
            let mut state = self.shared.lock();
            state.closed = true;
            state.outstanding = 0;
            (
                std::mem::take(&mut state.queued),
                std::mem::take(&mut state.completed),
            )
        };
        self.shared.available.notify_all();
        // Captured inputs/results can have arbitrary destructors. Drop them
        // outside the mutex so reentrant cleanup cannot deadlock the pool.
        drop(discarded);
        for worker in self.workers.drain(..) {
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
            // A last owner dropped by its own job cannot join itself. Its
            // worker exits as soon as that job returns, holding Shared alive.
        }
    }
}

impl<R: Send + 'static> Drop for JobPool<R> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop<R: Send + 'static>(shared: Arc<Shared<R>>) {
    loop {
        let job = {
            let mut state = shared.lock();
            loop {
                if state.closed {
                    return;
                }
                if let Some(job) = state.queued.pop_front() {
                    break job;
                }
                state = shared
                    .available
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };
        let Job { handle, work } = job;
        let cancellation = handle.cancellation;
        let outcome = match catch_unwind(AssertUnwindSafe(|| {
            if cancellation.is_cancelled() {
                // Cancellation still runs captured inputs' destructors. Keep
                // that cleanup inside the same panic boundary as execution.
                drop(work);
                JobOutcome::Cancelled
            } else {
                JobOutcome::Ready(work(cancellation.clone()))
            }
        })) {
            Ok(outcome) => outcome,
            Err(payload) => {
                // panic_any payloads can themselves panic when dropped. Catch
                // cleanup too, so a recovered job cannot terminate its worker.
                if let Err(nested) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                    // A second payload may repeat the same failing destructor;
                    // retaining it is the only way to guarantee recovery.
                    std::mem::forget(nested);
                }
                JobOutcome::Panicked
            }
        };
        let finished = Finished {
            cancellation,
            completion: Completion {
                id: handle.id,
                outcome,
            },
        };
        let mut state = shared.lock();
        if !state.closed {
            // Total outstanding capacity guarantees completion room. Workers
            // never wait on a full completion queue during shutdown.
            state.completed.push_back(finished);
        } else {
            drop(state);
            drop(finished);
        }
    }
}

#[cfg(test)]
mod tests;
