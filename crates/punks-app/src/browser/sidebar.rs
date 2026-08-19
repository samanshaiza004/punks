//! The Sidebar pane: folder picker, library state, tags. Relocated out of
//! `browser/mod.rs` (M2 built it inline); pure module split, no behavior
//! change.

use gpui::prelude::*;
use gpui::{px, Context};
use gpui_component::{button::*, h_flex, v_flex, ActiveTheme, Disableable, Sizable};

use super::{MainWindow, SIDEBAR_WIDTH};
use crate::actions::OpenFolder;
use crate::LibraryState;

impl MainWindow {
    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let library_state = inner.library_state();
        let tags = inner.library_tags().to_vec();
        let breadcrumbs = inner.breadcrumbs();
        let can_navigate_up = inner.can_navigate_up();

        let mut breadcrumb_row = h_flex().items_center().gap_1();
        if breadcrumbs.is_empty() {
            breadcrumb_row = breadcrumb_row.child("No folder open");
        } else {
            let last = breadcrumbs.len() - 1;
            for (level, crumb) in breadcrumbs.into_iter().enumerate() {
                if level > 0 {
                    breadcrumb_row = breadcrumb_row.child(">".to_string());
                }
                if level < last {
                    breadcrumb_row = breadcrumb_row.child(
                        Button::new(("breadcrumb", level as u64))
                            .label(crumb)
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.navigate_to_breadcrumb(level, cx);
                            })),
                    );
                } else {
                    breadcrumb_row = breadcrumb_row.child(
                        gpui::div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(crumb),
                    );
                }
            }
        }

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
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("navigate-up")
                            .label("↑")
                            .ghost()
                            .small()
                            .disabled(!can_navigate_up)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.navigate_back(window, cx);
                            })),
                    )
                    .child(breadcrumb_row),
            )
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

    fn navigate_to_breadcrumb(&mut self, level: usize, cx: &mut Context<Self>) {
        self.browser.update(cx, |browser, cx| {
            if let Err(error) = browser.inner.navigate_to_breadcrumb(level) {
                log::warn!("breadcrumb navigation failed: {error}");
            }
            browser.ensure_polling(cx);
            cx.notify();
        });
    }
}
