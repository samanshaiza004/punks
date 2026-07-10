//! Compiles `include/imgui_painter.h` against a minimal mock draw-list
//! struct (`fluent_header_mock.cpp`) via the same `cc` crate `build.rs`
//! uses for the core, proving the fluent C++ header type-checks and
//! compiles standalone — without vendoring Dear ImGui into this repo,
//! matching its "zero ImGui dependency, even in the C++ wrapper" design
//! goal. Compile-only: this never links or runs the mock, only proves
//! `cc` can turn it into an object file.
//!
//! `cc::Build` needs `TARGET`/`HOST`/`OPT_LEVEL`/`PROFILE` in the process
//! environment — Cargo only populates those for an actual build script,
//! not a `#[test]` binary — so `build.rs` forwards them into this crate's
//! compile-time environment via `cargo:rustc-env`, read back below through
//! `env!()`. This reuses `cc`'s own cross-platform compiler detection
//! (cl.exe on Windows, clang/gcc elsewhere) rather than hand-rolling a
//! second one just for this test.

#[test]
fn fluent_header_compiles_against_a_mock_draw_list() {
    std::env::set_var("TARGET", env!("IMGUI_PAINTER_BUILD_TARGET"));
    std::env::set_var("HOST", env!("IMGUI_PAINTER_BUILD_HOST"));
    std::env::set_var("OPT_LEVEL", env!("IMGUI_PAINTER_BUILD_OPT_LEVEL"));
    std::env::set_var("PROFILE", env!("IMGUI_PAINTER_BUILD_PROFILE"));

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out_dir = std::env::temp_dir().join(format!(
        "imgui_painter_header_check_{}_{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap(),
    ));
    std::fs::create_dir_all(&out_dir).expect("create scratch build dir");

    let result = cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(format!("{manifest_dir}/../../capi"))
        .include(format!("{manifest_dir}/../../include"))
        .file(format!("{manifest_dir}/tests/fluent_header_mock.cpp"))
        .out_dir(&out_dir)
        .try_compile("imgui_painter_header_check");

    let _ = std::fs::remove_dir_all(&out_dir);
    result.expect("include/imgui_painter.h failed to compile against the mock draw-list");
}
