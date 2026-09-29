use super::*;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn receive<R: Send + 'static>(pool: &JobPool<R>) -> Completion<R> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(completion) = pool.try_recv() {
            return completion;
        }
        assert!(Instant::now() < deadline, "completion timed out");
        thread::yield_now();
    }
}

fn wait_finished<R: Send + 'static>(pool: &JobPool<R>, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if pool.shared.lock().completed.len() == count {
            return;
        }
        assert!(Instant::now() < deadline, "worker timed out");
        thread::yield_now();
    }
}

#[test]
fn one_bound_covers_queued_running_and_completed_work_and_rejections_keep_inputs() {
    let pool = JobPool::new(1, 2).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    pool.try_submit(move |_| {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        1
    })
    .unwrap();
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    pool.try_submit(|_| 2).unwrap();
    assert_eq!((pool.capacity(), pool.pending()), (2, 2));
    let captured = vec![3, 4];
    let rejected = pool
        .try_submit(move |_| captured.iter().sum::<i32>())
        .unwrap_err();
    assert!(matches!(rejected, SubmitError::Full(_)));
    release_tx.send(()).unwrap();
    wait_finished(&pool, 2);
    assert_eq!(pool.pending(), 2);
    assert!(matches!(pool.try_submit(|_| 99), Err(SubmitError::Full(_))));
    assert!(matches!(receive(&pool).outcome, JobOutcome::Ready(1)));
    pool.try_submit(rejected.into_inner()).unwrap();
    let second = receive(&pool);
    let third = receive(&pool);
    assert!(matches!(second.outcome, JobOutcome::Ready(2)));
    assert!(matches!(third.outcome, JobOutcome::Ready(7)));
    assert_eq!(pool.pending(), 0);
}

#[test]
fn queued_running_and_finished_jobs_can_be_cancelled() {
    let pool = JobPool::new(1, 3).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let running = pool
        .try_submit(move |cancel| {
            started_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !cancel.is_cancelled() {
                assert!(Instant::now() < deadline, "cancellation never arrived");
                thread::yield_now();
            }
            1
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let queued = pool
        .try_submit(|_| panic!("cancelled queued work executed"))
        .unwrap();
    queued.cancel();
    running.cancel();
    assert!(matches!(receive(&pool).outcome, JobOutcome::Cancelled));
    assert!(matches!(receive(&pool).outcome, JobOutcome::Cancelled));
    let finished = pool.try_submit(|_| 3).unwrap();
    wait_finished(&pool, 1);
    finished.cancel();
    assert!(matches!(receive(&pool).outcome, JobOutcome::Cancelled));
}

#[test]
fn current_handle_rejects_obsolete_and_cross_pool_results() {
    let pool = JobPool::new(1, 2).unwrap();
    let old = pool.try_submit(|_| 1).unwrap();
    let current = pool.try_submit(|_| 2).unwrap();
    let stale = receive(&pool);
    assert_eq!(stale.id, old.id());
    assert!(!current.accepts(&stale));
    let latest = receive(&pool);
    assert!(current.accepts(&latest));
    current.cancel();
    assert!(!current.accepts(&latest));
    let other = JobPool::new(1, 1).unwrap();
    let other_handle = other.try_submit(|_| 3).unwrap();
    assert_ne!(other_handle.id(), old.id());
    assert!(!other_handle.accepts(&latest));
}

#[test]
fn shutdown_signals_running_jobs_discards_queue_and_never_needs_completion_drain() {
    let mut pool = JobPool::new(1, 2).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let running = pool
        .try_submit(move |cancel| {
            started_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !cancel.is_cancelled() {
                assert!(Instant::now() < deadline, "shutdown never arrived");
                thread::yield_now();
            }
            1
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    pool.try_submit(|_| panic!("shutdown queued work executed"))
        .unwrap();
    pool.shutdown();
    assert!(running.is_cancelled());
    assert_eq!(pool.pending(), 0);
    assert!(pool.try_recv().is_none());
    assert!(matches!(
        pool.try_submit(|_| 9),
        Err(SubmitError::Stopped(_))
    ));
    pool.shutdown();

    let mut full = JobPool::new(2, 2).unwrap();
    full.try_submit(|_| 1).unwrap();
    full.try_submit(|_| 2).unwrap();
    wait_finished(&full, 2);
    full.shutdown();
    assert_eq!(full.pending(), 0);
}

#[test]
fn panics_are_reported_and_the_worker_accepts_later_jobs() {
    let pool = JobPool::new(1, 1).unwrap();
    pool.try_submit(|_| -> u32 { panic!("expected test panic") })
        .unwrap();
    assert!(matches!(receive(&pool).outcome, JobOutcome::Panicked));
    pool.try_submit(|_| 42).unwrap();
    assert!(matches!(receive(&pool).outcome, JobOutcome::Ready(42)));
}

#[test]
fn zero_workers_or_capacity_is_rejected() {
    assert!(matches!(
        JobPool::<()>::new(0, 1),
        Err(JobPoolError::InvalidConfig)
    ));
    assert!(matches!(
        JobPool::<()>::new(1, 0),
        Err(JobPoolError::InvalidConfig)
    ));
}
