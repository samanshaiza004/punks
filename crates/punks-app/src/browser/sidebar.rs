//! The Sidebar pane: folder picker, library state, tags. Relocated out of
//! `browser/mod.rs` (M2 built it inline); pure module split, no behavior
//! change.

use gpui::prelude::*;
use gpui::{px, Context};
use gpui_component::{button::*, v_flex, ActiveTheme};

use super::{MainWindow, SIDEBAR_WIDTH};
use crate::actions::OpenFolder;
use crate::LibraryState;

impl MainWindow {
    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let library_state = inner.library_state();
        let tags = inner.library_tags().to_vec();
        let current_dir = inner
            .current_directory()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "No folder open".into());

        v_flex()
            .id("sidebar")
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .p_3()
            .gap_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(Button::new("open-folder").label("Open Folder...").on_click(
                cx.listener(|this, _, window, cx| this.open_folder(&OpenFolder, window, cx)),
            ))
            .child(current_dir)
            .child(match library_state {
                LibraryState::NotALibrary => "Library: not attached".to_string(),
                LibraryState::Scanning => "Library: scanning...".to_string(),
                LibraryState::Ready => "Library: ready".to_string(),
            })
            .child("Tags:")
            .children(
                tags.into_iter()
                    .map(|t| format!("  {} ({})", t.name, t.count)),
            )
    }
}
