//! Manual visual gate for imgui-painter. Three sections:
//!
//! - **Phase 1** (design doc §12 step 1): three hand-built "looks" — a
//!   macOS-style panel, a Fluent-style button, a GitHub-style button — via
//!   imgui-painter, each next to a plain-`ImDrawList` attempt at the same look,
//!   so a human can judge whether Painter alone (gradients/shadows/borders on
//!   `rounded_rect`) renders convincingly.
//! - **Phase 6** (`draw_decorated_widgets`): stock `ui.button()`,
//!   `ui.selectable()`, `ui.checkbox()`, and single-line `ui.input_text()`
//!   calls driven by one shared `Material` through typed decoration entry
//!   points, with no wrapper widget.
//! - **Phase 7** (`draw_styling_depth`): an Ableton-inspired layered surface
//!   and normal/hover/pressed/focus treatments composed from inset shadows,
//!   bevel bands, stacked borders, layered gradients, gloss, and DPI-aware
//!   hairlines.
//! - **Phase 8** (`draw_widget_anatomy`): stock Slider, Combo, and TreeNode
//!   controls exercising value, popup, and hierarchy anatomy while preserving
//!   ImGui's foreground and behavior.
//! - **Phase 9** (`draw_recipe_rack`): a token palette and fixed recipe family
//!   composed into an Ableton-inspired rack visual gate.
//!
//! The human's judgment — not a test suite — is the pass/fail gate for all three.
//!
//! Run with `cargo run -p imgui-painter --example painter_demo`.

#[path = "../common/mod.rs"]
mod common;

use imgui_painter::recipes::{
    browser_tree_row, combo_field, inset_control, inset_panel, panel, parameter_slider,
    raised_button, selected_row, toolbar_button, Palette,
};
use imgui_painter::{
    adapter, decorate_button, decorate_checkbox, decorate_combo, decorate_input_text,
    decorate_selectable, decorate_slider_f32, decorate_tree_node, rgba, Border, Canvas, Color,
    ColorStop, ComboStyle, Gradient, GradientMode, Material, Painter, Rect as PainterRect, Session,
    Shadow, SliderStyle, StateColors, TreeStyle, Vec2 as PainterVec2,
};

fn pv2(x: f32, y: f32) -> PainterVec2 {
    PainterVec2 { x, y }
}

fn style_color(color: Color) -> [f32; 4] {
    [
        (color & 0xff) as f32 / 255.0,
        ((color >> 8) & 0xff) as f32 / 255.0,
        ((color >> 16) & 0xff) as f32 / 255.0,
        ((color >> 24) & 0xff) as f32 / 255.0,
    ]
}

#[derive(Clone, Copy)]
enum ChromeState {
    Normal,
    Hover,
    Pressed,
    Focus,
}

struct ChromePalette {
    top: Color,
    middle: Color,
    bottom: Color,
    gloss_alpha: u8,
    inset_alpha: u8,
}

fn chrome_palette(state: ChromeState) -> ChromePalette {
    match state {
        ChromeState::Normal | ChromeState::Focus => ChromePalette {
            top: rgba(91, 119, 137, 255),
            middle: rgba(68, 91, 108, 255),
            bottom: rgba(47, 62, 74, 255),
            gloss_alpha: 42,
            inset_alpha: 70,
        },
        ChromeState::Hover => ChromePalette {
            top: rgba(108, 140, 160, 255),
            middle: rgba(79, 106, 125, 255),
            bottom: rgba(53, 70, 83, 255),
            gloss_alpha: 58,
            inset_alpha: 62,
        },
        ChromeState::Pressed => ChromePalette {
            top: rgba(43, 57, 68, 255),
            middle: rgba(57, 76, 90, 255),
            bottom: rgba(70, 92, 108, 255),
            gloss_alpha: 16,
            inset_alpha: 108,
        },
    }
}

