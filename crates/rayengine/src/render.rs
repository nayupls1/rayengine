//! Immediate drawing with corresponding 2D/3D passes and shared logical UI.

use crate::diagnostics::DrawCounters;
use crate::{
    Error,
    assets::{
        Assets, MaterialId, MeshId, ModelId, ShaderId, TextureId,
        materials::{Prepared, SurfaceGuard},
    },
    material::{MaterialDesc, UniformId, UniformValue},
};
use rayengine_core::{
    camera::{Camera2D, Camera3D},
    collision::{Aabb2, Aabb3},
    glam::{Mat4, Vec2, Vec3},
    mesh::MeshData,
    transform::Transform3D,
    ui::UiResponse,
    viewport::Viewport,
};
use raylib::prelude::*;

macro_rules! count {
    ($counts:expr, $field:ident, $amount:expr) => {
        if let Some(counters) = $counts.as_mut() {
            counters.$field = counters.$field.saturating_add($amount);
        }
    };
}

/// Concrete offscreen raylib drawing guard, available for advanced passes.
pub type TargetDraw<'draw, 'target> = RaylibTextureMode<'draw, 'target, RaylibHandle>;

/// One render frame. Game code chooses passes; the engine owns presentation.
pub struct Frame<'frame, 'audio> {
    pub(crate) counters: Option<DrawCounters>,
    pub(crate) raylib: &'frame mut RaylibHandle,
    pub(crate) thread: &'frame RaylibThread,
    pub(crate) target: &'frame mut RenderTexture2D,
    /// Assets available on the render thread, including explicit unloading.
    pub assets: &'frame mut Assets<'audio>,
    /// Current viewport in logical window coordinates.
    pub viewport: Viewport,
    /// Interpolation fraction between previous/current simulation state.
    pub alpha: f32,
    /// Zero-based render frame index.
    pub index: u64,
}

impl Frame<'_, '_> {
    /// Enables/disables submission counters for this frame. Runtime diagnostics
    /// enable them automatically; changing mode resets counters. Useful for
    /// identical enabled/disabled native benchmark workloads.
    pub fn set_draw_counters_enabled(&mut self, enabled: bool) {
        if enabled != self.counters.is_some() {
            self.counters = enabled.then(DrawCounters::default);
        }
    }
    /// Current successful SDK submissions, or None when counting is disabled.
    pub fn draw_counters(&self) -> Option<DrawCounters> {
        self.counters
    }

    /// Processes bounded mesh uploads before a drawing pass. The game decides
    /// whether each tag/revision is current and receives success/failure/stale outcomes.
    pub fn upload_meshes<K>(
        &mut self,
        queue: &mut crate::upload::MeshUploadQueue<K>,
        budget: crate::upload::UploadBudget,
        is_current: impl FnMut(&K, u64) -> bool,
        on_result: impl FnMut(crate::upload::MeshUploadResult<K>),
    ) -> crate::upload::UploadReport {
        queue.process(
            budget,
            is_current,
            |target, data| match target {
                crate::upload::MeshUploadTarget::Create => self.mesh(data),
                crate::upload::MeshUploadTarget::Replace(id) => {
                    self.replace_mesh(id, data).map(|()| id)
                }
            },
            on_result,
        )
    }

    /// Creates a material before entering a drawing pass.
    pub fn material(&mut self, desc: MaterialDesc) -> Result<MaterialId, Error> {
        self.assets.create_material(self.raylib, self.thread, desc)
    }
    /// Compiles shader source before entering a drawing pass; errors never use a fallback.
    pub fn shader_from_source(
        &mut self,
        vertex: Option<&str>,
        fragment: &str,
    ) -> Result<ShaderId, Error> {
        self.assets
            .shader_source(self.raylib, self.thread, vertex, fragment)
    }

    /// Loads/caches shader files before a pass; None uses the standard vertex shader.
    pub fn shader(
        &mut self,
        vertex: Option<&std::path::Path>,
        fragment: impl AsRef<std::path::Path>,
    ) -> Result<ShaderId, Error> {
        self.assets
            .load_shader(self.raylib, self.thread, vertex, fragment.as_ref())
    }
    /// Registers a cached uniform binding and its shader-wide default value.
    pub fn uniform(
        &mut self,
        shader: ShaderId,
        name: &str,
        initial: UniformValue,
    ) -> Result<UniformId, Error> {
        self.assets.uniform(shader, name, initial)
    }
    /// Validates and uploads CPU geometry before entering a drawing pass.
    ///
    /// Upload only when geometry changes. This allocates GPU resources and copies
    /// vertex data; it is not a per-frame drawing operation.
    pub fn mesh(&mut self, data: &MeshData) -> Result<MeshId, Error> {
        self.assets.upload_mesh(self.raylib, self.thread, data)
    }

