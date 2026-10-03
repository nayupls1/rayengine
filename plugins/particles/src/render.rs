//! Optional SDK adapters. GPU handles belong to one run and must be initialized
//! and unloaded on its render thread. Textures are borrowed from game-owned assets.

use crate::Emitter;
use rayengine::{
    assets::Assets,
    prelude::*,
    raylib::prelude::{RaylibDraw, RaylibDraw3D},
    render::{Canvas2D, Canvas3D},
};
use rayengine_core::glam::{Mat4, Vec4};

/// Camera chosen by the game for the `Plugin` drawing hook.
#[derive(Clone, Copy, Debug)]
pub enum ParticleView {
    /// Sprites in XY world coordinates.
    TwoD(Camera2D),
    /// Camera-facing square billboards in XYZ world coordinates.
    ThreeD(Camera3D),
}

/// Borrowed sprite texture and optional atlas region, shared by both adapters.
#[derive(Clone, Copy, Debug)]
pub struct ParticleSprite {
    /// Game-owned texture; the effect never unloads it.
    pub texture: TextureId,
    /// Optional pixel region. None samples the whole texture.
    pub region: Option<SpriteRegion>,
}
impl From<TextureId> for ParticleSprite {
    fn from(texture: TextureId) -> Self {
        Self {
            texture,
            region: None,
        }
    }
}

