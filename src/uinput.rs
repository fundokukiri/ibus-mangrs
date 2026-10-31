//! Fake key injection through `/dev/uinput`.
//!
//! Clients that drop the IBus `DeleteSurroundingText` signal (Sublime Text,
//! GNOME Console, Chromium-based apps in some paths) make protocol-level
//! rewriting impossible: the engine deletes from its internal state but the
//! application never sees it — words duplicate and backspace appears dead.
//! When that is detected, the server falls back to injecting real Backspace
//! key events through a virtual keyboard, which every toolkit honours.
//!
//! The uinput display modes (Smooth, Select) use the same device for more:
//! paced key presses, Shift+Left selection and Ctrl+V. Vietnamese glyphs have
//! no keycodes, so text is injected per ASCII character only — everything
//! else travels over IBus (CommitText or the clipboard, see `paste.rs`).
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

// Linux keycodes (input-event-codes.h) — the AT set-1 numbers are wrong here.
const KEY_BACKSPACE: u16 = 14;
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_LEFT: u16 = 105;
/// Public so the server can press Delete for the Select mode rewrite.
pub const KEYCODE_DELETE: u16 = 111;

/// `struct input_event` on 64-bit Linux: timeval (16 bytes) + type + code + value.
#[repr(C)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

/// US-QWERTY mapping for injectable characters: (keycode, shift needed).
/// Non-ASCII (Vietnamese) characters have no keycode and return `None` —
/// they must travel over IBus, not the virtual keyboard.
pub fn char_key(ch: char) -> Option<(u16, bool)> {
    Some(match ch {
        ' ' => (57, false),
        '\n' => (28, false),
        '\t' => (15, false),
        'a' => (30, false),
        'b' => (48, false),
        'c' => (46, false),
        'd' => (32, false),
        'e' => (18, false),
        'f' => (33, false),
        'g' => (34, false),
        'h' => (35, false),
        'i' => (23, false),
        'j' => (36, false),
        'k' => (37, false),
        'l' => (38, false),
        'm' => (50, false),
        'n' => (49, false),
        'o' => (24, false),
        'p' => (25, false),
        'q' => (16, false),
        'r' => (19, false),
        's' => (31, false),
        't' => (20, false),
        'u' => (22, false),
        'v' => (47, false),
        'w' => (17, false),
        'x' => (45, false),
        'y' => (21, false),
        'z' => (44, false),
        '1' => (2, false),
        '2' => (3, false),
        '3' => (4, false),
        '4' => (5, false),
        '5' => (6, false),
        '6' => (7, false),
        '7' => (8, false),
        '8' => (9, false),
        '9' => (10, false),
        '0' => (11, false),
        '.' => (52, false),
        ',' => (51, false),
        '/' => (53, false),
        ';' => (39, false),
        '\'' => (40, false),
        '-' => (12, false),
        '=' => (13, false),
        '[' => (26, false),
        ']' => (27, false),
        '\\' => (43, false),
        '`' => (41, false),
        'A' => (30, true),
        'B' => (48, true),
        'C' => (46, true),
        'D' => (32, true),
        'E' => (18, true),
        'F' => (33, true),
        'G' => (34, true),
        'H' => (35, true),
        'I' => (23, true),
        'J' => (36, true),
        'K' => (37, true),
        'L' => (38, true),
        'M' => (50, true),
        'N' => (49, true),
        'O' => (24, true),
        'P' => (25, true),
        'Q' => (16, true),
        'R' => (19, true),
        'S' => (31, true),
        'T' => (20, true),
        'U' => (22, true),
        'V' => (47, true),
        'W' => (17, true),
        'X' => (45, true),
        'Y' => (21, true),
        'Z' => (44, true),
        '!' => (2, true),
        '@' => (3, true),
        '#' => (4, true),
        '$' => (5, true),
        '%' => (6, true),
        '^' => (7, true),
        '&' => (8, true),
        '*' => (9, true),
        '(' => (10, true),
        ')' => (11, true),
        '?' => (53, true),
        ':' => (39, true),
        '"' => (40, true),
        '_' => (12, true),
        '+' => (13, true),
        '{' => (26, true),
        '}' => (27, true),
        '|' => (43, true),
        '<' => (51, true),
        '>' => (52, true),
        '~' => (41, true),
        _ => return None,
    })
}