    /// Uploads a complete replacement while keeping the same handle.
    ///
    /// Failure leaves the previous geometry intact. The new and old GPU buffers
    /// temporarily coexist. Partial buffer updates are not implemented.
    pub fn replace_mesh(&mut self, id: MeshId, data: &MeshData) -> Result<(), Error> {
        self.assets.replace_mesh(self.thread, id, data)
    }

    /// Clears color and depth at the start of the game frame.
    pub fn clear(&mut self, color: Color) {
        count!(self.counters, clears, 1);
        self.raylib
            .begin_texture_mode(self.thread, self.target)
            .clear_background(color);
    }

    /// Draws a 2D pass in world units, maintaining the camera's visible height.
    pub fn world_2d(
        &mut self,
        camera: Camera2D,
        draw: impl FnOnce(&mut Canvas2D<'_, RaylibMode2D<'_, TargetDraw<'_, '_>>>),
    ) {
        count!(self.counters, world_2d_passes, 1);
        let size = Vec2::new(
            self.target.texture().width as f32,
            self.target.texture().height as f32,
        );
        let camera = raylib::prelude::Camera2D {
            target: v2(camera.target),
            offset: v2(size * 0.5),
            rotation: camera.rotation.to_degrees(),
            zoom: size.y / camera.view_height,
        };
        let mut target = self.raylib.begin_texture_mode(self.thread, self.target);
        let mut raw = target.begin_mode2D(camera);
        draw(&mut Canvas2D {
            raw: &mut raw,
            textures: self.assets,
            counters: &mut self.counters,
        });
    }

    /// Draws a perspective 3D pass using the render target's preserved aspect ratio.
    pub fn world_3d(
        &mut self,
        camera: Camera3D,
        draw: impl FnOnce(&mut Canvas3D<'_, RaylibMode3D<'_, TargetDraw<'_, '_>>>),
    ) {
        count!(self.counters, world_3d_passes, 1);
        let camera = raylib::prelude::Camera3D {
            position: v3(camera.position),
            target: v3(camera.target),
            up: v3(camera.up),
            fovy: camera.vertical_fov,
            projection: CameraProjection::CAMERA_PERSPECTIVE,
        };
        let mut target = self.raylib.begin_texture_mode(self.thread, self.target);
        let mut raw = target.begin_mode3D(camera);
        let surface = self.assets.material_pass();
        draw(&mut Canvas3D {
            raw: &mut raw,
            models: self.assets,
            surface,
            counters: &mut self.counters,
        });
    }

    /// Draws screen UI in reference units, scaled independently from the world camera.
    pub fn ui(&mut self, draw: impl FnOnce(&mut UiCanvas<'_, TargetDraw<'_, '_>>)) {
        count!(self.counters, ui_passes, 1);
        let pixels = Vec2::new(
            self.target.texture().width as f32,
            self.target.texture().height as f32,
        );
        let scale = pixels / self.viewport.logical_size;
        let font = self.raylib.get_font_default();
        let mut raw = self.raylib.begin_texture_mode(self.thread, self.target);
        draw(&mut UiCanvas {
            raw: &mut raw,
            scale,
            logical_size: self.viewport.logical_size,
            font,
            textures: self.assets,
            counters: &mut self.counters,
        });
    }

    /// Direct raylib texture pass for shaders or drawing beyond the SDK primitives.
    pub fn with_raylib(&mut self, draw: impl FnOnce(&mut TargetDraw<'_, '_>)) {
        count!(self.counters, raw_passes, 1);
        let mut raw = self.raylib.begin_texture_mode(self.thread, self.target);
        draw(&mut raw);
    }
}

/// Immediate 2D primitives. No command buffer or allocation is introduced.
pub struct Canvas2D<'draw, D: RaylibDraw> {
    counters: &'draw mut Option<DrawCounters>,
    /// Raylib guard for advanced drawing within this camera pass.
    pub raw: &'draw mut D,
    textures: &'draw dyn TextureSource,
}

trait TextureSource {
    fn texture(&self, id: TextureId) -> Option<&Texture2D>;
}
impl TextureSource for Assets<'_> {
    fn texture(&self, id: TextureId) -> Option<&Texture2D> {
        self.texture(id)
    }
}

