#[path = "../common/mod.rs"]
mod common;

use imgui_painter::{decorate_button, rgba, Border, Material, StateColors};

fn main() {
    common::run("imgui-painter basic example", |ui, painter| {
        ui.text("A stock ImGui button decorated by imgui-painter:");
        ui.spacing();

        let material = Material {
            radius: 5.0,
            fill: StateColors {
                base: rgba(45, 108, 223, 255),
                hover: rgba(62, 128, 240, 255),
                active: rgba(35, 88, 190, 255),
            },
            border: Border {
                thickness: 1.0,
                color: rgba(255, 255, 255, 48),
            },
            shadow: None,
        };

        let mut frame = painter.begin_frame();
        // SAFETY: this runs inside the current ImGui window and frame, and
        // the closure issues exactly one stock widget item.
        unsafe {
            decorate_button(&mut frame, &material, || ui.button("Decorated Button"));
        }
    });
}
