//! Manual visual gate for imgui-painter. Two sections:
//!
//! - **Phase 1** (design doc §12 step 1): three hand-built "looks" — a
//!   macOS-style panel, a Fluent-style button, a GitHub-style button — via
//!   imgui-painter, each next to a plain-`ImDrawList` attempt at the same look,
//!   so a human can judge whether Painter alone (gradients/shadows/borders on
//!   `rounded_rect`) renders convincingly.
//! - **Phase 5** (`draw_decorated_widgets`): stock `ui.button()`,
//!   `ui.selectable()`, `ui.checkbox()`, and single-line `ui.input_text()`
//!   calls driven by one shared `Material` + `Decorator` API, with no wrapper
//!   widget.
//!
//! The human's judgment — not a test suite — is the pass/fail gate for both.
//!
//! Run with `cargo run -p imgui-painter --example painter_demo`.

#[path = "../common/mod.rs"]
mod common;

use imgui_painter::{
    adapter, item_paint, rgba, Border, ColorStop, Decorator, Gradient, GradientMode, Material,
    Painter, Rect as PainterRect, Session, Shadow, StateColors, Vec2 as PainterVec2,
};

fn pv2(x: f32, y: f32) -> PainterVec2 {
    PainterVec2 { x, y }
}

// --- The three looks: `_plain` draws with vanilla imgui-rs ImDrawList
// calls, `_painted` draws the same intent through imgui-painter. Colors
// deliberately match between the pair so the only variable a viewer judges
// is the rendering technique, not a color choice. ---

fn draw_macos_panel_plain(ui: &imgui::Ui, pos: [f32; 2], size: [f32; 2]) {
    let max = [pos[0] + size[0], pos[1] + size[1]];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect(pos, max, rgba(240, 240, 242, 255))
        .filled(true)
        .rounding(12.0)
        .build();
    draw_list
        .add_rect(pos, max, rgba(210, 210, 214, 255))
        .rounding(12.0)
        .thickness(1.0)
        .build();
}

fn draw_macos_panel_painted(
    painter: &mut Session,
    white_uv: PainterVec2,
    dl: *mut imgui::sys::ImDrawList,
    pos: [f32; 2],
    size: [f32; 2],
) {
    let rect = PainterRect {
        min: pv2(pos[0], pos[1]),
        max: pv2(pos[0] + size[0], pos[1] + size[1]),
    };
    painter.begin(white_uv);
    painter.rounded_rect(rect, 12.0);
    painter.add_shadow(&Shadow {
        offset: pv2(0.0, 6.0),
        blur: 24.0,
        spread: 2.0,
        color: rgba(0, 0, 0, 60),
        inset: false,
    });
    painter.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: pv2(pos[0], pos[1]),
        to: pv2(pos[0], pos[1] + size[1]),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: rgba(248, 248, 250, 255),
            },
            ColorStop {
                t: 1.0,
                color: rgba(228, 228, 232, 255),
            },
        ],
    });
    painter.add_border(&Border {
        thickness: 1.0,
        color: rgba(210, 210, 214, 255),
    });
    let mesh = painter.end();
    unsafe { adapter::paint_to_draw_list(dl, &mesh) };
}

fn draw_fluent_button_plain(ui: &imgui::Ui, pos: [f32; 2], size: [f32; 2]) {
    let max = [pos[0] + size[0], pos[1] + size[1]];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect(pos, max, rgba(0, 103, 192, 255))
        .filled(true)
        .rounding(4.0)
        .build();
}

fn draw_fluent_button_painted(
    painter: &mut Session,
    white_uv: PainterVec2,
    dl: *mut imgui::sys::ImDrawList,
    pos: [f32; 2],
    size: [f32; 2],
) {
    let rect = PainterRect {
        min: pv2(pos[0], pos[1]),
        max: pv2(pos[0] + size[0], pos[1] + size[1]),
    };
    painter.begin(white_uv);
    painter.rounded_rect(rect, 4.0);
    painter.add_shadow(&Shadow {
        offset: pv2(0.0, 2.0),
        blur: 6.0,
        spread: 0.0,
        color: rgba(0, 0, 0, 70),
        inset: false,
    });
    painter.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: pv2(pos[0], pos[1]),
        to: pv2(pos[0], pos[1] + size[1]),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: rgba(0, 120, 215, 255),
            },
            ColorStop {
                t: 1.0,
                color: rgba(0, 90, 180, 255),
            },
        ],
    });
    let mesh = painter.end();
    unsafe { adapter::paint_to_draw_list(dl, &mesh) };
}

fn draw_github_button_plain(ui: &imgui::Ui, pos: [f32; 2], size: [f32; 2]) {
    let max = [pos[0] + size[0], pos[1] + size[1]];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect(pos, max, rgba(246, 248, 250, 255))
        .filled(true)
        .rounding(6.0)
        .build();
    draw_list
        .add_rect(pos, max, rgba(31, 35, 40, 45))
        .rounding(6.0)
        .thickness(1.0)
        .build();
}

