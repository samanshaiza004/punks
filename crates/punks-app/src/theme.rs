//! Punks' compact blue-gray "Neon Live" desktop theme, ported to GPUI.
//!
//! `gpui-component`'s controls read colors from a global [`gpui_component::Theme`]
//! (a large semantic-token struct: `background`, `accent`, `button_primary`,
//! `list_active`, and dozens more). Rather than authoring a full parallel
//! theme (most of those tokens are for features Punks doesn't have -- charts,
//! description lists, etc.) or fighting every component's default styling
//! with per-use overrides, [`apply_neon_theme`] mutates just the handful of
//! fields that are visibly load-bearing for Punks' own UI, leaving the rest
//! at gpui-component's defaults. A separate theme would mean re-deriving
//! hover/active/focus states that gpui-component
//! already computes correctly from a handful of base tokens; overriding
//! those base tokens directly got the same visual result for a fraction of
//! the code and stays correct as gpui-component's components evolve.
//!
//! Punks' own custom-painted elements (the waveform, in particular) don't go
//! through gpui-component's theme system at all -- they paint directly via
//! GPUI's `canvas()`, so [`WAVEFORM_BAR`] and friends below are plain colors
//! for that direct use, not theme tokens.

use gpui::{rgb, App, Hsla};
use gpui_component::Theme;

pub const SURFACE: u32 = 0xBCCBDD;
pub const SURFACE_RAISED: u32 = 0xDCE7F2;
pub const SURFACE_INSET: u32 = 0xA8B9CD;
pub const BORDER_LIGHT: u32 = 0xF5F9FD;
pub const BORDER_DARK: u32 = 0x52677E;
pub const ACCENT: u32 = 0xE98D3D;
pub const SELECTION: u32 = 0x3D78C8;
pub const TEXT: u32 = 0x162231;

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Mutates gpui-component's global [`Theme`] with Punks' palette. Call once
/// at startup, after `gpui_component::init(cx)`.
pub fn apply_neon_theme(cx: &mut App) {
    let theme = Theme::global_mut(cx);

    theme.background = hsla(SURFACE);
    theme.foreground = hsla(TEXT);
    theme.border = hsla(BORDER_DARK);
    theme.accent = hsla(ACCENT);
    theme.accent_foreground = hsla(TEXT);

    theme.button = hsla(SURFACE_RAISED);
    theme.button_hover = hsla(SELECTION).opacity(0.18);
    theme.button_active = hsla(SELECTION);
    theme.button_foreground = hsla(TEXT);

    theme.button_primary = hsla(SELECTION);
    theme.button_primary_hover = hsla(SELECTION).opacity(0.85);
    theme.button_primary_active = hsla(SELECTION);
    theme.button_primary_foreground = hsla(BORDER_LIGHT);

    theme.input = hsla(SURFACE_INSET);

    // `Theme` has its own `list: ListSettings` field that shadows
    // `ThemeColor::list`/`list_active`/`list_active_border` (the color
    // tokens) via the Deref target -- those need the explicit `.colors.`
    // path to reach.
    theme.colors.list = hsla(SURFACE);
    theme.colors.list_active = hsla(SELECTION);
    theme.colors.list_active_border = hsla(SELECTION);
}

/// Waveform bar fill. Direct-paint color, not a theme token -- see module docs.
pub const WAVEFORM_BAR: u32 = TEXT;
// Not consumed yet; `waveform.rs` has no stroke or hover-label rendering.
#[allow(dead_code)]
/// Waveform outline stroke.
pub const WAVEFORM_OUTLINE: u32 = TEXT;
/// Playhead line color.
pub const WAVEFORM_PLAYHEAD: u32 = ACCENT;
#[allow(dead_code)]
/// Time-label / hover-crosshair text color.
pub const WAVEFORM_TEXT: u32 = TEXT;
