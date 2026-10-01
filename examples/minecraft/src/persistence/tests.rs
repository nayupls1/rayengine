use super::*;
use crate::{
    gameplay::Player,
    survival::{Item, Survival},
    terrain::{Terrain, TerrainSettings},
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rayengine-minecraft-save-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn store(&self) -> Store {
        Store::open(
            self.0.join("world.save"),
            SaveOptions {
                durability: save::Durability::Atomic,
                limits: LIMITS,
            },
        )
        .unwrap()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Snapshot, VoxelWorld, Player, Survival, BlockPos) {
    let terrain = Arc::new(Terrain::new(42, TerrainSettings::default()).unwrap());
    let spawn = terrain.find_spawn(0, 0, 16, 1089).unwrap();
    let player = Player::new(spawn.feet()).unwrap();
    let survival = Survival::default();
    let snapshot = Snapshot::new(terrain.clone(), spawn.support, &player, &survival).unwrap();
    let mut world = VoxelWorld::new(terrain.registry(), 160);
    let pos = spawn.support.split().0;
    world
        .insert_chunk(pos, terrain.chunk(pos).unwrap())
        .unwrap();
    (snapshot, world, player, survival, spawn.support)
}
fn settle(saving: &mut Saving, world: &mut VoxelWorld) {
    let end = Instant::now() + Duration::from_secs(3);
    while saving.status() == SaveStatus::Writing {
        saving.poll(world);
        assert!(Instant::now() < end, "save worker timed out");
        std::thread::yield_now();
    }
}
#[test]
fn snapshot_round_trip_restores_edits_player_inventory_and_regenerates_untouched_chunks() {
    let (base, mut world, mut player, mut survival, p) = fixture();
    survival.inventory.insert(Item::Log, 5);
    survival.inventory.swap(0, 20);
    survival.select(3);
    survival.health.damage(4);
    player.controller.set_look(1.2, -0.4).unwrap();
    world.set_block(p, BlockId::AIR).unwrap();
    let (snapshot, stamps) = base.capture(&world, &player, &survival).unwrap();
    assert_eq!(stamps.len(), 1);
    let bytes = snapshot.encode().unwrap();
    let restored = Snapshot::decode(&bytes).unwrap();
    assert_eq!(restored.encode().unwrap(), bytes);
    assert_eq!(restored.survival().unwrap().snapshot(), survival.snapshot());
    let loaded_player = restored.player().unwrap();
    assert_eq!(loaded_player.position(), player.position());
    assert!((loaded_player.controller.yaw() - player.controller.yaw()).abs() < 0.00001);
    assert_eq!(
        restored.chunk(p.split().0).unwrap().get(p.split().1),
        BlockId::AIR
    );
    let untouched = ChunkPos::new(-5, 3, 7);
    assert_eq!(
        restored.chunk(untouched).unwrap().blocks(),
        base.terrain.chunk(untouched).unwrap().blocks()
    );
    assert!(!restored.chunk(p.split().0).unwrap().is_dirty());
}
#[test]
fn named_slot_ignores_interrupted_siblings_and_rejects_corrupt_future_or_incompatible_saves() {
    let dir = Dir::new();
    let store = dir.store();
    let (snapshot, _, _, _, _) = fixture();
    store.write(&snapshot).unwrap();
    let original = fs::read(store.path()).unwrap();
    fs::write(dir.0.join(".rayengine-save-interrupted.tmp"), b"partial").unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().encode().unwrap(),
        snapshot.encode().unwrap()
    );
    let mut corrupt = original.clone();
    corrupt[save::HEADER_LEN] ^= 1;
    fs::write(store.path(), &corrupt).unwrap();
    assert!(matches!(
        store.load(),
        Err(Error::Container(SaveError::ChecksumMismatch { .. }))
    ));
    let future = save::encode(SCHEMA_VERSION + 1, &snapshot.encode().unwrap(), LIMITS).unwrap();
    fs::write(store.path(), &future).unwrap();
    assert!(matches!(
        store.load(),
        Err(Error::Container(SaveError::UnsupportedSchema { .. }))
    ));
    assert_eq!(fs::read(store.path()).unwrap(), future);
    for field in ["name", "version", "registry", "settings"] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&snapshot.encode().unwrap()).unwrap();
        match field {
            "name" => value["generator"][field] = serde_json::json!("other-generator"),
            "version" => value["generator"][field] = serde_json::json!(2),
            "registry" => value["generator"][field][1] = serde_json::json!("changed:bedrock"),
            _ => value["generator"][field]["relief"] = serde_json::json!(1000),
        }
        let bytes =
            save::encode(SCHEMA_VERSION, &serde_json::to_vec(&value).unwrap(), LIMITS).unwrap();
        fs::write(store.path(), &bytes).unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read(store.path()).unwrap(), bytes);
    }
    fs::write(store.path(), original).unwrap();
    assert!(Saving::open(store.clone(), Some(43)).is_err());
    assert_eq!(store.load().unwrap().unwrap().terrain.info().seed, 42);
}
#[test]
fn malformed_game_fields_counts_and_coordinates_are_rejected_before_live_state() {
    let (snapshot, _, _, _, _) = fixture();
    let base: serde_json::Value = serde_json::from_slice(&snapshot.encode().unwrap()).unwrap();
    for case in 0..10 {
        let mut v = base.clone();
        match case {
            0 => v["survival"]["selected"] = serde_json::json!(9),
            1 => v["survival"]["health"] = serde_json::json!(21),
            2 => v["survival"]["slots"][0] = serde_json::json!({"item":"wooden_pickaxe","count":2}),
            3 => v["survival"]["slots"][0] = serde_json::json!({"item":"dirt","count":0}),
            4 => v["survival"]["slots"] = serde_json::json!(vec![None::<u8>; 37]),
            5 => {
                v["survival"]["pickups"] = serde_json::json!([{ "position":[1e50,0.0,0.0],"stack":{"item":"log","count":1}}])
            }
            6 => v["player"]["velocity"] = serde_json::json!([0.0, -1000.0, 0.0]),
            7 => v["player"]["pitch"] = serde_json::json!(10.0),
            8 => v["player"]["origin"][0] = serde_json::json!(3),
            _ => v["survival"]["slots"] = serde_json::json!([]),
        }
        assert!(
            Snapshot::decode(&serde_json::to_vec(&v).unwrap()).is_err(),
            "invalid case {case}"
        );
    }
    let position = [5, 1, -3];
    for cells in [vec![0u16; 4095], vec![0; 4097], vec![99; 4096]] {
        let mut v = base.clone();
        v["chunks"] = serde_json::json!([{ "position":position,"cells":cells }]);
        assert!(Snapshot::decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    let mut v = base;
    v["chunks"] = serde_json::json!([{ "position":position,"cells":vec![0u16;4096] },{ "position":position,"cells":vec![0u16;4096]}]);
    assert!(Snapshot::decode(&serde_json::to_vec(&v).unwrap()).is_err());
}
#[test]
fn failed_encoding_preserves_old_slot_and_same_slot_instances_are_excluded_until_drop() {
    let dir = Dir::new();
    let store = dir.store();
    let (snapshot, _, _, _, _) = fixture();
    store.write(&snapshot).unwrap();
    let old = fs::read(store.path()).unwrap();
    let mut limited = store.clone();
    limited.options.limits.max_payload_bytes = 1;
    assert!(limited.write(&snapshot).is_err());
    assert_eq!(fs::read(store.path()).unwrap(), old);
    assert!(matches!(
        Store::open(store.path(), SaveOptions::default()),
        Err(Error::Locked)
    ));
    let path = store.path().to_owned();
    drop(limited);
    drop(store);
    let reopened = Store::open(path, SaveOptions::default()).unwrap();
    assert!(reopened.load().unwrap().is_some());
}
#[test]
fn concurrent_edits_and_reinstallation_reject_stale_save_acknowledgements() {
    let dir = Dir::new();
    let store = dir.store();
    let (base, mut world, player, mut survival, p) = fixture();
    world.set_block(p, BlockId::AIR).unwrap();
    survival.inventory.insert(Item::Dirt, 1);
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release = std::sync::Mutex::new(release_rx);
    let writer = Arc::new(move |store: &Store, snapshot: &Snapshot| {
        entered_tx.send(()).unwrap();
        release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        store.write(snapshot)
    });
    let mut saving = Saving::with_writer(store.clone(), base, false, writer).unwrap();
    saving.request();
    saving.start(&mut world, &player, &survival);
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    // Queue another request without copying another in-flight payload.
    saving.request();
    world
        .set_block(p, saving.checkpoint().terrain.blocks().dirt)
        .unwrap();
    survival.inventory.consume(0);
    release_tx.send(()).unwrap();
    settle(&mut saving, &mut world);
    assert!(world.chunk(p.split().0).unwrap().is_dirty());
    let disk = store.load().unwrap().unwrap();
    assert_eq!(
        disk.chunk(p.split().0).unwrap().get(p.split().1),
        BlockId::AIR
    );
    assert_eq!(disk.survival().unwrap().inventory.count(Item::Dirt), 1);
    saving.start(&mut world, &player, &survival);
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let chunk = world.remove_chunk(p.split().0).unwrap();
    world.insert_chunk(p.split().0, chunk).unwrap();
    release_tx.send(()).unwrap();
    settle(&mut saving, &mut world);
    assert!(world.chunk(p.split().0).unwrap().is_dirty());
    // The already committed equal content is safe to acknowledge without another write.
    saving.request();
    saving.start(&mut world, &player, &survival);
    assert!(!world.chunk(p.split().0).unwrap().is_dirty());
    assert_eq!(
        store
            .load()
            .unwrap()
            .unwrap()
            .survival()
            .unwrap()
            .inventory
            .count(Item::Dirt),
        0
    );
}
#[test]
fn write_failure_and_unconfirmed_directory_flush_pin_chunks_until_explicit_retry() {
    for committed in [false, true] {
        let dir = Dir::new();
        let store = dir.store();
        let (base, mut world, player, survival, p) = fixture();
        store.write(&base).unwrap();
        world.set_block(p, BlockId::AIR).unwrap();
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let state = fail.clone();
        let writer = Arc::new(move |store: &Store, snapshot: &Snapshot| {
            if state.load(Ordering::SeqCst) {
                if committed {
                    store.write(snapshot)?;
                }
                Err(Error::Container(SaveError::Io {
                    stage: if committed {
                        save::SaveStage::SyncDirectory
                    } else {
                        save::SaveStage::WriteTemporary
                    },
                    source: io::Error::other("injected storage failure"),
                }))
            } else {
                store.write(snapshot)
            }
        });
        let mut saving = Saving::with_writer(store.clone(), base, true, writer).unwrap();
        saving.request();
        saving.start(&mut world, &player, &survival);
        settle(&mut saving, &mut world);
        assert_eq!(saving.status(), SaveStatus::Failed);
        assert!(world.chunk(p.split().0).unwrap().is_dirty());
        let Error::Container(error) = saving.error().unwrap() else {
            panic!("wrong failure")
        };
        assert_eq!(
            error.commit_state(),
            if committed {
                save::CommitState::Committed
            } else {
                save::CommitState::NotCommitted
            }
        );
        saving.request();
        assert_eq!(saving.status(), SaveStatus::Failed);
        fail.store(false, Ordering::SeqCst);
        saving.retry();
        saving.start(&mut world, &player, &survival);
        settle(&mut saving, &mut world);
        assert!(saving.settled());
        assert!(!world.chunk(p.split().0).unwrap().is_dirty());
    }
}
#[test]
fn streaming_pins_unsaved_then_evicts_and_reloads_exact_saved_geometry() {
    let dir = Dir::new();
    let mut saving = Saving::open(dir.store(), Some(42)).unwrap();
    let terrain = saving.checkpoint().terrain();
    let p = saving.checkpoint().spawn();
    let mut world = VoxelWorld::new(terrain.registry(), 2);
    world
        .insert_chunk(p.split().0, terrain.chunk(p.split().0).unwrap())
        .unwrap();
    world.set_block(p, BlockId::AIR).unwrap();
    let loader = saving.loader();
    let mut streamer = ChunkStreamer::new(
        StreamConfig {
            radius: 0,
            max_resident: 2,
            ..Default::default()
        },
        move |p, r, c| loader.load(p, r, c),
    )
    .unwrap();
    let far = ChunkPos::new(10, 3, 10);
    assert_eq!(
        streamer
            .tick(&mut world, far, |_, _, _| Eviction::Keep)
            .unwrap()
            .pinned,
        1
    );
    let player = saving.checkpoint().player().unwrap();
    let survival = saving.checkpoint().survival().unwrap();
    saving.request();
    saving.start(&mut world, &player, &survival);
    settle(&mut saving, &mut world);
    let report = streamer
        .tick(&mut world, far, |_, _, _| Eviction::Keep)
        .unwrap();
    assert_eq!(report.evicted, 1);
    assert!(world.chunk(p.split().0).is_none());
    let deadline = Instant::now() + Duration::from_secs(3);
    while world.chunk(p.split().0).is_none() {
        streamer
            .tick(&mut world, p.split().0, |_, _, _| Eviction::Keep)
            .unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(world.block(p), Some(BlockId::AIR));
    assert!(!world.chunk(p.split().0).unwrap().is_dirty());
    streamer.shutdown();
}
#[test]
fn modified_chunk_budget_allows_existing_histories_but_rejects_new_ones() {
    let dir = Dir::new();
    let (mut base, mut world, player, survival, p) = fixture();
    let cells = Arc::new(schema::Cells(vec![0; CHUNK_VOLUME].into_boxed_slice()));
    base.edits = Arc::new(
        (0..MAX_EDITED_CHUNKS)
            .map(|i| ([1000 + i as i32, 0, 0], cells.clone()))
            .collect(),
    );
    let saving =
        Saving::with_writer(dir.store(), base.clone(), false, Arc::new(Store::write)).unwrap();
    let admission = saving.admission(&world);
    assert!(admission.full());
    assert!(!admission.allows(p));
    assert!(admission.allows(BlockPos::new(16000, 0, 0)));
    world.set_block(p, BlockId::AIR).unwrap();
    assert!(base.capture(&world, &player, &survival).is_err());
}

#[test]
fn uncollected_pickups_and_airborne_fall_peak_survive_reload() {
    let (base, mut world, mut player, mut survival, _) = fixture();
    let support_chunk = base.spawn().split().0;
    let above = ChunkPos::new(support_chunk.x, support_chunk.y + 1, support_chunk.z);
    world
        .insert_chunk(above, base.terrain.chunk(above).unwrap())
        .unwrap();
    survival.inventory.insert(Item::Dirt, 36 * 64);
    player.controller.set_look(0.0, -1.5).unwrap();
    let report = survival
        .interact(
            &mut crate::gameplay::Interaction::default(),
            &mut world,
            &player,
            crate::survival::SurvivalInput {
                mining: true,
                dt: 1.1,
                ..Default::default()
            },
            base.terrain.blocks(),
        )
        .unwrap();
    assert!(report.edit.is_some());
    assert_eq!(survival.pickups().len(), 1);
    survival.health.movement(player.position().y + 9.0, false);
    let (snapshot, _) = base.capture(&world, &player, &survival).unwrap();
    let restored = Snapshot::decode(&snapshot.encode().unwrap()).unwrap();
    let mut loaded = restored.survival().unwrap();
    assert_eq!(loaded.snapshot(), survival.snapshot());
    assert_eq!(loaded.collect(player.position()), 0);
    assert_eq!(loaded.pickups().len(), 1);
    assert_eq!(loaded.health.movement(player.position().y, true), 6);
    assert_eq!(loaded.health.value(), 14);
}
