//! Raw libei FFI surface: the crate's only C declarations, loaded at RUNTIME.
//!
//! `libei.so.1` is required at RUNTIME only when the Libei backend is
//! selected: the symbol table below is dlopen'd lazily and process-wide, and
//! a machine without the library (X11/XFCE, plain wlroots) denies the Libei
//! kind through the existing constructor path — the engine builds and runs
//! without it. The same denial applies when the library is present but does
//! not export every symbol the backend needs.
//!
//! Every name, enum value and signature is verified against
//! `/usr/include/libei-1.0/libei.h` (libei 1.6.0). The ownership and validity
//! contracts for each call live with the unsafe blocks that make them (see
//! `libei.rs`): libei owns the EI socket fd and closes it at teardown, events
//! and devices are refcounted, and `ei_seat_bind_capabilities` is a
//! NULL-sentinel varargs call. There is no `ei_is_connected`-style query in
//! the public header — connection and device discovery are event-driven
//! through `ei_dispatch` + `ei_get_event`.
//!
//! Keycodes into `ei_device_keyboard_key` are RAW evdev codes (libei adds the
//! EI wire offset +8 itself).

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::OnceLock;

// enum ei_event_type, values from the header; only the events the backend
// reacts to.
pub const EI_EVENT_DISCONNECT: c_int = 2;
pub const EI_EVENT_SEAT_ADDED: c_int = 3;
pub const EI_EVENT_DEVICE_ADDED: c_int = 5;
pub const EI_EVENT_DEVICE_REMOVED: c_int = 6;
pub const EI_EVENT_DEVICE_RESUMED: c_int = 8;

// enum ei_device_capability: EI_DEVICE_CAP_KEYBOARD = (1 << 2)
pub const EI_DEVICE_CAP_KEYBOARD: c_int = 1 << 2;

// Opaque libei objects; the zero-sized field keeps them uninstantiable.
#[repr(C)]
pub struct Ei {
    _private: [u8; 0],
}
#[repr(C)]
pub struct EiEvent {
    _private: [u8; 0],
}
#[repr(C)]
pub struct EiSeat {
    _private: [u8; 0],
}
#[repr(C)]
pub struct EiDevice {
    _private: [u8; 0],
}

/// The libei entry points the backend calls, resolved from `libei.so.1` by
/// dlsym. Function-pointer fields keep the call sites in `libei.rs` shaped
/// like a link-time extern surface.
pub struct Table {
    // Alias of ei_new_sender: returns a fresh sender context or NULL.
    pub ei_new: unsafe extern "C" fn(*mut c_void) -> *mut Ei,
    // Must run immediately after ei_new and before backend setup.
    pub ei_configure_name: unsafe extern "C" fn(*mut Ei, *const c_char),
    // Takes ownership of fd and closes it at teardown; 0 or negative errno.
    pub ei_setup_backend_fd: unsafe extern "C" fn(*mut Ei, c_int) -> c_int,
    pub ei_get_fd: unsafe extern "C" fn(*mut Ei) -> c_int,
    pub ei_dispatch: unsafe extern "C" fn(*mut Ei),
    pub ei_get_event: unsafe extern "C" fn(*mut Ei) -> *mut EiEvent,
    pub ei_event_unref: unsafe extern "C" fn(*mut EiEvent),
    pub ei_event_get_type: unsafe extern "C" fn(*mut EiEvent) -> c_int,
    pub ei_event_get_device: unsafe extern "C" fn(*mut EiEvent) -> *mut EiDevice,
    pub ei_event_get_seat: unsafe extern "C" fn(*mut EiEvent) -> *mut EiSeat,
    // The header declares the real symbol as a NULL-sentinel varargs call
    // (`ei_seat_bind_capabilities(seat, ..., NULL)`), which Rust cannot
    // express as a fn pointer, so the address is loaded through the fixed
    // three-argument prototype the backend always passes. Sound because only
    // integer/pointer args are involved: on the SysV targets (x86-64,
    // aarch64) they travel identically for variadic and fixed calls, and the
    // hidden x86-64 vector-count register (AL) only sizes the callee's own
    // register-save spill.
    pub ei_seat_bind_keyboard: unsafe extern "C" fn(*mut EiSeat, c_int, *mut c_void),
    pub ei_device_ref: unsafe extern "C" fn(*mut EiDevice) -> *mut EiDevice,
    pub ei_device_unref: unsafe extern "C" fn(*mut EiDevice),
    pub ei_device_has_capability: unsafe extern "C" fn(*mut EiDevice, c_int) -> bool,
    pub ei_device_start_emulating: unsafe extern "C" fn(*mut EiDevice, u32),
    pub ei_device_stop_emulating: unsafe extern "C" fn(*mut EiDevice),
    pub ei_device_keyboard_key: unsafe extern "C" fn(*mut EiDevice, u32, bool),
    // Header-recommended frame timestamp: ei_now = CLOCK_MONOTONIC microseconds.
    pub ei_device_frame: unsafe extern "C" fn(*mut EiDevice, u64),
    pub ei_now: unsafe extern "C" fn(*mut Ei) -> u64,
    pub ei_unref: unsafe extern "C" fn(*mut Ei) -> *mut Ei,
}

