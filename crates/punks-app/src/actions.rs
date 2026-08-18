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
        /// Open a folder to browse. Bound to a menu/button in M2; a
        /// dedicated `Focus Search` keybinding lands with `search.rs`.
        OpenFolder,
        /// Show/hide the Inspector pane.
        ToggleInspector,
    ]
);
