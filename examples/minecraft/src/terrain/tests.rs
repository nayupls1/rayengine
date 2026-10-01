use super::*;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
const FIXTURES: &[(ChunkPos, u64)] = &[
    (ChunkPos::new(0, 0, 0), 0x090aafbca6ad9c74),
    (ChunkPos::new(2, 1, -2), 0xff984ffeeca60982),
    (ChunkPos::new(0, 3, 0), 0x0b77011da62e050d),
    (ChunkPos::new(-1, 3, -1), 0xce927118867c5d1d),
    (ChunkPos::new(-3, 2, 2), 0x5942ea230eed841b),
    (ChunkPos::new(0, 8, 0), 0xb9d103fd6854a325),
    (ChunkPos::new(134217727, 2, -134217728), 0xce92446ac541fe1e),
    (ChunkPos::new(-134217728, 2, 134217727), 0xcfc946855ff39885),
];
#[test]
fn version_one_golden_cells_and_seed_bits_are_stable() {
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    assert_eq!(
        terrain.info(),
        GeneratorInfo {
            name: GENERATOR_NAME,
            version: 1,
            seed: 42
        }
    );
    for &(p, expected) in FIXTURES {
        let chunk = terrain.chunk(p).unwrap();
        assert_eq!(chunk_fingerprint(&chunk), expected, "fixture {p:?}");
        assert!(!chunk.is_dirty());
        assert_eq!(chunk.blocks().len(), CHUNK_VOLUME);
        assert!(std::ptr::eq(chunk.registry(), terrain.registry.as_ref()));
    }
    let other = Terrain::new(42 ^ (1 << 63), TerrainSettings::default()).unwrap();
    assert_ne!(
        chunk_fingerprint(&terrain.chunk(ChunkPos::new(0, 0, 0)).unwrap()),
        chunk_fingerprint(&other.chunk(ChunkPos::new(0, 0, 0)).unwrap())
    );
}
#[test]
fn reversed_and_concurrent_chunk_requests_produce_identical_cells() {
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let expected: HashMap<_, _> = FIXTURES
        .iter()
        .map(|(p, _)| (*p, terrain.chunk(*p).unwrap()))
        .collect();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = FIXTURES
            .iter()
            .rev()
            .map(|&(p, _)| {
                let recipe = &terrain;
                (p, scope.spawn(move || recipe.chunk(p).unwrap()))
            })
            .collect();
        for (p, task) in tasks {
            assert_eq!(task.join().unwrap().blocks(), expected[&p].blocks());
        }
    });
}
#[test]
fn bulk_generation_matches_world_sampler_on_all_faces_and_negative_chunks() {
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let center = ChunkPos::new(-1, 3, -1);
    for p in std::iter::once(center).chain(Face::ALL.map(|f| center.neighbor(f).unwrap())) {
        let chunk = terrain.chunk(p).unwrap();
        for i in 0..CHUNK_VOLUME {
            let local = LocalPos::from_index(i).unwrap();
            let world = p.block(local).unwrap();
            assert_eq!(chunk.get(local), terrain.block_at(world), "{world:?}");
        }
    }
}
#[test]
fn a_tree_crossing_horizontal_and_vertical_chunk_seams_is_complete() {
    let terrain = Terrain::new(
        42,
        TerrainSettings {
            base_height: 44,
            relief: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let mut crossing = None;
    for z in -4..0 {
        for x in -4..0 {
            let trees = terrain.trees(x * 16 - 2, x * 16 + 17, z * 16 - 2, z * 16 + 17);
            crossing = crossing.or_else(|| {
                trees
                    .iter()
                    .find(|t| t.x < 0 && (t.x - 2).div_euclid(16) != (t.x + 2).div_euclid(16))
            });
        }
    }
    let tree = crossing.expect("seeded negative-coordinate tree seam fixture");
    let mut world = VoxelWorld::new(terrain.registry(), 8);
    for y in 2..=3 {
        for z in (tree.z - 2).div_euclid(16)..=(tree.z + 2).div_euclid(16) {
            for x in (tree.x - 2).div_euclid(16)..=(tree.x + 2).div_euclid(16) {
                let p = ChunkPos::new(x as i32, y, z as i32);
                world.insert_chunk(p, terrain.chunk(p).unwrap()).unwrap();
            }
        }
    }
    let mut leaves = 0;
    for y in tree.ground + 1..=tree.ground + tree.height + 1 {
        for z in tree.z - 2..=tree.z + 2 {
            for x in tree.x - 2..=tree.x + 2 {
                let p = BlockPos::new(x as i32, y, z as i32);
                assert_eq!(world.block(p), Some(terrain.block_at(p)), "tree cell {p:?}");
                if tree.log(p) {
                    assert_eq!(world.block(p), Some(terrain.blocks.wood));
                }
                leaves += usize::from(world.block(p) == Some(terrain.blocks.leaves));
            }
        }
    }
    assert!(leaves > 20);
    assert!(world.len() > 2);
}
#[test]
fn caves_ores_bedrock_roof_and_vertical_domain_are_present() {
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let chunk = terrain.chunk(ChunkPos::default()).unwrap();
    assert!(chunk.blocks().contains(&BlockId::AIR));
    assert!(chunk.blocks().contains(&terrain.blocks.coal));
    assert!(chunk.blocks().contains(&terrain.blocks.iron));
    for z in 0..16 {
        for x in 0..16 {
            assert_eq!(
                chunk.get(LocalPos::new(x, 0, z).unwrap()),
                terrain.blocks.bedrock
            );
            let height = terrain.surface_height(i32::from(x), i32::from(z));
            assert_eq!(
                terrain.block_at(BlockPos::new(x.into(), height, z.into())),
                terrain.blocks.grass
            );
            for y in height - 4..height {
                assert_ne!(
                    terrain.block_at(BlockPos::new(x.into(), y, z.into())),
                    BlockId::AIR
                );
            }
        }
    }
    for y in [-1, 8, i32::MIN / 16, i32::MAX / 16] {
        assert!(
            terrain
                .chunk(ChunkPos::new(0, y, 0))
                .unwrap()
                .blocks()
                .iter()
                .all(|id| *id == BlockId::AIR)
        );
    }
    let plain = Terrain::new(
        42,
        TerrainSettings {
            caves: false,
            trees: false,
            ores: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !plain
            .chunk(ChunkPos::default())
            .unwrap()
            .blocks()
            .contains(&BlockId::AIR)
    );
    assert!(
        plain
            .chunk(ChunkPos::default())
            .unwrap()
            .blocks()
            .iter()
            .all(|id| [plain.blocks.stone, plain.blocks.bedrock].contains(id))
    );
}
#[test]
fn spawn_is_deterministic_clear_supported_and_safe_for_generated_chunks() {
    for seed in [0, 1, 42, u64::MAX] {
        let terrain = Terrain::new(seed, TerrainSettings::default()).unwrap();
        for (x, z) in [
            (0, 0),
            (-17, -1),
            (i32::MIN, i32::MAX),
            (i32::MAX, i32::MIN),
        ] {
            let spawn = terrain.find_spawn(x, z, 16, 1089).unwrap();
            assert_eq!(spawn, terrain.find_spawn(x, z, 16, 1089).unwrap());
            let p = spawn.support;
            assert_eq!(terrain.block_at(p), terrain.blocks.grass);
            assert_eq!(
                terrain.block_at(BlockPos::new(p.x, p.y + 1, p.z)),
                BlockId::AIR
            );
            assert_eq!(
                terrain.block_at(BlockPos::new(p.x, p.y + 2, p.z)),
                BlockId::AIR
            );
            let (owner, local) = p.split();
            assert_eq!(
                terrain.chunk(owner).unwrap().get(local),
                terrain.blocks.grass
            );
            assert_eq!(spawn.feet().y, f64::from(p.y + 1));
        }
    }
}
#[test]
fn spawn_search_obeys_limits_and_skips_tree_trunks() {
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let tree = (-4..=4)
        .find_map(|x| terrain.trees(x * 16 - 2, x * 16 + 17, -2, 17).iter().next())
        .unwrap();
    let (x, z) = (tree.x as i32, tree.z as i32);
    assert_eq!(terrain.find_spawn(x, z, 0, 1), Err(TerrainError::NoSpawn));
    let spawn = terrain.find_spawn(x, z, 16, 1089).unwrap();
    assert_ne!((spawn.support.x, spawn.support.z), (x, z));
    for (radius, columns) in [(33, 1), (1, 0), (1, 4226)] {
        assert_eq!(
            terrain.find_spawn(0, 0, radius, columns),
            Err(TerrainError::Search)
        );
    }
}
#[test]
fn settings_cancellation_and_registry_admission_are_checked() {
    for settings in [
        TerrainSettings {
            relief: -1,
            ..Default::default()
        },
        TerrainSettings {
            relief: 25,
            ..Default::default()
        },
        TerrainSettings {
            base_height: i32::MAX,
            ..Default::default()
        },
        TerrainSettings {
            base_height: i32::MIN,
            ..Default::default()
        },
        TerrainSettings {
            base_height: 8,
            relief: 24,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            Terrain::new(42, settings),
            Err(TerrainError::Settings)
        ));
    }
    let terrain = Terrain::new(42, TerrainSettings::default()).unwrap();
    let polls = AtomicUsize::new(0);
    let stop = || polls.fetch_add(1, Ordering::Relaxed) >= 4;
    assert!(matches!(
        generate_chunk(
            &terrain,
            ChunkPos::default(),
            &GenerationContext::new(terrain.registry(), &stop)
        ),
        Err(VoxelError::Cancelled)
    ));
    assert_eq!(polls.load(Ordering::Relaxed), 5);
    let other = GenerationContext::uncancelled(Arc::new(BlockRegistry::new()));
    assert!(matches!(
        generate_chunk(&terrain, ChunkPos::default(), &other),
        Err(VoxelError::RegistryMismatch)
    ));
    assert!(matches!(
        terrain.chunk(ChunkPos::new(i32::MAX, 0, 0)),
        Err(VoxelError::InvalidChunkPosition)
    ));
    let json = serde_json::to_string(&terrain.settings()).unwrap();
    assert_eq!(
        serde_json::from_str::<TerrainSettings>(&json).unwrap(),
        terrain.settings()
    );
    assert!(serde_json::from_str::<TerrainSettings>("{\"base_height\":48,\"relief\":18,\"caves\":true,\"trees\":true,\"ores\":true,\"extra\":0}").is_err());
}
