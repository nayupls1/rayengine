use super::*;
use std::path::Path;

/// Serializable button-action mapping. IDs are game-defined, never inferred.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionBinding {
    /// Game's numeric action ID.
    pub action: u16,
    /// Physical sources ORed together.
    pub buttons: Vec<Button>,
}

/// Serializable analog mapping, preserving source order for conflict resolution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisConfig {
    /// Game's numeric analog axis ID.
    pub axis: u16,
    /// Processed sources; greatest magnitude wins, first source wins ties.
    pub sources: Vec<AxisBinding>,
}

/// Binding settings schema 1. Games choose paths, defaults, action IDs and when
/// to load/save. Deserialization must be followed by [`Self::validate`] or
/// [`Bindings::from_config`] before use. Unknown fields and enum names fail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingConfig {
    /// Format version. Only 1 is currently supported.
    pub schema_version: u32,
    /// Unique action entries; sharing a physical source across actions is allowed.
    pub actions: Vec<ActionBinding>,
    /// Unique analog axis entries, independent of the action namespace.
    pub axes: Vec<AxisConfig>,
}

impl Default for BindingConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            actions: Vec::new(),
            axes: Vec::new(),
        }
    }
}

impl BindingConfig {
    /// Checks version, unique target IDs, physical sources and analog parameters.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
            return Err(invalid("unsupported binding schema_version; expected 1"));
        }
        let mut actions = std::collections::HashSet::new();
        for entry in &self.actions {
            if !actions.insert(entry.action) {
                return Err(invalid(format!("duplicate action ID {}", entry.action)));
            }
            for (i, button) in entry.buttons.iter().enumerate() {
                button
                    .validate()
                    .map_err(|e| invalid(format!("action {} button {i}: {e}", entry.action)))?;
            }
        }
        let mut axes = std::collections::HashSet::new();
        for entry in &self.axes {
            if !axes.insert(entry.axis) {
                return Err(invalid(format!("duplicate axis ID {}", entry.axis)));
            }
            for (i, source) in entry.sources.iter().enumerate() {
                source
                    .validate()
                    .map_err(|e| invalid(format!("axis {} source {i}: {e}", entry.axis)))?;
            }
        }
        Ok(())
    }
}

impl Bindings {
    /// Captures the complete mapping for serialization or a controls menu.
    pub fn config(&self) -> BindingConfig {
        BindingConfig {
            schema_version: 1,
            actions: self
                .actions
                .iter()
                .map(|(a, buttons)| ActionBinding {
                    action: a.0,
                    buttons: buttons.clone(),
                })
                .collect(),
            axes: self
                .axes
                .iter()
                .map(|(a, sources)| AxisConfig {
                    axis: a.0,
                    sources: sources.clone(),
                })
                .collect(),
        }
    }

    /// Validates a complete configuration before creating usable bindings.
    pub fn from_config(config: BindingConfig) -> Result<Self, Error> {
        config.validate()?;
        Ok(Self {
            actions: config
                .actions
                .into_iter()
                .filter(|a| !a.buttons.is_empty())
                .map(|a| (Action(a.action), a.buttons))
                .collect(),
            axes: config
                .axes
                .into_iter()
                .filter(|a| !a.sources.is_empty())
                .map(|a| (Axis(a.axis), a.sources))
                .collect(),
        })
    }

    /// Replaces every mapping atomically; invalid settings preserve the old set.
    pub fn replace(&mut self, config: BindingConfig) -> Result<(), Error> {
        *self = Self::from_config(config)?;
        Ok(())
    }

    /// Parses and validates schema-1 JSON, without filesystem access.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let config = serde_json::from_str(json)
            .map_err(|e| invalid(format!("invalid binding JSON: {e}")))?;
        Self::from_config(config)
    }

    /// Validates and serializes settings as human-readable JSON.
    pub fn to_json(&self) -> Result<String, Error> {
        self.validate()?;
        serde_json::to_string_pretty(&self.config())
            .map_err(|e| invalid(format!("binding serialization failed: {e}")))
    }

    /// Explicitly reads a game-owned path. Missing files are I/O errors; games
    /// decide whether to keep defaults. No automatic paths or fallback writes.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Explicitly writes JSON to a game-owned path. Parent directories must
    /// exist. This overwrites the file; it does not offer crash-safe commits.
    /// Games requiring atomic storage can persist [`Self::to_json`] themselves.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