/// One reusable ordered paint recipe. Every visual layer is a Painter
/// primitive; callers provide only geometry, state, and device scale.
fn paint_layered_chrome(canvas: &mut Canvas<'_>, rect: PainterRect, state: ChromeState) {
    let palette = chrome_palette(state);
    let hairline = canvas.device_pixel();
    let height = rect.max.y - rect.min.y;

    canvas.rounded_rect(rect, 6.0);

    // Background stage: focus glow, then the ordinary elevation shadow.
    if matches!(state, ChromeState::Focus) {
        canvas.add_shadow(&Shadow {
            offset: pv2(0.0, 0.0),
            blur: 10.0,
            spread: 2.0,
            color: rgba(86, 185, 255, 95),
            inset: false,
        });
    }
    canvas.add_shadow(&Shadow {
        offset: pv2(
            0.0,
            if matches!(state, ChromeState::Pressed) {
                1.0
            } else {
                3.0
            },
        ),
        blur: if matches!(state, ChromeState::Pressed) {
            3.0
        } else {
            9.0
        },
        spread: 0.0,
        color: rgba(0, 0, 0, 105),
        inset: false,
    });

    // Surface stage: a multi-stop base plus a second translucent gradient
    // clipped to the top half. The layers remain independently tunable.
    canvas.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: pv2(rect.min.x, rect.min.y),
        to: pv2(rect.min.x, rect.max.y),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: palette.top,
            },
            ColorStop {
                t: 0.48,
                color: palette.middle,
            },
            ColorStop {
                t: 1.0,
                color: palette.bottom,
            },
        ],
    });
    canvas.fill_band_gradient(
        rect.min.y,
        rect.min.y + height * 0.52,
        &Gradient {
            mode: GradientMode::Linear,
            from: pv2(rect.min.x, rect.min.y),
            to: pv2(rect.min.x, rect.min.y + height * 0.52),
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(255, 255, 255, palette.gloss_alpha),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(255, 255, 255, 0),
                },
            ],
        },
    );

    // Bevel/gloss stage: a device-pixel top highlight, a soft lower shade,
    // then an inset shadow whose offset makes the pressed state feel sunk.
    canvas.fill_band_color(
        rect.min.y + hairline,
        rect.min.y + hairline * 2.0,
        rgba(
            255,
            255,
            255,
            if matches!(state, ChromeState::Pressed) {
                28
            } else {
                88
            },
        ),
    );
    canvas.fill_band_gradient(
        rect.max.y - 9.0,
        rect.max.y,
        &Gradient {
            mode: GradientMode::Linear,
            from: pv2(rect.min.x, rect.max.y - 9.0),
            to: pv2(rect.min.x, rect.max.y),
            stops: vec![
                ColorStop {
                    t: 0.0,
                    color: rgba(0, 0, 0, 0),
                },
                ColorStop {
                    t: 1.0,
                    color: rgba(0, 0, 0, 48),
                },
            ],
        },
    );
    canvas.add_shadow(&Shadow {
        offset: pv2(
            0.0,
            if matches!(state, ChromeState::Pressed) {
                2.0
            } else {
                -1.0
            },
        ),
        blur: if matches!(state, ChromeState::Pressed) {
            8.0
        } else {
            5.0
        },
        spread: hairline,
        color: rgba(0, 0, 0, palette.inset_alpha),
        inset: true,
    });

    // Foreground stage: two genuinely distinct hairline outlines. Focus adds
    // one more inset outline without replacing the underlying chrome.
    canvas.add_border(&Border {
        thickness: hairline,
        color: rgba(12, 18, 22, 235),
    });
    canvas.add_border_inset(
        hairline,
        &Border {
            thickness: hairline,
            color: rgba(255, 255, 255, 42),
        },
    );
    if matches!(state, ChromeState::Focus) {
        canvas.add_border_inset(
            hairline * 2.0,
            &Border {
                thickness: hairline,
                color: rgba(103, 196, 255, 210),
            },
        );
    }
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
    // matching stock widget.
    unsafe {
        decorate_button(&mut frame, &primary, || ui.button("Save##dec"));
    }
    ui.spacing();
    ui.set_next_item_width(260.0);
    unsafe {
        decorate_selectable(&mut frame, &primary, || {
            ui.selectable("A selectable row##dec")
        });
    }
    ui.spacing();
    unsafe {
        decorate_checkbox(&mut frame, &primary, || {
            ui.checkbox("Enable processing##dec", checked)
        });
    }
    ui.spacing();
    ui.set_next_item_width(260.0);
    unsafe {
        decorate_input_text(&mut frame, &primary, || {
            ui.input_text("Name##dec_input", input)
                .hint("Type, select, copy, paste")
                .build()
        });
    }
}

