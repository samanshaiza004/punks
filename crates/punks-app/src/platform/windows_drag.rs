//! Private Windows OLE source-side drag bridge.
//!
//! GPUI's pinned Windows backend implements accepting drops but does not
//! promote an in-app drag to an OS drag. This is the smallest file-list source
//! needed by Punks: one `IDataObject` offering `CF_HDROP` and one
//! `IDropSource` with copy semantics. The bridge is called from GPUI's drag
//! gesture on the UI thread, where OLE owns its normal nested drag loop.

#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC, E_INVALIDARG,
    E_NOINTERFACE, E_NOTIMPL, E_OUTOFMEMORY, POINT, S_OK,
};
use windows_sys::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows_sys::Win32::System::Ole::{
    DoDragDrop, OleInitialize, OleUninitialize, CF_HDROP, DROPEFFECT_COPY,
};
use windows_sys::Win32::System::SystemServices::MK_LBUTTON;

use super::drag::DragPaths;

#[link(name = "kernel32")]
extern "system" {
    /// `windows-sys` 0.61 exposes the GlobalAlloc family except GlobalFree.
    /// CF_HDROP requires the matching kernel32 deallocator for its HGLOBAL.
    #[link_name = "GlobalFree"]
    fn global_free(handle: *mut c_void) -> *mut c_void;
}

const IID_IUNKNOWN: GUID = GUID {
    data1: 0x00000000,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};
const IID_IDATA_OBJECT: GUID = GUID {
    data1: 0x0000010e,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};
const IID_IDROP_SOURCE: GUID = GUID {
    data1: 0x00000121,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

#[repr(C)]
struct IUnknownVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct IDataObjectVtbl {
    parent: IUnknownVtbl,
    get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    get_data_here:
        unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    query_get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC) -> HRESULT,
    get_canonical_format_etc:
        unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut FORMATETC) -> HRESULT,
    set_data:
        unsafe extern "system" fn(*mut c_void, *const FORMATETC, *const STGMEDIUM, i32) -> HRESULT,
    enum_format_etc: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
    d_advise: unsafe extern "system" fn(
        *mut c_void,
        *const FORMATETC,
        u32,
        *const c_void,
        *mut u32,
    ) -> HRESULT,
    d_unadvise: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    enum_d_advise: unsafe extern "system" fn(*mut c_void, *const *const c_void) -> HRESULT,
}

#[repr(C)]
struct IDropSourceVtbl {
    parent: IUnknownVtbl,
    query_continue_drag: unsafe extern "system" fn(*mut c_void, i32, u32) -> HRESULT,
    give_feedback: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
}

#[repr(C)]
struct FileDataObject {
    vtable: *const IDataObjectVtbl,
    refs: AtomicU32,
    files: Vec<u16>,
}

#[repr(C)]
struct FileDropSource {
    vtable: *const IDropSourceVtbl,
    refs: AtomicU32,
}

/// Start a copy-semantics Windows file drag for the selected paths.
pub(super) fn start(paths: &DragPaths) {
    let files: Vec<PathBuf> = paths
        .0
        .iter()
        .filter_map(|path| match std::fs::canonicalize(path) {
            Ok(path) => Some(path),
            Err(error) => {
                log::warn!("skipping drag path that could not be canonicalized {path:?}: {error}");
                None
            }
        })
        .collect();
    if files.is_empty() {
        log::warn!("Windows drag ignored: no valid paths");
        return;
    }

    let mut file_list = Vec::new();
    for path in files {
        file_list.extend(path.as_os_str().encode_wide());
        file_list.push(0);
    }
    file_list.push(0);

    let data = Box::into_raw(Box::new(FileDataObject {
        vtable: &DATA_OBJECT_VTABLE,
        refs: AtomicU32::new(1),
        files: file_list,
    }));
    let source = Box::into_raw(Box::new(FileDropSource {
        vtable: &DROP_SOURCE_VTABLE,
        refs: AtomicU32::new(1),
    }));

    // GPUI's Windows application is expected to run its UI thread as an OLE
    // apartment. Initialize this call's apartment explicitly so the bridge
    // remains correct if that startup detail changes later.
    let ole_result = unsafe { OleInitialize(ptr::null()) };
    if ole_result < 0 {
        log::error!("Windows OLE initialization failed: HRESULT 0x{ole_result:08x}");
        unsafe {
            release_data(data.cast());
            release_drop_source(source.cast());
        }
        return;
    }

    let mut effect = 0;
    let result = unsafe { DoDragDrop(data.cast(), source.cast(), DROPEFFECT_COPY, &mut effect) };

    unsafe {
        release_data(data.cast());
        release_drop_source(source.cast());
        OleUninitialize();
    }

    if result < 0 {
        log::warn!("Windows file drag ended with HRESULT 0x{result:08x}");
    }
}

unsafe extern "system" fn data_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    if !guid_eq(*iid, IID_IUNKNOWN) && !guid_eq(*iid, IID_IDATA_OBJECT) {
        *out = ptr::null_mut();
        return E_NOINTERFACE;
    }
    *out = this;
    data_add_ref(this);
    S_OK
}

unsafe extern "system" fn data_add_ref(this: *mut c_void) -> u32 {
    let object = &*(this as *mut FileDataObject);
    object.refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn release_data(this: *mut c_void) -> u32 {
    let object = &*(this as *mut FileDataObject);
    let refs = object.refs.fetch_sub(1, Ordering::Release) - 1;
    if refs == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        drop(Box::from_raw(this as *mut FileDataObject));
    }
    refs
}