/// Game-owned particle plugin, with independently configured simulation.
///
/// Owns one reusable quad and one straight-alpha material, and borrows an optional
/// texture handle and atlas region. Unload frees only the quad/material and resets CPU state;
/// dropping alone leaves those GPU handles in the run's asset collection. Keep an
/// initialized instance paired with its original run. No ECS entities are created.
pub struct ParticleEffect {
    emitter: Emitter,
    texture: Option<ParticleSprite>,
    region: Option<SpriteRegion>,
    mesh: Option<MeshId>,
    material: Option<MaterialId>,
    order: Vec<usize>,
}
impl ParticleEffect {
    /// Wraps CPU state. None selects an untextured square in both adapters.
    /// Renderer ordering storage is reserved fallibly; no draw-time allocation.
    pub fn new(emitter: Emitter, texture: Option<ParticleSprite>) -> Result<Self, Error> {
        let mut order = Vec::new();
        order
            .try_reserve_exact(emitter.config().capacity)
            .map_err(|_| Error::Config("particle ordering allocation failed".into()))?;
        Ok(Self {
            emitter,
            texture,
            region: None,
            mesh: None,
            material: None,
            order,
        })
    }
    /// Read-only simulation state.
    pub fn emitter(&self) -> &Emitter {
        &self.emitter
    }
    /// Requests immediate births, returning the admitted count.
    pub fn burst(&mut self, count: usize) -> usize {
        self.emitter.burst(count)
    }
    /// Stops births while existing particles continue to age.
    pub fn stop(&mut self) {
        self.emitter.stop();
    }
    /// Resumes births.
    pub fn start(&mut self) {
        self.emitter.start();
    }
    /// Clears CPU particles and restores seeded variation/emission phase.
    pub fn reset(&mut self) {
        self.emitter.reset();
    }
    /// Moves future births without moving live particles.
    pub fn set_position(&mut self, position: Vec3) -> Result<(), crate::ParticleError> {
        self.emitter.set_position(position)
    }
    /// Explicit CPU step, also useful for collapsing interpolation when paused.
    pub fn step(&mut self, dt: f32) -> Result<usize, crate::ParticleError> {
        self.emitter.step(dt)
    }
    /// Draws sprites oldest to newest in a caller-owned 2D pass. XY is used;
    /// Z is ignored. Stale textures skip drawing. Straight-alpha compositing uses
    /// the SDK's scoped blend state, preserving destination alpha. Returns successful particle submissions.
    pub fn draw_2d<D: RaylibDraw>(&self, canvas: &mut Canvas2D<'_, D>, alpha: f32) -> usize {
        if self.mesh.is_none() || self.material.is_none() {
            return 0;
        }
        canvas
            .with_alpha_blend(|canvas| {
                let mut drawn = 0;
                for particle in self.emitter.particles() {
                    let appearance = self.emitter.appearance(particle, alpha);
                    if appearance.size == 0.0 || appearance.color.w == 0.0 {
                        continue;
                    }
                    let center = particle.interpolated_position(alpha).truncate();
                    let bounds = Aabb2::from_center(center, Vec2::splat(appearance.size));
                    let tint = color(appearance.color);
                    if let Some(texture) = self.texture {
                        drawn += usize::from(canvas.sprite(
                            texture.texture,
                            self.region.expect("initialized texture region"),
                            SpriteTransform {
                                position: center,
                                size: Vec2::splat(appearance.size),
                                origin: Vec2::splat(appearance.size * 0.5),
                                ..Default::default()
                            },
                            tint,
                        ));
                    } else {
                        canvas.rectangle(bounds, tint);
                        drawn += 1;
                    }
                }
                drawn
            })
            .unwrap_or(0)
    }
    /// Draws billboards far to near by camera-space depth, with stable birth-order
    /// ties. Call after opaque/cutout geometry. Depth testing remains enabled and
    /// blended particles do not write depth. Sorting uses reserved storage and an
    /// allocation-free unstable sort with an explicit birth-index tie break.
    /// Cross-emitter ordering belongs to the game. Returns successful submissions.
    /// Invalid/degenerate camera bases skip this emitter.
    pub fn draw_3d<D: RaylibDraw + RaylibDraw3D>(
        &mut self,
        canvas: &mut Canvas3D<'_, D>,
        camera: Camera3D,
        alpha: f32,
    ) -> usize {
        let (Some(mesh), Some(material)) = (self.mesh, self.material) else {
            return 0;
        };
        let Some((right, up, forward)) = camera_basis(camera) else {
            return 0;
        };
        self.order.clear();
        self.order.extend(0..self.emitter.len());
        let particles = self.emitter.particles();
        let depth = |index: usize| {
            (particles[index].interpolated_position(alpha).as_dvec3() - camera.position.as_dvec3())
                .dot(forward.as_dvec3())
        };
        self.order
            .sort_unstable_by(|a, b| depth(*b).total_cmp(&depth(*a)).then_with(|| a.cmp(b)));
        let mut drawn = 0;
        for &index in &self.order {
            let particle = &particles[index];
            let appearance = self.emitter.appearance(particle, alpha);
            if appearance.size == 0.0 || appearance.color.w == 0.0 {
                continue;
            }
            let transform = Mat4::from_cols(
                (right * appearance.size).extend(0.0),
                (up * appearance.size).extend(0.0),
                (-forward).extend(0.0),
                particle.interpolated_position(alpha).extend(1.0),
            );
            drawn += usize::from(canvas.mesh_material_matrix(
                mesh,
                material,
                transform,
                color(appearance.color),
            ));
        }
        drawn
    }
}
fn camera_basis(camera: Camera3D) -> Option<(Vec3, Vec3, Vec3)> {
    if !camera.position.is_finite() || !camera.target.is_finite() || !camera.up.is_finite() {
        return None;
    }
    let forward = (camera.target.as_dvec3() - camera.position.as_dvec3())
        .try_normalize()?
        .as_vec3();
    let right = forward
        .as_dvec3()
        .cross(camera.up.as_dvec3())
        .try_normalize()?
        .as_vec3();
    let up = right.cross(forward).try_normalize()?;
    Some((right, up, forward))
}
fn color(value: Vec4) -> Color {
    Color::new(
        (value.x * 255.0).round() as u8,
        (value.y * 255.0).round() as u8,
        (value.z * 255.0).round() as u8,
        (value.w * 255.0).round() as u8,
    )
}
impl Plugin<ParticleView> for ParticleEffect {
    fn init(
        &mut self,
        _: &mut ParticleView,
        context: &mut InitContext<'_, '_>,
    ) -> Result<(), Error> {
        if self.mesh.is_some() || self.material.is_some() {
            return Err(Error::Config(
                "unload a particle effect before reinitializing".into(),
            ));
        }
        let mut geometry = quad();
        let region = if let Some(sprite) = self.texture {
            let texture = context
                .assets
                .texture(sprite.texture)
                .ok_or_else(|| Error::Asset("particle texture is unloaded".into()))?;
            let region = match sprite.region {
                Some(region) => region,
                None => SpriteRegion::new(0, 0, texture.width as u32, texture.height as u32)
                    .map_err(|e| Error::Asset(e.to_string()))?,
            };
            if !region.fits(texture.width as u32, texture.height as u32) {
                return Err(Error::Asset(
                    "particle sprite region exceeds texture bounds".into(),
                ));
            }
            let origin = Vec2::new(region.x() as f32, region.y() as f32);
            let size = Vec2::new(region.width() as f32, region.height() as f32);
            let dimensions = Vec2::new(texture.width as f32, texture.height as f32);
            for uv in geometry.texcoords.as_mut().expect("quad UVs") {
                *uv = (origin + *uv * size) / dimensions;
            }
            Some(region)
        } else {
            None
        };
        // Create material first so a stale texture cannot leave a mesh behind.
        let material = context.material(MaterialDesc {
            texture: self.texture.map(|sprite| sprite.texture),
            alpha: AlphaMode::Blend,
            ..Default::default()
        })?;
        let mesh = match context.mesh(&geometry) {
            Ok(mesh) => mesh,
            Err(error) => {
                context.assets.unload_material(material);
                return Err(error);
            }
        };
        self.region = region;
        self.material = Some(material);
        self.mesh = Some(mesh);
        Ok(())
    }
    fn fixed_update(&mut self, _: &mut ParticleView, context: &mut Update<'_, '_>) {
        if self.mesh.is_some() {
            self.emitter
                .step(context.tick.dt)
                .expect("SDK fixed dt must be finite and nonnegative");
        }
    }
    fn draw(&mut self, view: &ParticleView, frame: &mut Frame<'_, '_>) {
        let alpha = frame.alpha;
        match *view {
            ParticleView::TwoD(camera) => frame.world_2d(camera, |canvas| {
                self.draw_2d(canvas, alpha);
            }),
            ParticleView::ThreeD(camera) => frame.world_3d(camera, |canvas| {
                self.draw_3d(canvas, camera, alpha);
            }),
        }
    }
    fn unload(&mut self, _: &mut ParticleView, assets: &mut Assets<'_>) {
        if let Some(mesh) = self.mesh.take() {
            assets.unload_mesh(mesh);
        }
        if let Some(material) = self.material.take() {
            assets.unload_material(material);
        }
        self.order.clear();
        self.region = None;
        self.emitter.reset();
    }
}
fn quad() -> MeshData {
    let mut data = MeshData::new(vec![
        Vec3::new(-0.5, -0.5, 0.0),
        Vec3::new(0.5, -0.5, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(-0.5, -0.5, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(-0.5, 0.5, 0.0),
    ]);
    data.texcoords = Some(vec![
        Vec2::new(0.0, 1.0),
        Vec2::ONE,
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::ZERO,
    ]);
    data
}

#[cfg(test)]
mod native_tests;
