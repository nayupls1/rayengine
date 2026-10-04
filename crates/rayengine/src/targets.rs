//! Run-owned offscreen targets with explicit sampling, sizing and lifetime.
use crate::{Error, quality};
use rayengine_core::{glam::Vec2, quality::RenderQuality, viewport::Viewport};
use raylib::prelude::*;

/// Versioned target handle. Do not share handles between game runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderTargetId {
    slot: usize,
    generation: u64,
}

/// How a target follows the content viewport (bars are excluded).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSize {
    /// Explicit pixel dimensions, independent of resizing and DPI.
    Fixed(u32, u32),
    /// Logical window pixels in the content viewport, independent of DPI.
    Logical,
    /// Physical framebuffer pixels in the content viewport, including DPI.
    Physical,
    /// Reference UI units, including expanded units under Expand.
    Reference,
}

/// Texture sampling for an offscreen target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TargetFilter {
    /// Nearest-neighbor sampling, suitable for pixel art.
    #[default]
    Point,
    /// Bilinear sampling.
    Bilinear,
}

/// Target creation options. Color storage is premultiplied RGBA8 with depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTargetDesc {
    /// Explicit or viewport-relative pixel dimensions.
    pub size: TargetSize,
    /// Sampling used when this target is drawn as a texture.
    pub filter: TargetFilter,
}
impl RenderTargetDesc {
    /// A fixed, point-filtered target.
    pub fn fixed(width: u32, height: u32) -> Self {
        Self {
            size: TargetSize::Fixed(width, height),
            filter: TargetFilter::Point,
        }
    }
    pub(crate) fn dimensions(self, view: &Viewport, dpi: Vec2) -> Result<(u32, u32), Error> {
        let pixels = match self.size {
            TargetSize::Fixed(w, h) => Vec2::new(w as f32, h as f32),
            TargetSize::Logical => view.size,
            TargetSize::Physical => view.size * dpi,
            TargetSize::Reference => view.logical_size,
        };
        if !pixels.is_finite()
            || pixels.min_element() <= 0.0
            || pixels.round().max_element() > RenderQuality::MAX_DIMENSION as f32
        {
            return Err(Error::Config(
                "render target dimensions must be in 1..=8192".into(),
            ));
        }
        Ok((
            pixels.x.round().max(1.0) as u32,
            pixels.y.round().max(1.0) as u32,
        ))
    }
}

