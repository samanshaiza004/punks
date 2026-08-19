//! Private Linux X11 XDND source bridge.
//!
//! GPUI's pinned X11 backend accepts drops but has no source-side external
//! drag promotion. This module opens a small independent Xlib connection,
//! owns a hidden source window, sends the bounded XDND message sequence, and
//! answers the target's `text/uri-list` selection request. Wayland is detected
//! and left to GPUI's upstream source implementation.

#![cfg(target_os = "linux")]

use std::ffi::{c_int, c_long, c_uchar, c_ulong, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::time::{Duration, Instant};

use x11::xlib;

use super::drag::DragPaths;

const XDND_VERSION: c_long = 5;
const XDND_STATUS_ACCEPTED: c_long = 1;

#[derive(Clone, Copy)]
struct Atoms {
    aware: xlib::Atom,
    enter: xlib::Atom,
    position: xlib::Atom,
    status: xlib::Atom,
    drop: xlib::Atom,
    leave: xlib::Atom,
    finished: xlib::Atom,
    selection: xlib::Atom,
    uri_list: xlib::Atom,
    action_copy: xlib::Atom,
}

struct DragState {
    target: xlib::Window,
    finished: bool,
    succeeded: bool,
}

/// Start XDND only when the process is running on an X11 display. Under a
/// Wayland compositor `DISPLAY` may still name XWayland; GPUI must retain
/// ownership there, so the presence of `WAYLAND_DISPLAY` wins.
pub(super) fn start(paths: &DragPaths) {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_none() {
        return;
    }

    let paths: Vec<PathBuf> = paths
        .0
        .iter()
        .filter_map(|path| match std::fs::canonicalize(path) {
            Ok(path) => Some(path),
            Err(error) => {
                log::warn!(
                    "skipping X11 drag path that could not be canonicalized {path:?}: {error}"
                );
                None
            }
        })
        .collect();
    if paths.is_empty() {
        log::warn!("X11 drag ignored: no valid paths");
        return;
    }

    let uri_list = paths_to_uri_list(&paths);
    unsafe { run_drag(&uri_list) };
}

unsafe fn run_drag(uri_list: &[u8]) {
    let display = xlib::XOpenDisplay(ptr::null());
    if display.is_null() {
        log::warn!("X11 drag ignored: XOpenDisplay failed");
        return;
    }

    let screen = xlib::XDefaultScreen(display);
    let root = xlib::XRootWindow(display, screen);
    let source = xlib::XCreateSimpleWindow(display, root, 0, 0, 1, 1, 0, 0, 0);
    let atoms = intern_atoms(display);

    let version = XDND_VERSION as c_ulong;
    xlib::XChangeProperty(
        display,
        source,
        atoms.aware,
        xlib::XA_ATOM,
        32,
        xlib::PropModeReplace,
        (&version as *const c_ulong).cast::<c_uchar>(),
        1,
    );
    xlib::XSelectInput(
        display,
        source,
        xlib::PropertyChangeMask | xlib::StructureNotifyMask,
    );
    xlib::XSetSelectionOwner(display, atoms.selection, source, xlib::CurrentTime);
    xlib::XMapWindow(display, source);
    xlib::XFlush(display);

    let mut state = DragState {
        target: 0,
        finished: false,
        succeeded: false,
    };
    let mut dropped = false;
    let mut root_x = 0;
    let mut root_y = 0;
    let mut window_x = 0;
    let mut window_y = 0;
    let mut mask = 0;

    loop {
        let mut root_return = 0;
        let mut child_return = 0;
        let pointer_ok = xlib::XQueryPointer(
            display,
            root,
            &mut root_return,
            &mut child_return,
            &mut root_x,
            &mut root_y,
            &mut window_x,
            &mut window_y,
            &mut mask,
        ) != xlib::False;

        if pointer_ok && !dropped {
            let next_target = find_dnd_target(display, root, child_return, atoms.aware);
            if next_target != state.target {
                if state.target != 0 {
                    send_message(
                        display,
                        state.target,
                        atoms.leave,
                        [source as c_long, 0, 0, 0, 0],
                    );
                }
                state.target = next_target;
                if state.target != 0 {
                    send_enter(display, state.target, source, atoms);
                }
            }
            if state.target != 0 {
                send_position(display, state.target, source, atoms, root_x, root_y);
            }
            xlib::XFlush(display);
        }

        drain_events(display, source, atoms, uri_list, &mut state);

        if !pointer_ok || mask & xlib::Button1Mask == 0 {
            if state.target != 0 && !dropped {
                send_message(
                    display,
                    state.target,
                    atoms.drop,
                    [source as c_long, 0, xlib::CurrentTime as c_long, 0, 0],
                );
                xlib::XFlush(display);
                dropped = true;
            }

            let deadline = Instant::now() + Duration::from_secs(2);
            while dropped && !state.finished && Instant::now() < deadline {
                drain_events(display, source, atoms, uri_list, &mut state);
                std::thread::sleep(Duration::from_millis(2));
            }
            break;
        }

        std::thread::sleep(Duration::from_millis(16));
    }

    if state.target != 0 && !dropped {
        send_message(
            display,
            state.target,
            atoms.leave,
            [source as c_long, 0, 0, 0, 0],
        );
    }
    xlib::XSetSelectionOwner(display, atoms.selection, 0, xlib::CurrentTime);
    xlib::XDestroyWindow(display, source);
    xlib::XCloseDisplay(display);

    if dropped && !state.succeeded {
        log::debug!("X11 XDND target did not report a successful drop");
    }
}

unsafe fn intern_atoms(display: *mut xlib::Display) -> Atoms {
    Atoms {
        aware: intern(display, "XdndAware"),
        enter: intern(display, "XdndEnter"),
        position: intern(display, "XdndPosition"),
        status: intern(display, "XdndStatus"),
        drop: intern(display, "XdndDrop"),
        leave: intern(display, "XdndLeave"),
        finished: intern(display, "XdndFinished"),
        selection: intern(display, "XdndSelection"),
        uri_list: intern(display, "text/uri-list"),
        action_copy: intern(display, "XdndActionCopy"),
    }
}

unsafe fn intern(display: *mut xlib::Display, name: &str) -> xlib::Atom {
    let name = CString::new(name).expect("atom names contain no NUL");
    xlib::XInternAtom(display, name.as_ptr(), xlib::False)
}

unsafe fn send_enter(
    display: *mut xlib::Display,
    target: xlib::Window,
    source: xlib::Window,
    atoms: Atoms,
) {
    // One offered type fits in the three inline type slots, so XdndTypeList
    // is unnecessary for this bounded file-list source.
    send_message(
        display,
        target,
        atoms.enter,
        [
            source as c_long,
            XDND_VERSION << 24,
            atoms.uri_list as c_long,
            0,
            0,
        ],
    );
}

unsafe fn send_position(
    display: *mut xlib::Display,
    target: xlib::Window,
    source: xlib::Window,
    atoms: Atoms,
    x: c_int,
    y: c_int,
) {
    let packed = (((x.clamp(-32768, 32767) as u32 & 0xffff) << 16)
        | (y.clamp(-32768, 32767) as u32 & 0xffff)) as c_long;
    send_message(
        display,
        target,
        atoms.position,
        [
            source as c_long,
            0,
            packed,
            xlib::CurrentTime as c_long,
            atoms.action_copy as c_long,
        ],
    );
}

unsafe fn send_message(
    display: *mut xlib::Display,
    target: xlib::Window,
    message_type: xlib::Atom,
    values: [c_long; 5],
) {
    let mut data = xlib::ClientMessageData::default();
    data.as_longs_mut().copy_from_slice(&values);
    let message = xlib::XClientMessageEvent {
        type_: xlib::ClientMessage,
        serial: 0,
        send_event: xlib::True,
        display,
        window: target,
        message_type,
        format: 32,
        data,
    };
    let mut event = xlib::XEvent {
        client_message: message,
    };
    xlib::XSendEvent(display, target, xlib::False, xlib::NoEventMask, &mut event);
}

unsafe fn find_dnd_target(
    display: *mut xlib::Display,
    root: xlib::Window,
    mut window: xlib::Window,
    aware: xlib::Atom,
) -> xlib::Window {
    while window != 0 && window != root {
        if has_property(display, window, aware) {
            return window;
        }

        let mut tree_root = 0;
        let mut parent = 0;
        let mut children = ptr::null_mut();
        let mut child_count = 0;
        if xlib::XQueryTree(
            display,
            window,
            &mut tree_root,
            &mut parent,
            &mut children,
            &mut child_count,
        ) == xlib::False
        {
            return 0;
        }
        if !children.is_null() {
            xlib::XFree(children.cast());
        }
        if parent == window {
            return 0;
        }
        window = parent;
    }
    0
}

unsafe fn has_property(
    display: *mut xlib::Display,
    window: xlib::Window,
    property: xlib::Atom,
) -> bool {
    let mut actual_type = 0;
    let mut actual_format = 0;
    let mut item_count = 0;
    let mut bytes_after = 0;
    let mut data: *mut c_uchar = ptr::null_mut();
    let result = xlib::XGetWindowProperty(
        display,
        window,
        property,
        0,
        1,
        xlib::False,
        xlib::AnyPropertyType as c_ulong,
        &mut actual_type,
        &mut actual_format,
        &mut item_count,
        &mut bytes_after,
        &mut data,
    );
    if !data.is_null() {
        xlib::XFree(data.cast());
    }
    result == 0 && actual_type != 0 && actual_format != 0 && item_count > 0
}

unsafe fn drain_events(
    display: *mut xlib::Display,
    source: xlib::Window,
    atoms: Atoms,
    uri_list: &[u8],
    state: &mut DragState,
) {
    while xlib::XPending(display) > 0 {
        let mut event = std::mem::zeroed::<xlib::XEvent>();
        xlib::XNextEvent(display, &mut event);
        match event.type_ {
            xlib::ClientMessage => {
                let message = event.client_message;
                let values = message.data.as_longs();
                if message.message_type == atoms.status && message.window == source {
                    let _accepted = (values[1] & XDND_STATUS_ACCEPTED) != 0;
                } else if message.message_type == atoms.finished && message.window == source {
                    state.finished = true;
                    state.succeeded = (values[1] & XDND_STATUS_ACCEPTED) != 0;
                }
            }
            xlib::SelectionRequest => {
                answer_selection_request(display, event.selection_request, atoms, uri_list);
            }
            _ => {}
        }
    }
}

unsafe fn answer_selection_request(
    display: *mut xlib::Display,
    request: xlib::XSelectionRequestEvent,
    atoms: Atoms,
    uri_list: &[u8],
) {
    if request.selection != atoms.selection {
        return;
    }

    let property = if request.property == 0 {
        request.target
    } else {
        request.property
    };
    let success = request.target == atoms.uri_list && property != 0;
    if success {
        xlib::XChangeProperty(
            display,
            request.requestor,
            property,
            atoms.uri_list,
            8,
            xlib::PropModeReplace,
            uri_list.as_ptr(),
            uri_list.len() as c_int,
        );
    }

    let response = xlib::XSelectionEvent {
        type_: xlib::SelectionNotify,
        serial: 0,
        send_event: xlib::True,
        display,
        requestor: request.requestor,
        selection: request.selection,
        target: request.target,
        property: if success { property } else { 0 },
        time: request.time,
    };
    let mut event = xlib::XEvent {
        selection: response,
    };
    xlib::XSendEvent(
        display,
        request.requestor,
        xlib::False,
        xlib::NoEventMask,
        &mut event,
    );
    xlib::XFlush(display);
}

fn paths_to_uri_list(paths: &[PathBuf]) -> Vec<u8> {
    let mut output = Vec::new();
    for path in paths {
        output.extend_from_slice(b"file://");
        for &byte in path.as_os_str().as_bytes() {
            if byte == b'/'
                || byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'.' | b'_' | b'~')
            {
                output.push(byte);
            } else {
                output.extend_from_slice(format!("%{byte:02X}").as_bytes());
            }
        }
        output.extend_from_slice(b"\r\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_escapes_file_names_without_escaping_slashes() {
        let list = paths_to_uri_list(&[PathBuf::from("/tmp/a sample.wav")]);
        assert_eq!(list, b"file:///tmp/a%20sample.wav\r\n");
    }
}