fn draw_styling_depth(ui: &imgui::Ui, painter: &mut Painter) {
    ui.spacing();
    ui.separator();
    ui.text("imgui-painter phase 7 \u{2014} styling depth gate");
    ui.text_disabled("Inset shadow + bevel + layered gradients + gloss + stacked DPI hairlines.");
    ui.spacing();

    // SAFETY: this function runs inside the current window and frame; the
    // canvas drops before that draw list or the white atlas UV expires.
    let dl = unsafe { imgui::sys::igGetWindowDrawList() };
    let origin = ui.cursor_screen_pos();
    let swatch_size = [150.0, 58.0];
    let gap = 18.0;
    let states = [
        ("Normal", ChromeState::Normal),
        ("Hover", ChromeState::Hover),
        ("Pressed", ChromeState::Pressed),
        ("Focus", ChromeState::Focus),
    ];

    let live_y = origin[1] + swatch_size[1] + 24.0;
    ui.set_cursor_screen_pos([origin[0], live_y]);
    ui.invisible_button("##phase7_live_chrome", [318.0, 58.0]);
    let live_state = if ui.is_item_active() {
        ChromeState::Pressed
    } else if ui.is_item_focused() {
        ChromeState::Focus
    } else if ui.is_item_hovered() {
        ChromeState::Hover
    } else {
        ChromeState::Normal
    };
    let live_rect = PainterRect {
        min: pv2(origin[0], live_y),
        max: pv2(origin[0] + 318.0, live_y + 58.0),
    };

    {
        let mut frame = painter.begin_frame();
        let mut canvas = unsafe { frame.canvas(dl) };
        for (index, (_, state)) in states.into_iter().enumerate() {
            let x = origin[0] + index as f32 * (swatch_size[0] + gap);
            let rect = PainterRect {
                min: pv2(x, origin[1]),
                max: pv2(x + swatch_size[0], origin[1] + swatch_size[1]),
            };
            paint_layered_chrome(&mut canvas, rect, state);
        }
        paint_layered_chrome(&mut canvas, live_rect, live_state);
    }

    for (index, (label, _)) in states.into_iter().enumerate() {
        let x = origin[0] + index as f32 * (swatch_size[0] + gap);
        ui.get_window_draw_list().add_text(
            [x + 12.0, origin[1] + 20.0],
            rgba(235, 242, 247, 255),
            label,
        );
    }
    ui.get_window_draw_list().add_text(
        [origin[0] + 14.0, live_y + 20.0],
        rgba(235, 242, 247, 255),
        "Live: hover, hold, click for focus",
    );

    ui.set_cursor_screen_pos([origin[0], live_y + 76.0]);
}

