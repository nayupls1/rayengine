use super::*;
use crate::{BlockDef, CollisionKind, RenderKind, TileId};

fn registry() -> (Arc<BlockRegistry>, BlockId, BlockId) {
    let mut blocks = BlockRegistry::new();
    let stone = blocks.register(BlockDef::new("demo:stone")).unwrap();
    let mut plant = BlockDef::new("demo:plant");
    plant.collision = CollisionKind::None;
    plant.render = RenderKind::Cutout;
    let plant = blocks.register(plant).unwrap();
    (Arc::new(blocks), stone, plant)
}

#[test]
fn definitions_validate_without_consuming_ids_and_expose_game_properties() {
    let mut registry = BlockRegistry::new();
    assert_eq!(std::mem::size_of::<BlockId>(), 2);
    assert_eq!(
        registry.get(BlockId::AIR).unwrap().collision,
        CollisionKind::None
    );
    assert_eq!(
        registry.get(BlockId::AIR).unwrap().render,
        RenderKind::Invisible
    );
    assert_eq!(registry.id("air"), Some(BlockId::AIR));
    assert!(!registry.is_empty());
    for name in ["", "has space", "line\nbreak", "💎"] {
        assert_eq!(
            registry.register(BlockDef::new(name)),
            Err(VoxelError::InvalidDefinition)
        );
    }
    assert_eq!(
        registry.register(BlockDef::new("a".repeat(129))),
        Err(VoxelError::InvalidDefinition)
    );
    for hardness in [f32::NAN, f32::INFINITY, -0.01] {
        let mut definition = BlockDef::new("invalid:hardness");
        definition.hardness = Some(hardness);
        assert_eq!(
            registry.register(definition),
            Err(VoxelError::InvalidDefinition)
        );
    }
    assert_eq!(
        registry.register(BlockDef::new("air")),
        Err(VoxelError::DuplicateName)
    );
    let mut stone = BlockDef::new("demo:stone");
    stone.hardness = None;
    stone.textures = [
        TileId(10),
        TileId(11),
        TileId(12),
        TileId(13),
        TileId(14),
        TileId(15),
    ];
    let id = registry.register(stone).unwrap();
    assert_eq!(id.raw(), 1);
    for face in Face::ALL {
        assert_eq!(
            registry.get(id).unwrap().texture(face),
            TileId(10 + face.index() as u16)
        );
        assert_eq!(face.opposite().opposite(), face);
    }
    assert_eq!(
        registry.register(BlockDef::new("demo:stone")),
        Err(VoxelError::DuplicateName)
    );
    assert_eq!(
        registry.iter().map(|(id, _)| id.raw()).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(registry.get(BlockId::from_raw(2)).is_none());
}

#[test]
fn all_16_bit_registry_ids_are_usable_without_wraparound() {
    let mut blocks = BlockRegistry::new();
    for id in 1..=u16::MAX {
        assert_eq!(
            blocks
                .register(BlockDef::new(format!("block:{id}")))
                .unwrap()
                .raw(),
            id
        );
    }
    assert_eq!(blocks.len(), 65536);
    assert!(blocks.get(BlockId::from_raw(u16::MAX)).is_some());
    assert_eq!(
        blocks.register(BlockDef::new("overflow")),
        Err(VoxelError::RegistryFull)
    );
    assert_eq!(blocks.len(), 65536);
}

#[test]
fn signed_coordinates_round_trip_across_boundaries_and_grid_extremes() {
    let values = [
        i32::MIN,
        i32::MIN + 15,
        -33,
        -32,
        -17,
        -16,
        -15,
        -1,
        0,
        1,
        15,
        16,
        17,
        31,
        32,
        i32::MAX - 15,
        i32::MAX,
    ];
    for x in values {
        for y in values {
            for z in values {
                let world = BlockPos::new(x, y, z);
                let (chunk, local) = world.split();
                assert_eq!(chunk.block(local).unwrap(), world);
                assert!(local.x() < 16 && local.y() < 16 && local.z() < 16);
            }
        }
    }
    let (chunk, local) = BlockPos::new(-1, -16, -17).split();
    assert_eq!(chunk, ChunkPos::new(-1, -1, -2));
    assert_eq!(local, LocalPos::new(15, 0, 15).unwrap());
    assert!(ChunkPos::new(i32::MAX, 0, 0).origin().is_err());
    assert!(ChunkPos::new(i32::MIN, 0, 0).origin().is_err());
    assert!(BlockPos::new(i32::MAX, 0, 0).neighbor(Face::PosX).is_none());
    assert!(BlockPos::new(0, i32::MIN, 0).neighbor(Face::NegY).is_none());
}

#[test]
fn dense_indices_are_bijective_and_validate_local_coordinates() {
    let mut seen = [false; CHUNK_VOLUME];
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                let local = LocalPos::new(x, y, z).unwrap();
                assert!(!seen[local.index()]);
                seen[local.index()] = true;
                assert_eq!(LocalPos::from_index(local.index()).unwrap(), local);
            }
        }
    }
    assert!(seen.into_iter().all(|v| v));
    assert_eq!(LocalPos::new(1, 2, 3).unwrap().index(), 561);
    for index in [CHUNK_VOLUME, usize::MAX] {
        assert!(LocalPos::from_index(index).is_err());
    }
    for (x, y, z) in [(16, 0, 0), (0, 16, 0), (0, 0, 16), (255, 255, 255)] {
        assert!(LocalPos::new(x, y, z).is_err());
    }
}

