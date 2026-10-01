use super::{bounded::Bounded, *};
use crate::{
    gameplay::{Player, as_global},
    survival::{Survival, SurvivalState},
    terrain::{GENERATOR_NAME, GENERATOR_VERSION, Terrain, TerrainSettings},
};
use rayengine_voxel::glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize, ser::SerializeSeq};
use std::{collections::BTreeMap, io::Write};
pub(super) type Edits = BTreeMap<[i32; 3], Arc<Cells>>;
#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Cells(pub Box<[u16]>);
impl<'de> Deserialize<'de> for Cells {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let Bounded(cells) = Bounded::<u16, CHUNK_VOLUME>::deserialize(d)?;
        if cells.len() != CHUNK_VOLUME {
            return Err(serde::de::Error::custom(
                "saved chunk needs exactly 4096 cells",
            ));
        }
        Ok(Self(cells.into_boxed_slice()))
    }
}
/// Origin-relative body/velocity/look snapshot, with precision retained far from zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerState {
    origin: [i32; 3],
    center: [f32; 3],
    velocity: [f32; 3],
    yaw: f32,
    pitch: f32,
    grounded: bool,
}
impl PlayerState {
    /// Snapshot the simulation pose, not the interpolated rendering camera.
    pub fn capture(player: &Player) -> Self {
        Self {
            origin: [player.origin.x, player.origin.y, player.origin.z],
            center: player.controller.body.position.to_array(),
            velocity: player.controller.body.velocity.to_array(),
            yaw: player.controller.yaw(),
            pitch: player.controller.pitch(),
            grounded: player.controller.body.grounded,
        }
    }
    /// Validate body/grid, origin alignment, finite bounded velocity and look limits.
    /// Recreate transient controller buffers/grace timers; retain body contact state.
    pub fn restore(&self) -> Result<Player, Error> {
        let invalid = || Error::Invalid("invalid saved player pose/velocity/look".into());
        if self.origin.iter().any(|a| a.rem_euclid(CHUNK_SIZE) != 0) {
            return Err(invalid());
        }
        let origin = BlockPos::new(self.origin[0], self.origin[1], self.origin[2]);
        origin.split().0.origin()?;
        let center = Vec3::from_array(self.center);
        let velocity = Vec3::from_array(self.velocity);
        if !center.is_finite()
            || center.min_element() < -1.0
            || center.max_element() > 17.0
            || !velocity.is_finite()
            || velocity.abs().max_element() > 128.0
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
        {
            return Err(invalid());
        }
        let mut player = Player::new(as_global(origin) + center.as_dvec3() - DVec3::Y * 0.9)
            .map_err(|_| invalid())?;
        let config = player.controller.config();
        if self.pitch < config.min_pitch
            || self.pitch > config.max_pitch
            || self.yaw.abs() > std::f32::consts::PI
        {
            return Err(invalid());
        }
        player.origin = origin;
        player.controller.teleport(center).map_err(|_| invalid())?;
        let bounds = player.controller.body.bounds();
        if (as_global(origin) + bounds.min.as_dvec3()).min_element() < f64::from(i32::MIN)
            || (as_global(origin) + bounds.max.as_dvec3()).max_element() > f64::from(i32::MAX) + 1.0
        {
            return Err(invalid());
        }
        player.controller.body.velocity = velocity;
        player.controller.body.grounded = self.grounded;
        player
            .controller
            .set_look(self.yaw, self.pitch)
            .map_err(|_| invalid())?;
        Ok(player)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Generator {
    name: String,
    version: u32,
    seed: u64,
    settings: TerrainSettings,
    registry: Bounded<String, 16>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkData {
    position: [i32; 3],
    cells: Arc<Cells>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Disk {
    generator: Generator,
    spawn: [i32; 3],
    player: PlayerState,
    survival: SurvivalState,
    chunks: Bounded<ChunkData, MAX_EDITED_CHUNKS>,
}
fn registry_names(terrain: &Terrain) -> Bounded<String, 16> {
    let registry = terrain.registry();
    Bounded(
        (0..registry.len())
            .map(|i| {
                registry
                    .get(BlockId::from_raw(i as u16))
                    .unwrap()
                    .name
                    .clone()
            })
            .collect(),
    )
}
/// Immutable checkpoint; edited cells are shared across saves and chunk loaders.
/// Only modified chunks are stored. Untouched terrain regenerates from identity/settings.
#[derive(Clone)]
pub struct Snapshot {
    pub(super) terrain: Arc<Terrain>,
    spawn: BlockPos,
    player: PlayerState,
    survival: SurvivalState,
    pub(super) edits: Arc<Edits>,
}
impl Snapshot {
    /// Empty edit history for a newly generated world. No filesystem work occurs.
    pub fn new(
        terrain: Arc<Terrain>,
        spawn: BlockPos,
        player: &Player,
        survival: &Survival,
    ) -> Result<Self, Error> {
        let result = Self {
            terrain,
            spawn,
            player: PlayerState::capture(player),
            survival: survival.snapshot(),
            edits: Arc::new(BTreeMap::new()),
        };
        result.validate()?;
        Ok(result)
    }
    /// Reusable generator allocation and saved settings/seed.
    pub fn terrain(&self) -> Arc<Terrain> {
        self.terrain.clone()
    }
    /// Original support cell used for safe respawn after edited-terrain checks.
    pub fn spawn(&self) -> BlockPos {
        self.spawn
    }
    /// Restore validated simulation pose.
    pub fn player(&self) -> Result<Player, Error> {
        self.player.restore()
    }
    /// Restore inventory, health, fall peak and uncollected pickups.
    pub fn survival(&self) -> Result<Survival, Error> {
        self.survival.restore()
    }
    /// Historical modified chunks, including those no longer resident.
    pub fn edited_chunks(&self) -> usize {
        self.edits.len()
    }
    /// Capture one consistent simulation point. Copy only dirty resident cells;
    /// reuse committed history and return exact installation/revision stamps.
    pub fn capture(
        &self,
        world: &VoxelWorld,
        player: &Player,
        survival: &Survival,
    ) -> Result<(Self, Vec<(ChunkPos, ChunkStamp)>), Error> {
        if world.len() > MAX_SNAPSHOT_CHUNKS
            || !Arc::ptr_eq(&world.shared_registry(), &self.terrain.registry())
        {
            return Err(Error::Invalid(
                "snapshot world/registry exceeds or differs from this session".into(),
            ));
        }
        let mut stamps = Vec::new();
        let mut edits = None;
        for (position, chunk) in world.chunks().filter(|(_, c)| c.is_dirty()) {
            let key = [position.x, position.y, position.z];
            let map = edits.get_or_insert_with(|| self.edits.as_ref().clone());
            if !map.contains_key(&key) && map.len() == MAX_EDITED_CHUNKS {
                return Err(Error::Invalid(
                    "modified chunk budget reached; no save was replaced".into(),
                ));
            }
            map.insert(
                key,
                Arc::new(Cells(
                    chunk
                        .blocks()
                        .iter()
                        .map(|b| b.raw())
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )),
            );
            stamps.push((position, world.stamp(position).unwrap()));
        }
        stamps.sort_unstable_by_key(|(p, _)| (p.x, p.y, p.z));
        let snapshot = Self {
            terrain: self.terrain.clone(),
            spawn: self.spawn,
            player: PlayerState::capture(player),
            survival: survival.snapshot(),
            edits: edits.map_or_else(|| self.edits.clone(), Arc::new),
        };
        snapshot.validate()?;
        Ok((snapshot, stamps))
    }
    /// Canonical, bounded game JSON; engine container encoding remains separate.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.encode_with_limit(MAX_PAYLOAD_BYTES)
    }
    pub(super) fn encode_with_limit(&self, limit: usize) -> Result<Vec<u8>, Error> {
        let mut writer = Limited {
            bytes: Vec::new(),
            limit,
        };
        serde_json::to_writer(&mut writer, self)?;
        Ok(writer.bytes)
    }
    /// Admit schema payload without trusting any saved gameplay state or counts.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Invalid(
                "payload exceeds Minecraft save budget".into(),
            ));
        }
        let disk: Disk = serde_json::from_slice(bytes)?;
        if disk.generator.name != GENERATOR_NAME || disk.generator.version != GENERATOR_VERSION {
            return Err(Error::Invalid(
                "unsupported terrain generator identity/version".into(),
            ));
        }
        let terrain = Arc::new(
            Terrain::new(disk.generator.seed, disk.generator.settings)
                .map_err(|e| Error::Invalid(e.to_string()))?,
        );
        if disk.generator.registry != registry_names(&terrain) {
            return Err(Error::Invalid(
                "saved block registry mapping differs from this generator".into(),
            ));
        }
        let mut edits = BTreeMap::new();
        let registry = terrain.registry();
        for chunk in disk.chunks.0 {
            let pos = ChunkPos::new(chunk.position[0], chunk.position[1], chunk.position[2]);
            pos.origin()?;
            if chunk
                .cells
                .0
                .iter()
                .any(|&id| registry.get(BlockId::from_raw(id)).is_none())
            {
                return Err(Error::Invalid("unknown saved block ID".into()));
            }
            if edits.insert(chunk.position, chunk.cells).is_some() {
                return Err(Error::Invalid("duplicate saved chunk position".into()));
            }
        }
        let result = Self {
            terrain,
            spawn: BlockPos::new(disk.spawn[0], disk.spawn[1], disk.spawn[2]),
            player: disk.player,
            survival: disk.survival,
            edits: Arc::new(edits),
        };
        result.validate()?;
        Ok(result)
    }
    /// Load saved geometry or regenerate a fresh untouched chunk, marking either
    /// clean only after a valid committed checkpoint supplied its content.
    pub fn chunk(&self, position: ChunkPos) -> Result<Chunk, Error> {
        self.chunk_with_registry(position, self.terrain.registry())
    }
    pub(super) fn chunk_with_registry(
        &self,
        position: ChunkPos,
        registry: Arc<BlockRegistry>,
    ) -> Result<Chunk, Error> {
        if !Arc::ptr_eq(&registry, &self.terrain.registry()) {
            return Err(Error::Invalid(
                "loader registry differs from checkpoint".into(),
            ));
        }
        let Some(cells) = self.edits.get(&[position.x, position.y, position.z]) else {
            return self
                .terrain
                .chunk(position)
                .map_err(|e| Error::Invalid(e.to_string()));
        };
        let mut chunk = Chunk::from_blocks(
            registry,
            cells.0.iter().map(|&id| BlockId::from_raw(id)).collect(),
        )?;
        chunk.mark_saved(chunk.revision());
        Ok(chunk)
    }
    pub(super) fn same_state(&self, other: &Self) -> bool {
        self.player == other.player
            && self.survival == other.survival
            && self.spawn == other.spawn
            && self.edits.len() == other.edits.len()
            && self.edits.iter().all(|(k, v)| {
                other
                    .edits
                    .get(k)
                    .is_some_and(|other| Arc::ptr_eq(v, other) || v == other)
            })
    }
    fn validate(&self) -> Result<(), Error> {
        self.survival.restore()?;
        let player = self.player.restore()?;
        // A save cannot install a body inside terrain. Check only its bounded
        // local cell neighborhood using the same f32 bounds as collision.
        let bounds = player.controller.body.bounds();
        let origin = as_global(player.origin);
        let registry = self.terrain.registry();
        let min = (origin + bounds.min.as_dvec3())
            .floor()
            .to_array()
            .map(|v| v as i64);
        let max = (origin + bounds.max.as_dvec3())
            .ceil()
            .to_array()
            .map(|v| v as i64);
        for y in min[1]..max[1] {
            for z in min[2]..max[2] {
                for x in min[0]..max[0] {
                    let position = BlockPos::new(x as i32, y as i32, z as i32);
                    let (chunk, local) = position.split();
                    let id = self.edits.get(&[chunk.x, chunk.y, chunk.z]).map_or_else(
                        || self.terrain.block_at(position),
                        |cells| BlockId::from_raw(cells.0[local.index()]),
                    );
                    if registry.get(id).unwrap().collision == CollisionKind::Solid
                        && block_bounds(position, player.origin)
                            .map_err(|e| Error::Invalid(e.to_string()))?
                            .intersects(&bounds)
                    {
                        return Err(Error::Invalid(
                            "saved player intersects world geometry".into(),
                        ));
                    }
                }
            }
        }
        // Respawn can consult edited geometry later; the original support itself
        // must still be a valid generated spawn identity and within the grid.
        let above = self
            .spawn
            .y
            .checked_add(2)
            .ok_or_else(|| Error::Invalid("spawn outside grid".into()))?;
        if self.terrain.block_at(self.spawn) != self.terrain.blocks().grass
            || self
                .terrain
                .block_at(BlockPos::new(self.spawn.x, self.spawn.y + 1, self.spawn.z))
                != BlockId::AIR
            || self
                .terrain
                .block_at(BlockPos::new(self.spawn.x, above, self.spawn.z))
                != BlockId::AIR
        {
            return Err(Error::Invalid("invalid original generated spawn".into()));
        }
        Ok(())
    }
}
struct ChunkList<'a>(&'a Edits);
impl Serialize for ChunkList<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for (&position, cells) in self.0 {
            seq.serialize_element(&ChunkData {
                position,
                cells: cells.clone(),
            })?;
        }
        seq.end()
    }
}
impl Serialize for Snapshot {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = s.serialize_struct("MinecraftWorld", 5)?;
        let info = self.terrain.info();
        state.serialize_field(
            "generator",
            &Generator {
                name: GENERATOR_NAME.into(),
                version: GENERATOR_VERSION,
                seed: info.seed,
                settings: self.terrain.settings(),
                registry: registry_names(&self.terrain),
            },
        )?;
        state.serialize_field("spawn", &[self.spawn.x, self.spawn.y, self.spawn.z])?;
        state.serialize_field("player", &self.player)?;
        state.serialize_field("survival", &self.survival)?;
        state.serialize_field("chunks", &ChunkList(&self.edits))?;
        state.end()
    }
}
struct Limited {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("Minecraft save payload limit exceeded"));
        }
        let needed = self.bytes.len() + bytes.len();
        if needed > self.bytes.capacity() {
            // Keep geometric growth efficient without requesting capacity beyond
            // the payload budget. Allocator bookkeeping remains additional.
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(needed)
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
