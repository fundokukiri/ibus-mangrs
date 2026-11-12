//! Fake key injection through `/dev/uinput`.
//!
//! Clients that drop the IBus `DeleteSurroundingText` signal (Sublime Text,
//! GNOME Console, Chromium-based apps in some paths) make protocol-level
//! rewriting impossible: the engine deletes from its internal state but the
//! application never sees it — words duplicate and backspace appears dead.
//! When that is detected, the server falls back to injecting real Backspace
//! key events through a virtual keyboard, which every toolkit honours.
//!
//! The uinput display modes (Flow, Select) use the same device for more:
//! paced key presses, Shift+Left selection and Ctrl+V. Vietnamese glyphs have
//! no keycodes, so text is injected per ASCII character only — everything
//! else travels over IBus (CommitText or the clipboard, see `paste.rs`).
//!
//! The device is opened lazily and kept for the engine's lifetime; a failed
//! attempt is retried every couple of seconds, because the session ACL that
//! grants access is applied by logind around login — later than the engine
//! may start. If `/dev/uinput` is missing or not permitted, injection is
//! skipped and the protocol delete is used as before.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

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

/// The only keycodes the virtual keyboard registers. The kernel drops events
/// for unregistered codes, so a bug in the injection path can press these six
/// keys and nothing else — no letters, so it cannot type text. Keep the list in
/// sync with the calls in `Uinput` (`backspace`, `select_left`, `ctrl_v`) and
/// with `KEYCODE_DELETE` pressed by the server.
const KEYCODES: [u16; 6] = [
    KEY_BACKSPACE,
    KEYCODE_DELETE,
    KEY_LEFT,
    KEY_LEFTSHIFT,
    KEY_LEFTCTRL,
    KEY_V_CODE,
];

pub struct Uinput {
    file: File,
}

/// Pause between two failed opens. The session ACL for `/dev/uinput` is
/// applied by logind around login, which can be later than the engine start,
/// so a failed open must not be final — but retrying on every keystroke would
/// put an `open()` on the key path.
const RETRY: Duration = Duration::from_secs(2);

/// The device slot: `None` until an open succeeds, plus when to try again.
struct Slot {
    device: Option<&'static Uinput>,
    next_try: Option<Instant>,
}

static DEVICE: Mutex<Slot> = Mutex::new(Slot {
    device: None,
    next_try: None,
});

/// Whether a fresh open attempt is due at `now`.
fn retry_due(next_try: Option<Instant>, now: Instant) -> bool {
    match next_try {
        Some(at) => now >= at,
        None => true,
    }
}

impl Uinput {
    /// Open the virtual keyboard once per process and keep it for the process
    /// lifetime; a failed attempt is retried after `RETRY`.
    fn device() -> Option<&'static Uinput> {
        let mut slot = DEVICE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(device) = slot.device {
            return Some(device);
        }
        if !retry_due(slot.next_try, Instant::now()) {
            return None;
        }
        match Self::open() {
            Some(device) => {
                // Lives as long as the process, like the old `OnceLock`.
                slot.device = Some(Box::leak(Box::new(device)));
                slot.next_try = None;
            }
            None => slot.next_try = Some(Instant::now() + RETRY),
        }
        slot.device
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
            // dropped by the kernel — which is exactly the capability limit
            // this device relies on.
            for code in KEYCODES {
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
    fn device_registers_only_the_whitelisted_keys() {
        // The kernel drops events for unregistered keycodes, so this list is
        // the device's whole capability. No letter or digit may be added: with
        // one, a bug in the injection path could type arbitrary text. `V`
        // appears only as Ctrl+V (the clipboard channel).
        assert_eq!(
            KEYCODES,
            [
                KEY_BACKSPACE,
                KEYCODE_DELETE,
                KEY_LEFT,
                KEY_LEFTSHIFT,
                KEY_LEFTCTRL,
                KEY_V_CODE,
            ]
        );
    }

    #[test]
    fn retry_backoff_gates_failed_opens() {
        let now = Instant::now();
        assert!(retry_due(None, now));
        assert!(retry_due(Some(now), now));
        assert!(!retry_due(Some(now + RETRY), now));
    }

    #[test]
    fn device_opens_with_correct_permissions() {
        // Needs /dev/uinput readable+writable (udev rule 70-uinput.rules).
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
