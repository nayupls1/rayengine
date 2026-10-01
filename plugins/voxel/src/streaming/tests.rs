use super::*;
use crate::{BlockDef, BlockId, BlockPos};
use std::{sync::mpsc, thread, time::Duration};
fn fixture(radius: u32, capacity: usize) -> (VoxelWorld, ChunkStreamer, BlockId) {
    let mut registry = BlockRegistry::new();
    let stone = registry.register(BlockDef::new("test:stone")).unwrap();
    let config = StreamConfig {
        radius,
        max_resident: capacity,
        workers: 1,
        max_jobs: 2,
        max_meshes: 2,
        ..Default::default()
    };
    let streamer = ChunkStreamer::new(config, move |_, registry, _| {
        let mut chunk = Chunk::filled(registry, stone)?;
        chunk.mark_saved(chunk.revision());
        Ok(chunk)
    })
    .unwrap();
    (
        VoxelWorld::new(Arc::new(registry), capacity),
        streamer,
        stone,
    )
}
fn tick(s: &mut ChunkStreamer, w: &mut VoxelWorld, p: ChunkPos) -> StreamReport {
    let report = s.tick(w, p, |_, _, _| Eviction::Keep).unwrap();
    assert!(report.resident <= s.config.max_resident);
    assert!(report.jobs <= s.config.max_jobs);
    assert!(report.mesh_slots <= s.config.max_meshes);
    assert!(report.ready_bytes <= s.config.mesh_bytes * s.config.max_meshes);
    report
}
fn wait(
    s: &mut ChunkStreamer,
    w: &mut VoxelWorld,
    p: ChunkPos,
    predicate: impl Fn(&ChunkStreamer, &VoxelWorld) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        tick(s, w, p);
        if predicate(s, w) {
            return;
        }
        assert!(Instant::now() < deadline, "streaming did not settle");
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn rapid_travel_cancels_old_generation_and_bounds_history() {
    let (mut world, mut s, _) = fixture(0, 2);
    let (started_tx, started_rx) = mpsc::channel();
    s.loader = Arc::new(move |p, registry, cancel| {
        if p.x == 0 {
            started_tx.send(()).unwrap();
            while !cancel.is_cancelled() {
                thread::yield_now();
            }
        }
        let mut c = Chunk::filled(registry, BlockId::AIR)?;
        c.mark_saved(c.revision());
        Ok(c)
    });
    tick(&mut s, &mut world, ChunkPos::default());
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    for x in 1..100 {
        tick(&mut s, &mut world, ChunkPos::new(x, 0, 0));
    }
    let last = ChunkPos::new(100, 0, 0);
    wait(&mut s, &mut world, last, |s, _| !s.ready.is_empty());
    assert!(world.chunk(ChunkPos::default()).is_none());
    assert_eq!(world.len(), 1);
    assert_eq!(s.desired(), &[last]);
    let mesh = s.take_mesh(&world).unwrap();
    assert_eq!(mesh.dependencies().position(), last);
    assert!(s.finish_mesh(mesh.dependencies(), true));
}
#[test]
fn edits_in_flight_and_old_acknowledgements_cannot_install_stale_work() {
    let (mut world, mut s, _) = fixture(0, 2);
    let p = ChunkPos::default();
    wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
    let old = s.take_mesh(&world).unwrap();
    for x in 0..8 {
        world
            .set_block(BlockPos::new(x, 0, 0), BlockId::AIR)
            .unwrap();
    }
    tick(&mut s, &mut world, p);
    assert!(s.ready.is_empty()); // Consumer still owns its bounded slot/position.
    assert!(!old.dependencies().is_current(&world));
    s.finish_mesh(old.dependencies(), false);
    wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
    let new = s.take_mesh(&world).unwrap();
    assert!(!s.finish_mesh(old.dependencies(), true));
    assert!(new.dependencies().is_current(&world));
    assert!(s.finish_mesh(new.dependencies(), true));
    let report = tick(&mut s, &mut world, p);
    assert_eq!(report.submitted, 0);
}
#[test]
fn neighbor_arrival_and_removal_rebuild_receipts() {
    let (mut world, mut s, stone) = fixture(0, 2);
    let p = ChunkPos::default();
    wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
    let old = s.take_mesh(&world).unwrap();
    s.finish_mesh(old.dependencies(), true);
    let neighbor = ChunkPos::new(1, 0, 0);
    // Dirty neighbor is pinned outside the wanted region, allowing its border to occlude.
    world
        .insert_chunk(
            neighbor,
            Chunk::filled(world.shared_registry(), stone).unwrap(),
        )
        .unwrap();
    wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
    let joined = s.take_mesh(&world).unwrap();
    assert_eq!(joined.stats().quads, 5);
    s.finish_mesh(joined.dependencies(), true);
    world.remove_chunk(neighbor);
    wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
    assert_eq!(s.take_mesh(&world).unwrap().stats().quads, 6);
}
#[test]
fn ready_meshes_are_discarded_after_repeated_edits() {
    let (mut world, mut s, stone) = fixture(0, 2);
    let p = ChunkPos::default();
    for i in 0..10 {
        wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
        world
            .set_block(
                BlockPos::new(0, 0, 0),
                if i % 2 == 0 { BlockId::AIR } else { stone },
            )
            .unwrap();
        assert!(s.take_mesh(&world).is_none());
    }
    wait(&mut s, &mut world, p, |s, _| !s.ready.is_empty());
    assert!(
        s.take_mesh(&world)
            .unwrap()
            .dependencies()
            .is_current(&world)
    );
}
#[test]
fn failed_save_pins_data_and_blocks_load_until_exact_acknowledgement() {
    let (mut world, mut s, _) = fixture(0, 1);
    let origin = ChunkPos::default();
    wait(&mut s, &mut world, origin, |_, w| w.chunk(origin).is_some());
    world.set_block(BlockPos::default(), BlockId::AIR).unwrap();
    let next = ChunkPos::new(4, 0, 0);
    let report = tick(&mut s, &mut world, next);
    assert_eq!(report.pinned, 1);
    assert_eq!(report.submitted, 0);
    let report = s
        .tick(&mut world, next, |p, c, stamp| {
            assert_eq!(p, origin);
            assert!(c.is_dirty());
            assert_eq!(c.revision(), stamp.revision);
            Eviction::Saved
        })
        .unwrap();
    assert_eq!(report.evicted, 1);
    wait(&mut s, &mut world, next, |_, w| w.chunk(next).is_some());
}
#[test]
fn load_errors_pause_until_explicit_retry_and_panics_do_not_kill_pool() {
    let (mut world, mut s, _) = fixture(0, 1);
    s.loader = Arc::new(|_, _, _| Err(VoxelError::Allocation));
    let p = ChunkPos::default();
    wait(&mut s, &mut world, p, |s, _| s.failure(p).is_some());
    assert_eq!(tick(&mut s, &mut world, p).submitted, 0);
    s.retry(p);
    s.loader = Arc::new(|_, _, _| panic!("injected load panic"));
    wait(&mut s, &mut world, p, |s, _| {
        s.failure(p) == Some(&StreamFailure::Panicked)
    });
    s.retry(p);
    s.loader = Arc::new(|_, registry, _| Chunk::filled(registry, BlockId::AIR));
    wait(&mut s, &mut world, p, |_, w| w.chunk(p).is_some());
}
#[test]
fn shutdown_cancels_workers_and_preserves_game_owned_dirty_chunks() {
    let (mut world, mut s, _) = fixture(0, 2);
    let p = ChunkPos::default();
    world
        .insert_chunk(
            p,
            Chunk::filled(world.shared_registry(), BlockId::AIR).unwrap(),
        )
        .unwrap();
    tick(&mut s, &mut world, p);
    s.shutdown();
    s.shutdown();
    assert_eq!(s.pool.pending(), 0);
    assert!(s.ready.is_empty());
    assert_eq!(world.len(), 1);
    assert!(world.chunk(p).unwrap().is_dirty());
    assert_eq!(tick(&mut s, &mut world, p).submitted, 0);
    drop(s);
    assert_eq!(world.len(), 1);
}
#[test]
fn nearest_first_focus_clips_world_edges_and_rejects_other_worlds() {
    let (mut world, mut s, _) = fixture(1, 9);
    tick(&mut s, &mut world, ChunkPos::default());
    assert_eq!(s.desired()[0], ChunkPos::default());
    assert_eq!(s.active[0].position, ChunkPos::default());
    assert_eq!(s.active[1].position, ChunkPos::new(-1, 0, 0));
    let edge = ChunkPos::new(134217727, 0, 0);
    tick(&mut s, &mut world, edge);
    assert_eq!(s.desired.len(), 6);
    assert!(s.desired.iter().all(|p| p.origin().is_ok()));
    let mut other = VoxelWorld::new(world.shared_registry(), 9);
    assert!(matches!(
        s.tick(&mut other, edge, |_, _, _| Eviction::Keep),
        Err(StreamError::World)
    ));
    let bad = StreamConfig {
        radius: u32::MAX,
        ..Default::default()
    };
    assert!(matches!(
        ChunkStreamer::new(bad, |_, r, _| Chunk::filled(r, BlockId::AIR)),
        Err(StreamError::Config)
    ));
}
#[test]
fn mesher_cooperates_with_cancellation() {
    let (mut world, _, _) = fixture(0, 1);
    let p = ChunkPos::default();
    world
        .insert_chunk(
            p,
            Chunk::filled(world.shared_registry(), BlockId::AIR).unwrap(),
        )
        .unwrap();
    let mut polls = 0;
    let result =
        MeshInput::capture(&world, p)
            .unwrap()
            .build_cancellable(MeshingOptions::default(), || {
                polls += 1;
                polls == 3
            });
    assert!(matches!(result, Err(MeshingError::Cancelled)));
    assert_eq!(polls, 3);
}