/// Every keycode this module may ever press (device registration list).
fn all_keycodes() -> Vec<u16> {
    let mut codes: Vec<u16> = vec![
        KEY_BACKSPACE,
        KEY_LEFTCTRL,
        KEY_LEFTSHIFT,
        KEY_LEFT,
        KEYCODE_DELETE,
        28, // enter
        15, // tab
        57, // space
    ];
    // Collect the distinct keycodes from the whole character table.
    for ch in "abcdefghijklmnopqrstuvwxyz0123456789.,/;'-=[]\\`!@#$%^&*()?:\"_+{}|<>~"
        .chars()
        .filter_map(char_key)
    {
        codes.push(ch.0);
    }
    codes.sort_unstable();
    codes.dedup();
    codes
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

    /// Press Backspace `n` times on the virtual keyboard, `interval_ms`
    /// apart. Returns false when no device could be opened — the caller must
    /// then fall back to the protocol delete instead of silently dropping
    /// the deletion.
    pub fn inject_backspaces(n: u32, interval_ms: u64) -> bool {
        match Self::device() {
            Some(dev) => {
                dev.backspace(n, interval_ms);
                true
            }
            None => false,
        }
    }

    /// Press a sequence of (keycode, shift) keys, `interval_ms` apart.
    /// Returns false when the device is unavailable.
    pub fn press_keys(keys: &[(u16, bool)], interval_ms: u64) -> bool {
        match Self::device() {
            Some(dev) => {
                for &(code, shift) in keys {
                    dev.press(code, shift);
                    if interval_ms > 0 {
                        sleep(Duration::from_millis(interval_ms));
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Select `n` characters to the left with Shift+Left, paced. Returns
    /// false when the device is unavailable.
    pub fn select_left(n: u32, gap_ms: u64) -> bool {
        match Self::device() {
            Some(dev) => {
                dev.key_event(KEY_LEFTSHIFT, 1);
                for _ in 0..n {
                    dev.press(KEY_LEFT, false);
                    if gap_ms > 0 {
                        sleep(Duration::from_millis(gap_ms));
                    }
                }
                dev.key_event(KEY_LEFTSHIFT, 0);
                true
            }
            None => false,
        }
    }

    /// Press Ctrl+V (paste). Returns false when the device is unavailable.
    pub fn ctrl_v() -> bool {
        match Self::device() {
            Some(dev) => {
                dev.key_event(KEY_LEFTCTRL, 1);
                dev.press(KEY_V_CODE, false);
                dev.key_event(KEY_LEFTCTRL, 0);
                true
            }
            None => false,
        }
    }
}

/// `KEY_V` alias for readability at the call site above.
const KEY_V_CODE: u16 = 47;

impl Uinput {
    /// Create a virtual keyboard covering every key this module can press.
    /// Returns `None` when the device cannot be opened (no permission, no
    /// module) — callers then keep the protocol delete.
    pub fn open() -> Option<Self> {
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .open("/dev/uinput")
            .ok()?;
        let fd = file.as_raw_fd();
        unsafe {
            if libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_int) != 0 {
                return None;
            }
            // Every keycode must be registered or its events are silently
            // dropped by the kernel.
            for code in all_keycodes() {
                if libc::ioctl(fd, UI_SET_KEYBIT, code as libc::c_int) != 0 {
                    return None;
                }
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

    /// Press and release one key (with Shift held when `shift`).
    fn press(&self, code: u16, shift: bool) {
        if shift {
            self.key_event(KEY_LEFTSHIFT, 1);
        }
        self.key_event(code, 1);
        self.key_event(code, 0);
        if shift {
            self.key_event(KEY_LEFTSHIFT, 0);
        }
    }

    /// Emit one raw key event.
    fn key_event(&self, code: u16, value: i32) {
        let fd = self.file.as_raw_fd();
        let event = InputEvent {
            tv_sec: 0,
            tv_usec: 0,
            type_: EV_KEY,
            code,
            value,
        };
        let report = InputEvent {
            type_: EV_SYN,
            code: SYN_REPORT,
            value: 0,
            ..event
        };
        unsafe {
            let _ = libc::write(
                fd,
                &event as *const InputEvent as *const libc::c_void,
                std::mem::size_of::<InputEvent>(),
            );
            let _ = libc::write(
                fd,
                &report as *const InputEvent as *const libc::c_void,
                std::mem::size_of::<InputEvent>(),
            );
        }
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
            let release = InputEvent {
                type_: EV_KEY,
                code: KEY_BACKSPACE,
                value: 0,
                ..press
            };
            let report = InputEvent {
                type_: EV_SYN,
                code: SYN_REPORT,
                value: 0,
                ..press
            };
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
            if interval_ms > 0 {
                sleep(Duration::from_millis(interval_ms));
            }
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
    fn char_table_covers_ascii_and_refuses_vietnamese() {
        for ch in ' '..='~' {
            assert!(char_key(ch).is_some(), "printable ASCII {ch:?} must map");
        }
        assert_eq!(char_key('ư'), None);
        assert_eq!(char_key('ạ'), None);
        assert_eq!(char_key('ế'), None);
        // shift correctness spot checks
        assert_eq!(char_key('a'), Some((30, false)));
        assert_eq!(char_key('A'), Some((30, true)));
        assert_eq!(char_key('?'), Some((53, true)));
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
