//! Owned outline fonts, target-aware rasterization, and shared text layout.
//!
//! See [`crate::guides::fonts`] for ownership, limits, and manifest declarations.

use crate::Error;
use rayengine_core::{collision::Aabb2, glam::Vec2};
use raylib::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

mod gpu;
mod manifest;
pub use manifest::{FontDeclaration, FontDeclarations};

/// Stable font handle in one game run. Unloading permanently invalidates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub(crate) usize);

/// Glyph atlas sampling, independent of rasterization resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontSampling {
    /// Bilinear filtering of grayscale coverage for smooth outline text.
    #[default]
    Smooth,
    /// Point filtering, preserving deliberately visible font pixels.
    Nearest,
}

/// Policy for choosing atlas resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontRasterization {
    /// Cache larger atlases when the requested text size in target pixels grows.
    #[default]
    Adaptive,
    /// Always use raster_size, including when pixel text is enlarged.
    Fixed,
}

/// Cached font configuration. Coverage always includes space and `?`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FontOptions {
    /// Minimum atlas em size in pixels, 8..=512; fixed policy uses this exact size.
    pub raster_size: u32,
    /// Unicode characters to rasterize, at most 1024 unique printable characters.
    /// The default is printable ASCII. Unsupported characters render as `?`.
    pub glyphs: String,
    /// Smooth or nearest texture filtering.
    pub sampling: FontSampling,
    /// Adaptive or fixed atlas size policy.
    pub rasterization: FontRasterization,
}
impl Default for FontOptions {
    fn default() -> Self {
        Self {
            raster_size: 32,
            glyphs: (' '..='~').collect(),
            sampling: FontSampling::Smooth,
            rasterization: FontRasterization::Adaptive,
        }
    }
}
impl FontOptions {
    /// Validate and normalize coverage before filesystem or GPU operations.
    pub fn validate(&self) -> Result<(), Error> {
        self.normalized().map(|_| ())
    }
    fn normalized(&self) -> Result<Self, Error> {
        if !(8..=512).contains(&self.raster_size) {
            return Err(Error::Asset("font raster_size must be 8..=512".into()));
        }
        if self.glyphs.chars().any(char::is_control) {
            return Err(Error::Asset(
                "font glyphs must contain printable characters, without NUL or controls".into(),
            ));
        }
        let mut chars: Vec<_> = self.glyphs.chars().chain([' ', '?']).collect();
        chars.sort_unstable();
        chars.dedup();
        if chars.len() > 1024 {
            return Err(Error::Asset(
                "font coverage exceeds 1024 unique glyphs".into(),
            ));
        }
        Ok(Self {
            glyphs: chars.into_iter().collect(),
            ..self.clone()
        })
    }
    fn atlas_size(&self, physical_size: f32) -> Result<u32, Error> {
        if !physical_size.is_finite() || physical_size <= 0.0 {
            return Err(Error::Asset(
                "font target size must be finite and positive".into(),
            ));
        }
        if self.rasterization == FontRasterization::Fixed
            || physical_size <= self.raster_size as f32
        {
            return Ok(self.raster_size);
        }
        if physical_size > 512.0 {
            return Err(Error::Asset(
                "adaptive font size exceeds 512 target pixels; reduce text size or render scale"
                    .into(),
            ));
        }
        Ok((physical_size.ceil() as u32).next_power_of_two())
    }
}

