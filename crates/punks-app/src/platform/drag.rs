//! External (OS-level) file drag-out.
//!
//! - **macOS**: GPUI's own `on_drag` + `external_drag_payload` path, proven
//!   end-to-end by the viability spike (a real file dropped into Finder,
//!   byte-identical to the source). No platform glue needed here.
//! - **Wayland**: same GPUI mechanism; upstream `gpui_linux` implements the
//!   source side of the Wayland drag protocol too, but this is
//!   source-verified only -- no Linux hardware available to run it.
//! - **Windows / X11**: GPUI's window layer never promotes an in-app drag to
//!   a native OS session there (confirmed by reading `gpui_windows` and
//!   `gpui_linux/x11`: neither overrides `start_external_drag`, both fall
//!   back to a hardcoded `false`). Windows gets a temporary stopgap below,
//!   reusing the same `drag` crate this app used before the GPUI rewrite, so
//!   Windows users don't lose drag-out during the transition. X11 has no
//!   stopgap (never worked before the rewrite either, so nothing regresses)
//!   and is deferred, along with retiring this stopgap, to a real native
//!   XDND / OLE bridge in a later milestone.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    ExternalDragPayload, FileDragPaths, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme};

/// One or more files being dragged out (drag-out of a multi-selection sends
/// every selected path in one gesture).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragPaths(pub Vec<PathBuf>);

/// Attaches OS-level external file drag-out to a stateful element (must
/// already have `.id(...)` called, i.e. be `Stateful<Div>` or similar).
pub fn draggable<E: StatefulInteractiveElement>(el: E, paths: DragPaths) -> E {
    let ghost_label: SharedString = paths
        .0
        .first()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string()
        .into();

    el.on_drag(paths, move |_paths, position, _window, cx| {
        #[cfg(target_os = "windows")]
        start_windows_drag_stopgap(_window, _paths);

        cx.new(|_| DragGhost {
            label: ghost_label.clone(),
            position,
        })
    })
    .external_drag_payload(move |paths: &DragPaths, _window, _cx| {
        Some(ExternalDragPayload::Files(FileDragPaths::new(
            paths.0.iter().cloned().map(|path| (path, false)),
        )))
    })
}

/// The floating label that follows the cursor during the in-app portion of a
/// drag gesture, before (or instead of, on platforms without promotion) the
/// OS takes over.
struct DragGhost {
    label: SharedString,
    position: Point<Pixels>,
}

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .pl(self.position.x - gpui::px(60.))
            .pt(self.position.y - gpui::px(14.))
            .child(
                v_flex()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(cx.theme().accent)
                    .text_color(cx.theme().accent_foreground)
                    .text_xs()
                    .shadow_md()
                    .child(self.label.clone()),
            )
    }
}

/// `// TODO(M6): replace with a native OLE IDropSource/DoDragDrop bridge.`
///
/// Reuses the `drag` crate this app called from the ImGui/winit frontend
/// before the GPUI rewrite -- `gpui::Window` implements
/// `raw_window_handle::HasWindowHandle`, exactly what `drag::start_drag`
/// requires, so this is a straight port of the old call site onto the new
/// window type. **Not independently verified**: no Windows hardware is
/// available in this environment. Source-verified only.
#[cfg(target_os = "windows")]
fn start_windows_drag_stopgap(window: &Window, paths: &DragPaths) {
    const DRAG_PREVIEW_ICON_PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 255, 255, 63, 0,
        5, 254, 2, 254, 167, 53, 129, 207, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    let drag_paths: Vec<PathBuf> = paths
        .0
        .iter()
        .filter_map(|path| match std::fs::canonicalize(path) {
            Ok(path) => Some(path),
            Err(err) => {
                log::warn!("skipping drag path that could not be canonicalized {path:?}: {err}");
                None
            }
        })
        .collect();
    if drag_paths.is_empty() {
        log::error!("failed to start Windows drag: no valid paths remain");
        return;
    }

    let item = drag::DragItem::Files(drag_paths);
    let preview = drag::Image::Raw(DRAG_PREVIEW_ICON_PNG.to_vec());
    if let Err(err) = drag::start_drag(
        window,
        item,
        preview,
        |result, _cursor_position| {
            log::debug!("drag finished: {result:?}");
        },
        Default::default(),
    ) {
        log::error!("failed to start Windows drag operation: {err}");
    }
}
