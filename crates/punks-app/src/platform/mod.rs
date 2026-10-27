//! OS-glue that doesn't belong in GPUI-agnostic application logic.

pub mod drag;

#[cfg(target_os = "windows")]
pub(crate) mod windows_drag;

#[cfg(target_os = "linux")]
pub(crate) mod x11_drag;
