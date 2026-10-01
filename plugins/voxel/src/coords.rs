//! Signed world positions and validated local positions with explicit face order.
use crate::VoxelError;

/// Cells along each axis of a chunk. A block occupies one world unit.
pub const CHUNK_SIZE: i32 = 16;
/// Dense block count of one 16×16×16 chunk.
pub const CHUNK_VOLUME: usize = 4096;

/// Signed cell coordinate. Cell `(x,y,z)` owns `[x,x+1) × [y,y+1) × [z,z+1)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlockPos {
    /// VoxelWorld X; positive points right.
    pub x: i32,
    /// VoxelWorld Y; positive points up.
    pub y: i32,
    /// VoxelWorld Z; uses the engine's right-handed coordinate convention.
    pub z: i32,
}
impl BlockPos {
    /// Creates a signed cell position.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Splits with Euclidean division, including negative coordinates.
    pub fn split(self) -> (ChunkPos, LocalPos) {
        let chunk = ChunkPos::new(
            self.x.div_euclid(CHUNK_SIZE),
            self.y.div_euclid(CHUNK_SIZE),
            self.z.div_euclid(CHUNK_SIZE),
        );
        let local = LocalPos {
            x: self.x.rem_euclid(CHUNK_SIZE) as u8,
            y: self.y.rem_euclid(CHUNK_SIZE) as u8,
            z: self.z.rem_euclid(CHUNK_SIZE) as u8,
        };
        (chunk, local)
    }

    /// Neighbor through a face, or `None` at the edge of the i32 grid.
    pub fn neighbor(self, face: Face) -> Option<Self> {
        let [x, y, z] = face.normal();
        Some(Self::new(
            self.x.checked_add(x)?,
            self.y.checked_add(y)?,
            self.z.checked_add(z)?,
        ))
    }
}

/// Chunk coordinate; validate with [`Self::origin`] before using it as resident storage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    /// Signed chunk X.
    pub x: i32,
    /// Signed chunk Y.
    pub y: i32,
    /// Signed chunk Z.
    pub z: i32,
}
impl ChunkPos {
    /// Creates a chunk coordinate; storage rejects coordinates outside the grid.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Lowest world cell. All 16³ cells must fit in i32 coordinates.
    pub fn origin(self) -> Result<BlockPos, VoxelError> {
        let axis = |v: i32| {
            let origin = v
                .checked_mul(CHUNK_SIZE)
                .ok_or(VoxelError::InvalidChunkPosition)?;
            origin
                .checked_add(CHUNK_SIZE - 1)
                .ok_or(VoxelError::InvalidChunkPosition)?;
            Ok(origin)
        };
        Ok(BlockPos::new(axis(self.x)?, axis(self.y)?, axis(self.z)?))
    }

    /// Reconstructs a world cell without overflow.
    pub fn block(self, local: LocalPos) -> Result<BlockPos, VoxelError> {
        let origin = self.origin()?;
        Ok(BlockPos::new(
            origin.x + i32::from(local.x),
            origin.y + i32::from(local.y),
            origin.z + i32::from(local.z),
        ))
    }

    /// Adjacent representable chunk, or `None` at the world grid edge.
    pub fn neighbor(self, face: Face) -> Option<Self> {
        let [x, y, z] = face.normal();
        let next = Self::new(
            self.x.checked_add(x)?,
            self.y.checked_add(y)?,
            self.z.checked_add(z)?,
        );
        next.origin().ok()?;
        Some(next)
    }
}

/// Validated coordinates within a chunk. X is the fastest axis, then Z, then Y.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LocalPos {
    x: u8,
    y: u8,
    z: u8,
}
impl LocalPos {
    /// Rejects components outside 0..16.
    pub fn new(x: u8, y: u8, z: u8) -> Result<Self, VoxelError> {
        if x >= 16 || y >= 16 || z >= 16 {
            return Err(VoxelError::InvalidLocalPosition);
        }
        Ok(Self { x, y, z })
    }
    /// Local X.
    pub const fn x(self) -> u8 {
        self.x
    }
    /// Local Y.
    pub const fn y(self) -> u8 {
        self.y
    }
    /// Local Z.
    pub const fn z(self) -> u8 {
        self.z
    }
    /// Dense offset `x + 16*z + 256*y` in 0..4096.
    pub const fn index(self) -> usize {
        self.x as usize + 16 * self.z as usize + 256 * self.y as usize
    }
    /// Inverse of [`Self::index`]; rejects offsets outside a chunk.
    pub fn from_index(index: usize) -> Result<Self, VoxelError> {
        if index >= CHUNK_VOLUME {
            return Err(VoxelError::InvalidLocalPosition);
        }
        Ok(Self {
            x: (index % 16) as u8,
            y: (index / 256) as u8,
            z: ((index / 16) % 16) as u8,
        })
    }
}

/// Outward cell faces. This order also indexes [`crate::BlockDef::textures`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Face {
    /// Negative X.
    NegX = 0,
    /// Positive X.
    PosX = 1,
    /// Negative Y (bottom).
    NegY = 2,
    /// Positive Y (top).
    PosY = 3,
    /// Negative Z.
    NegZ = 4,
    /// Positive Z.
    PosZ = 5,
}
impl Face {
    /// All six faces in texture-array order.
    pub const ALL: [Self; 6] = [
        Self::NegX,
        Self::PosX,
        Self::NegY,
        Self::PosY,
        Self::NegZ,
        Self::PosZ,
    ];
    /// Texture-array index.
    pub const fn index(self) -> usize {
        self as usize
    }
    /// Integer outward normal, also a neighbor-cell offset.
    pub const fn normal(self) -> [i32; 3] {
        match self {
            Self::NegX => [-1, 0, 0],
            Self::PosX => [1, 0, 0],
            Self::NegY => [0, -1, 0],
            Self::PosY => [0, 1, 0],
            Self::NegZ => [0, 0, -1],
            Self::PosZ => [0, 0, 1],
        }
    }
    /// Opposite face.
    pub const fn opposite(self) -> Self {
        match self {
            Self::NegX => Self::PosX,
            Self::PosX => Self::NegX,
            Self::NegY => Self::PosY,
            Self::PosY => Self::NegY,
            Self::NegZ => Self::PosZ,
            Self::PosZ => Self::NegZ,
        }
    }
}
