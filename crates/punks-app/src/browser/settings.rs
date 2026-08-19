//! M6 settings state for the eight keybindings that are functional in GPUI.
//!
//! The four tab keybind fields remain in `config::Keybinds` for compatibility,
//! but are intentionally absent here until the GPUI frontend has visible tabs.

use gpui::{AppContext, Context, Entity, Window};
use gpui_component::input::InputState;

use crate::config::{Keybinds, PunksConfig};

/// The settings form owns draft values in ordinary gpui-component inputs. The
/// persisted config is changed only by an explicit Apply click.
pub(crate) struct KeybindInputs {
    pub(crate) navigate_up: Entity<InputState>,
    pub(crate) navigate_down: Entity<InputState>,
    pub(crate) navigate_back: Entity<InputState>,
    pub(crate) confirm: Entity<InputState>,
    pub(crate) play_stop: Entity<InputState>,
    pub(crate) toggle_inspector: Entity<InputState>,
    pub(crate) undo: Entity<InputState>,
    pub(crate) redo: Entity<InputState>,
}

impl KeybindInputs {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<super::MainWindow>,
        keybinds: &Keybinds,
    ) -> Self {
        Self {
            navigate_up: new_input(window, cx, &keybinds.navigate_up),
            navigate_down: new_input(window, cx, &keybinds.navigate_down),
            navigate_back: new_input(window, cx, &keybinds.navigate_back),
            confirm: new_input(window, cx, &keybinds.confirm),
            play_stop: new_input(window, cx, &keybinds.play_stop),
            toggle_inspector: new_input(window, cx, &keybinds.toggle_inspector),
            undo: new_input(window, cx, &keybinds.undo),
            redo: new_input(window, cx, &keybinds.redo),
        }
    }

    pub(crate) fn read(&self, cx: &Context<super::MainWindow>) -> Keybinds {
        Keybinds {
            navigate_up: value(&self.navigate_up, cx),
            navigate_down: value(&self.navigate_down, cx),
            navigate_back: value(&self.navigate_back, cx),
            confirm: value(&self.confirm, cx),
            play_stop: value(&self.play_stop, cx),
            toggle_inspector: value(&self.toggle_inspector, cx),
            undo: value(&self.undo, cx),
            redo: value(&self.redo, cx),
            ..Keybinds::default()
        }
    }
}

fn new_input(
    window: &mut Window,
    cx: &mut Context<super::MainWindow>,
    initial: &str,
) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(initial, window, cx);
        state
    })
}

fn value(input: &Entity<InputState>, cx: &Context<super::MainWindow>) -> String {
    input.read(cx).value().to_string()
}

/// Normalize the user-entered key name for GPUI's keymap parser.
///
/// GPUI accepts names such as `space`, `left`, and `secondary-f`; whitespace
/// cannot be a key name and would otherwise save a setting that silently never
/// fires after restart.
pub(crate) fn normalize_key(value: &str) -> Result<String, &'static str> {
    let value = value.trim().to_lowercase();
    if value.is_empty() {
        return Err("Every shortcut must contain a key name.");
    }
    if value.chars().any(char::is_whitespace) {
        return Err("Shortcut names cannot contain whitespace.");
    }
    Ok(value)
}

/// Apply only the eight functional fields, preserving dormant tab fields and
/// every unrelated preference in the existing config.
pub(crate) fn apply_functional_keybinds(
    config: &mut PunksConfig,
    draft: &Keybinds,
) -> Result<(), &'static str> {
    config.keybinds.navigate_up = normalize_key(&draft.navigate_up)?;
    config.keybinds.navigate_down = normalize_key(&draft.navigate_down)?;
    config.keybinds.navigate_back = normalize_key(&draft.navigate_back)?;
    config.keybinds.confirm = normalize_key(&draft.confirm)?;
    config.keybinds.play_stop = normalize_key(&draft.play_stop)?;
    config.keybinds.toggle_inspector = normalize_key(&draft.toggle_inspector)?;
    config.keybinds.undo = normalize_key(&draft.undo)?;
    config.keybinds.redo = normalize_key(&draft.redo)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_accepts_gpui_style_names() {
        assert_eq!(normalize_key(" Space ").unwrap(), "space");
        assert_eq!(normalize_key("secondary-f").unwrap(), "secondary-f");
    }

    #[test]
    fn normalize_key_rejects_empty_and_whitespace() {
        assert!(normalize_key(" ").is_err());
        assert!(normalize_key("left arrow").is_err());
    }

    #[test]
    fn applying_functional_bindings_preserves_dormant_tab_fields() {
        let mut config = PunksConfig::default();
        config.keybinds.new_tab = "custom-new".into();
        config.keybinds.close_tab = "custom-close".into();
        config.keybinds.prev_tab = "custom-prev".into();
        config.keybinds.next_tab = "custom-next".into();

        let draft = Keybinds {
            navigate_up: "q".into(),
            navigate_down: "e".into(),
            navigate_back: "a".into(),
            confirm: "d".into(),
            play_stop: "space".into(),
            toggle_inspector: "i".into(),
            undo: "u".into(),
            redo: "r".into(),
            ..Keybinds::default()
        };
        apply_functional_keybinds(&mut config, &draft).unwrap();

        assert_eq!(config.keybinds.navigate_up, "q");
        assert_eq!(config.keybinds.play_stop, "space");
        assert_eq!(config.keybinds.new_tab, "custom-new");
        assert_eq!(config.keybinds.close_tab, "custom-close");
        assert_eq!(config.keybinds.prev_tab, "custom-prev");
        assert_eq!(config.keybinds.next_tab, "custom-next");
    }
}
