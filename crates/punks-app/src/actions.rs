//! GPUI actions: user/application intent, dispatched through GPUI's own
//! action system. Distinct from `command.rs`'s `Command` trait, which is
//! reversible *content* mutation (tag assignment, metadata writes, ...) —
//! see the module doc there. The action set here is intentionally small for
//! M2's shell; it grows as each feature (search, transport, undo/redo, ...)
//! actually lands.

use gpui::actions;

actions!(
    punks,
    [
        /// Open a folder to browse.
        OpenFolder,
        /// Move focus to the search input (`search.rs`).
        FocusSearch,
        /// Show/hide the Inspector pane.
        ToggleInspector,
        /// Move the Results cursor up one row (config: `keybinds.navigate_up`,
        /// default W), auto-selecting and auditioning the new row.
        CursorUp,
        /// Move the Results cursor down one row (config:
        /// `keybinds.navigate_down`, default S).
        CursorDown,
        /// Navigate up one directory, independent of selection (config:
        /// `keybinds.navigate_back`, default A).
        NavigateBack,
        /// Confirm the cursor row: enter a directory, or play a file (config:
        /// `keybinds.confirm`, default D; Enter is always bound too).
        Confirm,
        /// Toggle playback using the same transport logic as the Play/Stop button.
        TogglePlayback,
        /// Undo the active library command.
        Undo,
        /// Redo the active library command.
        Redo,
        /// Show or hide the Punks settings panel.
        ShowSettings,
    ]
);
