//! Compiles imgui-painter's C++ core + C API into a static lib linked into
//! whatever consumes this crate. Same mechanism imgui-sys itself uses to
//! compile cimgui (see imgui-sys's own build.rs).
//!
//! The core (`../../src`) never includes an ImGui/cimgui header; only the
//! Rust adapter (`src/adapter.rs`) reaches into `imgui_sys` to copy a
//! finished mesh into a real `ImDrawList`. That keeps this build script
//! independent of whatever ImGui version the host app links.

fn main() {
    let capi_dir = "../../capi";
    let src_dir = "../../src";

    println!("cargo:rerun-if-changed={capi_dir}");
    println!("cargo:rerun-if-changed={src_dir}");

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(capi_dir)
        .include("../../include")
        .file(format!("{capi_dir}/imgui_painter_c.cpp"))
        .file(format!("{src_dir}/painter.cpp"))
        .warnings(true)
        .compile("imgui_painter_core");

    // Forward the cc-relevant build-script env vars into this crate's own
    // compile-time environment (readable via env!() in tests/) so
    // tests/fluent_header_compiles.rs can drive `cc::Build` too — `cc`
    // reads these from the process environment and only Cargo populates
    // them for an actual build script, not for a `#[test]` binary.
    for var in ["TARGET", "HOST", "OPT_LEVEL", "PROFILE"] {
        println!(
            "cargo:rustc-env=IMGUI_PAINTER_BUILD_{var}={}",
            std::env::var(var).unwrap_or_else(|_| panic!("build script env var {var} missing"))
        );
    }
}
