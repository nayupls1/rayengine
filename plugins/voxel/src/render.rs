//! Optional render-thread ownership, materials, and bounded streaming uploads.
mod streaming;
use crate::glam::Vec4;
use crate::meshing::{chunk_bounds, chunk_translation};
use crate::{
    BlockPos, ChunkMesh, ChunkPos, MeshDependencies, MeshLayer, MeshStats, SurfaceKey, TileId,
    VoxelWorld,
};
use rayengine::{
    assets::Assets,
    prelude::*,
    raylib::prelude::{RaylibDraw, RaylibDraw3D},
    render::Canvas3D,
};
use std::collections::{HashMap, HashSet};
pub use streaming::{StreamRenderConfig, StreamRenderReport, StreamRenderer, StreamResources};

/// GLSL 330 fragment shader used by built-in materials. Custom shaders can reuse
/// its per-tile repetition, unwrapped texture gradients and SDK alpha uniforms.
pub const REPEAT_FRAGMENT_SHADER: &str = include_str!("render/repeat.fs");

/// A game-owned image (or atlas region) for one tile key.
#[derive(Clone, Copy, Debug)]
pub struct TileTexture {
    /// Game tile key.
    pub tile: TileId,
    /// Borrowed SDK texture; unloading materials never unloads it.
    pub texture: TextureId,
    /// Normalized `(left, top, width, height)` within the texture.
    /// For filtering/mipmaps, the caller must provide atlas gutters/insets.
    pub rect: Vec4,
}
impl TileTexture {
    /// Uses the whole image, repeating once per block on merged geometry.
    pub fn whole(tile: TileId, texture: TextureId) -> Self {
        Self {
            tile,
            texture,
            rect: Vec4::new(0.0, 0.0, 1.0, 1.0),
        }
    }
    fn valid(self) -> bool {
        self.rect.is_finite()
            && self.rect.x >= 0.0
            && self.rect.y >= 0.0
            && self.rect.z > 0.0
            && self.rect.w > 0.0
            && self.rect.x + self.rect.z <= 1.0
            && self.rect.y + self.rect.w <= 1.0
    }
}

/// Material lookup with explicitly owned built-in materials and repeat shader.
/// Textures and custom bindings are borrowed. Keep this and its textures alive
/// while drawing chunks; unload chunks before unloading materials. Not Clone.
#[derive(Default)]
pub struct VoxelMaterials {
    surfaces: HashMap<SurfaceKey, MaterialId>,
    owned: Vec<MaterialId>,
    shader: Option<ShaderId>,
}
impl VoxelMaterials {
    /// Empty mapping for caller-owned custom materials via bind.
    pub fn new() -> Self {
        Self::default()
    }
    /// Creates one shared shader and opaque/cutout materials for each tile.
    /// Validates tiles before native creation and rolls back on later failure.
    /// Zero tiles is valid and allocates no shader. Cutoff must be finite in 0..=1.
    /// The repeat shader uses fract(UV), so greedy faces preserve texel density
    /// even with clamp wrapping or an atlas. Nearest filtering is recommended;
    /// automatic mipmap generation/atlas packing is outside this adapter.
    pub fn create(
        context: &mut InitContext<'_, '_>,
        tiles: &[TileTexture],
        cutoff: f32,
    ) -> Result<Self, Error> {
        if !cutoff.is_finite() || !(0.0..=1.0).contains(&cutoff) || tiles.len() > 65536 {
            return Err(Error::Config("invalid voxel tiles or alpha cutoff".into()));
        }
        let mut result = Self::new();
        result
            .surfaces
            .try_reserve(tiles.len() * 2)
            .map_err(|_| allocation())?;
        result
            .owned
            .try_reserve_exact(tiles.len() * 2)
            .map_err(|_| allocation())?;
        let mut seen = HashSet::new();
        seen.try_reserve(tiles.len()).map_err(|_| allocation())?;
        for tile in tiles {
            if !tile.valid() || context.assets.texture(tile.texture).is_none() {
                return Err(Error::Config(
                    "voxel tile needs a valid rectangle and live texture".into(),
                ));
            }
            if !seen.insert(tile.tile) {
                return Err(Error::Config("duplicate voxel tile key".into()));
            }
        }
        if tiles.is_empty() {
            return Ok(result);
        }
        let setup = (|| {
            let shader = context.shader_from_source(None, REPEAT_FRAGMENT_SHADER)?;
            result.shader = Some(shader);
            let rect = context.uniform(
                shader,
                "voxelTileRect",
                UniformValue::Vec4(Vec4::new(0.0, 0.0, 1.0, 1.0)),
            )?;
            for tile in tiles {
                for (layer, alpha) in [
                    (MeshLayer::Opaque, AlphaMode::Opaque),
                    (MeshLayer::Cutout, AlphaMode::Cutout(cutoff)),
                ] {
                    let material = context.material(MaterialDesc {
                        texture: Some(tile.texture),
                        shader: Some(shader),
                        alpha,
                        parameters: vec![MaterialParam {
                            uniform: rect,
                            value: UniformValue::Vec4(tile.rect),
                        }],
                        ..Default::default()
                    })?;
                    result.owned.push(material);
                    result.surfaces.insert(
                        SurfaceKey {
                            tile: tile.tile,
                            layer,
                        },
                        material,
                    );
                }
            }
            Ok::<_, Error>(())
        })();
        if let Err(error) = setup {
            result.unload(context.assets);
            return Err(error);
        }
        Ok(result)
    }
    /// Borrowed custom surface binding. The caller owns its material and must
    /// provide repeating UV semantics when using greedy geometry. Returns the
    /// previous binding; alpha/dependency validity is checked during chunk upload.
    pub fn bind(
        &mut self,
        surface: SurfaceKey,
        material: MaterialId,
    ) -> Result<Option<MaterialId>, Error> {
        self.surfaces.try_reserve(1).map_err(|_| allocation())?;
        Ok(self.surfaces.insert(surface, material))
    }
    /// Current binding, or None for an undefined tile/layer.
    pub fn surface(&self, surface: SurfaceKey) -> Option<MaterialId> {
        self.surfaces.get(&surface).copied()
    }
    /// Releases only internally created descriptions/shader; textures and custom
    /// materials stay in Assets. Repeated unload is harmless. Drop alone leaves
    /// resources in Assets until the run ends, following the SDK plugin contract.
    pub fn unload(&mut self, assets: &mut Assets<'_>) {
        for id in self.owned.drain(..) {
            assets.unload_material(id);
        }
        if let Some(shader) = self.shader.take() {
            assets.unload_shader(shader);
        }
        self.surfaces.clear();
    }
}
fn allocation() -> Error {
    Error::Asset("voxel render allocation failed".into())
}

