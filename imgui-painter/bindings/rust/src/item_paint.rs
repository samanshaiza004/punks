//! ImGui-aware item decoration built on the renderer-agnostic painter core.
//!
//! [`item_paint`] brackets a stock ImGui widget: its normal frame colors are
//! suppressed, its item state is captured, and a [`Decorator`] paints the
//! corresponding [`Material`] behind it through a [`crate::Canvas`].

use imgui_sys as sys;

use crate::{Border, Canvas, Color, Frame, Rect, Shadow, Vec2};

/// The item geometry and interaction state available to a [`Decorator`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemState {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub hovered: bool,
    pub active: bool,
}

/// Fill colors for the interaction states shared by Button and Selectable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateColors {
    pub base: Color,
    pub hover: Color,
    pub active: Color,
}

impl StateColors {
    /// Resolve the active color first because an active item is also hovered.
    pub fn for_state(&self, state: &ItemState) -> Color {
        if state.active {
            self.active
        } else if state.hovered {
            self.hover
        } else {
            self.base
        }
    }
}

/// The deliberately small visual input shared by today's decorators.
///
/// This contains only properties every widget could plausibly consume.
/// Gradients, gloss, typography, overlays, effects, `Resolver`, `Recipe`,
/// themes, and material scope guards wait for Phase 5, when more widget
/// shapes provide concrete consumers for them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub radius: f32,
    pub fill: StateColors,
    pub border: Border,
    pub shadow: Option<Shadow>,
}

/// The closed set of stock ImGui widgets whose frame colors can be decorated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decorator {
    Button,
    Selectable,
}

impl Decorator {
    fn suppress_cols(&self) -> [sys::ImGuiCol; 3] {
        match self {
            Decorator::Button => [
                sys::ImGuiCol_Button as _,
                sys::ImGuiCol_ButtonHovered as _,
                sys::ImGuiCol_ButtonActive as _,
            ],
            Decorator::Selectable => [
                sys::ImGuiCol_Header as _,
                sys::ImGuiCol_HeaderHovered as _,
                sys::ImGuiCol_HeaderActive as _,
            ],
        }
    }

    fn paint(&self, material: &Material, state: &ItemState, canvas: &mut Canvas<'_>) {
        let rect = Rect {
            min: Vec2 {
                x: state.min[0],
                y: state.min[1],
            },
            max: Vec2 {
                x: state.max[0],
                y: state.max[1],
            },
        };

        canvas.session.rounded_rect(rect, material.radius);
        if let Some(shadow) = material.shadow {
            canvas.session.add_shadow(&shadow);
        }
        canvas.session.fill_color(material.fill.for_state(state));
        canvas.session.add_border(&material.border);
    }
}

const TRANSPARENT: sys::ImVec4 = sys::ImVec4 {
    x: 0.0,
    y: 0.0,
    z: 0.0,
    w: 0.0,
};

/// Paint a [`Material`] behind one stock ImGui widget while preserving the
/// widget's layout, input handling, text, and return value.
///
/// `widget` must issue exactly one item whose rectangle and interaction state
/// are available through ImGui's `GetItemRect*`/`IsItem*` calls afterward.
///
/// # Safety
///
/// Must be called inside a live ImGui window on the context-owning thread.
/// The current window draw list must be valid and must not already be inside
/// an ImGui channel split. `frame` must belong to the current ImGui frame.
pub unsafe fn item_paint(
    frame: &mut Frame<'_>,
    decorator: Decorator,
    material: &Material,
    widget: impl FnOnce() -> bool,
) -> bool {
    let draw_list = sys::igGetWindowDrawList();
    sys::ImDrawList_ChannelsSplit(draw_list, 3);
    sys::ImDrawList_ChannelsSetCurrent(draw_list, 1);

    for col in decorator.suppress_cols() {
        sys::igPushStyleColor_Vec4(col, TRANSPARENT);
    }
    let result = widget();
    sys::igPopStyleColor(3);

    let mut min = sys::ImVec2 { x: 0.0, y: 0.0 };
    let mut max = sys::ImVec2 { x: 0.0, y: 0.0 };
    sys::igGetItemRectMin(&mut min);
    sys::igGetItemRectMax(&mut max);
    let state = ItemState {
        min: [min.x, min.y],
        max: [max.x, max.y],
        hovered: sys::igIsItemHovered(0),
        active: sys::igIsItemActive(),
    };

    sys::ImDrawList_ChannelsSetCurrent(draw_list, 0);
    {
        let mut canvas = frame.canvas(draw_list);
        decorator.paint(material, &state, &mut canvas);
    }
    sys::ImDrawList_ChannelsMerge(draw_list);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Color = 1;
    const HOVER: Color = 2;
    const ACTIVE: Color = 3;

    fn state(hovered: bool, active: bool) -> ItemState {
        ItemState {
            min: [0.0, 0.0],
            max: [10.0, 10.0],
            hovered,
            active,
        }
    }

    #[test]
    fn state_colors_resolve_base_hover_and_active() {
        let colors = StateColors {
            base: BASE,
            hover: HOVER,
            active: ACTIVE,
        };

        assert_eq!(colors.for_state(&state(false, false)), BASE);
        assert_eq!(colors.for_state(&state(true, false)), HOVER);
        assert_eq!(colors.for_state(&state(true, true)), ACTIVE);
    }
}