#[test]
fn imported_chunks_validate_length_ids_and_transfer_dense_buffers() {
    let (registry, stone, _) = registry();
    for len in [0, 1, 4095, 4097] {
        assert!(matches!(
            Chunk::from_blocks(registry.clone(), vec![stone; len]),
            Err(VoxelError::InvalidChunkLength)
        ));
    }
    let unknown = BlockId::from_raw(3);
    let mut data = vec![BlockId::AIR; CHUNK_VOLUME];
    data[CHUNK_VOLUME - 1] = unknown;
    assert!(
        matches!(Chunk::from_blocks(registry.clone(),data),Err(VoxelError::UnknownBlock(id)) if id == unknown)
    );
    assert!(matches!(
        Chunk::filled(registry.clone(), unknown),
        Err(VoxelError::UnknownBlock(_))
    ));
    let data = vec![stone; CHUNK_VOLUME];
    let pointer = data.as_ptr();
    let chunk = Chunk::from_blocks(registry, data).unwrap();
    assert_eq!(chunk.blocks().as_ptr(), pointer);
    assert_eq!(std::mem::size_of_val(chunk.blocks()), 8192);
    assert!(chunk.blocks().iter().all(|&id| id == stone));
    assert!(chunk.is_dirty());
}

#[test]
fn edits_are_transactional_and_stale_save_acknowledgements_cannot_clear_new_edits() {
    let (registry, stone, _) = registry();
    let mut chunk = Chunk::filled(registry, BlockId::AIR).unwrap();
    let local = LocalPos::default();
    assert!(chunk.mark_saved(0));
    assert!(!chunk.is_dirty());
    assert_eq!(chunk.set(local, BlockId::AIR).unwrap(), None);
    assert_eq!(chunk.revision(), 0);
    assert_eq!(
        chunk.set(local, BlockId::from_raw(255)),
        Err(VoxelError::UnknownBlock(BlockId::from_raw(255)))
    );
    assert!(!chunk.is_dirty());
    assert_eq!(chunk.set(local, stone).unwrap(), Some(BlockId::AIR));
    assert_eq!(chunk.revision(), 1);
    assert!(!chunk.mark_saved(0));
    assert!(!chunk.mark_saved(2));
    assert!(chunk.is_dirty());
    chunk.set(local, BlockId::AIR).unwrap();
    assert_eq!(chunk.revision(), 2);
    assert!(chunk.is_dirty()); // Returning to original contents is still an edit.
    assert!(chunk.mark_saved(2));
    assert!(!chunk.mark_saved(1));
    assert!(!chunk.is_dirty());
    chunk.revision = u64::MAX;
    assert!(chunk.mark_saved(u64::MAX));
    assert_eq!(chunk.set(local, stone), Err(VoxelError::RevisionExhausted));
    assert_eq!(chunk.get(local), BlockId::AIR);
    assert!(!chunk.is_dirty());
    assert_eq!(chunk.set(local, BlockId::AIR).unwrap(), None);
}

