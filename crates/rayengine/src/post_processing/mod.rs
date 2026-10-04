//! Ordered full-frame material passes after the quality resolve, before presentation.
use crate::{
    Error,
    assets::{Assets, MaterialId},
    material::{AlphaMode, MaterialDesc},
};
use raylib::prelude::*;

/// Whether the logical UI layer participates in frame effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiPlacement {
    /// Filter the world, then overlay crisp UI at the output resolution.
    #[default]
    AfterEffects,
    /// Compose UI with the resolved world, then filter both.
    BeforeEffects,
}
/// A chain evaluated in vector order. Empty chains allocate no effect targets.
#[derive(Clone, Debug, Default)]
pub struct PostProcessing {
    /// Each material samples the preceding pass through texture0.
    pub materials: Vec<MaterialId>,
    /// UI composition relative to effects. Letterbox bars are never filtered.
    pub ui: UiPlacement,
}

/// Example shaders using the ordinary material/uniform APIs.
#[derive(Clone, Copy, Debug)]
pub enum BuiltinEffect {
    /// Darkens edges; float uniform `strength`, typically 0..1.
    Vignette,
    /// Multiplies RGB; vec3 uniforms `gain` and `lift` perform a simple color grade.
    ColorGrade,
    /// Scanline modulation; float uniforms `strength` (0..1) and `lines` (>0).
    Scanlines,
}
impl BuiltinEffect {
    /// GLSL 330 fragment source with raylib's standard vertex shader.
    /// Output preserves premultiplied alpha; shader parameters are registered explicitly.
    pub fn fragment_source(self) -> &'static str {
        match self {
            Self::Vignette => include_str!("vignette.fs"),
            Self::ColorGrade => include_str!("grade.fs"),
            Self::Scanlines => include_str!("scanlines.fs"),
        }
    }
    /// A surface description for the compiled shader, ready for parameter overrides.
    pub fn material(self, shader: crate::assets::ShaderId) -> MaterialDesc {
        MaterialDesc {
            shader: Some(shader),
            alpha: AlphaMode::Blend,
            ..Default::default()
        }
    }
}

pub(crate) struct PostTargets {
    targets: [RenderTexture2D; 2],
    pub size: (u32, u32),
    last: usize,
}
impl PostTargets {
    pub fn bytes(size: (u32, u32)) -> u64 {
        16 * u64::from(size.0) * u64::from(size.1)
    }
    pub fn new(
        rl: &mut RaylibHandle,
        thread: &RaylibThread,
        size: (u32, u32),
        point: bool,
    ) -> Result<Self, Error> {
        Ok(Self {
            targets: [
                crate::quality::target(rl, thread, size, point)?,
                crate::quality::target(rl, thread, size, point)?,
            ],
            size,
            last: 0,
        })
    }
    pub fn apply(
        &mut self,
        rl: &mut RaylibHandle,
        thread: &RaylibThread,
        source: &RenderTexture2D,
        ui: Option<&RenderTexture2D>,
        chain: &PostProcessing,
        assets: &mut Assets<'_>,
    ) -> Result<(), Error> {
        for (i, id) in chain.materials.iter().enumerate() {
            let (first, second) = self.targets.split_at_mut(1);
            let (destination, input) = if i % 2 == 0 {
                (&mut first[0], if i == 0 { source } else { &second[0] })
            } else {
                (&mut second[0], &first[0])
            };
            let mut raw = rl.begin_texture_mode(thread, destination);
            raw.clear_background(Color::BLANK);
            let mut copy = raw.begin_blend_mode(BlendMode::BLEND_ALPHA_PREMULTIPLY);
            assets.post_blit(*id, &mut copy, input.texture(), self.size)?;
            self.last = i % 2;
        }
        if chain.ui == UiPlacement::AfterEffects
            && let Some(ui) = ui
        {
            let mut raw = rl.begin_texture_mode(thread, &mut self.targets[self.last]);
            let mut overlay = raw.begin_blend_mode(BlendMode::BLEND_ALPHA_PREMULTIPLY);
            crate::quality::blit(&mut overlay, ui.texture(), self.size);
        }
        Ok(())
    }
    pub fn presented(&self) -> &RenderTexture2D {
        &self.targets[self.last]
    }
}

#[cfg(test)]
mod tests;
