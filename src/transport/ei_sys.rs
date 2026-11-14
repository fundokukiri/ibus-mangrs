//! Raw libei FFI surface: the crate's only C declarations.
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

use std::os::raw::{c_char, c_int, c_void};

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

#[link(name = "ei")]
extern "C" {
    // Alias of ei_new_sender: returns a fresh sender context or NULL.
    pub fn ei_new(user_data: *mut c_void) -> *mut Ei;
    // Must run immediately after ei_new and before backend setup.
    pub fn ei_configure_name(ei: *mut Ei, name: *const c_char);
    // Takes ownership of fd and closes it at teardown; 0 or negative errno.
    pub fn ei_setup_backend_fd(ei: *mut Ei, fd: c_int) -> c_int;
    pub fn ei_get_fd(ei: *mut Ei) -> c_int;
    pub fn ei_dispatch(ei: *mut Ei);
    pub fn ei_get_event(ei: *mut Ei) -> *mut EiEvent;
    pub fn ei_event_unref(event: *mut EiEvent);
    pub fn ei_event_get_type(event: *mut EiEvent) -> c_int;
    pub fn ei_event_get_device(event: *mut EiEvent) -> *mut EiDevice;
    pub fn ei_event_get_seat(event: *mut EiEvent) -> *mut EiSeat;
    // Varargs terminated by a NULL sentinel (header: __attribute__((sentinel))).
    pub fn ei_seat_bind_capabilities(seat: *mut EiSeat, ...);
    pub fn ei_device_ref(device: *mut EiDevice) -> *mut EiDevice;
    pub fn ei_device_unref(device: *mut EiDevice);
    pub fn ei_device_has_capability(device: *mut EiDevice, cap: c_int) -> bool;
    pub fn ei_device_start_emulating(device: *mut EiDevice, sequence: u32);
    pub fn ei_device_stop_emulating(device: *mut EiDevice);
    pub fn ei_device_keyboard_key(device: *mut EiDevice, keycode: u32, is_press: bool);
    // Header-recommended frame timestamp: ei_now = CLOCK_MONOTONIC microseconds.
    pub fn ei_device_frame(device: *mut EiDevice, time: u64);
    pub fn ei_now(ei: *mut Ei) -> u64;
    pub fn ei_unref(ei: *mut Ei) -> *mut Ei;
}