/// Label layout in logical UI units, shared by drawing and measurement.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    /// Font owned by Assets in this run.
    pub font: FontId,
    /// Em size in UI units, positive and at most 512.
    pub size: f32,
    /// Extra spacing between adjacent characters in UI units, 0..=512.
    pub spacing: f32,
    /// Extra gap between lines, beyond the font's intrinsic line height, 0..=512.
    pub line_spacing: f32,
}
impl TextStyle {
    /// A font and em size, with zero additional letter or line spacing.
    pub fn new(font: FontId, size: f32) -> Self {
        Self {
            font,
            size,
            spacing: 0.0,
            line_spacing: 0.0,
        }
    }
    pub(crate) fn validate(self, text: &str) -> Result<(), Error> {
        if !self.size.is_finite()
            || self.size <= 0.0
            || self.size > 512.0
            || !self.spacing.is_finite()
            || !(0.0..=512.0).contains(&self.spacing)
            || !self.line_spacing.is_finite()
            || !(0.0..=512.0).contains(&self.line_spacing)
            || text.contains('\0')
        {
            return Err(Error::Asset(
                "text needs size in (0,512], spacing/line_spacing in [0,512], and no NUL".into(),
            ));
        }
        Ok(())
    }
}

/// Logical text metrics independent of atlas resolution, DPI, and render quality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    /// Maximum line advance and total line-box height. Empty text is zero.
    pub size: Vec2,
    /// Outline ink bounds relative to the top-left line box. May extend left
    /// of zero (italic bearings); whitespace has zero ink bounds.
    pub ink_bounds: Aabb2,
    /// Characters substituted with `?` because coverage or the font lacks them.
    pub missing_glyphs: usize,
}
impl Default for TextMetrics {
    fn default() -> Self {
        Self {
            size: Vec2::ZERO,
            ink_bounds: Aabb2 {
                min: Vec2::ZERO,
                max: Vec2::ZERO,
            },
            missing_glyphs: 0,
        }
    }
}

