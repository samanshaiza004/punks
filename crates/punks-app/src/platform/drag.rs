//! External (OS-level) file drag-out.
//!
//! - **macOS**: GPUI's own `on_drag` + `external_drag_payload` path, proven
//!   end-to-end by the viability spike (a real file dropped into Finder,
//!   byte-identical to the source). No platform glue needed here.
//! - **Wayland**: same GPUI mechanism; upstream `gpui_linux` implements the
//!   source side of the Wayland drag protocol too, but this is
//!   source-verified only -- no Linux hardware available to run it.
//! - **Windows**: GPUI's window layer has no source-side promotion, so
//!   `platform/windows_drag.rs` supplies a private OLE `IDataObject` /
//!   `IDropSource` bridge.
//! - **Linux X11**: GPUI's X11 backend has only a drop-target implementation,
//!   so `platform/x11_drag.rs` supplies the private XDND source side.

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
        crate::platform::windows_drag::start(_paths);

        #[cfg(target_os = "linux")]
        crate::platform::x11_drag::start(_paths);

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
