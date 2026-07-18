//! Punks' compact blue-gray desktop theme and application-specific surfaces.

use imgui_painter::{
    recipes, recipes::Palette, rgba, Border, Canvas, Color, ColorStop, Gradient, GradientMode,
    Material, Rect, Shadow, StateColors, Vec2,
};

pub(crate) fn neon_palette() -> Palette {
    Palette {
        surface: rgba(188, 203, 221, 255),
        surface_raised: rgba(220, 231, 242, 255),
        surface_inset: rgba(168, 185, 205, 255),
        border_light: rgba(245, 249, 253, 255),
        border_dark: rgba(82, 103, 126, 255),
        accent: rgba(233, 141, 61, 255),
        selection: rgba(61, 120, 200, 255),
        text: rgba(22, 34, 49, 255),
        text_muted: rgba(75, 92, 112, 255),
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

pub(crate) fn row_material() -> Material {
    let palette = neon_palette();
    Material {
        radius: 1.0,
        fill: StateColors {
            base: palette.surface,
            hover: mix(palette.surface, palette.selection, 0.15),
            active: palette.selection,
        },
        border: Border {
            thickness: 0.0,
            color: palette.surface,
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

fn alpha(color: Color, value: u8) -> Color {
    (color & 0x00ff_ffff) | ((value as Color) << 24)
}

fn point(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

/// Paint the full application workspace before any controls are submitted.
pub(crate) fn paint_workspace_surface(canvas: &mut Canvas<'_>, rect: Rect) {
    let palette = neon_palette();
    let hairline = canvas.device_pixel();
    canvas.rounded_rect(rect, 0.0);
    canvas.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: point(rect.min.x, rect.min.y),
        to: point(rect.min.x, rect.max.y),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: tint(palette.surface, 0.08),
            },
            ColorStop {
                t: 0.42,
                color: palette.surface,
            },
            ColorStop {
                t: 1.0,
                color: shade(palette.surface, 0.96),
            },
        ],
    });
    canvas.fill_band_color(
        rect.min.y + hairline,
        rect.min.y + hairline * 2.0,
        alpha(palette.border_light, 180),
    );
}

/// Paint an explicit popup rectangle. The caller owns popup sizing/lifecycle.
pub(crate) fn paint_popup_surface(canvas: &mut Canvas<'_>, rect: Rect) {
    let palette = neon_palette();
    let hairline = canvas.device_pixel();
    canvas.rounded_rect(rect, 3.0);
    canvas.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: point(rect.min.x, rect.min.y),
        to: point(rect.min.x, rect.max.y),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: tint(palette.surface_raised, 0.05),
            },
            ColorStop {
                t: 1.0,
                color: palette.surface_raised,
            },
        ],
    });
    canvas.add_shadow(&Shadow {
        offset: point(0.0, 1.0),
        blur: 4.0,
        spread: hairline,
        color: alpha(palette.border_dark, 90),
        inset: true,
    });
    canvas.add_border(&Border {
        thickness: hairline,
        color: palette.border_dark,
    });
    canvas.fill_band_color(
        rect.min.y + hairline,
        rect.min.y + hairline * 2.0,
        alpha(palette.border_light, 190),
    );
}

/// Paint the orange clip surface; waveform geometry is layered over it by the caller.
pub(crate) fn paint_waveform_surface(canvas: &mut Canvas<'_>, rect: Rect) {
    let palette = neon_palette();
    let hairline = canvas.device_pixel();
    canvas.rounded_rect(rect, 2.0);
    canvas.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: point(rect.min.x, rect.min.y),
        to: point(rect.min.x, rect.max.y),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: tint(palette.surface_inset, 0.06),
            },
            ColorStop {
                t: 0.42,
                color: palette.surface_inset,
            },
            ColorStop {
                t: 1.0,
                color: shade(palette.surface_inset, 0.90),
            },
        ],
    });
    canvas.add_shadow(&Shadow {
        offset: point(0.0, 2.0),
        blur: 4.0,
        spread: hairline,
        color: alpha(palette.text, 72),
        inset: true,
    });
    canvas.add_border(&Border {
        thickness: hairline,
        color: palette.border_dark,
    });
    canvas.fill_band_color(
        rect.min.y + hairline,
        rect.min.y + hairline * 2.0,
        alpha(palette.border_light, 130),
    );
}

pub(crate) fn waveform_bar_color() -> Color {
    alpha(neon_palette().text, 205)
}

pub(crate) fn waveform_outline_color() -> Color {
    alpha(neon_palette().text, 150)
}

pub(crate) fn waveform_playhead_color() -> Color {
    neon_palette().accent
}

pub(crate) fn waveform_text_color() -> Color {
    alpha(neon_palette().text, 225)
}

/// Applies the punks Neon Live theme; punks-standalone calls this at startup.
pub fn apply_theme(style: &mut imgui::Style) {
    let palette = neon_palette();
    recipes::apply_imgui_colors(&mut style.colors, &palette);

    style.window_rounding = 0.0;
    style.child_rounding = 3.0;
    style.popup_rounding = 3.0;
    style.frame_rounding = 2.0;
    style.grab_rounding = 2.0;
    style.scrollbar_rounding = 2.0;
    style.tab_rounding = 2.0;
    style.frame_border_size = 0.0;
    style.window_border_size = 0.0;
    style.popup_border_size = 1.0;
    style.window_padding = [8.0, 7.0];
    style.frame_padding = [6.0, 3.0];
    style.item_spacing = [6.0, 4.0];
    style.item_inner_spacing = [4.0, 3.0];
    style.cell_padding = [4.0, 2.0];
    style.scrollbar_size = 10.0;
    style.grab_min_size = 8.0;
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
