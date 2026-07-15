//! ImGui-aware item decoration built on the renderer-agnostic painter core.
//!
//! [`item_paint`] brackets a stock ImGui widget: its normal frame colors are
//! suppressed, its item state is captured, and a [`Decorator`] paints the
//! corresponding [`Material`] behind it through a [`crate::Canvas`].

use imgui_sys as sys;

use crate::{Border, Canvas, Color, Frame, Rect, Shadow, Vec2};

/// The complete item geometry and interaction state available to a
/// [`Decorator`]. For multipart widgets this rectangle includes labels and
/// other non-chrome parts; the decorator resolves its private paint rectangle
/// separately.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemState {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub hovered: bool,
    pub active: bool,
}

/// Fill colors for the interaction states shared by the current decorators.
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
/// themes, and material scope guards remain deferred until the Phase 5 widget
/// breadth findings provide concrete requirements for them.
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
    Checkbox,
    /// A single-line InputText. Multiline input owns a child-window rendering
    /// path and is deliberately outside this decorator's contract.
    InputText,
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
            Decorator::Checkbox | Decorator::InputText => [
                sys::ImGuiCol_FrameBg as _,
                sys::ImGuiCol_FrameBgHovered as _,
                sys::ImGuiCol_FrameBgActive as _,
            ],
        }
    }

    /// Capture chrome geometry before ImGui consumes next-item width and
    /// advances the cursor. Button/Selectable expose their chrome as the full
    /// post-item rectangle, so they need no pre-capture.
    ///
    /// ponytail: these formulas reproduce imgui-sys 0.12's current Checkbox
    /// and single-line InputText layout conventions through public functions;
    /// Dear ImGui does not promise stable widget-part geometry. An ImGui bump
    /// can silently desynchronize them. Upgrade path: rerun the visual gate on
    /// every bump, then use stable upstream part geometry if it ever exists.
    unsafe fn capture_chrome_rect(&self) -> Option<Rect> {
        let width = match self {
            Decorator::Button | Decorator::Selectable => return None,
            Decorator::Checkbox => sys::igGetFrameHeight(),
            Decorator::InputText => sys::igCalcItemWidth(),
        };
        let height = sys::igGetFrameHeight();
        let mut min = sys::ImVec2 { x: 0.0, y: 0.0 };
        sys::igGetCursorScreenPos(&mut min);
        Some(Rect {
            min: Vec2 { x: min.x, y: min.y },
            max: Vec2 {
                x: min.x + width,
                y: min.y + height,
            },
        })
    }

    fn paint_rect(&self, state: &ItemState, captured: Option<Rect>) -> Rect {
        match self {
            Decorator::Button | Decorator::Selectable => item_rect(state),
            Decorator::Checkbox | Decorator::InputText => {
                captured.expect("multipart decorators capture chrome before drawing")
            }
        }
    }

    fn paint(&self, material: &Material, state: &ItemState, rect: Rect, canvas: &mut Canvas<'_>) {
        debug_assert!(rect_is_valid(rect));
        debug_assert!(rect_contains(item_rect(state), rect));
        canvas.session.rounded_rect(rect, material.radius);
        if let Some(shadow) = material.shadow {
            canvas.session.add_shadow(&shadow);
        }
        canvas.session.fill_color(material.fill.for_state(state));
        canvas.session.add_border(&material.border);
    }
}

fn item_rect(state: &ItemState) -> Rect {
    Rect {
        min: Vec2 {
            x: state.min[0],
            y: state.min[1],
        },
        max: Vec2 {
            x: state.max[0],
            y: state.max[1],
        },
    }
}

fn rect_is_valid(rect: Rect) -> bool {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
        .into_iter()
        .all(f32::is_finite)
        && rect.max.x >= rect.min.x
        && rect.max.y >= rect.min.y
}

