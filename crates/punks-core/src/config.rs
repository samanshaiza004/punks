use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybinds {
    #[serde(default = "default_navigate_up")]
    pub navigate_up: String,
    #[serde(default = "default_navigate_down")]
    pub navigate_down: String,
    #[serde(default = "default_navigate_back")]
    pub navigate_back: String,
    #[serde(default = "default_confirm")]
    pub confirm: String,
    #[serde(default = "default_new_tab")]
    pub new_tab: String,
    #[serde(default = "default_close_tab")]
    pub close_tab: String,
    #[serde(default = "default_prev_tab")]
    pub prev_tab: String,
    #[serde(default = "default_next_tab")]
    pub next_tab: String,
}

fn default_navigate_up() -> String {
    "W".into()
}
fn default_navigate_down() -> String {
    "S".into()
}
fn default_navigate_back() -> String {
    "A".into()
}
fn default_confirm() -> String {
    "D".into()
}
fn default_new_tab() -> String {
    "T".into()
}
fn default_close_tab() -> String {
    "X".into()
}
fn default_prev_tab() -> String {
    "LeftArrow".into()
}
fn default_next_tab() -> String {
    "RightArrow".into()
}
fn default_volume() -> f32 {
    1.0
}

impl Default for Keybinds {
    fn default() -> Self {
        Keybinds {
            navigate_up: default_navigate_up(),
            navigate_down: default_navigate_down(),
            navigate_back: default_navigate_back(),
            confirm: default_confirm(),
            new_tab: default_new_tab(),
            close_tab: default_close_tab(),
            prev_tab: default_prev_tab(),
            next_tab: default_next_tab(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PunksConfig {
    #[serde(default)]
    pub last_directory: Option<PathBuf>,
    #[serde(default)]
    pub keybinds: Keybinds,
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Open tab directories, in tab order (blank tabs omitted — there's
    /// nothing in them worth restoring). Missing in configs saved before
    /// tabs existed, so `#[serde(default)]` makes an empty list the fallback
    /// signal to restore from `last_directory` instead.
    #[serde(default)]
    pub tabs: Vec<PathBuf>,
    /// Index into `tabs` of the tab that was active. Clamped on restore, so
    /// a stale value from before some `tabs` entries vanished is harmless.
    #[serde(default)]
    pub active_tab: usize,
}

impl Default for PunksConfig {
    fn default() -> Self {
        PunksConfig {
            last_directory: None,
            keybinds: Keybinds::default(),
            volume: default_volume(),
            tabs: Vec::new(),
            active_tab: 0,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("punks").join("config.json"))
}

pub fn load() -> PunksConfig {
    let Some(path) = config_path() else {
        return PunksConfig::default();
    };

    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
            log::warn!("failed to parse {}: {e}", path.display());
            PunksConfig::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PunksConfig::default(),
        Err(e) => {
            log::warn!("failed to read {}: {e}", path.display());
            PunksConfig::default()
        }
    }
}

pub fn save(config: &PunksConfig) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("failed to create {}: {e}", parent.display());
            return;
        }
    }

    let json = match serde_json::to_string_pretty(config) {
        Ok(j) => j,
        Err(e) => {
            log::warn!("failed to serialize config: {e}");
            return;
        }
    };

    if let Err(e) = std::fs::write(&path, json) {
        log::warn!("failed to write {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_without_tabs_field_deserializes_with_defaults() {
        // Configs saved before tab persistence existed have no "tabs"/
        // "active_tab" keys at all; loading one must not fail.
        let json = r#"{"last_directory": "/some/dir", "volume": 0.5}"#;
        let cfg: PunksConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.tabs.is_empty());
        assert_eq!(cfg.active_tab, 0);
        assert_eq!(cfg.last_directory, Some(PathBuf::from("/some/dir")));
    }

    #[test]
    fn tabs_round_trip_through_json() {
        let cfg = PunksConfig {
            tabs: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            active_tab: 1,
            ..PunksConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: PunksConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tabs, cfg.tabs);
        assert_eq!(back.active_tab, 1);
    }
}
