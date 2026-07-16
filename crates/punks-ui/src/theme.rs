//! Palette tokens and stock ImGui style colors have no imgui-painter bridge, so they are hand-synced here; this duplication is recorded in the redesign findings.

use imgui::StyleColor;
use imgui_painter::{recipes, recipes::Palette, rgba, Border, Color, Material, StateColors};

pub(crate) fn neon_palette() -> Palette {
    Palette {
        surface: rgba(197, 211, 226, 255),
        surface_raised: rgba(220, 231, 242, 255),
        surface_inset: rgba(174, 191, 210, 255),
        border_light: rgba(240, 246, 252, 255),
        border_dark: rgba(90, 107, 128, 255),
        accent: rgba(240, 145, 58, 255),
        selection: rgba(59, 120, 208, 255),
        text: rgba(16, 23, 34, 255),
        text_muted: rgba(58, 72, 92, 255),
    }
}

fn shade(color: Color, scale: f32) -> Color {
    let channel = |shift| ((color >> shift) & 0xff_u32) as f32;
    rgba(
        (channel(0) * scale).round() as u8,
        (channel(8) * scale).round() as u8,
        (channel(16) * scale).round() as u8,
        ((color >> 24) & 0xff) as u8,
    )
}

fn tint(color: Color, amount: f32) -> Color {
    let channel = |shift| ((color >> shift) & 0xff_u32) as f32;
    let lift = |value: f32| (value + (u8::MAX as f32 - value) * amount).round() as u8;
    rgba(
        lift(channel(0)),
        lift(channel(8)),
        lift(channel(16)),
        ((color >> 24) & 0xff) as u8,
    )
}

pub(crate) fn tab_active_material() -> Material {
    let palette = neon_palette();
    Material {
        radius: 2.0,
        fill: StateColors {
            base: shade(palette.selection, 0.86),
            hover: tint(palette.selection, 0.10),
            active: palette.selection,
        },
        border: Border {
            thickness: 1.0,
            color: palette.border_dark,
        },
        shadow: None,
    }
}

/// Per-channel lerp between two packed colors (alpha kept from `a`).
fn mix(a: Color, b: Color, t: f32) -> Color {
    let channel = |color: Color, shift| ((color >> shift) & 0xff_u32) as f32;
    let lerp = |x: f32, y: f32| (x + (y - x) * t).round() as u8;
    rgba(
        lerp(channel(a, 0), channel(b, 0)),
        lerp(channel(a, 8), channel(b, 8)),
        lerp(channel(a, 16), channel(b, 16)),
        ((a >> 24) & 0xff) as u8,
    )
}

pub(crate) fn tab_inactive_material() -> Material {
    let mut material = toolbar_material();
    material.radius = 2.0;
    material
}

pub(crate) fn toolbar_material() -> Material {
    let palette = neon_palette();
    let mut material = recipes::toolbar_button(&palette);
    // The recipe's hover (one surface step up) is imperceptible on this light
    // palette, so hover leans toward the selection blue instead — buttons
    // visibly react (recorded in the redesign findings).
    material.fill.hover = mix(palette.surface_raised, palette.selection, 0.18);
    material
}

// decorate_selectable has no persistent-selection parameter (Selectable.active
// is activation interaction, not selection), so the app swaps materials per
// row — the same pattern the tab bar uses.
pub(crate) fn row_material() -> Material {
    let palette = neon_palette();
    Material {
        radius: 1.0,
        fill: StateColors {
            base: palette.surface,
            hover: mix(palette.surface, palette.selection, 0.15),
            active: mix(palette.surface, palette.selection, 0.35),
        },
        border: Border {
            thickness: 0.0,
            color: palette.surface,
        },
        shadow: None,
    }
}

pub(crate) fn row_selected_material() -> Material {
    let palette = neon_palette();
    Material {
        radius: 1.0,
        fill: StateColors {
            base: palette.selection,
            hover: tint(palette.selection, 0.08),
            active: shade(palette.selection, 0.90),
        },
        border: Border {
            thickness: 0.0,
            color: palette.selection,
        },
        shadow: None,
    }
}

pub(crate) fn inset_field_material() -> Material {
    let palette = neon_palette();
    let mut material = recipes::inset_control(&palette);
    material.fill.hover = mix(palette.surface_inset, palette.selection, 0.12);
    material
}

