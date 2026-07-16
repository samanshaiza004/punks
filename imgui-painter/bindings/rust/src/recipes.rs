//! Small chrome recipes derived from host-owned palette tokens.

use crate::{
    Border, Canvas, Color, ColorStop, ComboStyle, Gradient, GradientMode, Material, Rect, Shadow,
    SliderStyle, StateColors, TreeStyle, Vec2,
};

/// A minimal chrome token palette. imgui-painter paints chrome only; `text` and `text_muted` exist so hosts keep typography coherent with the chrome, applied through stock ImGui style APIs (e.g. push_style_color(Text, ..)) — imgui-painter never paints text, and this crate links only imgui-sys so it deliberately owns no imgui-rs helper for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_inset: Color,
    pub border_light: Color,
    pub border_dark: Color,
    pub accent: Color,
    pub selection: Color,
    pub text: Color,
    pub text_muted: Color,
}

fn channel(color: Color, shift: u32) -> u8 {
    ((color >> shift) & 0xff) as u8
}

fn shade(color: Color, amount: f32) -> Color {
    let scale = 1.0 - amount;
    crate::rgba(
        (channel(color, 0) as f32 * scale).round() as u8,
        (channel(color, 8) as f32 * scale).round() as u8,
        (channel(color, 16) as f32 * scale).round() as u8,
        channel(color, 24),
    )
}

fn tint(color: Color, amount: f32) -> Color {
    let lift = |value: u8| (value as f32 + (u8::MAX - value) as f32 * amount).round() as u8;
    crate::rgba(
        lift(channel(color, 0)),
        lift(channel(color, 8)),
        lift(channel(color, 16)),
        channel(color, 24),
    )
}

fn with_alpha(color: Color, alpha: u8) -> Color {
    (color & 0x00ff_ffff) | ((alpha as Color) << 24)
}

fn point(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

fn border(color: Color) -> Border {
    Border {
        thickness: 1.0,
        color,
    }
}

fn colors(base: Color, hover: Color, active: Color) -> StateColors {
    StateColors {
        base,
        hover,
        active,
    }
}

/// Raised stock-button chrome for the painter_demo rack transport.
pub fn raised_button(palette: &Palette) -> Material {
    Material {
        radius: 3.0,
        fill: colors(
            palette.surface_raised,
            tint(palette.surface_raised, 0.10),
            shade(palette.surface_raised, 0.14),
        ),
        border: border(palette.border_dark),
        shadow: Some(Shadow {
            offset: point(0.0, 1.0),
            blur: 3.0,
            spread: 0.0,
            color: with_alpha(palette.border_dark, 96),
            inset: false,
        }),
    }
}

/// Compact low-elevation button chrome for the painter_demo rack toolbar.
pub fn toolbar_button(palette: &Palette) -> Material {
    Material {
        radius: 2.0,
        fill: colors(
            palette.surface,
            palette.surface_raised,
            palette.surface_inset,
        ),
        border: border(palette.border_dark),
        shadow: None,
    }
}

/// Sunken single-line InputText chrome for the painter_demo rack.
pub fn inset_control(palette: &Palette) -> Material {
    Material {
        radius: 2.0,
        fill: colors(
            palette.surface_inset,
            tint(palette.surface_inset, 0.06),
            shade(palette.surface_inset, 0.08),
        ),
        border: border(palette.border_dark),
        shadow: Some(Shadow {
            offset: point(0.0, 1.0),
            blur: 3.0,
            spread: 0.0,
            color: with_alpha(palette.border_dark, 112),
            inset: true,
        }),
    }
}

/// Selection-led Selectable row chrome for the painter_demo rack.
pub fn selected_row(palette: &Palette) -> Material {
    Material {
        radius: 1.0,
        fill: colors(
            palette.surface,
            tint(palette.selection, 0.08),
            palette.selection,
        ),
        border: border(palette.border_dark),
        shadow: None,
    }
}

/// Hierarchical browser-row chrome for the painter_demo rack.
pub fn browser_tree_row(palette: &Palette) -> TreeStyle {
    TreeStyle {
        row: selected_row(palette),
        disclosure: Material {
            radius: 1.0,
            fill: colors(
                palette.surface_inset,
                tint(palette.selection, 0.08),
                palette.selection,
            ),
            border: border(palette.border_dark),
            shadow: None,
        },
    }
}

/// Inset track, accent fill, and raised grab for the painter_demo rack.
pub fn parameter_slider(palette: &Palette) -> SliderStyle {
    SliderStyle {
        track: inset_control(palette),
        fill: Material {
            radius: 2.0,
            fill: colors(
                palette.accent,
                tint(palette.accent, 0.10),
                shade(palette.accent, 0.12),
            ),
            border: border(shade(palette.accent, 0.28)),
            shadow: None,
        },
        grab: raised_button(palette),
    }
}

/// Inset field and raised arrow region for the painter_demo rack.
pub fn combo_field(palette: &Palette) -> ComboStyle {
    ComboStyle {
        frame: inset_control(palette),
        arrow_region: raised_button(palette),
    }
}

/// Paint the raised layered outer surface consumed by the painter_demo rack.
pub fn panel(canvas: &mut Canvas<'_>, rect: Rect, palette: &Palette) {
    let hairline = canvas.device_pixel();
    canvas.rounded_rect(rect, 5.0);
    canvas.add_shadow(&Shadow {
        offset: point(0.0, 3.0),
        blur: 9.0,
        spread: 0.0,
        color: with_alpha(palette.border_dark, 104),
        inset: false,
    });
    canvas.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: point(rect.min.x, rect.min.y),
        to: point(rect.min.x, rect.max.y),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: palette.surface_raised,
            },
            ColorStop {
                t: 1.0,
                color: palette.surface,
            },
        ],
    });
    canvas.fill_band_color(
        rect.min.y + hairline,
        rect.min.y + hairline * 2.0,
        palette.border_light,
    );
    canvas.add_border(&Border {
        thickness: hairline,
        color: palette.border_dark,
    });
}

