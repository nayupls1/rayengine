use super::model::ROOM_COUNT;
use rayengine::save::{self, SaveLimits, SaveOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub checkpoint: Option<usize>,
    pub victories: u32,
    pub music: f32,
    pub sfx: f32,
    pub crt: bool,
    pub arrows: bool,
    pub alternate_attack: bool,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            checkpoint: None,
            victories: 0,
            music: 0.65,
            sfx: 0.8,
            crt: false,
            arrows: false,
            alternate_attack: false,
        }
    }
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.checkpoint.is_some_and(|room| room >= ROOM_COUNT) {
            return Err("invalid checkpoint".into());
        }
        for gain in [self.music, self.sfx] {
            if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
                return Err("invalid audio volume".into());
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
            Ok(_) => (),
        }
        save::load_with(path, SaveLimits { max_payload_bytes: 4096 }, |schema, bytes| {
            if schema != 1 { return Err("unsupported Embervault save version".to_string()); }
            let data: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            data.validate()?; Ok(data)
        }).map_err(|e| format!("{}: {e}. Existing save preserved; choose another RAYENGINE_DUNGEON_SAVE path to start fresh.",path.display()))
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        save::save_with(path, 1, self, SaveOptions::default(), serde_json::to_vec)
            .map_err(|e| e.to_string())
    }
}
pub fn path() -> PathBuf {
    std::env::var_os("RAYENGINE_DUNGEON_SAVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
                .unwrap_or_else(|| PathBuf::from("."));
            base.join("embervault/profile.save")
        })
}