const MAX_ATLAS_PIXELS: usize = 4 * 1024 * 1024;
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) struct FontAssets {
    slots: Vec<Option<FontEntry>>,
    paths: HashMap<(PathBuf, FontOptions), FontId>,
}
struct FontEntry {
    outline: fontdue::Font,
    options: FontOptions,
    coverage: Vec<char>,
    atlases: BTreeMap<u32, Atlas>,
}
struct Atlas {
    texture: Texture2D,
    glyphs: BTreeMap<char, AtlasGlyph>,
}
struct AtlasGlyph {
    source: Rectangle,
    offset: Vec2,
}
impl FontAssets {
    pub(crate) fn new() -> Self {
        Self {
            slots: Vec::new(),
            paths: HashMap::new(),
        }
    }
    pub(crate) fn load(
        &mut self,
        thread: &RaylibThread,
        path: &Path,
        options: FontOptions,
    ) -> Result<FontId, Error> {
        let options = options.normalized()?;
        let path = path.canonicalize()?;
        let key = (path, options);
        if let Some(id) = self.paths.get(&key) {
            return Ok(*id);
        }
        if std::fs::metadata(&key.0)?.len() > 16 * 1024 * 1024 {
            return Err(Error::Asset("font file exceeds 16 MiB".into()));
        }
        let bytes = std::fs::read(&key.0)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(Error::Asset("font file exceeds 16 MiB".into()));
        }
        let outline = fontdue::Font::from_bytes(
            bytes,
            fontdue::FontSettings {
                load_substitutions: false,
                ..Default::default()
            },
        )
        .map_err(|e| Error::Asset(format!("font {}: {e}", key.0.display())))?;
        if !outline.has_glyph('?') || !outline.has_glyph(' ') {
            return Err(Error::Asset(format!(
                "font {} must provide '?' and space fallback glyphs",
                key.0.display()
            )));
        }
        let coverage = key
            .1
            .glyphs
            .chars()
            .filter(|c| outline.has_glyph(*c))
            .collect();
        let mut entry = FontEntry {
            outline,
            options: key.1.clone(),
            coverage,
            atlases: BTreeMap::new(),
        };
        entry.prepare(thread, key.1.raster_size)?;
        let id = FontId(self.slots.len());
        self.slots.push(Some(entry));
        self.paths.insert(key, id);
        Ok(id)
    }
    pub(crate) fn unload(&mut self, id: FontId) -> bool {
        let unloaded = self.slots.get_mut(id.0).and_then(Option::take).is_some();
        self.paths.retain(|_, value| *value != id);
        unloaded
    }
    pub(crate) fn measure(&self, text: &str, style: TextStyle) -> Result<TextMetrics, Error> {
        style.validate(text)?;
        self.entry(style.font)?.layout(text, style, |_, _| {})
    }
    fn entry(&self, id: FontId) -> Result<&FontEntry, Error> {
        self.slots
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::Asset("font handle is stale or unloaded".into()))
    }
    #[allow(clippy::too_many_arguments)] // Explicit render-thread drawing inputs, kept private.
    pub(crate) fn draw<D: RaylibDraw>(
        &mut self,
        thread: &RaylibThread,
        raw: &mut D,
        text: &str,
        position: Vec2,
        style: TextStyle,
        scale: Vec2,
        color: Color,
    ) -> Result<TextMetrics, Error> {
        style.validate(text)?;
        if !position.is_finite() || !scale.is_finite() || scale.min_element() <= 0.0 {
            return Err(Error::Asset(
                "text position and target scale must be finite".into(),
            ));
        }
        let entry = self
            .slots
            .get_mut(style.font.0)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::Asset("font handle is stale or unloaded".into()))?;
        let raster = entry.options.atlas_size(style.size * scale.max_element())?;
        entry.prepare(thread, raster)?;
        let atlas = &entry.atlases[&raster];
        let factor = style.size / raster as f32;
        entry.layout(text, style, |character, baseline| {
            let glyph = &atlas.glyphs[&character];
            if glyph.source.width > 0.0 && glyph.source.height > 0.0 {
                let min = (position + baseline + glyph.offset * factor) * scale;
                let size = Vec2::new(glyph.source.width, glyph.source.height) * factor * scale;
                raw.draw_texture_pro(
                    &atlas.texture,
                    glyph.source,
                    Rectangle::new(min.x, min.y, size.x, size.y),
                    Vector2::zero(),
                    0.0,
                    color,
                );
            }
        })
    }
    pub(crate) fn usage(&self) -> (u64, u64, u64) {
        let mut counts = (0, 0, 0);
        for entry in self.slots.iter().flatten() {
            counts.0 += 1;
            counts.1 += entry.atlases.len() as u64;
            counts.2 += entry.bytes();
        }
        counts
    }
    /// Atlas size selected most recently can be inspected in native probes.
    #[cfg(test)]
    pub(crate) fn atlas_sizes(&self, id: FontId) -> Vec<u32> {
        self.entry(id).unwrap().atlases.keys().copied().collect()
    }
}
impl FontEntry {
    fn bytes(&self) -> u64 {
        self.atlases
            .values()
            .map(|a| a.texture.width as u64 * a.texture.height as u64 * 4)
            .sum()
    }
    fn prepare(&mut self, thread: &RaylibThread, raster: u32) -> Result<(), Error> {
        if self.atlases.contains_key(&raster) {
            return Ok(());
        }
        // Shelf packing uses precomputed metrics before any bitmap allocation.
        let metrics: Vec<_> = self
            .coverage
            .iter()
            .map(|c| (*c, self.outline.metrics(*c, raster as f32)))
            .collect();
        let mut area = 0_usize;
        let mut widest = 0;
        for (_, m) in &metrics {
            if m.width > 2046 || m.height > 2046 {
                return Err(Error::Asset("font glyph exceeds atlas dimensions".into()));
            }
            area = area.saturating_add((m.width + 2) * (m.height + 2));
            widest = widest.max(m.width + 2);
        }
        if area > MAX_ATLAS_PIXELS {
            return Err(Error::Asset("font atlas exceeds 4 million pixels".into()));
        }
        let width = ((area as f64).sqrt().ceil() as usize)
            .max(widest)
            .next_power_of_two()
            .min(2048);
        let (mut x, mut y, mut row) = (0, 0, 0);
        let mut glyphs = BTreeMap::new();
        for (c, m) in &metrics {
            if x + m.width + 2 > width {
                x = 0;
                y += row;
                row = 0;
            }
            glyphs.insert(
                *c,
                AtlasGlyph {
                    source: Rectangle::new(
                        (x + 1) as f32,
                        (y + 1) as f32,
                        m.width as f32,
                        m.height as f32,
                    ),
                    offset: Vec2::new(m.xmin as f32, -(m.ymin as f32 + m.height as f32)),
                },
            );
            x += m.width + 2;
            row = row.max(m.height + 2);
        }
        let height = (y + row).next_power_of_two();
        if width * height > MAX_ATLAS_PIXELS
            || self.bytes() + (width * height * 4) as u64 > MAX_FONT_BYTES
        {
            return Err(Error::Asset(
                "font atlas exceeds pixel or 64 MiB per-font cache limit".into(),
            ));
        }
        // White RGB even in the padding prevents dark bilinear fringes.
        let mut pixels = vec![255_u8; width * height * 4];
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 0;
        }
        for (c, m) in &metrics {
            let (_, bitmap) = self.outline.rasterize(*c, raster as f32);
            let source = glyphs[c].source;
            for by in 0..m.height {
                for bx in 0..m.width {
                    let offset =
                        ((source.y as usize + by) * width + source.x as usize + bx) * 4 + 3;
                    pixels[offset] = bitmap[by * m.width + bx];
                }
            }
        }
        let texture = gpu::upload(
            thread,
            &pixels,
            width as i32,
            height as i32,
            self.options.sampling,
        )?;
        self.atlases.insert(raster, Atlas { texture, glyphs });
        Ok(())
    }
    fn layout(
        &self,
        text: &str,
        style: TextStyle,
        mut glyph: impl FnMut(char, Vec2),
    ) -> Result<TextMetrics, Error> {
        if text.is_empty() {
            return Ok(TextMetrics::default());
        }
        let line = self
            .outline
            .horizontal_line_metrics(style.size)
            .ok_or_else(|| Error::Asset("font has no horizontal line metrics".into()))?;
        let mut result = TextMetrics::default();
        let (mut x, mut y, mut width, mut has_ink, mut first) =
            (0.0_f32, 0.0_f32, 0.0_f32, false, true);
        for c in text.chars() {
            if c == '\n' {
                width = width.max(x);
                x = 0.0;
                y += line.new_line_size + style.line_spacing;
                first = true;
                continue;
            }
            let c = if c == '\t' {
                ' '
            } else if self.coverage.binary_search(&c).is_ok() {
                c
            } else {
                result.missing_glyphs += 1;
                '?'
            };
            if !first {
                x += style.spacing;
            }
            first = false;
            let m = self.outline.metrics(c, style.size);
            let baseline = Vec2::new(x, y + line.ascent);
            glyph(c, baseline);
            if m.bounds.width > 0.0 && m.bounds.height > 0.0 {
                let min = baseline + Vec2::new(m.bounds.xmin, -m.bounds.ymin - m.bounds.height);
                let max = min + Vec2::new(m.bounds.width, m.bounds.height);
                if has_ink {
                    result.ink_bounds.min = result.ink_bounds.min.min(min);
                    result.ink_bounds.max = result.ink_bounds.max.max(max);
                } else {
                    result.ink_bounds = Aabb2 { min, max };
                    has_ink = true;
                }
            }
            x += m.advance_width;
        }
        result.size = Vec2::new(width.max(x), y + line.ascent - line.descent);
        if !result.size.is_finite()
            || !result.ink_bounds.min.is_finite()
            || !result.ink_bounds.max.is_finite()
        {
            return Err(Error::Asset(
                "text layout exceeds finite coordinates".into(),
            ));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