pub(crate) fn raised_material() -> Material {
    let palette = neon_palette();
    let mut material = recipes::raised_button(&palette);
    // Same hover-visibility override as toolbar_material.
    material.fill.hover = mix(palette.surface_raised, palette.selection, 0.20);
    material
}

pub(crate) fn volume_slider_style() -> imgui_painter::SliderStyle {
    recipes::parameter_slider(&neon_palette())
}

pub(crate) fn color_f32(color: Color) -> [f32; 4] {
    const BYTE: f32 = u8::MAX as f32;
    [
        (color & 0xff) as f32 / BYTE,
        ((color >> 8) & 0xff) as f32 / BYTE,
        ((color >> 16) & 0xff) as f32 / BYTE,
        ((color >> 24) & 0xff) as f32 / BYTE,
    ]
}

fn lighter(color: [f32; 4]) -> [f32; 4] {
    const AMOUNT: f32 = 0.10;
    [
        color[0] + (1.0 - color[0]) * AMOUNT,
        color[1] + (1.0 - color[1]) * AMOUNT,
        color[2] + (1.0 - color[2]) * AMOUNT,
        color[3],
    ]
}

fn darker(color: [f32; 4]) -> [f32; 4] {
    const SCALE: f32 = 0.86;
    [
        color[0] * SCALE,
        color[1] * SCALE,
        color[2] * SCALE,
        color[3],
    ]
}

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}

/// Applies the punks Neon Live theme; punks-standalone calls this at startup.
pub fn apply_theme(style: &mut imgui::Style) {
    let palette = neon_palette();
    let surface = color_f32(palette.surface);
    let surface_raised = color_f32(palette.surface_raised);
    let surface_inset = color_f32(palette.surface_inset);
    let border_dark = color_f32(palette.border_dark);
    let selection = color_f32(palette.selection);
    let text = color_f32(palette.text);
    let text_muted = color_f32(palette.text_muted);

    style.window_rounding = 0.0;
    style.child_rounding = 2.0;
    style.popup_rounding = 3.0;
    style.frame_rounding = 2.0;
    style.grab_rounding = 2.0;
    style.scrollbar_rounding = 2.0;
    style.frame_border_size = 0.0;
    style.window_border_size = 0.0;
    style.frame_padding = [8.0, 4.0];
    style.item_spacing = [8.0, 6.0];
    style.scrollbar_size = 12.0;

    style[StyleColor::WindowBg] = surface;
    style[StyleColor::ChildBg] = surface;
    style[StyleColor::PopupBg] = surface_raised;
    style[StyleColor::Text] = text;
    style[StyleColor::TextDisabled] = text_muted;
    style[StyleColor::Button] = surface_raised;
    style[StyleColor::ButtonHovered] = lighter(surface_raised);
    style[StyleColor::ButtonActive] = darker(surface_raised);
    style[StyleColor::FrameBg] = surface_inset;
    style[StyleColor::FrameBgHovered] = lighter(surface_inset);
    style[StyleColor::FrameBgActive] = darker(surface_inset);
    style[StyleColor::Header] = with_alpha(selection, 0.85);
    style[StyleColor::HeaderHovered] = with_alpha(selection, 0.95);
    style[StyleColor::HeaderActive] = selection;
    style[StyleColor::SliderGrab] = border_dark;
    style[StyleColor::SliderGrabActive] = selection;
    style[StyleColor::CheckMark] = selection;
    style[StyleColor::Border] = border_dark;
    style[StyleColor::Separator] = with_alpha(border_dark, 0.6);
    style[StyleColor::ScrollbarBg] = with_alpha(surface_inset, 0.6);
    style[StyleColor::ScrollbarGrab] = with_alpha(border_dark, 0.55);
    style[StyleColor::ScrollbarGrabHovered] = with_alpha(border_dark, 0.75);
    style[StyleColor::ScrollbarGrabActive] = border_dark;
    style[StyleColor::PlotHistogram] = selection;
    style[StyleColor::NavHighlight] = selection;
    style[StyleColor::TitleBg] = surface;
    style[StyleColor::TitleBgActive] = surface;
    style[StyleColor::TitleBgCollapsed] = surface;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_color_converts_in_imgui_channel_order() {
        assert_eq!(
            color_f32(rgba(255, 128, 0, 64)),
            [1.0, 128.0 / 255.0, 0.0, 64.0 / 255.0]
        );
    }
}
