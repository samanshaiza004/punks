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
    #[serde(default = "default_undo")]
    pub undo: String,
    #[serde(default = "default_redo")]
    pub redo: String,
    #[serde(default = "default_play_stop")]
    pub play_stop: String,
    #[serde(default = "default_toggle_inspector")]
    pub toggle_inspector: String,
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
fn default_undo() -> String {
    "U".into()
}
fn default_redo() -> String {
    "R".into()
}
fn default_play_stop() -> String {
    "Space".into()
}
fn default_toggle_inspector() -> String {
    "I".into()
}
fn default_volume() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_inspector_width() -> f32 {
    300.0
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
            undo: default_undo(),
            redo: default_redo(),
            play_stop: default_play_stop(),
            toggle_inspector: default_toggle_inspector(),
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
    /// Whether the right-hand Inspector pane is shown. The Inspector is
    /// secondary (metadata/facts/tags); hiding it gives Results the full width.
    #[serde(default = "default_true")]
    pub inspector_visible: bool,
    /// Inspector pane width in pixels, set by dragging the splitter between
    /// Results and the Inspector. Per-section open/closed state is deliberately
    /// *not* persisted — sections just default sensibly each launch.
    #[serde(default = "default_inspector_width")]
    pub inspector_width: f32,
}

impl Default for PunksConfig {
    fn default() -> Self {
        PunksConfig {
            last_directory: None,
            keybinds: Keybinds::default(),
            volume: default_volume(),
            tabs: Vec::new(),
            active_tab: 0,
            inspector_visible: default_true(),
            inspector_width: default_inspector_width(),
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
        // Inspector keys are likewise absent in old configs: default to a
        // visible pane at the standard width.
        assert!(cfg.inspector_visible);
        assert_eq!(cfg.inspector_width, default_inspector_width());
    }

    #[test]
    fn inspector_state_round_trips_through_json() {
        let cfg = PunksConfig {
            inspector_visible: false,
            inspector_width: 420.0,
            ..PunksConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: PunksConfig = serde_json::from_str(&json).unwrap();
        assert!(!back.inspector_visible);
        assert_eq!(back.inspector_width, 420.0);
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

    #[test]
    fn keybinds_without_undo_field_deserializes_with_default() {
        // A config saved before the undo keybind existed has a "keybinds"
        // object with no "undo" key; loading it must not fail, and must
        // fall back to the documented default.
        let json = r#"{"keybinds": {"navigate_up": "W"}}"#;
        let cfg: PunksConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.keybinds.navigate_up, "W");
        assert_eq!(cfg.keybinds.undo, "U");
        // Keybinds added after this config was written fall back to defaults.
        assert_eq!(cfg.keybinds.redo, "R");
        assert_eq!(cfg.keybinds.play_stop, "Space");
        assert_eq!(cfg.keybinds.toggle_inspector, "I");
    }
}