impl<D: RaylibDraw> Canvas2D<'_, D> {
    /// Filled world-space rectangle.
    pub fn rectangle(&mut self, bounds: Aabb2, color: Color) {
        count!(self.counters, primitives_2d, 1);
        self.raw.draw_rectangle_rec(rect(bounds), color);
    }
    /// Filled world-space circle.
    pub fn circle(&mut self, center: Vec2, radius: f32, color: Color) {
        count!(self.counters, primitives_2d, 1);
        self.raw.draw_circle_v(v2(center), radius, color);
    }
    /// World-space line with explicit width.
    pub fn line(&mut self, start: Vec2, end: Vec2, width: f32, color: Color) {
        count!(self.counters, primitives_2d, 1);
        self.raw.draw_line_ex(v2(start), v2(end), width, color);
    }
    /// Draws a texture into world-space bounds; false for an unloaded handle.
    pub fn texture(&mut self, id: TextureId, bounds: Aabb2, tint: Color) -> bool {
        if let Some(texture) = self.textures.texture(id) {
            self.raw.draw_texture_pro(
                texture,
                Rectangle::new(0.0, 0.0, texture.width as f32, texture.height as f32),
                rect(bounds),
                Vector2::zero(),
                0.0,
                tint,
            );
            count!(self.counters, textures, 1);
            true
        } else {
            false
        }
    }
}

/// Immediate 3D primitives corresponding to the 2D drawing API.
pub struct Canvas3D<'draw, D: RaylibDraw> {
    counters: &'draw mut Option<DrawCounters>,
    /// Raylib guard for advanced drawing within this camera pass.
    pub raw: &'draw mut D,
    models: &'draw mut dyn ModelSource,
    surface: Option<SurfaceGuard>,
}

trait ModelSource {
    fn model(&self, id: ModelId) -> Option<&Model>;
    fn mesh(&mut self, id: MeshId, tint: Color) -> Option<(&Mesh, WeakMaterial)>;
    fn mesh_material(
        &mut self,
        mesh: MeshId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Mesh, Prepared<'_>)>;
    fn model_material(
        &mut self,
        model: ModelId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Model, Prepared<'_>)>;
}
impl ModelSource for Assets<'_> {
    fn model(&self, id: ModelId) -> Option<&Model> {
        self.model(id)
    }
    fn mesh(&mut self, id: MeshId, tint: Color) -> Option<(&Mesh, WeakMaterial)> {
        self.mesh_for_draw(id, tint)
    }
    fn mesh_material(
        &mut self,
        mesh: MeshId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Mesh, Prepared<'_>)> {
        self.mesh_material(mesh, material, tint)
    }
    fn model_material(
        &mut self,
        model: ModelId,
        material: MaterialId,
        tint: Color,
    ) -> Option<(&Model, Prepared<'_>)> {
        self.model_material(model, material, tint)
    }
}

impl<D: RaylibDraw + RaylibDraw3D> Canvas3D<'_, D> {
    fn legacy(&mut self) {
        if let Some(surface) = &mut self.surface {
            surface.legacy();
        }
    }
    /// Draws generated geometry with a reusable material; false for stale dependencies.
    /// Draw opaque/cutout surfaces first, then blended surfaces from far to near.
    pub fn mesh_material(
        &mut self,
        mesh: MeshId,
        material: MaterialId,
        transform: Transform3D,
        tint: Color,
    ) -> bool {
        self.mesh_material_matrix(mesh, material, transform.matrix(), tint)
    }
    /// Material drawing with an affine scene/world matrix. No command buffer or heap allocation.
    pub fn mesh_material_matrix(
        &mut self,
        mesh: MeshId,
        material: MaterialId,
        transform: Mat4,
        tint: Color,
    ) -> bool {
        if let Some((mesh, material)) = self.models.mesh_material(mesh, material, tint) {
            if let Some(surface) = &mut self.surface {
                surface.apply(material.alpha);
            }
            material.draw(self.raw, mesh, matrix(transform));
            count!(self.counters, meshes, 1);
            true
        } else {
            false
        }
    }
    /// Overrides every mesh of an imported model with this material, applying its native transform.
    /// Returns false for an unloaded model, material, shader, or texture.
    pub fn model_material(
        &mut self,
        model: ModelId,
        material: MaterialId,
        transform: Transform3D,
        tint: Color,
    ) -> bool {
        self.model_material_matrix(model, material, transform.matrix(), tint)
    }
    /// Model material override with an affine scene/world matrix.
    pub fn model_material_matrix(
        &mut self,
        model: ModelId,
        material: MaterialId,
        transform: Mat4,
        tint: Color,
    ) -> bool {
        if let Some((model, material)) = self.models.model_material(model, material, tint) {
            if let Some(surface) = &mut self.surface {
                surface.apply(material.alpha);
            }
            let m = model.transform;
            let local = Mat4::from_cols_array(&[
                m.m0, m.m1, m.m2, m.m3, m.m4, m.m5, m.m6, m.m7, m.m8, m.m9, m.m10, m.m11, m.m12,
                m.m13, m.m14, m.m15,
            ]);
            let transform = matrix(transform * local);
            count!(self.counters, models, 1);
            count!(self.counters, meshes, model.meshes().len() as u64);
            for mesh in model.meshes() {
                material.draw(self.raw, mesh, transform);
            }
            true
        } else {
            false
        }
    }
    /// Draws generated geometry with translation, rotation, scale, and tint.
    ///
    /// Uses the default unlit material and multiplies tint by vertex colors.
    /// Returns false for an unloaded handle. Use mesh_material for custom
    /// textures, shader parameters, and explicit alpha/depth policies.
    pub fn mesh(&mut self, id: MeshId, transform: Transform3D, tint: Color) -> bool {
        self.mesh_matrix(id, transform.matrix(), tint)
    }

