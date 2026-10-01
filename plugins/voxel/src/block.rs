//! Stable compact IDs, immutable registered properties, and game-owned tile keys.
use crate::{Face, VoxelError};
use std::collections::HashMap;

/// Compact registry index. Air is zero; other IDs follow registration order.
/// Raw values must be validated against a registry when importing chunk data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct BlockId(u16);
impl BlockId {
    /// Reserved empty block, present in every registry.
    pub const AIR: Self = Self(0);
    /// Raw storage/persistence value. It is meaningful only with the same registry.
    pub const fn raw(self) -> u16 {
        self.0
    }
    /// Imports an unvalidated numeric value. Chunk constructors/edits validate it.
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }
}

/// Game-defined texture tile key; this is not a GPU asset handle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TileId(pub u16);

/// Full-cell collision policy. Partial shapes are outside this initial storage API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionKind {
    /// Does not obstruct movement.
    None,
    /// Entire cell is a solid collision box.
    Solid,
}
/// Surface category for future mesh/material adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderKind {
    /// No surface geometry.
    Invisible,
    /// Opaque surface.
    Opaque,
    /// Alpha-cutout surface, such as foliage.
    Cutout,
    /// Blended transparent surface; ordering is a renderer policy.
    Transparent,
}

/// Block properties registered once and then shared immutably.
/// Gameplay can maintain parallel tables keyed by [`BlockId`] for drops/recipes.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockDef {
    /// Unique ASCII key, 1..=128 bytes; letters/digits or `_.:/-` are accepted.
    pub name: String,
    /// Full-cell movement policy, independent of rendering.
    pub collision: CollisionKind,
    /// Mesh/material category.
    pub render: RenderKind,
    /// Nonnegative finite mining hardness; `None` means unbreakable.
    pub hardness: Option<f32>,
    /// Six game-defined tile keys in [`Face::ALL`] order.
    pub textures: [TileId; 6],
}
impl BlockDef {
    /// Opaque solid defaults, hardness 1, all faces using tile zero.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            collision: CollisionKind::Solid,
            render: RenderKind::Opaque,
            hardness: Some(1.0),
            textures: [TileId(0); 6],
        }
    }
    /// Texture tile selected for a face.
    pub fn texture(&self, face: Face) -> TileId {
        self.textures[face.index()]
    }
}

/// Append-only definitions, built before sharing with chunks/worlds via `Arc`.
/// IDs are not reused and definitions cannot be mutated through this API.
/// Names are lookup keys, not an automatic serialization/migration scheme.
pub struct BlockRegistry {
    definitions: Vec<BlockDef>,
    names: HashMap<String, BlockId>,
}
impl Default for BlockRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl BlockRegistry {
    /// Creates a registry containing only reserved invisible, noncolliding air.
    pub fn new() -> Self {
        let air = BlockDef {
            name: "air".into(),
            collision: CollisionKind::None,
            render: RenderKind::Invisible,
            hardness: Some(0.0),
            textures: [TileId(0); 6],
        };
        Self {
            names: HashMap::from([(air.name.clone(), BlockId::AIR)]),
            definitions: vec![air],
        }
    }
    /// Registers validated properties; rejected definitions never change IDs.
    pub fn register(&mut self, definition: BlockDef) -> Result<BlockId, VoxelError> {
        if definition.name.is_empty()
            || definition.name.len() > 128
            || !definition
                .name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.:/-".contains(&c))
            || definition
                .hardness
                .is_some_and(|h| !h.is_finite() || h < 0.0)
        {
            return Err(VoxelError::InvalidDefinition);
        }
        if self.names.contains_key(&definition.name) {
            return Err(VoxelError::DuplicateName);
        }
        let raw = u16::try_from(self.definitions.len()).map_err(|_| VoxelError::RegistryFull)?;
        self.definitions
            .try_reserve(1)
            .map_err(|_| VoxelError::Allocation)?;
        self.names
            .try_reserve(1)
            .map_err(|_| VoxelError::Allocation)?;
        let mut key = String::new();
        key.try_reserve_exact(definition.name.len())
            .map_err(|_| VoxelError::Allocation)?;
        key.push_str(&definition.name);
        let id = BlockId(raw);
        self.definitions.push(definition);
        self.names.insert(key, id);
        Ok(id)
    }
    /// Gets properties, or `None` for an unregistered raw ID.
    pub fn get(&self, id: BlockId) -> Option<&BlockDef> {
        self.definitions.get(usize::from(id.raw()))
    }
    /// Looks up a registered stable name.
    pub fn id(&self, name: &str) -> Option<BlockId> {
        self.names.get(name).copied()
    }
    /// Definition count, including air.
    pub fn len(&self) -> usize {
        self.definitions.len()
    }
    /// Always false for a valid registry because air is reserved.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
    /// Definitions and IDs in registration order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (BlockId, &BlockDef)> {
        self.definitions
            .iter()
            .enumerate()
            .map(|(i, d)| (BlockId(i as u16), d))
    }
}