/// Phase 8's three anatomy classes: value, popup, and hierarchical controls.
fn draw_widget_anatomy(
    ui: &imgui::Ui,
    painter: &mut Painter,
    gain: &mut f32,
    disabled_gain: &mut f32,
    mode: &mut usize,
    combo_input: &mut String,
    tree_selection: &mut usize,
) {
    const MODES: [&str; 3] = ["Classic", "Texture", "Transient"];
    ui.spacing();
    ui.separator();
    ui.text("imgui-painter phase 8 \u{2014} widget anatomy gate");
    ui.text_disabled("Slider = value parts; Combo = parent + popup; TreeNode = row + disclosure.");
    ui.spacing();

    let material = Material {
        radius: 4.0,
        fill: StateColors {
            base: rgba(62, 119, 150, 255),
            hover: rgba(77, 149, 187, 255),
            active: rgba(43, 92, 119, 255),
        },
        border: Border {
            thickness: 1.0,
            color: rgba(10, 18, 23, 220),
        },
        shadow: Some(Shadow {
            offset: pv2(0.0, 1.0),
            blur: 4.0,
            spread: 0.0,
            color: rgba(0, 0, 0, 90),
            inset: false,
        }),
    };
    let slider_style = SliderStyle {
        track: Material {
            fill: StateColors {
                base: rgba(31, 51, 62, 255),
                hover: rgba(38, 65, 79, 255),
                active: rgba(26, 45, 55, 255),
            },
            shadow: None,
            ..material
        },
        fill: material,
        grab: Material {
            fill: StateColors {
                base: rgba(114, 201, 230, 255),
                hover: rgba(142, 221, 246, 255),
                active: rgba(83, 166, 199, 255),
            },
            ..material
        },
    };
    let combo_style = ComboStyle {
        frame: material,
        arrow_region: Material {
            fill: StateColors {
                base: rgba(42, 82, 103, 255),
                hover: rgba(53, 107, 134, 255),
                active: rgba(31, 68, 88, 255),
            },
            shadow: None,
            ..material
        },
    };
    let tree_style = TreeStyle {
        row: material,
        disclosure: Material {
            fill: StateColors {
                base: rgba(42, 82, 103, 255),
                hover: rgba(53, 107, 134, 255),
                active: rgba(31, 68, 88, 255),
            },
            shadow: None,
            ..material
        },
    };

    let mut frame = painter.begin_frame();
    ui.set_next_item_width(300.0);
    // SAFETY: each closure submits exactly the documented stock widget in
    // the current window, and no caller-owned channel split is active.
    unsafe {
        decorate_slider_f32(&mut frame, &slider_style, 0.0, 1.0, gain, |value| {
            ui.slider_config("Gain", 0.0, 1.0)
                .display_format("%.2f")
                .build(value)
        });
    }

    ui.set_next_item_width(300.0);
    let preview = MODES[*mode];
    unsafe {
        decorate_combo(
            &mut frame,
            &combo_style,
            || ui.begin_combo("Mode", preview),
            |_token| {
                // These stock controls must retain ordinary chrome while the
                // parent Combo's FrameBg*/Button* colors are suppressed.
                ui.button("Popup button");
                ui.input_text("Popup input", combo_input).build();
                ui.separator();
                for (index, label) in MODES.iter().enumerate() {
                    if ui.selectable_config(label).selected(index == *mode).build() {
                        *mode = index;
                    }
                }
            },
        );
    }

    ui.set_next_item_width(300.0);
    unsafe {
        decorate_combo(
            &mut frame,
            &combo_style,
            || ui.begin_combo("##hidden_mode", MODES[*mode]),
            |_token| {
                ui.text_disabled("Hidden-label Combo uses the same frame anatomy.");
            },
        );
    }

    ui.spacing();
    ui.text("Browser tree");
    let branch_flags = imgui::TreeNodeFlags::SPAN_AVAIL_WIDTH | imgui::TreeNodeFlags::OPEN_ON_ARROW;
    let root = unsafe {
        decorate_tree_node(&mut frame, &tree_style, *tree_selection == 0, false, || {
            ui.tree_node_config("Library##phase8_tree")
                .flags(branch_flags | imgui::TreeNodeFlags::DEFAULT_OPEN)
                .selected(*tree_selection == 0)
                .push()
        })
    };
    if ui.is_item_clicked() {
        *tree_selection = 0;
    }
    if let Some(root_token) = root {
        let branch = unsafe {
            decorate_tree_node(&mut frame, &tree_style, *tree_selection == 1, false, || {
                ui.tree_node_config("Drums##phase8_tree")
                    .flags(branch_flags | imgui::TreeNodeFlags::DEFAULT_OPEN)
                    .selected(*tree_selection == 1)
                    .push()
            })
        };
        if ui.is_item_clicked() {
            *tree_selection = 1;
        }
        if let Some(branch_token) = branch {
            let leaf_flags = imgui::TreeNodeFlags::SPAN_AVAIL_WIDTH
                | imgui::TreeNodeFlags::LEAF
                | imgui::TreeNodeFlags::NO_TREE_PUSH_ON_OPEN;
            let leaf = unsafe {
                decorate_tree_node(&mut frame, &tree_style, *tree_selection == 2, true, || {
                    ui.tree_node_config("Kick 01.wav##phase8_tree")
                        .flags(leaf_flags)
                        .selected(*tree_selection == 2)
                        .push()
                })
            };
            drop(leaf);
            if ui.is_item_clicked() {
                *tree_selection = 2;
            }
            branch_token.end();
        }
        root_token.end();
    }

    ui.spacing();
    ui.text("Disabled alpha");
    ui.disabled(true, || {
        ui.set_next_item_width(300.0);
        unsafe {
            decorate_slider_f32(
                &mut frame,
                &slider_style,
                0.0,
                1.0,
                disabled_gain,
                |value| ui.slider("Disabled gain", 0.0, 1.0, value),
            );
        }
    });
}

struct RackState {
    tree_selection: usize,
    parameters: [f32; 3],
    mode: usize,
    row_selection: usize,
    name: String,
    disabled_parameter: f32,
}