struct Slot {
    generation: u64,
    desc: Option<RenderTargetDesc>,
    native: Option<RenderTexture2D>,
    active: bool,
}
#[derive(Default)]
pub(crate) struct TargetAssets {
    slots: Vec<Slot>,
}
impl TargetAssets {
    pub fn create(&mut self, desc: RenderTargetDesc) -> Result<RenderTargetId, Error> {
        if let TargetSize::Fixed(w, h) = desc.size
            && (w == 0
                || h == 0
                || w > RenderQuality::MAX_DIMENSION
                || h > RenderQuality::MAX_DIMENSION)
        {
            return Err(Error::Config(
                "render target dimensions must be in 1..=8192".into(),
            ));
        }
        let slot = self
            .slots
            .iter()
            .position(|s| s.desc.is_none() && s.generation < u64::MAX)
            .unwrap_or(self.slots.len());
        if slot == self.slots.len() {
            self.slots.push(Slot {
                generation: 0,
                desc: None,
                native: None,
                active: false,
            });
        }
        let entry = &mut self.slots[slot];
        entry.generation += 1;
        entry.desc = Some(desc);
        Ok(RenderTargetId {
            slot,
            generation: entry.generation,
        })
    }
    fn slot(&self, id: RenderTargetId) -> Option<&Slot> {
        self.slots
            .get(id.slot)
            .filter(|s| s.generation == id.generation && s.desc.is_some())
    }
    pub fn valid(&self, id: RenderTargetId) -> bool {
        self.slot(id).is_some()
    }
    pub fn texture(&self, id: RenderTargetId) -> Option<&WeakTexture2D> {
        self.slot(id)?.native.as_ref().map(|t| t.texture())
    }
    pub fn unload(&mut self, id: RenderTargetId) -> bool {
        if self.slot(id).is_none_or(|s| s.active) {
            return false;
        }
        let s = &mut self.slots[id.slot];
        s.native = None;
        s.desc = None;
        true
    }
    pub fn usage(&self) -> (u64, u64) {
        self.slots
            .iter()
            .filter_map(|s| s.native.as_ref())
            .fold((0, 0), |(n, b), t| {
                (
                    n + 1,
                    b + 8 * t.texture().width as u64 * t.texture().height as u64,
                )
            })
    }
    pub fn planned_bytes(&self, view: &Viewport, dpi: Vec2) -> Result<u64, Error> {
        self.slots
            .iter()
            .filter_map(|s| s.desc)
            .try_fold(0, |bytes, d| {
                let (w, h) = d.dimensions(view, dpi)?;
                Ok(bytes + 8 * u64::from(w) * u64::from(h))
            })
    }
    pub fn sync(
        &mut self,
        rl: &mut RaylibHandle,
        thread: &RaylibThread,
        view: &Viewport,
        dpi: Vec2,
        reserved: u64,
    ) -> Result<(), Error> {
        let bytes = self.planned_bytes(view, dpi)? + reserved;
        if bytes > RenderQuality::MAX_TARGET_BYTES {
            return Err(Error::Config(format!(
                "render targets need {bytes} bytes; limit is {}",
                RenderQuality::MAX_TARGET_BYTES
            )));
        }
        self.release_resized(view, dpi)?;
        for s in &mut self.slots {
            if let Some(desc) = s.desc
                && s.native.is_none()
                && !s.active
            {
                let size = desc.dimensions(view, dpi)?;
                let mut t = quality::target(rl, thread, size, desc.filter == TargetFilter::Point)?;
                rl.begin_texture_mode(thread, &mut t)
                    .clear_background(Color::BLANK);
                s.native = Some(t);
            }
        }
        Ok(())
    }
    pub fn release_resized(&mut self, view: &Viewport, dpi: Vec2) -> Result<(), Error> {
        // Release all resized resources before allocating any replacement.
        for s in &mut self.slots {
            if let (Some(desc), Some(t)) = (s.desc, &s.native) {
                let size = desc.dimensions(view, dpi)?;
                if size != (t.texture().width as u32, t.texture().height as u32) {
                    s.native = None;
                }
            }
        }
        Ok(())
    }
    pub fn take(&mut self, id: RenderTargetId) -> Result<RenderTexture2D, Error> {
        if self.slot(id).is_none() {
            return Err(Error::Asset("render target is unloaded".into()));
        }
        let s = &mut self.slots[id.slot];
        let t = s
            .native
            .take()
            .ok_or_else(|| Error::Asset("render target is active or not allocated yet".into()))?;
        s.active = true;
        Ok(t)
    }
    pub fn restore(&mut self, id: RenderTargetId, native: RenderTexture2D) {
        let s = &mut self.slots[id.slot];
        s.active = false;
        s.native = Some(native);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayengine_core::viewport::ScaleMode;
    #[test]
    fn sizing_policies_and_stale_handles() {
        for mode in [ScaleMode::Fit, ScaleMode::Expand, ScaleMode::IntegerFit] {
            for window in [Vec2::new(320.0, 180.0), Vec2::new(180.0, 320.0)] {
                let view = Viewport::new(window, Vec2::new(160.0, 90.0), mode).unwrap();
                for dpi in [Vec2::ONE, Vec2::splat(1.25), Vec2::splat(2.0)] {
                    for (size, expected) in [
                        (TargetSize::Fixed(32, 16), Vec2::new(32.0, 16.0)),
                        (TargetSize::Logical, view.size),
                        (TargetSize::Physical, view.size * dpi),
                        (TargetSize::Reference, view.logical_size),
                    ] {
                        let desc = RenderTargetDesc {
                            size,
                            filter: TargetFilter::Point,
                        };
                        assert_eq!(
                            desc.dimensions(&view, dpi).unwrap(),
                            (expected.x.round() as u32, expected.y.round() as u32)
                        );
                    }
                }
            }
        }
        let mut assets = TargetAssets::default();
        let first = assets.create(RenderTargetDesc::fixed(32, 16)).unwrap();
        assert!(assets.unload(first));
        let next = assets.create(RenderTargetDesc::fixed(32, 16)).unwrap();
        assert_ne!(first, next);
        assert!(!assets.valid(first));
        assert!(assets.valid(next));
        assert_eq!(assets.slots.len(), 1);
        assert!(assets.create(RenderTargetDesc::fixed(0, 32)).is_err());
    }
}