#[test]
fn world_admission_preserves_existing_and_rejected_data_at_capacity() {
    let (registry, stone, _) = registry();
    let pos = ChunkPos::default();
    let mut world = VoxelWorld::new(registry.clone(), 1);
    assert!(world.is_empty());
    assert!(world.block(BlockPos::default()).is_none());
    world
        .insert_chunk(pos, Chunk::filled(registry.clone(), stone).unwrap())
        .unwrap();
    let original_stamp = world.stamp(pos).unwrap();
    let mut incoming = Chunk::filled(registry.clone(), BlockId::AIR).unwrap();
    incoming.set(LocalPos::default(), stone).unwrap();
    let rejected = world
        .insert_chunk(ChunkPos::new(1, 0, 0), incoming)
        .unwrap_err();
    assert_eq!(rejected.error, VoxelError::WorldFull);
    assert!(rejected.chunk.is_dirty());
    assert_eq!(rejected.chunk.revision(), 1);
    assert_eq!(world.stamp(pos), Some(original_stamp));
    assert_eq!(world.len(), 1);
    assert_eq!(world.capacity(), 1);
    // Replacement at capacity is allowed; the caller receives the old data.
    let old = world
        .insert_chunk(pos, Chunk::filled(registry.clone(), BlockId::AIR).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(old.get(LocalPos::default()), stone);
    assert_eq!(world.block(BlockPos::default()), Some(BlockId::AIR));
    assert_ne!(
        world.stamp(pos).unwrap().generation,
        original_stamp.generation
    );
    let stamp = world.stamp(pos).unwrap();
    let different_registry = Arc::new(BlockRegistry::new());
    let failure = world
        .insert_chunk(
            pos,
            Chunk::filled(different_registry, BlockId::AIR).unwrap(),
        )
        .unwrap_err();
    assert_eq!(failure.error, VoxelError::RegistryMismatch);
    assert_eq!(world.stamp(pos), Some(stamp));
    let failure = world
        .insert_chunk(
            ChunkPos::new(i32::MAX, 0, 0),
            Chunk::filled(registry.clone(), stone).unwrap(),
        )
        .unwrap_err();
    assert_eq!(failure.error, VoxelError::InvalidChunkPosition);
    assert_eq!(world.stamp(pos), Some(stamp));
    world.generation = u64::MAX;
    let failure = world
        .insert_chunk(pos, Chunk::filled(registry.clone(), stone).unwrap())
        .unwrap_err();
    assert_eq!(failure.error, VoxelError::RevisionExhausted);
    assert_eq!(world.block(BlockPos::default()), Some(BlockId::AIR));
    let mut zero = VoxelWorld::new(registry.clone(), 0);
    assert_eq!(
        zero.insert_chunk(pos, Chunk::filled(registry, stone).unwrap())
            .unwrap_err()
            .error,
        VoxelError::WorldFull
    );
}

#[test]
fn cross_chunk_edits_report_face_neighbors_without_mutating_them() {
    let (registry, stone, _) = registry();
    let mut world = VoxelWorld::new(registry.clone(), 4);
    for x in -1..=0 {
        for z in -1..=0 {
            let pos = ChunkPos::new(x, 0, z);
            world
                .insert_chunk(pos, Chunk::filled(registry.clone(), BlockId::AIR).unwrap())
                .unwrap();
            assert!(world.mark_saved(pos, world.stamp(pos).unwrap()));
        }
    }
    for position in [BlockPos::new(-1, 3, -1), BlockPos::new(0, 3, 0)] {
        let edit = world.set_block(position, stone).unwrap().unwrap();
        assert_eq!(world.block(position), Some(stone));
        assert_eq!(edit.previous, BlockId::AIR);
        assert_eq!(edit.current, stone);
        assert_eq!(edit.affected_chunks.as_slice().len(), 3);
    }
    let untouched = ChunkPos::new(-1, 0, 0);
    assert_eq!(world.chunk(untouched).unwrap().revision(), 0);
    assert!(!world.chunk(untouched).unwrap().is_dirty()); // Mesh invalidation is not save dirtiness.
    let interior = world
        .set_block(BlockPos::new(1, 1, 1), stone)
        .unwrap()
        .unwrap();
    assert_eq!(interior.affected_chunks.as_slice(), &[ChunkPos::default()]);
    let corner = world
        .set_block(BlockPos::default(), stone)
        .unwrap()
        .unwrap();
    assert_eq!(
        corner.affected_chunks.as_slice(),
        &[
            ChunkPos::default(),
            ChunkPos::new(-1, 0, 0),
            ChunkPos::new(0, -1, 0),
            ChunkPos::new(0, 0, -1)
        ]
    );
    assert!(
        world
            .set_block(BlockPos::default(), stone)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        world.set_block(BlockPos::new(999, 0, 0), stone),
        Err(VoxelError::MissingChunk(_))
    ));
    assert_eq!(world.len(), 4);
}

#[test]
fn replacement_generations_reject_old_saves_even_if_revisions_match() {
    let (registry, stone, _) = registry();
    let pos = ChunkPos::default();
    let mut world = VoxelWorld::new(registry.clone(), 1);
    world
        .insert_chunk(pos, Chunk::filled(registry.clone(), stone).unwrap())
        .unwrap();
    let old = world.stamp(pos).unwrap();
    assert!(world.mark_saved(pos, old));
    let chunk = world.remove_chunk(pos).unwrap();
    assert!(!world.mark_saved(pos, old));
    world.insert_chunk(pos, chunk).unwrap();
    let current = world.stamp(pos).unwrap();
    assert_eq!(current.revision, old.revision);
    assert_ne!(current.generation, old.generation);
    assert!(!world.mark_saved(pos, old));
    assert!(world.mark_saved(pos, current));
    let snapshot = current;
    world.set_block(BlockPos::default(), BlockId::AIR).unwrap();
    assert!(!world.mark_saved(pos, snapshot));
    assert!(world.chunk(pos).unwrap().is_dirty());
    assert_eq!(world.chunks().count(), 1);
}

#[test]
fn extreme_chunk_borders_never_report_unrepresentable_neighbors() {
    let pos = BlockPos::new(i32::MIN, i32::MAX, i32::MIN);
    let (chunk, local) = pos.split();
    assert_eq!(DirtyChunks::for_edit(chunk, local).as_slice(), &[chunk]);
}

#[test]
fn a_world_can_supply_the_registry_for_generated_chunks_without_an_external_handle() {
    let (registry, stone, _) = registry();
    let mut world = VoxelWorld::new(registry, 1);
    let chunk = Chunk::filled(world.shared_registry(), stone).unwrap();
    world.insert_chunk(ChunkPos::default(), chunk).unwrap();
    assert_eq!(world.block(BlockPos::default()), Some(stone));
}
