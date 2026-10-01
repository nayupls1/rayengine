//! Game-owned, deterministic chunk generation with cooperative cancellation.
use crate::{BlockRegistry, Chunk, ChunkPos, VoxelError};
use std::sync::Arc;

/// Compatibility identity for regenerating untouched terrain. Save settings and
/// block-registry mapping alongside this identity; the plugin does not serialize them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratorInfo {
    /// Stable game-defined recipe key.
    pub name: &'static str,
    /// Recipe version; bump when generated cells or registry interpretation change.
    pub version: u32,
    /// All seed bits are significant; no global random state is used.
    pub seed: u64,
}
/// Registry and cancellation for one bounded CPU generation call.
/// The generator must bound its own scratch work and poll during long operations.
pub struct GenerationContext<'a> {
    registry: Arc<BlockRegistry>,
    cancelled: &'a (dyn Fn() -> bool + Sync),
}
impl<'a> GenerationContext<'a> {
    /// Uses a game/job cancellation predicate. No worker or native resources are created.
    pub fn new(registry: Arc<BlockRegistry>, cancelled: &'a (dyn Fn() -> bool + Sync)) -> Self {
        Self {
            registry,
            cancelled,
        }
    }
    /// Shared allocation required by the receiving world's chunk storage.
    pub fn registry(&self) -> &Arc<BlockRegistry> {
        &self.registry
    }
    /// Returns Cancelled when the owning job/game no longer wants this result.
    pub fn check_cancelled(&self) -> Result<(), VoxelError> {
        if (self.cancelled)() {
            Err(VoxelError::Cancelled)
        } else {
            Ok(())
        }
    }
}
impl GenerationContext<'static> {
    /// Synchronous generation without a cancellation source, useful for fixtures/tools.
    pub fn uncancelled(registry: Arc<BlockRegistry>) -> Self {
        Self::new(registry, &|| false)
    }
}
/// Immutable game recipe, shareable among fixed CPU workers. Content, seed/settings,
/// and save policy remain game-owned. Implementations must produce the same cells
/// independently of request order and express features in world coordinates.
/// Prefer integer arithmetic when promising bit-exact cross-platform fixtures.
/// Constructed chunks start dirty; explicitly acknowledge reproducible output if
/// your save policy permits regenerating it instead of storing it.
pub trait ChunkGenerator: Send + Sync {
    /// Compatibility information for the concrete recipe.
    fn info(&self) -> GeneratorInfo;
    /// Produces exactly one chunk using the supplied registry allocation.
    /// Called by generate_chunk after validating coordinates/cancellation.
    fn generate(
        &self,
        position: ChunkPos,
        context: &GenerationContext<'_>,
    ) -> Result<Chunk, VoxelError>;
}
/// Validates coordinates, cancellation before/after work, and registry identity.
/// No scheduling or automatic save acknowledgement occurs. This directly fits
/// ChunkStreamer's loader callback; construct a context using the job token.
pub fn generate_chunk(
    generator: &dyn ChunkGenerator,
    position: ChunkPos,
    context: &GenerationContext<'_>,
) -> Result<Chunk, VoxelError> {
    position.origin()?;
    context.check_cancelled()?;
    let chunk = generator.generate(position, context)?;
    context.check_cancelled()?;
    if !std::ptr::eq(chunk.registry(), context.registry().as_ref()) {
        return Err(VoxelError::RegistryMismatch);
    }
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockId;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Empty;
    impl ChunkGenerator for Empty {
        fn info(&self) -> GeneratorInfo {
            GeneratorInfo {
                name: "test:empty",
                version: 1,
                seed: 0,
            }
        }
        fn generate(&self, _: ChunkPos, ctx: &GenerationContext<'_>) -> Result<Chunk, VoxelError> {
            Chunk::filled(ctx.registry().clone(), BlockId::AIR)
        }
    }
    #[test]
    fn validates_position_and_preserves_game_save_policy() {
        let registry = Arc::new(BlockRegistry::new());
        let context = GenerationContext::uncancelled(registry);
        assert!(
            generate_chunk(&Empty, ChunkPos::default(), &context)
                .unwrap()
                .is_dirty()
        );
        assert!(matches!(
            generate_chunk(&Empty, ChunkPos::new(i32::MAX, 0, 0), &context),
            Err(VoxelError::InvalidChunkPosition)
        ));
        assert!(matches!(
            generate_chunk(
                &Empty,
                ChunkPos::default(),
                &GenerationContext::new(context.registry().clone(), &|| true)
            ),
            Err(VoxelError::Cancelled)
        ));
    }
    #[test]
    fn rejects_cancellation_after_work_and_a_foreign_registry() {
        struct Cancel<'a>(&'a AtomicBool);
        impl ChunkGenerator for Cancel<'_> {
            fn info(&self) -> GeneratorInfo {
                Empty.info()
            }
            fn generate(
                &self,
                _: ChunkPos,
                ctx: &GenerationContext<'_>,
            ) -> Result<Chunk, VoxelError> {
                self.0.store(true, Ordering::Release);
                Empty.generate(ChunkPos::default(), ctx)
            }
        }
        let flag = AtomicBool::new(false);
        let cancelled = || flag.load(Ordering::Acquire);
        let context = GenerationContext::new(Arc::new(BlockRegistry::new()), &cancelled);
        assert!(matches!(
            generate_chunk(&Cancel(&flag), ChunkPos::default(), &context),
            Err(VoxelError::Cancelled)
        ));
        struct Foreign;
        impl ChunkGenerator for Foreign {
            fn info(&self) -> GeneratorInfo {
                Empty.info()
            }
            fn generate(
                &self,
                _: ChunkPos,
                _: &GenerationContext<'_>,
            ) -> Result<Chunk, VoxelError> {
                Chunk::filled(Arc::new(BlockRegistry::new()), BlockId::AIR)
            }
        }
        flag.store(false, Ordering::Release);
        assert!(matches!(
            generate_chunk(&Foreign, ChunkPos::default(), &context),
            Err(VoxelError::RegistryMismatch)
        ));
    }
}
