//! The Inspector pane: Facts, Tags, Metadata, and Health sections for
//! whichever file is currently loaded (`SampleBrowser::current_file()` --
//! the same file the waveform/transport strip show, set by auditioning a
//! row or pressing Play). Every content mutation here calls
//! `SampleBrowser`'s own methods (`assign_tag`/`unassign_tag`/
//! `clear_override`/`set_description`/`check_library_health`) -- never
//! `command.rs`'s `Command` types directly. Those already construct the
//! right `Command`, call `execute_command`, and keep caches in sync (e.g.
//! `assign_tag`, `lib.rs:2015`); `inspector.rs` never imports `command.rs`
//! at all.

use std::path::Path;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{Context, Entity, Window};
use gpui_component::button::*;
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, Sizable};

use super::{MainWindow, INSPECTOR_WIDTH};
use crate::{Fact, HealthIssue, HealthIssueKind};

/// Display a `Fact` value as a plain string (integers without a trailing
/// `.0`). Ported verbatim from the old ImGui frontend's `fact_display`.
fn fact_display(f: &Fact) -> String {
    match f {
        Fact::Text(s) => s.clone(),
        Fact::Real(v) if v.fract() == 0.0 => format!("{}", *v as i64),
        Fact::Real(v) => format!("{v}"),
        Fact::Blob(_) => "<blob>".into(),
    }
}

/// One-line human-readable description of a metadata health issue. Ported
/// verbatim from the old ImGui frontend's `health_issue_summary`.
fn health_issue_summary(issue: &HealthIssue) -> String {
    match &issue.kind {
        HealthIssueKind::CacheDrift { embedded, cached } => {
            format!(
                "{:?} changed since last scan (file: \"{embedded}\", cached: \"{cached}\")",
                issue.field
            )
        }
        HealthIssueKind::OverrideDrift {
            embedded,
            overridden,
        } => format!(
            "{:?} disagrees with override (file: \"{embedded}\", override: \"{overridden}\")",
            issue.field
        ),
    }
}

/// Uppercase the first character. Ported verbatim from the old ImGui
/// frontend's `capitalize`.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Track length for the analysis readout. Ported verbatim from the old
/// ImGui frontend's `format_duration`.
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let total = secs as u64;
        format!("{}:{:02}", total / 60, total % 60)
    }
}

const FACT_METRICS: [&str; 3] = ["instrument", "key", "bpm"];

impl MainWindow {
    pub(super) fn new_description_input(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        cx.new(|cx| InputState::new(window, cx).placeholder("Description..."))
    }

    pub(super) fn render_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .id("inspector")
            .w(gpui::px(INSPECTOR_WIDTH))
            .h_full()
            .p_3()
            .gap_3()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(self.render_facts(cx))
            .child(self.render_tags(cx))
            .child(self.render_metadata(window, cx))
            .child(self.render_health(cx))
    }

    fn render_facts(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let path = inner.current_file().map(Path::to_path_buf);
        let duration = inner.current_track_info().map(|t| t.source_duration);
        let pending = inner.current_analysis_pending();

        v_flex()
            .id("facts")
            .gap_1()
            .child("Facts")
            .when_some(duration, |el, d| {
                el.child(format!("Length: {}", format_duration(d)))
            })
            .when(pending, |el| el.child("Analyzing..."))
            .children(FACT_METRICS.into_iter().map(|metric| {
                let value = inner
                    .current_resolved(metric)
                    .map(|f| fact_display(&f))
                    .unwrap_or_else(|| "--".into());
                let has_override = path
                    .as_deref()
                    .and_then(|p| inner.override_state(p, metric))
                    .is_some();
                let row_path = path.clone();

                h_flex()
                    .gap_2()
                    .child(format!("{}: {value}", capitalize(metric)))
                    .when(has_override, |el| {
                        el.child(
                            Button::new(format!("clear-override-{metric}"))
                                .label("Clear override")
                                .xsmall()
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    let Some(path) = row_path.clone() else {
                                        return;
                                    };
                                    this.browser.update(cx, |b, cx| {
                                        b.inner.clear_override(&path, metric);
                                        cx.notify();
                                    });
                                })),
                        )
                    })
            }))
    }

    fn render_tags(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let path = inner.current_file().map(Path::to_path_buf);
        let all_tags = inner.library_tags().to_vec();
        let assigned: Vec<i64> = path
            .as_deref()
            .map(|p| inner.tag_ids_for_path(p))
            .unwrap_or_default();

        v_flex()
            .id("tags")
            .gap_1()
            .child("Tags")
            .children(all_tags.into_iter().map(|tag| {
                let is_assigned = assigned.contains(&tag.id);
                let row_path = path.clone();
                let tag_id = tag.id;

                h_flex()
                    .gap_2()
                    .child(format!("{} ({})", tag.name, tag.count))
                    .child(
                        Button::new(("toggle-tag", tag.id as u64))
                            .label(if is_assigned { "Unassign" } else { "Assign" })
                            .xsmall()
                            .disabled(row_path.is_none())
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                let Some(path) = row_path.clone() else {
                                    return;
                                };
                                this.browser.update(cx, |b, cx| {
                                    if is_assigned {
                                        b.inner.unassign_tag(&path, tag_id);
                                    } else {
                                        b.inner.assign_tag(&path, tag_id);
                                    }
                                    cx.notify();
                                });
                            })),
                    )
            }))
    }

    fn render_metadata(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let path = inner.current_file().map(Path::to_path_buf);

        // Re-seed the description field when the inspected file changes, so
        // typing into one file's field never leaks onto the next.
        if self.description_seeded_for != path {
            let description = path
                .as_deref()
                .and_then(|p| inner.read_resolved_metadata(p))
                .and_then(|m| m.description)
                .map(|sourced| sourced.value)
                .unwrap_or_default();
            self.description_input.update(cx, |s, cx| {
                s.set_value(description, window, cx);
            });
            self.description_seeded_for = path.clone();
        }

        v_flex()
            .id("metadata")
            .gap_1()
            .child("Metadata")
            .child(Input::new(&self.description_input))
            .child(
                Button::new("save-description")
                    .label("Save Description")
                    .small()
                    .disabled(path.is_none())
                    .on_click(cx.listener(|this, _, _window, cx| {
                        let Some(path) = this.description_seeded_for.clone() else {
                            return;
                        };
                        let value = this.description_input.read(cx).value().to_string();
                        this.browser.update(cx, |b, cx| {
                            b.inner.set_description(&path, &value);
                            cx.notify();
                        });
                    })),
            )
    }

    fn render_health(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let browser = self.browser.read(cx);
        let inner = &browser.inner;
        let running = inner.health_check_running();
        let issues: Vec<HealthIssue> = inner.health_issues().unwrap_or(&[]).to_vec();

        v_flex()
            .id("health")
            .gap_1()
            .child(
                h_flex().gap_2().child("Health").child(
                    Button::new("run-health-check")
                        .label(if running {
                            "Checking..."
                        } else {
                            "Run Health Check"
                        })
                        .xsmall()
                        .disabled(running)
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.browser.update(cx, |b, cx| {
                                b.inner.check_library_health();
                                b.ensure_polling(cx);
                                cx.notify();
                            });
                        })),
                ),
            )
            .when(issues.is_empty() && !running, |el| {
                el.child("No issues found")
            })
            .children(
                issues
                    .iter()
                    .map(|issue| gpui::div().child(health_issue_summary(issue))),
            )
    }
}
