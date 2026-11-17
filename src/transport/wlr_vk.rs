//! wlroots-style `virtual-keyboard` backend: key injection through the
//! compositor itself.
//!
//! The backend binds `zwp_virtual_keyboard_manager_v1` on the default
//! `WAYLAND_DISPLAY` socket and creates a `zwp_virtual_keyboard_v1` on the
//! first seat; the compositor then routes the injected keys to the focused
//! surface like any other keyboard input. That compositor-side routing is why
//! this backend is exempt from the mutter-relay ban that keeps the Uinput
//! backend off Wayland sessions: the ban exists because relayed clients never
//! see kernel-uinput events as such, while events from this protocol enter
//! the compositor's own input pipeline as if they came from a real keyboard.
//!
//! The key-event ops are a straight mapping over `zwp_virtual_keyboard_v1`
//! requests and are not unit-tested here (no compositor in CI) — they are
//! exercised only by the live checklist on a real Wayland session. The
//! connection, keyboard and keymap are created once per backend instance and
//! dropped with it.

use std::fs::File;
use std::io::{Seek, Write};
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::thread::sleep;
use std::time::{Duration, Instant};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{
    wl_keyboard::{KeyState, KeymapFormat},
    wl_registry::WlRegistry,
    wl_seat::WlSeat,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use super::{BackendOps, Kind};

// X keycodes = Linux evdev + 8: BACKSPACE 14->22, DELETE 111->119.
// Identical to XTest's whitelist: no op can reach a keycode beyond these.
const KEYCODE_BACKSPACE: u32 = 22;
const KEYCODE_DELETE: u32 = 119;

// Keymap for the whitelisted keys. The standard xkeyboard-config includes
// are resolved by the compositor's own xkbcommon, so Backspace and Delete
// arrive as ordinary key events in every client.
const KEYMAP: &str = r#"xkb_keymap {
    xkb_keycodes { include "evdev" };
    xkb_types { include "complete" };
    xkb_compat { include "complete" };
    xkb_symbols { include "pc+us" };
}"#;

pub struct WlrVkBackend {
    conn: Connection,
    keyboard: ZwpVirtualKeyboardV1,
    // Event-time base: one monotonic clock per keyboard, per the protocol's
    // "all requests must share the same clock" rule.
    start: Instant,
}

/// Build for the constructor table; `None` when no compositor answers, the
/// virtual-keyboard global or a seat is missing, or the keymap setup fails,
/// so the kind is denied instead of failing per keystroke.
pub fn construct() -> Option<Box<dyn BackendOps>> {
    Some(Box::new(WlrVkBackend::connect()?))
}

/// The dispatch target for the connection: no event from these interfaces
/// carries anything key injection needs, so every handler is a no-op. The
/// queue exists to satisfy the typed bind/create calls and to drive the
/// construction roundtrip.
struct VkState;

impl Dispatch<WlRegistry, GlobalListContents> for VkState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for VkState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for VkState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpVirtualKeyboardManagerV1,
        _event: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for VkState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpVirtualKeyboardV1,
        _event: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl WlrVkBackend {
    /// Connect, bind the virtual-keyboard global on a seat and install the
    /// keymap.
    fn connect() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = registry_queue_init::<VkState>(&conn).ok()?;
        let qh = queue.handle();
        // Both globals bind at version 1, the protocol's only version.
        let seat: WlSeat = globals.bind(&qh, 1..=1, ()).ok()?;
        let manager: ZwpVirtualKeyboardManagerV1 = globals.bind(&qh, 1..=1, ()).ok()?;
        let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
        // Send failures fall back to an inert proxy, and an untrusted-client
        // refusal travels as a protocol error — both show up here.
        if !keyboard.is_alive() {
            return None;
        }
        let (fd, size) = keymap_fd()?;
        keyboard.keymap(u32::from(KeymapFormat::XkbV1), fd.as_fd(), size);
        // Flushes the keymap request and dispatches the seat capabilities
        // queued at bind time; a compositor-side rejection surfaces here.
        queue.roundtrip(&mut VkState).ok()?;
        Some(Self {
            conn,
            keyboard,
            start: Instant::now(),
        })
    }

    /// Inject one key event and wait for the compositor to process it.
    /// Every op goes through here, so the whitelist is the constants above,
    /// not the callers.
    fn key(&self, keycode: u32, pressed: bool) -> bool {
        let state = if pressed {
            KeyState::Pressed
        } else {
            KeyState::Released
        };
        self.keyboard.key(self.now_ms(), keycode, u32::from(state));
        // The request only sits in the client's buffer until now: the
        // roundtrip flushes it and blocks until the compositor has answered,
        // so paced events cannot arrive as one burst.
        self.conn.roundtrip().is_ok()
    }

    fn now_ms(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }
}

/// The keymap over a memfd, as the protocol wants a file descriptor it can
/// mmap. On failure the fd closes with the `File` and the kind is denied.
fn keymap_fd() -> Option<(OwnedFd, u32)> {
    // SAFETY: memfd_create returns -1 or a fresh fd we then own; nothing else
    // holds it.
    let raw = unsafe { libc::memfd_create(c"ibus-mangrs-keymap".as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        return None;
    }
    // SAFETY: raw is the fresh fd from memfd_create, owned only by us.
    let mut file = unsafe { File::from_raw_fd(raw) };
    file.write_all(KEYMAP.as_bytes()).ok()?;
    // The compositor maps at offset 0; the rewind is for any reader that
    // uses read() instead of mmap.
    file.rewind().ok()?;
    Some((file.into(), KEYMAP.len() as u32))
}

impl BackendOps for WlrVkBackend {
    fn kind(&self) -> Kind {
        Kind::WlrVirtualKeyboard
    }

    fn available(&self) -> bool {
        // The keyboard object dies with the connection (or on a compositor
        // reset), so this is a cheap connection-alive probe; per-op failures
        // surface on the ops themselves either way.
        self.keyboard.is_alive()
    }

    fn backspaces(&self, n: u32, gap_ms: u64) -> bool {
        for _ in 0..n {
            if !self.key(KEYCODE_BACKSPACE, true) || !self.key(KEYCODE_BACKSPACE, false) {
                return false;
            }
            if gap_ms > 0 {
                sleep(Duration::from_millis(gap_ms));
            }
        }
        true
    }

    fn delete(&self, n: u32) -> bool {
        for _ in 0..n {
            if !self.key(KEYCODE_DELETE, true) || !self.key(KEYCODE_DELETE, false) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::transport::{constructors, Kind};

    #[test]
    fn constructor_returns_none_without_wayland_socket() {
        // Both env vars unset: connect_to_env finds no socket, so the kind is
        // denied with None instead of panicking. Calling in a normal test fn
        // is the no-panic assertion. The env is process-global and the
        // preference tests mutate WAYLAND_DISPLAY under the parallel runner,
        // so hold the shared test-env lock — a parallel restore re-setting
        // the socket would otherwise reach the live compositor.
        let _lock = crate::transport::test_env::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env =
            crate::transport::test_env::EnvGuard::save(&["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"]);
        _env.unset("WAYLAND_DISPLAY");
        _env.unset("XDG_RUNTIME_DIR");
        let backend = constructors(Kind::WlrVirtualKeyboard);
        assert!(backend.is_none());
    }
}
