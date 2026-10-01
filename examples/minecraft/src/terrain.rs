//! Game-owned integer recipe: hills, protected surface, caves, ores, and trees.
mod noise;
use noise::{NoiseGrid, hash, noise2, noise3};
use rayengine_voxel::{glam::DVec3, prelude::*};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

/// Stable recipe identity; bump for any cell/noise/block-mapping semantic change.
pub const GENERATOR_NAME: &str = "rayengine-minecraft:alpha-terrain";
/// Version one uses only integer arithmetic to generate cells.
pub const GENERATOR_VERSION: u32 = 1;
/// Finite vertical domain: bedrock at zero, air below zero and at/above 128.
pub const WORLD_HEIGHT: i32 = 128;
const HILLS: u64 = 0x7465727261696e31;
const DETAIL: u64 = 0x7465727261696e32;
const CAVE_A: u64 = 0x6361766573303031;
const CAVE_B: u64 = 0x6361766573303032;
const COAL: u64 = 0x636f616c30303031;
const IRON: u64 = 0x69726f6e30303031;
const TREES: u64 = 0x7472656573303031;
/// Validated recipe settings. Save these alongside seed/version before regeneration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSettings {
    /// Middle surface height, in blocks.
    pub base_height: i32,
    /// Broad height amplitude; finer detail uses one third of this amplitude.
    pub relief: i32,
    /// Carve tunnels below a five-block surface roof and above the protected base.
    pub caves: bool,
    /// Place world-anchored trunks/canopies, including those rooted in neighboring chunks.
    pub trees: bool,
    /// Add coal and iron to stone using world-coordinate density fields.
    pub ores: bool,
}
impl Default for TerrainSettings {
    fn default() -> Self {
        Self {
            base_height: 48,
            relief: 18,
            caves: true,
            trees: true,
            ores: true,
        }
    }
}
impl TerrainSettings {
    /// Rejects unsupported relief/height before allocating or starting workers.
    /// Surface always stays in 8..=112; all tree cells fit below 128.
    pub fn validate(self) -> Result<(), TerrainError> {
        if !(0..=24).contains(&self.relief)
            || self.base_height < 8
            || self.base_height > 112
            || self.base_height - self.relief - self.relief / 3 < 8
            || self.base_height + self.relief + self.relief / 3 > 112
        {
            return Err(TerrainError::Settings);
        }
        Ok(())
    }
}
/// Validation, voxel admission, or bounded spawn-search failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerrainError {
    /// Unsupported settings.
    Settings,
    /// Registry, generation or coordinate error.
    Voxel(VoxelError),
    /// Invalid radius/budget; radius supports 0..=32 and columns 1..=4225.
    Search,
    /// No safe site within the requested radius/column budget.
    NoSpawn,
}
impl From<VoxelError> for TerrainError {
    fn from(e: VoxelError) -> Self {
        Self::Voxel(e)
    }
}
impl fmt::Display for TerrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Settings => {
                f.write_str("terrain surface must stay in 8..=112 with relief 0..=24")
            }
            Self::Voxel(e) => e.fmt(f),
            Self::Search => f.write_str("invalid spawn search limits"),
            Self::NoSpawn => f.write_str("no safe spawn inside the search budget"),
        }
    }
}
impl std::error::Error for TerrainError {}
/// Concrete demo block mapping; IDs follow this exact order after reserved air.
#[derive(Clone, Copy, Debug)]
pub struct DemoBlocks {
    /// Unbreakable bottom layer.
    pub bedrock: BlockId,
    /// Underground rock.
    pub stone: BlockId,
    /// Three cells below grass.
    pub dirt: BlockId,
    /// Surface support.
    pub grass: BlockId,
    /// Coal-bearing stone.
    pub coal: BlockId,
    /// Iron-bearing stone.
    pub iron: BlockId,
    /// Tree trunk.
    pub wood: BlockId,
    /// Solid cutout tree canopy.
    pub leaves: BlockId,
}
fn definitions() -> Result<(Arc<BlockRegistry>, DemoBlocks), VoxelError> {
    let mut registry = BlockRegistry::new();
    let mut ids = Vec::new();
    for (i, name) in [
        "bedrock", "stone", "dirt", "grass", "coal_ore", "iron_ore", "wood", "leaves",
    ]
    .into_iter()
    .enumerate()
    {
        let mut def = BlockDef::new(format!("demo:{name}"));
        def.textures = [TileId(i as u16); 6];
        def.hardness = match name {
            "bedrock" => None,
            "stone" | "coal_ore" | "iron_ore" => Some(2.0),
            _ => Some(1.0),
        };
        if name == "leaves" {
            def.render = RenderKind::Cutout;
        }
        if name == "grass" {
            def.textures = [crate::textures::Tile::GrassSide.id(); 6];
            def.textures[Face::PosY.index()] = crate::textures::Tile::GrassTop.id();
            def.textures[Face::NegY.index()] = crate::textures::Tile::Dirt.id();
        }
        if name == "wood" {
            def.textures[Face::PosY.index()] = crate::textures::Tile::LogTop.id();
            def.textures[Face::NegY.index()] = crate::textures::Tile::LogTop.id();
        }
        ids.push(registry.register(def)?);
    }
    Ok((
        Arc::new(registry),
        DemoBlocks {
            bedrock: ids[0],
            stone: ids[1],
            dirt: ids[2],
            grass: ids[3],
            coal: ids[4],
            iron: ids[5],
            wood: ids[6],
            leaves: ids[7],
        },
    ))
}
/// Safe support and feet position for a centered player no wider than one block
/// and no taller than two blocks. Player movement/collision is the next issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    /// Grass cell underneath the centered player.
    pub support: BlockPos,
}
impl Spawn {
    /// Exact cell center in f64; do not convert distant absolute coordinates to f32.
    pub fn feet(self) -> DVec3 {
        DVec3::new(
            f64::from(self.support.x) + 0.5,
            f64::from(self.support.y) + 1.0,
            f64::from(self.support.z) + 0.5,
        )
    }
}
/// Immutable seeded generator. No neighbor reads, RNG state, globals or native dependencies.
/// All feature precedence and block IDs belong to this demo, not the voxel plugin.
pub struct Terrain {
    seed: u64,
    settings: TerrainSettings,
    registry: Arc<BlockRegistry>,
    blocks: DemoBlocks,
}
impl Terrain {
    /// Validates settings and builds the stable shared block registry.
    pub fn new(seed: u64, settings: TerrainSettings) -> Result<Self, TerrainError> {
        settings.validate()?;
        let (registry, blocks) = definitions()?;
        Ok(Self {
            seed,
            settings,
            registry,
            blocks,
        })
    }
    /// Shared allocation for VoxelWorld/GenerationContext.
    pub fn registry(&self) -> Arc<BlockRegistry> {
        self.registry.clone()
    }
    /// Fixed concrete demo block mapping.
    pub fn blocks(&self) -> DemoBlocks {
        self.blocks
    }
    /// Validated settings; generation never mutates these.
    pub fn settings(&self) -> TerrainSettings {
        self.settings
    }
    /// Highest solid ground cell before tree decoration. No floating-point arithmetic.
    pub fn surface_height(&self, x: i32, z: i32) -> i32 {
        self.height(x.into(), z.into())
    }
    fn height(&self, x: i64, z: i64) -> i32 {
        self.settings.base_height
            + noise2(self.seed, HILLS, x, z, 96) * self.settings.relief / 32768
            + noise2(self.seed, DETAIL, x, z, 24) * (self.settings.relief / 3) / 32768
    }
    fn base(
        &self,
        p: BlockPos,
        surface: i32,
        mut sample: impl FnMut(u64, [i64; 3]) -> i32,
    ) -> BlockId {
        if p.y < 0 || p.y >= WORLD_HEIGHT || p.y > surface {
            return BlockId::AIR;
        }
        if p.y == 0 {
            return self.blocks.bedrock;
        }
        if p.y == surface {
            return self.blocks.grass;
        }
        if p.y >= surface - 3 {
            return self.blocks.dirt;
        }
        if self.settings.caves
            && p.y >= 4
            && p.y < surface - 4
            && sample(CAVE_A, [24, 16, 24]).abs() < 2600
            && sample(CAVE_B, [11, 9, 11]).abs() < 9000
        {
            return BlockId::AIR;
        }
        if self.settings.ores {
            if p.y < 40 && sample(IRON, [7; 3]) > 21000 {
                return self.blocks.iron;
            }
            if p.y < 64 && sample(COAL, [6; 3]) > 18000 {
                return self.blocks.coal;
            }
        }
        self.blocks.stone
    }
    fn trees(&self, min_x: i64, max_x: i64, min_z: i64, max_z: i64) -> Candidates {
        let mut result = Candidates { trees: [None; 9] };
        if !self.settings.trees {
            return result;
        }
        let mut i = 0;
        for z in min_z.div_euclid(12)..=max_z.div_euclid(12) {
            for x in min_x.div_euclid(12)..=max_x.div_euclid(12) {
                let h = hash(self.seed, TREES, x, 0, z);
                if !h.is_multiple_of(5) {
                    continue;
                }
                let tx = x * 12 + 2 + ((h >> 8) % 8) as i64;
                let tz = z * 12 + 2 + ((h >> 16) % 8) as i64;
                let ground = self.height(tx, tz);
                result.trees[i] = Some(Tree {
                    x: tx,
                    z: tz,
                    ground,
                    height: 4 + ((h >> 24) % 3) as i32,
                });
                i += 1;
            }
        }
        result
    }
    /// Samples an untouched world cell without loading chunks. Uses the same
    /// analytical recipe as bulk generation; intended for spawn/tools, not a mutable world.
    pub fn block_at(&self, position: BlockPos) -> BlockId {
        let surface = self.surface_height(position.x, position.z);
        let base = self.base(position, surface, |salt, scale| {
            noise3(
                self.seed,
                salt,
                [position.x.into(), position.y.into(), position.z.into()],
                scale,
            )
        });
        if base != BlockId::AIR || position.y < 0 || position.y >= WORLD_HEIGHT {
            return base;
        }
        let trees = self.trees(
            i64::from(position.x) - 2,
            i64::from(position.x) + 2,
            i64::from(position.z) - 2,
            i64::from(position.z) + 2,
        );
        if trees.iter().any(|t| t.log(position)) {
            return self.blocks.wood;
        }
        if trees.iter().any(|t| t.leaf(position)) {
            return self.blocks.leaves;
        }
        base
    }
    /// Generates one reproducible, clean chunk synchronously through the plugin contract.
    pub fn chunk(&self, position: ChunkPos) -> Result<Chunk, VoxelError> {
        generate_chunk(
            self,
            position,
            &GenerationContext::uncancelled(self.registry()),
        )
    }
    /// Searches a bounded square spiral, clipping coordinates at the i32 grid edge.
    /// Requires grass support and two air cells above it; caves never breach the roof.
    pub fn find_spawn(
        &self,
        x: i32,
        z: i32,
        radius: u32,
        max_columns: usize,
    ) -> Result<Spawn, TerrainError> {
        if radius > 32 || max_columns == 0 || max_columns > 4225 {
            return Err(TerrainError::Search);
        }
        let mut visited = 0;
        for ring in 0..=i64::from(radius) {
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs().max(dz.abs()) != ring {
                        continue;
                    }
                    let (Ok(x), Ok(z)) = (
                        i32::try_from(i64::from(x) + dx),
                        i32::try_from(i64::from(z) + dz),
                    ) else {
                        continue;
                    };
                    if visited == max_columns {
                        return Err(TerrainError::NoSpawn);
                    }
                    visited += 1;
                    let support = BlockPos::new(x, self.surface_height(x, z), z);
                    if self.block_at(support) == self.blocks.grass
                        && self.block_at(BlockPos::new(x, support.y + 1, z)) == BlockId::AIR
                        && self.block_at(BlockPos::new(x, support.y + 2, z)) == BlockId::AIR
                    {
                        return Ok(Spawn { support });
                    }
                }
            }
        }
        Err(TerrainError::NoSpawn)
    }
}
impl ChunkGenerator for Terrain {
    fn info(&self) -> GeneratorInfo {
        GeneratorInfo {
            name: GENERATOR_NAME,
            version: GENERATOR_VERSION,
            seed: self.seed,
        }
    }
    fn generate(
        &self,
        position: ChunkPos,
        context: &GenerationContext<'_>,
    ) -> Result<Chunk, VoxelError> {
        let origin = position.origin()?;
        context.check_cancelled()?;
        if !Arc::ptr_eq(&self.registry, context.registry()) {
            return Err(VoxelError::RegistryMismatch);
        }
        let max_surface =
            self.settings.base_height + self.settings.relief + self.settings.relief / 3;
        if origin.y < 0 || origin.y >= WORLD_HEIGHT || origin.y > max_surface + 7 {
            let mut chunk = Chunk::filled(context.registry().clone(), BlockId::AIR)?;
            chunk.mark_saved(chunk.revision());
            return Ok(chunk);
        }
        let mut heights = [0; 256];
        for z in 0..16 {
            for x in 0..16 {
                heights[x + z * 16] = self.surface_height(origin.x + x as i32, origin.z + z as i32);
            }
        }
        let cave_a = NoiseGrid::new(self.seed, CAVE_A, origin, [24, 16, 24]);
        let cave_b = NoiseGrid::new(self.seed, CAVE_B, origin, [11, 9, 11]);
        let coal = NoiseGrid::new(self.seed, COAL, origin, [6; 3]);
        let iron = NoiseGrid::new(self.seed, IRON, origin, [7; 3]);
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(CHUNK_VOLUME)
            .map_err(|_| VoxelError::Allocation)?;
        cells.resize(CHUNK_VOLUME, BlockId::AIR);
        for y in 0..16 {
            context.check_cancelled()?;
            for z in 0..16 {
                for x in 0..16 {
                    let p = BlockPos::new(
                        origin.x + x as i32,
                        origin.y + y as i32,
                        origin.z + z as i32,
                    );
                    cells[x + z * 16 + y * 256] =
                        self.base(p, heights[x + z * 16], |salt, _| match salt {
                            CAVE_A => cave_a.sample(p),
                            CAVE_B => cave_b.sample(p),
                            COAL => coal.sample(p),
                            IRON => iron.sample(p),
                            _ => unreachable!(),
                        });
                }
            }
        }
        let trees = self.trees(
            i64::from(origin.x) - 2,
            i64::from(origin.x) + 17,
            i64::from(origin.z) - 2,
            i64::from(origin.z) + 17,
        );
        // Logs win over leaves; decorations replace only air, never terrain or ores.
        for logs in [true, false] {
            for tree in trees.iter() {
                context.check_cancelled()?;
                for y in (tree.ground + 1).max(origin.y)
                    ..=(tree.ground + tree.height + 1).min(origin.y + 15)
                {
                    for z in (tree.z - 2).max(i64::from(origin.z))
                        ..=(tree.z + 2).min(i64::from(origin.z) + 15)
                    {
                        for x in (tree.x - 2).max(i64::from(origin.x))
                            ..=(tree.x + 2).min(i64::from(origin.x) + 15)
                        {
                            let p = BlockPos::new(x as i32, y, z as i32);
                            let i = (p.x - origin.x) as usize
                                + (p.z - origin.z) as usize * 16
                                + (p.y - origin.y) as usize * 256;
                            if cells[i] == BlockId::AIR
                                && if logs { tree.log(p) } else { tree.leaf(p) }
                            {
                                cells[i] = if logs {
                                    self.blocks.wood
                                } else {
                                    self.blocks.leaves
                                };
                            }
                        }
                    }
                }
            }
        }
        let mut chunk = Chunk::from_blocks(context.registry().clone(), cells)?;
        chunk.mark_saved(chunk.revision());
        Ok(chunk)
    }
}
#[derive(Clone, Copy)]
struct Tree {
    x: i64,
    z: i64,
    ground: i32,
    height: i32,
}
impl Tree {
    fn log(self, p: BlockPos) -> bool {
        i64::from(p.x) == self.x
            && i64::from(p.z) == self.z
            && p.y > self.ground
            && p.y <= self.ground + self.height
    }
    fn leaf(self, p: BlockPos) -> bool {
        let dy = p.y - (self.ground + self.height);
        let dx = (i64::from(p.x) - self.x).abs();
        let dz = (i64::from(p.z) - self.z).abs();
        if (-2..=0).contains(&dy) {
            dx <= 2 && dz <= 2 && !(dx == 2 && dz == 2)
        } else {
            dy == 1 && dx <= 1 && dz <= 1
        }
    }
}
struct Candidates {
    trees: [Option<Tree>; 9],
}
impl Candidates {
    fn iter(&self) -> impl Iterator<Item = Tree> + '_ {
        self.trees.iter().flatten().copied()
    }
}
/// Stable FNV-1a fingerprint over little-endian raw block IDs (not a security hash).
pub fn chunk_fingerprint(chunk: &Chunk) -> u64 {
    chunk
        .blocks()
        .iter()
        .flat_map(|id| id.raw().to_le_bytes())
        .fold(0xcbf29ce484222325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        })
}
#[cfg(test)]
mod tests;