/// Paint the sunken sub-well consumed by the painter_demo rack browser.
pub fn inset_panel(canvas: &mut Canvas<'_>, rect: Rect, palette: &Palette) {
    let hairline = canvas.device_pixel();
    canvas.rounded_rect(rect, 3.0);
    canvas.fill_color(palette.surface_inset);
    canvas.add_shadow(&Shadow {
        offset: point(0.0, 2.0),
        blur: 5.0,
        spread: hairline,
        color: with_alpha(palette.border_dark, 128),
        inset: true,
    });
    canvas.add_border(&Border {
        thickness: hairline,
        color: palette.border_dark,
    });
    canvas.fill_band_color(
        rect.max.y - hairline * 2.0,
        rect.max.y - hairline,
        palette.border_light,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            surface: crate::rgba(70, 80, 90, 255),
            surface_raised: crate::rgba(90, 100, 110, 255),
            surface_inset: crate::rgba(40, 50, 60, 255),
            border_light: crate::rgba(130, 140, 150, 255),
            border_dark: crate::rgba(15, 20, 25, 255),
            accent: crate::rgba(50, 150, 210, 255),
            selection: crate::rgba(45, 105, 145, 255),
            text: crate::rgba(230, 235, 240, 255),
            text_muted: crate::rgba(160, 170, 180, 255),
        }
    }

    #[test]
    fn button_and_control_recipes_derive_from_surface_tokens() {
        let palette = palette();
        let raised = raised_button(&palette);
        assert_eq!(raised.fill.base, palette.surface_raised);
        assert_eq!(raised.fill.hover, tint(palette.surface_raised, 0.10));
        assert_eq!(raised.fill.active, shade(palette.surface_raised, 0.14));

        let toolbar = toolbar_button(&palette);
        assert_eq!(toolbar.fill.base, palette.surface);
        assert_eq!(toolbar.fill.hover, palette.surface_raised);
        assert_eq!(toolbar.fill.active, palette.surface_inset);

        let inset = inset_control(&palette);
        assert_eq!(inset.fill.base, palette.surface_inset);
        assert_eq!(inset.fill.hover, tint(palette.surface_inset, 0.06));
        assert_eq!(inset.border.color, palette.border_dark);
    }

    #[test]
    fn row_recipes_are_selection_driven() {
        let palette = palette();
        let row = selected_row(&palette);
        assert_eq!(row.fill.hover, tint(palette.selection, 0.08));
        assert_eq!(row.fill.active, palette.selection);

        let tree = browser_tree_row(&palette);
        assert_eq!(tree.row, row);
        assert_eq!(tree.disclosure.fill.base, palette.surface_inset);
        assert_eq!(tree.disclosure.fill.active, palette.selection);
    }

    #[test]
    fn multipart_control_recipes_preserve_token_roles() {
        let palette = palette();
        let slider = parameter_slider(&palette);
        assert_eq!(slider.track.fill.base, palette.surface_inset);
        assert_eq!(slider.fill.fill.base, palette.accent);
        assert_eq!(slider.fill.fill.hover, tint(palette.accent, 0.10));
        assert_eq!(slider.grab.fill.base, palette.surface_raised);

        let combo = combo_field(&palette);
        assert_eq!(combo.frame, inset_control(&palette));
        assert_eq!(combo.arrow_region, raised_button(&palette));
    }
}