fn rect_contains(outer: Rect, inner: Rect) -> bool {
    const EPSILON: f32 = 0.5;
    inner.min.x >= outer.min.x - EPSILON
        && inner.min.y >= outer.min.y - EPSILON
        && inner.max.x <= outer.max.x + EPSILON
        && inner.max.y <= outer.max.y + EPSILON
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
///
/// # Correctness
///
/// `decorator` must match the one stock widget emitted by `widget`, and
/// [`Decorator::InputText`] supports single-line input only. Dear ImGui exposes
/// generic last-item geometry/state but no stable public widget-type tag, so
/// this cannot be reliably checked without fragile heuristics. A mismatch is
/// not a memory-safety violation; it typically suppresses the wrong color slots
/// and quietly double-paints or mis-sizes the widget.
pub unsafe fn item_paint(
    frame: &mut Frame<'_>,
    decorator: Decorator,
    material: &Material,
    widget: impl FnOnce() -> bool,
) -> bool {
    let draw_list = sys::igGetWindowDrawList();
    let captured_rect = decorator.capture_chrome_rect();
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
    let paint_rect = decorator.paint_rect(&state, captured_rect);

    sys::ImDrawList_ChannelsSetCurrent(draw_list, 0);
    {
        let mut canvas = frame.canvas(draw_list);
        decorator.paint(material, &state, paint_rect, &mut canvas);
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

    #[test]
    fn button_and_selectable_paint_the_complete_item() {
        let state = ItemState {
            min: [2.0, 3.0],
            max: [42.0, 23.0],
            hovered: false,
            active: false,
        };
        let expected = item_rect(&state);

        assert_eq!(Decorator::Button.paint_rect(&state, None), expected);
        assert_eq!(Decorator::Selectable.paint_rect(&state, None), expected);
    }

    #[test]
    fn multipart_decorators_paint_only_the_captured_chrome() {
        let state = ItemState {
            min: [2.0, 3.0],
            max: [142.0, 23.0],
            hovered: false,
            active: false,
        };
        let checkbox = Rect {
            min: Vec2 { x: 2.0, y: 3.0 },
            max: Vec2 { x: 22.0, y: 23.0 },
        };
        let input = Rect {
            min: Vec2 { x: 2.0, y: 3.0 },
            max: Vec2 { x: 102.0, y: 23.0 },
        };

        assert_eq!(
            Decorator::Checkbox.paint_rect(&state, Some(checkbox)),
            checkbox
        );
        assert_eq!(Decorator::InputText.paint_rect(&state, Some(input)), input);
    }

    #[test]
    fn geometry_validation_rejects_invalid_or_outside_rects() {
        let outer = Rect {
            min: Vec2 { x: 2.0, y: 3.0 },
            max: Vec2 { x: 102.0, y: 23.0 },
        };
        let inside = Rect {
            min: Vec2 { x: 2.0, y: 3.0 },
            max: Vec2 { x: 22.0, y: 23.0 },
        };
        let outside = Rect {
            min: Vec2 { x: 2.0, y: 3.0 },
            max: Vec2 { x: 122.0, y: 23.0 },
        };
        let inverted = Rect {
            min: Vec2 { x: 5.0, y: 3.0 },
            max: Vec2 { x: 4.0, y: 23.0 },
        };
        let non_finite = Rect {
            min: Vec2 {
                x: f32::NAN,
                y: 3.0,
            },
            max: Vec2 { x: 22.0, y: 23.0 },
        };

        assert!(rect_is_valid(inside));
        assert!(rect_contains(outer, inside));
        assert!(!rect_contains(outer, outside));
        assert!(!rect_is_valid(inverted));
        assert!(!rect_is_valid(non_finite));
    }

    #[test]
    fn decorators_suppress_the_expected_color_families() {
        assert_eq!(
            Decorator::Button.suppress_cols(),
            [
                sys::ImGuiCol_Button as _,
                sys::ImGuiCol_ButtonHovered as _,
                sys::ImGuiCol_ButtonActive as _,
            ]
        );
        assert_eq!(
            Decorator::Selectable.suppress_cols(),
            [
                sys::ImGuiCol_Header as _,
                sys::ImGuiCol_HeaderHovered as _,
                sys::ImGuiCol_HeaderActive as _,
            ]
        );
        let frame = [
            sys::ImGuiCol_FrameBg as _,
            sys::ImGuiCol_FrameBgHovered as _,
            sys::ImGuiCol_FrameBgActive as _,
        ];
        assert_eq!(Decorator::Checkbox.suppress_cols(), frame);
        assert_eq!(Decorator::InputText.suppress_cols(), frame);
    }
}