    /// Draws generated geometry with an affine matrix, including scene transforms.
    ///
    /// Pass `GlobalTransform3D.0` to include scene ancestors. No allocation occurs.
    pub fn mesh_matrix(&mut self, id: MeshId, transform: Mat4, tint: Color) -> bool {
        self.legacy();
        if let Some((mesh, material)) = self.models.mesh(id, tint) {
            self.raw.draw_mesh(mesh, material, matrix(transform));
            count!(self.counters, meshes, 1);
            true
        } else {
            false
        }
    }

    /// Filled world-space box.
    pub fn cube(&mut self, bounds: Aabb3, color: Color) {
        count!(self.counters, primitives_3d, 1);
        self.legacy();
        self.raw
            .draw_cube_v(v3(bounds.center()), v3(bounds.size()), color);
    }
    /// Box wireframe.
    pub fn wire_cube(&mut self, bounds: Aabb3, color: Color) {
        count!(self.counters, primitives_3d, 1);
        self.legacy();
        self.raw
            .draw_cube_wires_v(v3(bounds.center()), v3(bounds.size()), color);
    }
    /// Filled sphere with fixed low polygon count.
    pub fn sphere(&mut self, center: Vec3, radius: f32, color: Color) {
        count!(self.counters, primitives_3d, 1);
        self.legacy();
        self.raw.draw_sphere_ex(v3(center), radius, 12, 16, color);
    }
    /// World-space line.
    pub fn line(&mut self, start: Vec3, end: Vec3, color: Color) {
        count!(self.counters, primitives_3d, 1);
        self.legacy();
        self.raw.draw_line3D(v3(start), v3(end), color);
    }
    /// Draws a model with uniform scale; false for an unloaded handle.
    pub fn model(&mut self, id: ModelId, position: Vec3, scale: f32, tint: Color) -> bool {
        self.legacy();
        if let Some(model) = self.models.model(id) {
            self.raw.draw_model(model, v3(position), scale, tint);
            count!(self.counters, models, 1);
            count!(self.counters, meshes, model.meshes().len() as u64);
            true
        } else {
            false
        }
    }
}

/// UI primitives in logical reference units, shared by 2D and 3D.
pub struct UiCanvas<'draw, D: RaylibDraw> {
    counters: &'draw mut Option<DrawCounters>,
    /// Raylib guard for advanced screen-space drawing (coordinates are target pixels).
    pub raw: &'draw mut D,
    /// Current content dimensions in UI units.
    pub logical_size: Vec2,
    scale: Vec2,
    font: WeakFont,
    textures: &'draw dyn TextureSource,
}

/// Optional default button appearance. Layout and interaction remain game-owned.
#[derive(Clone, Copy, Debug)]
pub struct UiButtonStyle {
    /// Resting fill.
    pub normal: Color,
    /// Pointer hover fill.
    pub hovered: Color,
    /// Held pointer fill.
    pub pressed: Color,
    /// Disabled fill.
    pub disabled: Color,
    /// Label color.
    pub text: Color,
    /// Keyboard focus outline.
    pub focus: Color,
    /// Font size in reference UI units.
    pub font_size: f32,
}

impl Default for UiButtonStyle {
    fn default() -> Self {
        Self {
            normal: Color::new(37, 49, 66, 255),
            hovered: Color::new(55, 76, 99, 255),
            pressed: Color::new(25, 102, 120, 255),
            disabled: Color::new(44, 44, 48, 255),
            text: Color::WHITE,
            focus: Color::new(93, 217, 225, 255),
            font_size: 20.0,
        }
    }
}