/// Phase 9's token recipes composed without app-owned draw-list geometry.
fn draw_recipe_rack(ui: &imgui::Ui, painter: &mut Painter, state: &mut RackState) {
    const MODES: [&str; 3] = ["Warp", "Repitch", "Complex"];
    let palette = Palette {
        surface: rgba(61, 73, 82, 255),
        surface_raised: rgba(83, 99, 110, 255),
        surface_inset: rgba(34, 43, 49, 255),
        border_light: rgba(132, 151, 163, 155),
        border_dark: rgba(16, 22, 26, 235),
        accent: rgba(72, 173, 222, 255),
        selection: rgba(54, 116, 151, 255),
        text: rgba(231, 238, 242, 255),
        text_muted: rgba(158, 174, 184, 255),
    };

    ui.spacing();
    ui.separator();
    ui.text("imgui-painter phase 9 \u{2014} recipe rack gate");
    {
        let _muted = ui.push_style_color(imgui::StyleColor::Text, style_color(palette.text_muted));
        ui.text("Palette tokens + stock ImGui behavior + recipe-owned chrome.");
    }
    ui.spacing();

    let line_height = ui.text_line_height_with_spacing();
    let frame_height = ui.frame_height_with_spacing();
    let padding = line_height * 0.7;
    let rack_width = ui.content_region_avail()[0].clamp(420.0, 900.0);
    let rack_height = frame_height * 9.0 + line_height * 10.0 + padding * 10.0;
    let rack_origin = ui.cursor_screen_pos();
    let rack_rect = PainterRect {
        min: pv2(rack_origin[0], rack_origin[1]),
        max: pv2(rack_origin[0] + rack_width, rack_origin[1] + rack_height),
    };
    let dl = unsafe { imgui::sys::igGetWindowDrawList() };
    let mut frame = painter.begin_frame();
    {
        let mut canvas = unsafe { frame.canvas(dl) };
        panel(&mut canvas, rack_rect, &palette);
    }

    let content_x = rack_origin[0] + padding;
    let content_width = rack_width - padding * 2.0;
    ui.set_cursor_screen_pos([content_x, rack_origin[1] + padding]);
    let _text = ui.push_style_color(imgui::StyleColor::Text, style_color(palette.text));

    let raised = raised_button(&palette);
    let toolbar = toolbar_button(&palette);
    for (index, label) in ["Play", "Stop", "Record"].into_iter().enumerate() {
        if index > 0 {
            ui.same_line();
        }
        unsafe {
            decorate_button(&mut frame, &raised, || ui.button(label));
        }
    }
    for label in ["Loop", "Metronome"] {
        ui.same_line();
        unsafe {
            decorate_button(&mut frame, &toolbar, || ui.button(label));
        }
    }

    ui.spacing();
    {
        let _muted = ui.push_style_color(imgui::StyleColor::Text, style_color(palette.text_muted));
        ui.text("BROWSER");
    }
    let well_origin = ui.cursor_screen_pos();
    let well_height = line_height * 5.0 + padding * 2.0;
    let well_rect = PainterRect {
        min: pv2(well_origin[0], well_origin[1]),
        max: pv2(well_origin[0] + content_width, well_origin[1] + well_height),
    };
    {
        let mut canvas = unsafe { frame.canvas(dl) };
        inset_panel(&mut canvas, well_rect, &palette);
    }
    ui.set_cursor_screen_pos([well_origin[0] + padding, well_origin[1] + padding * 0.5]);

    let tree_style = browser_tree_row(&palette);
    let branch_flags = imgui::TreeNodeFlags::SPAN_AVAIL_WIDTH | imgui::TreeNodeFlags::OPEN_ON_ARROW;
    let root = unsafe {
        decorate_tree_node(
            &mut frame,
            &tree_style,
            state.tree_selection == 0,
            false,
            || {
                ui.tree_node_config("Packs##phase9_tree")
                    .flags(branch_flags | imgui::TreeNodeFlags::DEFAULT_OPEN)
                    .selected(state.tree_selection == 0)
                    .push()
            },
        )
    };
    if ui.is_item_clicked() {
        state.tree_selection = 0;
    }
    if let Some(root_token) = root {
        let child = unsafe {
            decorate_tree_node(
                &mut frame,
                &tree_style,
                state.tree_selection == 1,
                false,
                || {
                    ui.tree_node_config("Drums##phase9_tree")
                        .flags(branch_flags | imgui::TreeNodeFlags::DEFAULT_OPEN)
                        .selected(state.tree_selection == 1)
                        .push()
                },
            )
        };
        if ui.is_item_clicked() {
            state.tree_selection = 1;
        }
        if let Some(child_token) = child {
            let leaf_flags = imgui::TreeNodeFlags::SPAN_AVAIL_WIDTH
                | imgui::TreeNodeFlags::LEAF
                | imgui::TreeNodeFlags::NO_TREE_PUSH_ON_OPEN;
            for (index, label) in [
                "Kick 01.wav##phase9_tree",
                "Snare Tight.wav##phase9_tree",
                "Hat Closed.wav##phase9_tree",
            ]
            .into_iter()
            .enumerate()
            {
                let selection = index + 2;
                let leaf = unsafe {
                    decorate_tree_node(
                        &mut frame,
                        &tree_style,
                        state.tree_selection == selection,
                        true,
                        || {
                            ui.tree_node_config(label)
                                .flags(leaf_flags)
                                .selected(state.tree_selection == selection)
                                .push()
                        },
                    )
                };
                drop(leaf);
                if ui.is_item_clicked() {
                    state.tree_selection = selection;
                }
            }
            child_token.end();
        }
        root_token.end();
    }
    ui.set_cursor_screen_pos([well_origin[0], well_origin[1] + well_height + padding * 0.5]);

    {
        let _muted = ui.push_style_color(imgui::StyleColor::Text, style_color(palette.text_muted));
        ui.text("PARAMETERS");
    }
    let slider = parameter_slider(&palette);
    for (index, (label, range)) in [
        ("Gain", (-24.0, 6.0)),
        ("Transpose", (-12.0, 12.0)),
        ("Dry / Wet", (0.0, 100.0)),
    ]
    .into_iter()
    .enumerate()
    {
        ui.set_next_item_width(content_width * 0.62);
        unsafe {
            decorate_slider_f32(
                &mut frame,
                &slider,
                range.0,
                range.1,
                &mut state.parameters[index],
                |value| ui.slider_config(label, range.0, range.1).build(value),
            );
        }
    }

    ui.set_next_item_width(content_width * 0.45);
    let combo = combo_field(&palette);
    let preview = MODES[state.mode];
    unsafe {
        decorate_combo(
            &mut frame,
            &combo,
            || ui.begin_combo("Warp mode", preview),
            |_token| {
                for (index, mode) in MODES.iter().enumerate() {
                    if ui
                        .selectable_config(mode)
                        .selected(index == state.mode)
                        .build()
                    {
                        state.mode = index;
                    }
                }
            },
        );
    }

    {
        let _muted = ui.push_style_color(imgui::StyleColor::Text, style_color(palette.text_muted));
        ui.text("CLIP ROWS");
    }
    let row = selected_row(&palette);
    for (index, label) in ["Transient layer", "Texture layer"].into_iter().enumerate() {
        if unsafe {
            decorate_selectable(&mut frame, &row, || {
                ui.selectable_config(label)
                    .selected(state.row_selection == index)
                    .build()
            })
        } {
            state.row_selection = index;
        }
    }

    ui.set_next_item_width(content_width * 0.62);
    let inset = inset_control(&palette);
    unsafe {
        decorate_input_text(&mut frame, &inset, || {
            ui.input_text("Clip name", &mut state.name).build()
        });
    }

    {
        let _muted = ui.push_style_color(imgui::StyleColor::Text, style_color(palette.text_muted));
        ui.text("Disabled integration behavior (style alpha)");
    }
    ui.disabled(true, || {
        unsafe {
            decorate_button(&mut frame, &raised, || ui.button("Commit"));
        }
        ui.same_line();
        ui.set_next_item_width(content_width * 0.42);
        unsafe {
            decorate_slider_f32(
                &mut frame,
                &slider,
                0.0,
                1.0,
                &mut state.disabled_parameter,
                |value| ui.slider("Disabled amount", 0.0, 1.0, value),
            );
        }
    });

    drop(_text);
    ui.set_cursor_screen_pos([rack_origin[0], rack_rect.max.y + padding]);
}

fn main() {
    let mut session = Session::new();
    let mut checked = false;
    let mut input = String::new();
    let mut gain = 0.64;
    let mut disabled_gain = 0.35;
    let mut mode = 0;
    let mut combo_input = String::new();
    let mut tree_selection = 2;
    let mut rack = RackState {
        tree_selection: 3,
        parameters: [-6.0, 2.0, 72.0],
        mode: 0,
        row_selection: 0,
        name: "Warehouse Kit".to_owned(),
        disabled_parameter: 0.45,
    };
    common::run("imgui-painter demo", move |ui, painter| {
        draw_demo(ui, &mut session);
        draw_decorated_widgets(ui, painter, &mut checked, &mut input);
        draw_styling_depth(ui, painter);
        draw_widget_anatomy(
            ui,
            painter,
            &mut gain,
            &mut disabled_gain,
            &mut mode,
            &mut combo_input,
            &mut tree_selection,
        );
        draw_recipe_rack(ui, painter, &mut rack);
    });
}