unsafe extern "system" fn data_get_data(
    this: *mut c_void,
    format: *const FORMATETC,
    medium: *mut STGMEDIUM,
) -> HRESULT {
    if format.is_null() || medium.is_null() {
        return E_INVALIDARG;
    }
    if data_supports_format(&*format) != S_OK {
        return DV_E_FORMATETC;
    }

    let object = &*(this as *mut FileDataObject);
    let bytes = match size_of::<windows_sys::Win32::UI::Shell::DROPFILES>()
        .checked_add(object.files.len() * size_of::<u16>())
    {
        Some(bytes) => bytes,
        None => return E_OUTOFMEMORY,
    };
    let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes);
    if handle.is_null() {
        return E_OUTOFMEMORY;
    }
    let memory = GlobalLock(handle);
    if memory.is_null() {
        global_free(handle);
        return E_OUTOFMEMORY;
    }

    let dropfiles = windows_sys::Win32::UI::Shell::DROPFILES {
        pFiles: size_of::<windows_sys::Win32::UI::Shell::DROPFILES>() as u32,
        pt: POINT { x: 0, y: 0 },
        fNC: 0,
        fWide: 1,
    };
    ptr::write_unaligned(memory.cast(), dropfiles);
    ptr::copy_nonoverlapping(
        object.files.as_ptr(),
        memory
            .add(size_of::<windows_sys::Win32::UI::Shell::DROPFILES>())
            .cast(),
        object.files.len(),
    );
    GlobalUnlock(handle);

    *medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        pUnkForRelease: ptr::null_mut(),
    };
    S_OK
}

unsafe extern "system" fn data_query_get_data(
    _this: *mut c_void,
    format: *const FORMATETC,
) -> HRESULT {
    if format.is_null() {
        E_INVALIDARG
    } else {
        data_supports_format(&*format)
    }
}

fn data_supports_format(format: &FORMATETC) -> HRESULT {
    if format.cfFormat == CF_HDROP
        && format.dwAspect == DVASPECT_CONTENT
        && format.lindex == -1
        && (format.tymed & TYMED_HGLOBAL as u32) != 0
    {
        S_OK
    } else {
        DV_E_FORMATETC
    }
}

unsafe extern "system" fn not_implemented_get_data_here(
    _this: *mut c_void,
    _format: *const FORMATETC,
    _medium: *mut STGMEDIUM,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_canonical(
    _this: *mut c_void,
    _input: *const FORMATETC,
    _output: *mut FORMATETC,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_set_data(
    _this: *mut c_void,
    _format: *const FORMATETC,
    _medium: *const STGMEDIUM,
    _release: i32,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_enum_format(
    _this: *mut c_void,
    _direction: u32,
    _out: *mut *mut c_void,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_advise(
    _this: *mut c_void,
    _format: *const FORMATETC,
    _flags: u32,
    _sink: *const c_void,
    _connection: *mut u32,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_unadvise(
    _this: *mut c_void,
    _connection: u32,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_enum_advise(
    _this: *mut c_void,
    _out: *const *const c_void,
) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn source_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    if !guid_eq(*iid, IID_IUNKNOWN) && !guid_eq(*iid, IID_IDROP_SOURCE) {
        *out = ptr::null_mut();
        return E_NOINTERFACE;
    }
    *out = this;
    source_add_ref(this);
    S_OK
}

unsafe extern "system" fn source_add_ref(this: *mut c_void) -> u32 {
    let source = &*(this as *mut FileDropSource);
    source.refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn release_drop_source(this: *mut c_void) -> u32 {
    let source = &*(this as *mut FileDropSource);
    let refs = source.refs.fetch_sub(1, Ordering::Release) - 1;
    if refs == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        drop(Box::from_raw(this as *mut FileDropSource));
    }
    refs
}

unsafe extern "system" fn source_query_continue(
    _this: *mut c_void,
    escape_pressed: i32,
    key_state: u32,
) -> HRESULT {
    if escape_pressed != 0 {
        DRAGDROP_S_CANCEL
    } else if key_state & MK_LBUTTON == 0 {
        DRAGDROP_S_DROP
    } else {
        S_OK
    }
}

unsafe extern "system" fn source_give_feedback(_this: *mut c_void, _effect: u32) -> HRESULT {
    DRAGDROP_S_USEDEFAULTCURSORS
}

static DATA_OBJECT_VTABLE: IDataObjectVtbl = IDataObjectVtbl {
    parent: IUnknownVtbl {
        query_interface: data_query_interface,
        add_ref: data_add_ref,
        release: release_data,
    },
    get_data: data_get_data,
    get_data_here: not_implemented_get_data_here,
    query_get_data: data_query_get_data,
    get_canonical_format_etc: not_implemented_canonical,
    set_data: not_implemented_set_data,
    enum_format_etc: not_implemented_enum_format,
    d_advise: not_implemented_advise,
    d_unadvise: not_implemented_unadvise,
    enum_d_advise: not_implemented_enum_advise,
};

static DROP_SOURCE_VTABLE: IDropSourceVtbl = IDropSourceVtbl {
    parent: IUnknownVtbl {
        query_interface: source_query_interface,
        add_ref: source_add_ref,
        release: release_drop_source,
    },
    query_continue_drag: source_query_continue,
    give_feedback: source_give_feedback,
};

fn guid_eq(left: GUID, right: GUID) -> bool {
    left.data1 == right.data1
        && left.data2 == right.data2
        && left.data3 == right.data3
        && left.data4 == right.data4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_comparison_accepts_only_identical_interface_ids() {
        assert!(guid_eq(IID_IUNKNOWN, IID_IUNKNOWN));
        assert!(!guid_eq(IID_IUNKNOWN, IID_IDATA_OBJECT));
    }
}