fn draw_github_button_painted(
    painter: &mut Session,
    white_uv: PainterVec2,
    dl: *mut imgui::sys::ImDrawList,
    pos: [f32; 2],
    size: [f32; 2],
) {
    let rect = PainterRect {
        min: pv2(pos[0], pos[1]),
        max: pv2(pos[0] + size[0], pos[1] + size[1]),
    };
    painter.begin(white_uv);
    painter.rounded_rect(rect, 6.0);
    painter.add_shadow(&Shadow {
        offset: pv2(0.0, 1.0),
        blur: 2.0,
        spread: 0.0,
        color: rgba(31, 35, 40, 35),
        inset: false,
    });
    painter.fill_color(rgba(246, 248, 250, 255));
    painter.add_border(&Border {
        thickness: 1.0,
        color: rgba(31, 35, 40, 45),
    });
    let mesh = painter.end();
    unsafe { adapter::paint_to_draw_list(dl, &mesh) };
}

const BOX_W: f32 = 220.0;
const BOX_H: f32 = 90.0;
const GAP_X: f32 = 60.0;
const GAP_Y: f32 = 70.0;
const LABEL_H: f32 = 22.0;

fn draw_demo(ui: &imgui::Ui, painter: &mut Session) {
    ui.text("imgui-painter phase 1 \u{2014} three looks gate (design doc \u{a7}12 step 1)");
    ui.text_disabled("Left column: plain ImDrawList.  Right column: imgui-painter.");
    ui.separator();
    ui.spacing();

    // SAFETY: called once per frame while this window's draw list is the
    // active one, matching igGetWindowDrawList's normal per-frame usage.
    let white_uv = unsafe { adapter::white_pixel_uv() };
    let origin = ui.cursor_screen_pos();

    let rows: [(&str, PlainFn, PaintedFn); 3] = [
        (
            "macOS-style panel",
            draw_macos_panel_plain,
            draw_macos_panel_painted,
        ),
        (
            "Fluent-style button",
            draw_fluent_button_plain,
            draw_fluent_button_painted,
        ),
        (
            "GitHub-style button",
            draw_github_button_plain,
            draw_github_button_painted,
        ),
    ];

    for (row, (label, plain_fn, painted_fn)) in rows.into_iter().enumerate() {
        let y = origin[1] + row as f32 * (BOX_H + LABEL_H + GAP_Y);
        ui.set_cursor_screen_pos([origin[0], y]);
        ui.text(label);

        let plain_pos = [origin[0], y + LABEL_H];
        plain_fn(ui, plain_pos, [BOX_W, BOX_H]);

        let painted_pos = [origin[0] + BOX_W + GAP_X, y + LABEL_H];
        // SAFETY: this window's draw list is the currently active one for
        // the duration of this call (same frame, same window scope).
        let dl = unsafe { imgui::sys::igGetWindowDrawList() };
        painted_fn(painter, white_uv, dl, painted_pos, [BOX_W, BOX_H]);
    }

    // The raw draw-list calls above don't advance imgui's own layout
    // cursor; reserve the space explicitly so window sizing/scrolling stays
    // correct.
    let content_bottom = origin[1] + rows.len() as f32 * (BOX_H + LABEL_H + GAP_Y);
    ui.set_cursor_screen_pos([origin[0], content_bottom]);
}

type PlainFn = fn(&imgui::Ui, [f32; 2], [f32; 2]);
type PaintedFn = fn(&mut Session, PainterVec2, *mut imgui::sys::ImDrawList, [f32; 2], [f32; 2]);

/// The Phase 5 section: broader stock-widget anatomy exercised through one Material.
fn draw_decorated_widgets(
    ui: &imgui::Ui,
    painter: &mut Painter,
    checked: &mut bool,
    input: &mut String,
) {
    ui.spacing();
    ui.separator();
    ui.text("imgui-painter phase 5 \u{2014} widget breadth gate");
    ui.text_disabled("One Material across Button, Selectable, Checkbox, and InputText.");
    ui.spacing();

    let primary = Material {
        radius: 6.0,
        fill: StateColors {
            base: rgba(64, 102, 168, 255),
            hover: rgba(96, 144, 224, 255),
            active: rgba(70, 104, 168, 255),
        },
        border: Border {
            thickness: 1.0,
            color: rgba(255, 255, 255, 48),
        },
        shadow: Some(Shadow {
            offset: pv2(0.0, 2.0),
            blur: 10.0,
            spread: 0.0,
            color: rgba(0, 0, 0, 90),
            inset: false,
        }),
    };

    let mut frame = painter.begin_frame();
    // SAFETY: all calls run inside the current ImGui window and frame, no
    // caller-owned channel split is active, and each closure emits exactly one
    // stock widget matching its Decorator.
    unsafe {
        item_paint(&mut frame, Decorator::Button, &primary, || {
            ui.button("Save##dec")
        });
    }
    ui.spacing();
    ui.set_next_item_width(260.0);
    unsafe {
        item_paint(&mut frame, Decorator::Selectable, &primary, || {
            ui.selectable("A selectable row##dec")
        });
    }
    ui.spacing();
    unsafe {
        item_paint(&mut frame, Decorator::Checkbox, &primary, || {
            ui.checkbox("Enable processing##dec", checked)
        });
    }
    ui.spacing();
    ui.set_next_item_width(260.0);
    unsafe {
        item_paint(&mut frame, Decorator::InputText, &primary, || {
            ui.input_text("Name##dec_input", input)
                .hint("Type, select, copy, paste")
                .build()
        });
    }
}

fn main() {
    let mut session = Session::new();
    let mut checked = false;
    let mut input = String::new();
    common::run("imgui-painter demo", move |ui, painter| {
        draw_demo(ui, &mut session);
        draw_decorated_widgets(ui, painter, &mut checked, &mut input);
    });
}
