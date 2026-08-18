//! M0 smoke test: does a real `gpui-component` Button and Input compile,
//! render, and take focus/keyboard input against Punks' frozen GPUI
//! revision? See docs/gpui-component-audit.md for the result.
//!
//! Run with: cargo run -p punks-app --example gpui_component_smoke

use gpui::*;
use gpui_component::{
    button::*,
    input::{Input, InputEvent, InputState},
    *,
};

struct Smoke {
    input_state: Entity<InputState>,
    echoed: SharedString,
    clicks: u32,
    _subscriptions: Vec<Subscription>,
}

impl Smoke {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("Type here..."));

        let _subscriptions = vec![cx.subscribe_in(&input_state, window, {
            let input_state = input_state.clone();
            move |this: &mut Self, _, ev: &InputEvent, _window, cx| {
                if let InputEvent::Change = ev {
                    this.echoed = input_state.read(cx).value().clone();
                    cx.notify();
                }
            }
        })];

        Self {
            input_state,
            echoed: SharedString::default(),
            clicks: 0,
            _subscriptions,
        }
    }
}

impl Render for Smoke {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .p_5()
            .gap_3()
            .size_full()
            .items_center()
            .justify_center()
            .child("gpui-component M0 smoke test")
            .child(Input::new(&self.input_state))
            .child(format!("echo: {}", self.echoed))
            .child(
                Button::new("smoke-button")
                    .primary()
                    .label(format!("Clicked {} times", self.clicks))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.clicks += 1;
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(500.0), px(300.0)), cx)),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| Smoke::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open smoke-test window");
        })
        .detach();

        cx.activate(true);
    });
}
