//! Fake key injection through `/dev/uinput`.
//!
//! Clients that drop the IBus `DeleteSurroundingText` signal (Sublime Text,
//! GNOME Console, Chromium-based apps in some paths) make protocol-level
//! rewriting impossible: the engine deletes from its internal state but the
//! application never sees it — words duplicate and backspace appears dead.
//! When that is detected, the server falls back to injecting real Backspace
//! key events through a virtual keyboard, which every toolkit honours.
//!
//! The device is opened lazily and kept for the engine's lifetime; if
//! `/dev/uinput` is missing or not permitted, injection is skipped and the
//! protocol delete is used as before.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::thread::sleep;
use std::time::Duration;

// ioctl requests from linux/uinput.h (64-bit values, computed with _IOW).
const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const SYN_REPORT: u16 = 0;
const KEY_BACKSPACE: u16 = 14;

/// `struct input_event` on 64-bit Linux: timeval (16 bytes) + type + code + value.
#[repr(C)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

pub struct Uinput {
    file: File,
}

impl Uinput {
    /// Open the virtual keyboard once per process; `None` when unavailable.
    fn device() -> Option<&'static Uinput> {
        static DEVICE: std::sync::OnceLock<Option<Uinput>> = std::sync::OnceLock::new();
        DEVICE.get_or_init(Uinput::open).as_ref()
    }

    /// Whether a virtual keyboard is available.
    pub fn available() -> bool {
        Self::device().is_some()
    }

    /// Press Backspace `n` times on the virtual keyboard. Returns false when
    /// no device could be opened — the caller must then fall back to the
    /// protocol delete instead of silently dropping the deletion.
    pub fn inject_backspaces(n: u32) -> bool {
        match Self::device() {
            Some(dev) => {
                dev.backspace(n, 10);
                true
            }
            None => false,
        }
    }
}

impl Uinput {
    /// Create a minimal virtual keyboard that can only press Backspace.
    /// Returns `None` when the device cannot be opened (no permission, no
    /// module) — callers then keep the protocol delete.
    pub fn open() -> Option<Self> {
        let file =
            OpenOptions::new().write(true).read(true).open("/dev/uinput").ok()?;
        let fd = file.as_raw_fd();
        unsafe {
            if libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_int) != 0 {
                return None;
            }
            if libc::ioctl(fd, UI_SET_KEYBIT, KEY_BACKSPACE as libc::c_int) != 0 {
                return None;
            }
            // Legacy device setup: write a `struct uinput_user_dev` straight
            // to the fd (the UI_DEV_SETUP ioctl path is rejected by this
            // kernel with "write device info first").
            #[repr(C)]
            struct UinputUserDev {
                name: [u8; 80],
                id: [u16; 4], // bustype, vendor, product, version
                ff_effects_max: u32,
                abs: [i32; 256], // absmax/absmin/absfuzz/absflat, ABS_CNT=64 each
            }
            let mut setup = UinputUserDev {
                name: [0; 80],
                id: [0x03, 0x1, 0x6a71, 0x1], // bus=USB, vendor, product, version
                ff_effects_max: 0,
                abs: [0; 256],
            };
            let name = b"ibus-mangrs-inject";
            setup.name[..name.len()].copy_from_slice(name);
            let bytes = std::slice::from_raw_parts(
                &setup as *const UinputUserDev as *const u8,
                std::mem::size_of::<UinputUserDev>(),
            );
            if libc::write(fd, bytes.as_ptr() as *const libc::c_void, bytes.len()) < 0 {
                return None;
            }
            if libc::ioctl(fd, UI_DEV_CREATE) != 0 {
                return None;
            }
        }
        Some(Self { file })
    }

    /// Press Backspace `n` times, `interval_ms` apart (10ms default keeps
    /// slow clients from reordering events).
    pub fn backspace(&self, n: u32, interval_ms: u64) {
        let fd = self.file.as_raw_fd();
        for _ in 0..n {
            let press = InputEvent {
                tv_sec: 0,
                tv_usec: 0,
                type_: EV_KEY,
                code: KEY_BACKSPACE,
                value: 1,
            };
            let release = InputEvent { type_: EV_KEY, code: KEY_BACKSPACE, value: 0, ..press };
            let report = InputEvent { type_: EV_SYN, code: SYN_REPORT, value: 0, ..press };
            unsafe {
                let _ = libc::write(
                    fd,
                    &press as *const InputEvent as *const libc::c_void,
                    std::mem::size_of::<InputEvent>(),
                );
                let _ = libc::write(
                    fd,
                    &report as *const InputEvent as *const libc::c_void,
                    std::mem::size_of::<InputEvent>(),
                );
                let _ = libc::write(
                    fd,
                    &release as *const InputEvent as *const libc::c_void,
                    std::mem::size_of::<InputEvent>(),
                );
                let _ = libc::write(
                    fd,
                    &report as *const InputEvent as *const libc::c_void,
                    std::mem::size_of::<InputEvent>(),
                );
            }
            sleep(Duration::from_millis(interval_ms));
        }
    }
}

impl Drop for Uinput {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_event_layout_is_24_bytes() {
        assert_eq!(std::mem::size_of::<InputEvent>(), 24);
    }

    #[test]
    fn device_opens_with_correct_permissions() {
        // Needs /dev/uinput readable+writable (udev rule 80-uinput.rules).
        // Without it the engine falls back to the protocol delete, so a
        // sandbox/CI without the device is not a code failure. Set
        // MANGRS_REQUIRE_UINPUT=1 to enforce the hardware check.
        match Uinput::open() {
            Some(_) => eprintln!("uinput: OK"),
            None if std::env::var_os("MANGRS_REQUIRE_UINPUT").is_some() => {
                panic!("uinput device could not be opened (check /dev/uinput permissions)")
            }
            None => eprintln!("uinput: SKIP (no writable /dev/uinput)"),
        }
    }
}

