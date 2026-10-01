//! Headless voxel query example; no native toolchain or graphics context needed.
use rayengine_voxel::{glam::DVec3, prelude::*};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut definitions = BlockRegistry::new();
    let stone = definitions.register(BlockDef::new("demo:stone"))?;
    let definitions = Arc::new(definitions);
    let mut world = VoxelWorld::new(definitions.clone(), 2);
    for pos in [ChunkPos::new(-1, 0, 0), ChunkPos::new(0, 0, 0)] {
        world.insert_chunk(pos, Chunk::filled(definitions.clone(), BlockId::AIR)?)?;
    }
    let edit = world
        .set_block(BlockPos::new(0, 1, 1), stone)?
        .expect("changed air");
    println!("Edit affects {:?}", edit.affected_chunks.as_slice());
    let ray = GridRay::new(DVec3::new(-2.5, 1.5, 1.5), DVec3::X)?;
    let result = world.raycast(ray, RaycastOptions::default(), |_, block| {
        block.collision == CollisionKind::Solid
    })?;
    if let RaycastOutcome::Hit(hit) = result.outcome {
        assert_eq!(hit.position, BlockPos::new(0, 1, 1));
        assert_eq!(hit.adjacent, Some(BlockPos::new(-1, 1, 1)));
        println!(
            "Hit {:?} at {} units through {:?}",
            hit.position, hit.distance, hit.face
        );
    }
    Ok(())
}
