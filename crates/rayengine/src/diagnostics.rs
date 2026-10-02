//! Opt-in, bounded runtime summaries. Timings are CPU wall times, not GPU timers.

use crate::Error;
use serde::Serialize;
use std::{path::PathBuf, time::Duration};

/// Enables diagnostics with a game-defined stable workload identifier.
#[derive(Clone, Debug)]
pub struct DiagnosticsConfig {
    /// ASCII workload ID; compare identical workloads and settings.
    pub workload: String,
    /// Optional JSON report destination, written after the run, outside timings.
    pub output: Option<PathBuf>,
}
impl DiagnosticsConfig {
    /// Creates an in-memory report. No per-frame output or sample vector is stored.
    pub fn new(workload: impl Into<String>) -> Self {
        Self {
            workload: workload.into(),
            output: None,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.workload.is_empty()
            || self.workload.len() > 128
            || !self
                .workload
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        {
            return Err(Error::Config(
                "diagnostic workload must be 1..=128 ASCII letters/digits or ._-/".into(),
            ));
        }
        if self
            .output
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
        {
            return Err(Error::Config(
                "diagnostic output path cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

/// Successful SDK submissions, not hardware draw calls. Raw raylib calls are excluded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DrawCounters {
    /// Color/depth clear requests.
    pub clears: u64,
    /// Camera passes in two dimensions.
    pub world_2d_passes: u64,
    /// Camera passes in three dimensions.
    pub world_3d_passes: u64,
    /// UI passes.
    pub ui_passes: u64,
    /// Explicit raw passes; submissions inside cannot be counted.
    pub raw_passes: u64,
    /// 2D rectangles, circles and lines.
    pub primitives_2d: u64,
    /// 3D boxes, wireframes, spheres and lines.
    pub primitives_3d: u64,
    /// Mesh submissions, including constituent meshes of imported models.
    pub meshes: u64,
    /// Imported model requests.
    pub models: u64,
    /// World textures and UI icons.
    pub textures: u64,
    /// UI rectangles, circles and button focus outlines.
    pub ui_primitives: u64,
    /// UI text requests; glyph counts are not tracked.
    pub text: u64,
}
impl DrawCounters {
    pub(crate) fn add(&mut self, other: Self) {
        macro_rules! add { ($($f:ident),*) => { $(self.$f = self.$f.saturating_add(other.$f);)* }; }
        add!(
            clears,
            world_2d_passes,
            world_3d_passes,
            ui_passes,
            raw_passes,
            primitives_2d,
            primitives_3d,
            meshes,
            models,
            textures,
            ui_primitives,
            text
        );
    }
}

/// Live resources owned by Assets, excluding game-owned native handles and raylib defaults.
/// Bytes describe logical payloads, not driver allocation size or total VRAM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ResourceCounts {
    /// Live custom font handles.
    pub fonts: u64,
    /// Cached font atlas textures, including adaptive size variants.
    pub font_atlases: u64,
    /// RGBA font atlas payload bytes, excluding CPU outline storage.
    pub font_bytes: u64,
    /// Standalone cached textures.
    pub textures: u64,
    /// Imported models.
    pub models: u64,
    /// Loaded sounds; audio storage bytes are not estimated.
    pub sounds: u64,
    /// Generated meshes.
    pub meshes: u64,
    /// Custom shaders plus the SDK material shader when initialized.
    pub shaders: u64,
    /// Game material descriptions.
    pub materials: u64,
    /// Standalone texture payload bytes, including mip levels.
    pub texture_bytes: u64,
    /// Generated vertex/attribute/index buffer payload bytes, including fallback UVs.
    pub generated_mesh_bytes: u64,
    /// Standard imported geometry arrays; excludes animations, bones and model textures.
    pub model_geometry_bytes: u64,
}
impl ResourceCounts {
    pub(crate) fn maximize(&mut self, other: Self) {
        macro_rules! max { ($($f:ident),*) => { $(self.$f = self.$f.max(other.$f);)* }; }
        max!(
            fonts,
            font_atlases,
            font_bytes,
            textures,
            models,
            sounds,
            meshes,
            shaders,
            materials,
            texture_bytes,
            generated_mesh_bytes,
            model_geometry_bytes
        );
    }
}

/// Constant-space timing summary, with explicit nanosecond units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TimingStats {
    /// Number of samples.
    pub samples: u64,
    /// Saturating total nanoseconds; divide by samples for the mean.
    pub total_ns: u64,
    /// Minimum, zero when empty.
    pub min_ns: u64,
    /// Maximum, zero when empty.
    pub max_ns: u64,
}
impl TimingStats {
    /// Adds a wall-time sample without allocation. Totals saturate rather than wrap.
    pub fn record(&mut self, elapsed: Duration) {
        let ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.min_ns = if self.samples == 0 {
            ns
        } else {
            self.min_ns.min(ns)
        };
        self.max_ns = self.max_ns.max(ns);
        self.samples = self.samples.saturating_add(1);
        self.total_ns = self.total_ns.saturating_add(ns);
    }
}

/// Settings required to interpret a run. Machine/driver provenance comes from benchmark scripts.
#[derive(Clone, Debug, Serialize)]
pub struct RunSettings {
    /// Compiled window backend.
    pub backend: &'static str,
    /// OS target.
    pub os: &'static str,
    /// Architecture target.
    pub arch: &'static str,
    /// SDK version.
    pub sdk_version: &'static str,
    /// Initial logical window dimensions.
    pub window_size: (u32, u32),
    /// Reference UI dimensions.
    pub reference_size: [f32; 2],
    /// Viewport policy.
    pub scale_mode: &'static str,
    /// Fixed simulation frequency.
    pub fixed_hz: u32,
    /// Catch-up limit.
    pub max_catch_up: u32,
    /// Requested effective render cap, zero for uncapped.
    pub target_fps: u32,
    /// Requested effective display synchronization; driver behavior may differ.
    pub vsync: bool,
    /// Last offscreen render size, including DPI/viewport effects.
    pub render_size: (u32, u32),
}

/// JSON schema 1. Reports retain totals/extrema instead of unbounded frame samples.
#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticsReport {
    /// Schema version, currently 1.
    pub schema_version: u32,
    /// Stable game-provided workload ID.
    pub workload: String,
    /// Effective settings and target information.
    pub settings: RunSettings,
    /// Presented frames, excluding minimized polling.
    pub frames: u64,
    /// Executed fixed updates.
    pub updates: u64,
    /// Time discarded by catch-up, nanoseconds.
    pub dropped_ns: u64,
    /// Complete active-frame wall time, including presentation and enabled diagnostics.
    pub frame: TimingStats,
    /// Individual game fixed-update callback wall times.
    pub update: TimingStats,
    /// Target preparation, clear and game draw callback, excluding presentation.
    pub render: TimingStats,
    /// Window presentation including EndDrawing, swaps, frame cap and driver stalls.
    pub present: TimingStats,
    /// Aggregate SDK submissions; excludes the runner's presentation blit.
    pub draws: DrawCounters,
    /// Last sampled resources, before teardown.
    pub resources: ResourceCounts,
    /// Per-field resource high-water marks, sampled after init and each active frame.
    pub peak_resources: ResourceCounts,
}
impl DiagnosticsReport {
    pub(crate) fn new(
        config: &DiagnosticsConfig,
        settings: RunSettings,
        resources: ResourceCounts,
    ) -> Self {
        Self {
            schema_version: 1,
            workload: config.workload.clone(),
            settings,
            frames: 0,
            updates: 0,
            dropped_ns: 0,
            frame: TimingStats::default(),
            update: TimingStats::default(),
            render: TimingStats::default(),
            present: TimingStats::default(),
            draws: DrawCounters::default(),
            resources,
            peak_resources: resources,
        }
    }
    pub(crate) fn record_frame(&mut self, draws: DrawCounters, resources: ResourceCounts) {
        self.frames = self.frames.saturating_add(1);
        self.draws.add(draws);
        self.resources = resources;
        self.peak_resources.maximize(resources);
    }
    /// Serializes schema 1 outside measured frames. Serialization failures propagate.
    pub fn write_json(&self, output: impl std::io::Write) -> Result<(), serde_json::Error> {
        serde_json::to_writer_pretty(output, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timings_and_counters_saturate_and_peaks_do_not_sum() {
        let mut t = TimingStats::default();
        t.record(Duration::from_nanos(20));
        t.record(Duration::from_nanos(10));
        assert_eq!(
            t,
            TimingStats {
                samples: 2,
                total_ns: 30,
                min_ns: 10,
                max_ns: 20
            }
        );
        t.record(Duration::MAX);
        assert_eq!(t.total_ns, u64::MAX);
        let mut draws = DrawCounters {
            meshes: u64::MAX,
            ..Default::default()
        };
        draws.add(DrawCounters {
            meshes: 1,
            textures: 2,
            ..Default::default()
        });
        assert_eq!(draws.meshes, u64::MAX);
        assert_eq!(draws.textures, 2);
        let mut resources = ResourceCounts {
            meshes: 4,
            ..Default::default()
        };
        resources.maximize(ResourceCounts {
            meshes: 2,
            texture_bytes: 100,
            ..Default::default()
        });
        assert_eq!(resources.meshes, 4);
        assert_eq!(resources.texture_bytes, 100);
    }
    #[test]
    fn workload_validation_and_json_write_errors_are_explicit() {
        for id in ["", "contains spaces", "a\n", &"x".repeat(129)] {
            assert!(DiagnosticsConfig::new(id).validate().is_err());
        }
        DiagnosticsConfig::new("mesh/default-100.v1")
            .validate()
            .unwrap();
        assert!(serde_json::to_writer(std::io::sink(), &DrawCounters::default()).is_ok());
    }

    #[test]
    fn report_schema_is_stable_and_output_errors_propagate() {
        let settings = RunSettings {
            backend: "test",
            os: "test",
            arch: "test",
            sdk_version: "0.0.1",
            window_size: (64, 64),
            reference_size: [64.0; 2],
            scale_mode: "fit",
            fixed_hz: 120,
            max_catch_up: 8,
            target_fps: 0,
            vsync: false,
            render_size: (64, 64),
        };
        let mut report = DiagnosticsReport::new(
            &DiagnosticsConfig::new("fixture.v1"),
            settings,
            ResourceCounts::default(),
        );
        report.record_frame(
            DrawCounters {
                meshes: 4,
                ..Default::default()
            },
            ResourceCounts {
                generated_mesh_bytes: 100,
                ..Default::default()
            },
        );
        report.record_frame(
            DrawCounters {
                meshes: 2,
                ..Default::default()
            },
            ResourceCounts::default(),
        );
        report.frame.record(Duration::from_nanos(50));
        let mut bytes = Vec::new();
        report.write_json(&mut bytes).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut keys: Vec<_> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "draws",
                "dropped_ns",
                "frame",
                "frames",
                "peak_resources",
                "present",
                "render",
                "resources",
                "schema_version",
                "settings",
                "update",
                "updates",
                "workload"
            ]
        );
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["frames"], 2);
        assert_eq!(json["draws"]["meshes"], 6);
        assert_eq!(json["resources"]["generated_mesh_bytes"], 0);
        assert_eq!(json["peak_resources"]["generated_mesh_bytes"], 100);
        assert_eq!(json["frame"]["total_ns"], 50);
        struct Rejected;
        impl std::io::Write for Rejected {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(report.write_json(Rejected).unwrap_err().is_io());
    }
}