/// The resolved symbols, loaded once per process.
static TABLE: OnceLock<Table> = OnceLock::new();

/// The libei entry points, or None when `libei.so.1` is missing or does not
/// export every symbol the backend needs.
pub fn table() -> Option<&'static Table> {
    if let Some(t) = TABLE.get() {
        return Some(t);
    }
    let t = load()?;
    // A racing caller may have installed its own resolution first; both come
    // from dlsym on the same library and are interchangeable.
    let _ = TABLE.set(t);
    TABLE.get()
}

/// dlopen the library and resolve every needed symbol; on a partial load the
/// handle is released again, so a failed attempt leaves nothing behind.
fn load() -> Option<Table> {
    // SAFETY: dlopen takes only the library name and flags; the handle it
    // returns is owned by this function and never shared before the table is
    // complete.
    let handle = unsafe { libc::dlopen(c"libei.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        return None;
    }
    let table = resolve(handle);
    if table.is_none() {
        // SAFETY: no pointer resolved from the handle escaped (resolve
        // returned None), so the library can be released again.
        unsafe { libc::dlclose(handle) };
    }
    table
}

/// Resolve every symbol the backend needs; None when any one is missing.
fn resolve(handle: *mut c_void) -> Option<Table> {
    Some(Table {
        ei_new: symbol(handle, c"ei_new")?,
        ei_configure_name: symbol(handle, c"ei_configure_name")?,
        ei_setup_backend_fd: symbol(handle, c"ei_setup_backend_fd")?,
        ei_get_fd: symbol(handle, c"ei_get_fd")?,
        ei_dispatch: symbol(handle, c"ei_dispatch")?,
        ei_get_event: symbol(handle, c"ei_get_event")?,
        ei_event_unref: symbol(handle, c"ei_event_unref")?,
        ei_event_get_type: symbol(handle, c"ei_event_get_type")?,
        ei_event_get_device: symbol(handle, c"ei_event_get_device")?,
        ei_event_get_seat: symbol(handle, c"ei_event_get_seat")?,
        ei_seat_bind_keyboard: symbol(handle, c"ei_seat_bind_capabilities")?,
        ei_device_ref: symbol(handle, c"ei_device_ref")?,
        ei_device_unref: symbol(handle, c"ei_device_unref")?,
        ei_device_has_capability: symbol(handle, c"ei_device_has_capability")?,
        ei_device_start_emulating: symbol(handle, c"ei_device_start_emulating")?,
        ei_device_stop_emulating: symbol(handle, c"ei_device_stop_emulating")?,
        ei_device_keyboard_key: symbol(handle, c"ei_device_keyboard_key")?,
        ei_device_frame: symbol(handle, c"ei_device_frame")?,
        ei_now: symbol(handle, c"ei_now")?,
        ei_unref: symbol(handle, c"ei_unref")?,
    })
    // On success the handle is intentionally never dlclosed: the fn pointers
    // above must stay valid for the process lifetime (dlopen semantics).
}

/// Resolve one dlsym address into the fn-pointer type its Table field
/// expects; `None` when the library does not export the name.
fn symbol<F: Copy>(handle: *mut c_void, name: &'static CStr) -> Option<F> {
    // SAFETY: handle is a live dlopen handle; dlsym only reads it and the
    // name, returning NULL for a missing symbol.
    let addr = unsafe { libc::dlsym(handle, name.as_ptr()) };
    if addr.is_null() {
        return None;
    }
    // SAFETY: the address is the entry point of the named C function and F
    // is the fn-pointer type matching its libei.h prototype (same pointer
    // representation) — the standard dlsym loader cast.
    Some(unsafe { std::mem::transmute_copy(&addr) })
}
