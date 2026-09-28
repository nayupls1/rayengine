//! Immediate drawing with corresponding 2D/3D passes and shared logical UI.

use crate::assets::{Assets, ModelId, TextureId};
use rayengine_core::{
    camera::{Camera2D, Camera3D},
    collision::{Aabb2, Aabb3},
    glam::{Vec2, Vec3},
    viewport::Viewport,
};
use raylib::prelude::*;

/// Concrete offscreen raylib drawing guard, available for advanced passes.
pub type TargetDraw<'draw, 'target> = RaylibTextureMode<'draw, 'target, RaylibHandle>;

/// One render frame. Game code chooses passes; the engine owns presentation.
pub struct Frame<'frame, 'audio> {
    pub(crate) raylib: &'frame mut RaylibHandle,
    pub(crate) thread: &'frame RaylibThread,
    pub(crate) target: &'frame mut RenderTexture2D,
    /// Asset handles loaded during initialization.
    pub assets: &'frame Assets<'audio>,
    /// Current viewport in logical window coordinates.
    pub viewport: Viewport,
    /// Interpolation fraction between previous/current simulation state.
    pub alpha: f32,
    /// Zero-based render frame index.
    pub index: u64,
}

impl Frame<'_, '_> {
    /// Clears color and depth at the start of the game frame.
    pub fn clear(&mut self, color: Color) {
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
        });
    }

    /// Draws a perspective 3D pass using the render target's preserved aspect ratio.
    pub fn world_3d(
        &mut self,
        camera: Camera3D,
        draw: impl FnOnce(&mut Canvas3D<'_, RaylibMode3D<'_, TargetDraw<'_, '_>>>),
    ) {
        let camera = raylib::prelude::Camera3D {
            position: v3(camera.position),
            target: v3(camera.target),
            up: v3(camera.up),
            fovy: camera.vertical_fov,
            projection: CameraProjection::CAMERA_PERSPECTIVE,
        };
        let mut target = self.raylib.begin_texture_mode(self.thread, self.target);
        let mut raw = target.begin_mode3D(camera);
        draw(&mut Canvas3D {
            raw: &mut raw,
            models: self.assets,
        });
    }

    /// Draws screen UI in reference units, scaled independently from the world camera.
    pub fn ui(&mut self, draw: impl FnOnce(&mut UiCanvas<'_, TargetDraw<'_, '_>>)) {
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
        });
    }

    /// Direct raylib texture pass for shaders or drawing beyond the SDK primitives.
    pub fn with_raylib(&mut self, draw: impl FnOnce(&mut TargetDraw<'_, '_>)) {
        let mut raw = self.raylib.begin_texture_mode(self.thread, self.target);
        draw(&mut raw);
    }
}

/// Immediate 2D primitives. No command buffer or allocation is introduced.
pub struct Canvas2D<'draw, D: RaylibDraw> {
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
        self.raw.draw_rectangle_rec(rect(bounds), color);
    }
    /// Filled world-space circle.
    pub fn circle(&mut self, center: Vec2, radius: f32, color: Color) {
        self.raw.draw_circle_v(v2(center), radius, color);
    }
    /// World-space line with explicit width.
    pub fn line(&mut self, start: Vec2, end: Vec2, width: f32, color: Color) {
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
            true
        } else {
            false
        }
    }
}

/// Immediate 3D primitives corresponding to the 2D drawing API.
pub struct Canvas3D<'draw, D: RaylibDraw> {
    /// Raylib guard for advanced drawing within this camera pass.
    pub raw: &'draw mut D,
    models: &'draw dyn ModelSource,
}

trait ModelSource {
    fn model(&self, id: ModelId) -> Option<&Model>;
}
impl ModelSource for Assets<'_> {
    fn model(&self, id: ModelId) -> Option<&Model> {
        self.model(id)
    }
}

impl<D: RaylibDraw + RaylibDraw3D> Canvas3D<'_, D> {
    /// Filled world-space box.
    pub fn cube(&mut self, bounds: Aabb3, color: Color) {
        self.raw
            .draw_cube_v(v3(bounds.center()), v3(bounds.size()), color);
    }
    /// Box wireframe.
    pub fn wire_cube(&mut self, bounds: Aabb3, color: Color) {
        self.raw
            .draw_cube_wires_v(v3(bounds.center()), v3(bounds.size()), color);
    }
    /// Filled sphere with fixed low polygon count.
    pub fn sphere(&mut self, center: Vec3, radius: f32, color: Color) {
        self.raw.draw_sphere_ex(v3(center), radius, 12, 16, color);
    }
    /// World-space line.
    pub fn line(&mut self, start: Vec3, end: Vec3, color: Color) {
        self.raw.draw_line3D(v3(start), v3(end), color);
    }
    /// Draws a model with uniform scale; false for an unloaded handle.
    pub fn model(&mut self, id: ModelId, position: Vec3, scale: f32, tint: Color) -> bool {
        if let Some(model) = self.models.model(id) {
            self.raw.draw_model(model, v3(position), scale, tint);
            true
        } else {
            false
        }
    }
}

/// UI primitives in logical reference units, shared by 2D and 3D.
pub struct UiCanvas<'draw, D: RaylibDraw> {
    /// Raylib guard for advanced screen-space drawing (coordinates are target pixels).
    pub raw: &'draw mut D,
    /// Current content dimensions in UI units.
    pub logical_size: Vec2,
    scale: Vec2,
    font: WeakFont,
}

impl<D: RaylibDraw> UiCanvas<'_, D> {
    /// Filled UI rectangle, automatically scaled to render pixels.
    pub fn rectangle(&mut self, bounds: Aabb2, color: Color) {
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
pub(crate) fn rect(bounds: Aabb2) -> Rectangle {
    Rectangle::new(bounds.min.x, bounds.min.y, bounds.size().x, bounds.size().y)
}