impl<D: RaylibDraw> UiCanvas<'_, D> {
    /// Draws an already-resolved button response; activation is handled during
    /// fixed update. Use short labels that fit the supplied bounds.
    pub fn button(
        &mut self,
        bounds: Aabb2,
        label: &str,
        response: &UiResponse,
        style: UiButtonStyle,
    ) {
        let fill = if !response.enabled {
            style.disabled
        } else if response.held {
            style.pressed
        } else if response.hovered {
            style.hovered
        } else {
            style.normal
        };
        self.rectangle(bounds, fill);
        if response.focused {
            count!(self.counters, ui_primitives, 1);
            self.raw.draw_rectangle_lines_ex(
                rect(Aabb2 {
                    min: bounds.min * self.scale,
                    max: bounds.max * self.scale,
                }),
                2.0 * self.scale.min_element(),
                style.focus,
            );
        }
        let measured = self.font.measure_text(label, style.font_size, 1.0);
        self.text(
            label,
            bounds.center() - Vec2::new(measured.x, measured.y) * 0.5,
            style.font_size,
            style.text,
        );
    }

    /// Draws a loaded texture icon into UI-unit bounds. Returns false for a
    /// stale/unloaded texture. Icons can share an independent interactive region.
    pub fn icon(&mut self, id: TextureId, bounds: Aabb2, tint: Color) -> bool {
        if let Some(texture) = self.textures.texture(id) {
            self.raw.draw_texture_pro(
                texture,
                Rectangle::new(0.0, 0.0, texture.width as f32, texture.height as f32),
                rect(Aabb2 {
                    min: bounds.min * self.scale,
                    max: bounds.max * self.scale,
                }),
                Vector2::zero(),
                0.0,
                tint,
            );
            count!(self.counters, textures, 1);
            true
        } else {
            false
        }
    }
    /// Filled UI rectangle, automatically scaled to render pixels.
    pub fn rectangle(&mut self, bounds: Aabb2, color: Color) {
        count!(self.counters, ui_primitives, 1);
        self.raw.draw_rectangle_rec(
            rect(Aabb2 {
                min: bounds.min * self.scale,
                max: bounds.max * self.scale,
            }),
            color,
        );
    }
    /// UI text using raylib's default font. Cache formatted strings when possible.
    pub fn text(&mut self, text: &str, position: Vec2, size: f32, color: Color) {
        count!(self.counters, text, 1);
        self.raw.draw_text_ex(
            &self.font,
            text,
            v2(position * self.scale),
            size * self.scale.y,
            self.scale.y,
            color,
        );
    }
    /// UI circle, preserving its proportions.
    pub fn circle(&mut self, center: Vec2, radius: f32, color: Color) {
        count!(self.counters, ui_primitives, 1);
        self.raw.draw_circle_v(
            v2(center * self.scale),
            radius * self.scale.min_element(),
            color,
        );
    }
}

pub(crate) fn v2(v: Vec2) -> Vector2 {
    Vector2::new(v.x, v.y)
}
pub(crate) fn v3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

pub(crate) fn matrix(matrix: Mat4) -> Matrix {
    let a = matrix.to_cols_array();
    Matrix {
        m0: a[0],
        m1: a[1],
        m2: a[2],
        m3: a[3],
        m4: a[4],
        m5: a[5],
        m6: a[6],
        m7: a[7],
        m8: a[8],
        m9: a[9],
        m10: a[10],
        m11: a[11],
        m12: a[12],
        m13: a[13],
        m14: a[14],
        m15: a[15],
    }
}
pub(crate) fn rect(bounds: Aabb2) -> Rectangle {
    Rectangle::new(bounds.min.x, bounds.min.y, bounds.size().x, bounds.size().y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayengine_core::glam::Quat;

    #[test]
    fn mesh_matrix_preserves_translation_rotation_and_nonuniform_scale() {
        let transform = Transform3D {
            position: Vec3::new(4.0, -2.0, 7.0),
            rotation: Quat::from_rotation_y(0.7) * Quat::from_rotation_z(-0.3),
            scale: Vec3::new(2.0, 3.0, 0.5),
        };
        let world = Mat4::from_translation(Vec3::new(-5.0, 1.0, 2.0)) * transform.matrix();
        let raylib_matrix = matrix(world);
        for point in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z, Vec3::ONE] {
            let actual = v3(point).transform(raylib_matrix);
            let expected = world.transform_point3(point);
            assert!((Vec3::new(actual.x, actual.y, actual.z) - expected).length() < 0.00001);
        }
    }
}