struct GpuBatch {
    mesh: MeshId,
    material: MaterialId,
}
/// Successful submissions and missing GPU/material dependencies for one chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkDraw {
    /// Frustum rejected the chunk before any draw calls.
    pub culled: bool,
    /// Successful mesh submissions (not hardware draw calls).
    pub submitted: usize,
    /// Failed draws due to externally unloaded resources.
    pub unavailable: usize,
}
/// Owned uploaded batches for a single chunk, including its dependency receipt.
/// Game/streamer owns the collection and replacement scheduling. Not Clone.
pub struct RenderedChunk {
    position: ChunkPos,
    batches: Vec<GpuBatch>,
    dependencies: Option<MeshDependencies>,
    stats: MeshStats,
}
impl RenderedChunk {
    /// World chunk coordinate; local vertices are translated only during drawing.
    pub fn position(&self) -> ChunkPos {
        self.position
    }
    /// Creates an empty handle with no GPU allocation; validates the signed grid.
    pub fn new(position: ChunkPos) -> Result<Self, crate::VoxelError> {
        position.origin()?;
        Ok(Self {
            position,
            batches: Vec::new(),
            dependencies: None,
            stats: MeshStats::default(),
        })
    }
    /// Accepted receipt; None before upload/after unload.
    pub fn dependencies(&self) -> Option<&MeshDependencies> {
        self.dependencies.as_ref()
    }
    /// Installed geometry counts and logical buffer bytes.
    pub fn stats(&self) -> MeshStats {
        self.stats
    }
    /// Uploads during game initialization. See replace for transaction semantics.
    pub fn upload_init(
        &mut self,
        world: &VoxelWorld,
        mesh: &ChunkMesh,
        materials: &VoxelMaterials,
        context: &mut InitContext<'_, '_>,
    ) -> Result<(), Error> {
        self.install(world, mesh, materials, context)
    }
    /// Accepts only current owner/neighbor receipts, validates every material,
    /// uploads all new batches, then commits and releases old batches. Any error
    /// releases partial new uploads and preserves all old handles/geometry.
    /// Empty results deliberately remove old geometry. Old/new buffers coexist
    /// during admission; scheduling/time/VRAM budgets belong to the next streamer.
    /// Call before a camera pass, on the same run's render thread.
    pub fn replace(
        &mut self,
        world: &VoxelWorld,
        mesh: &ChunkMesh,
        materials: &VoxelMaterials,
        frame: &mut Frame<'_, '_>,
    ) -> Result<(), Error> {
        self.install(world, mesh, materials, frame)
    }
    fn install<'audio>(
        &mut self,
        world: &VoxelWorld,
        mesh: &ChunkMesh,
        materials: &VoxelMaterials,
        sink: &mut impl MeshSink<'audio>,
    ) -> Result<(), Error> {
        if mesh.dependencies().position() != self.position || !mesh.dependencies().is_current(world)
        {
            return Err(Error::Asset("stale or mismatched voxel mesh result".into()));
        }
        let mut pending: Vec<GpuBatch> = Vec::new();
        pending
            .try_reserve_exact(mesh.batches().len())
            .map_err(|_| allocation())?;
        for batch in mesh.batches() {
            let key = batch.surface();
            let id = materials
                .surface(key)
                .ok_or_else(|| Error::Asset(format!("undefined voxel surface {key:?}")))?;
            let desc = sink
                .assets()
                .material(id)
                .ok_or_else(|| Error::Asset("voxel material is unloaded".into()))?;
            if !matches!(
                (key.layer, desc.alpha),
                (MeshLayer::Opaque, AlphaMode::Opaque) | (MeshLayer::Cutout, AlphaMode::Cutout(_))
            ) {
                return Err(Error::Asset(
                    "voxel material alpha policy does not match mesh layer".into(),
                ));
            }
            sink.assets().validate_material(desc)?;
        }
        for batch in mesh.batches() {
            match sink.upload(batch.data()) {
                Ok(id) => pending.push(GpuBatch {
                    mesh: id,
                    material: materials.surface(batch.surface()).unwrap(),
                }),
                Err(error) => {
                    for batch in pending {
                        sink.assets_mut().unload_mesh(batch.mesh);
                    }
                    return Err(error);
                }
            }
        }
        for batch in std::mem::replace(&mut self.batches, pending) {
            sink.assets_mut().unload_mesh(batch.mesh);
        }
        self.dependencies = Some(mesh.dependencies().clone());
        self.stats = mesh.stats();
        Ok(())
    }
    /// Draws local vertices at a camera-relative translation. Use a frustum
    /// captured from the same relative camera/viewport; no allocation or world
    /// lookup occurs here. Missing dependencies are reported rather than hidden.
    pub fn draw<D: RaylibDraw + RaylibDraw3D>(
        &self,
        canvas: &mut Canvas3D<'_, D>,
        view: &Frustum3D,
        render_origin: BlockPos,
    ) -> ChunkDraw {
        if !view.intersects(chunk_bounds(self.position, render_origin)) {
            return ChunkDraw {
                culled: true,
                ..Default::default()
            };
        }
        let transform = Transform3D::at(chunk_translation(self.position, render_origin));
        let mut report = ChunkDraw::default();
        for batch in &self.batches {
            if canvas.mesh_material(batch.mesh, batch.material, transform, Color::WHITE) {
                report.submitted += 1;
            } else {
                report.unavailable += 1;
            }
        }
        report
    }
    /// Releases owned mesh batches, preserving game-owned materials and textures.
    pub fn unload(&mut self, assets: &mut Assets<'_>) {
        for batch in self.batches.drain(..) {
            assets.unload_mesh(batch.mesh);
        }
        self.dependencies = None;
        self.stats = MeshStats::default();
    }
}
trait MeshSink<'audio> {
    fn upload(&mut self, data: &MeshData) -> Result<MeshId, Error>;
    fn assets(&self) -> &Assets<'audio>;
    fn assets_mut(&mut self) -> &mut Assets<'audio>;
}
impl<'audio> MeshSink<'audio> for InitContext<'_, 'audio> {
    fn upload(&mut self, data: &MeshData) -> Result<MeshId, Error> {
        self.mesh(data)
    }
    fn assets(&self) -> &Assets<'audio> {
        self.assets
    }
    fn assets_mut(&mut self) -> &mut Assets<'audio> {
        self.assets
    }
}
impl<'audio> MeshSink<'audio> for Frame<'_, 'audio> {
    fn upload(&mut self, data: &MeshData) -> Result<MeshId, Error> {
        self.mesh(data)
    }
    fn assets(&self) -> &Assets<'audio> {
        self.assets
    }
    fn assets_mut(&mut self) -> &mut Assets<'audio> {
        self.assets
    }
}

#[cfg(test)]
mod native_tests;
