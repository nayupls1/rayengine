//! Offscreen quality resolve and context-owned render targets.
mod gpu;
use crate::Error;
pub(crate) use gpu::{graphics_info, ui_blend_factors};
use rayengine_core::quality::{AntiAliasing, RenderPlan, RenderQuality};
use raylib::prelude::*;

pub(crate) struct QualityTargets {
    pub world: RenderTexture2D,
    pub ui: Option<RenderTexture2D>,
    pub output: Option<RenderTexture2D>,
    pub plan: RenderPlan,
}

fn target(
    rl: &mut RaylibHandle,
    thread: &RaylibThread,
    size: (u32, u32),
    point: bool,
) -> Result<RenderTexture2D, Error> {
    let target = rl
        .load_render_texture(thread, size.0, size.1)
        .map_err(|e| Error::Backend(format!("render target {}x{}: {e}", size.0, size.1)))?;
    if !target.is_render_texture_valid()
        || !target.texture().is_texture_valid()
        || !gpu::complete(&target, thread)
    {
        return Err(Error::Backend(format!(
            "render target {}x{} is incomplete",
            size.0, size.1
        )));
    }
    target.texture().set_texture_filter(
        thread,
        if point {
            TextureFilter::TEXTURE_FILTER_POINT
        } else {
            TextureFilter::TEXTURE_FILTER_BILINEAR
        },
    );
    target
        .texture()
        .set_texture_wrap(thread, TextureWrap::TEXTURE_WRAP_CLAMP);
    Ok(target)
}

impl QualityTargets {
    pub fn new(
        rl: &mut RaylibHandle,
        thread: &RaylibThread,
        plan: RenderPlan,
        point: bool,
    ) -> Result<Self, Error> {
        let limit = gpu::max_dimension(thread)?;
        if plan
            .world
            .0
            .max(plan.world.1)
            .max(plan.output.0.max(plan.output.1))
            > limit
        {
            return Err(Error::Backend(format!(
                "render dimensions exceed device limit {limit}"
            )));
        }
        Ok(Self {
            world: target(rl, thread, plan.world, point)?,
            ui: if plan.separate_ui {
                Some(target(rl, thread, plan.output, false)?)
            } else {
                None
            },
            output: if plan.separate_ui {
                Some(target(rl, thread, plan.output, false)?)
            } else {
                None
            },
            plan,
        })
    }
    pub fn resolve(
        &mut self,
        rl: &mut RaylibHandle,
        thread: &RaylibThread,
        fxaa: Option<&mut Shader>,
    ) {
        let Some(output) = &mut self.output else {
            return;
        };
        let mut raw = rl.begin_texture_mode(thread, output);
        raw.clear_background(Color::BLANK);
        // Copy into an empty target without applying texture alpha a second time.
        // This preserves transparent world clears as well as ordinary opaque games.
        let mut world = raw.begin_blend_mode(BlendMode::BLEND_ALPHA_PREMULTIPLY);
        if let Some(shader) = fxaa {
            let loc = shader.get_shader_location("inverseSize");
            shader.set_shader_value(
                loc,
                Vector2::new(
                    1.0 / self.plan.world.0 as f32,
                    1.0 / self.plan.world.1 as f32,
                ),
            );
            let mut filter = world.begin_shader_mode(shader);
            blit(&mut filter, self.world.texture(), self.plan.output);
        } else {
            blit(&mut world, self.world.texture(), self.plan.output);
        }
        drop(world);
        if let Some(ui) = &self.ui {
            let mut overlay = raw.begin_blend_mode(BlendMode::BLEND_ALPHA_PREMULTIPLY);
            blit(&mut overlay, ui.texture(), self.plan.output);
        }
    }
    pub fn presented(&self) -> &RenderTexture2D {
        self.output.as_ref().unwrap_or(&self.world)
    }
}

fn blit(raw: &mut impl RaylibDraw, texture: &WeakTexture2D, size: (u32, u32)) {
    raw.draw_texture_pro(
        texture,
        Rectangle::new(0.0, 0.0, texture.width as f32, -(texture.height as f32)),
        Rectangle::new(0.0, 0.0, size.0 as f32, size.1 as f32),
        Vector2::zero(),
        0.0,
        Color::WHITE,
    );
}

pub(crate) fn shader(
    rl: &mut RaylibHandle,
    thread: &RaylibThread,
    quality: RenderQuality,
) -> Result<Option<Shader>, Error> {
    if quality.anti_aliasing == AntiAliasing::None {
        return Ok(None);
    }
    let shader = rl.load_shader_from_memory(thread, None, Some(include_str!("fxaa.fs")));
    // raylib may silently return its default shader on a compile failure.
    if !shader.is_shader_valid()
        || shader.id == RaylibHandle::get_shader_default().id
        || shader.get_shader_location("inverseSize") < 0
    {
        return Err(Error::Backend(
            "FXAA shader unavailable; refusing an unfiltered fallback".into(),
        ));
    }
    Ok(Some(shader))
}

#[cfg(test)]
mod tests;
