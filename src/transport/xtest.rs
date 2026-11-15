//! X11 XTest backend: fake key events through the X server's XTEST extension.
//!
//! The key-event ops are a straight mapping over `xtest_fake_input` and are
//! not unit-tested here (no X server in CI) — they are exercised only by the
//! live checklist on a real X11 session. The connection is created once per
//! backend instance and dropped with it.

use std::thread::sleep;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::Window;
use x11rb::protocol::xtest::ConnectionExt;
use x11rb::rust_connection::RustConnection;

use super::{BackendOps, Kind};

// X keycodes = Linux evdev + 8: BACKSPACE 14->22, DELETE 111->119.
const KEYCODE_BACKSPACE: u8 = 22;
const KEYCODE_DELETE: u8 = 119;

// X event types for `fake_input` (the core KeyPress/KeyRelease numbers).
const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

pub struct XtestBackend {
    conn: RustConnection,
    root: Window,
}

/// Build for the constructor table; `None` when `DISPLAY` is unusable or
/// the XTEST extension is missing, so the kind is denied instead of failing
/// per keystroke.
pub fn construct() -> Option<Box<dyn BackendOps>> {
    Some(Box::new(XtestBackend::connect()?))
}

impl XtestBackend {
    /// Connect to `DISPLAY` and verify the XTEST extension.
    fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let reply = conn.xtest_get_version(2, 2).ok()?.reply().ok()?;
        // XTEST below 1.0 has no fake input to lean on.
        if reply.major_version < 1 {
            return None;
        }
        let root = conn.setup().roots.get(screen)?.root;
        Some(Self { conn, root })
    }

    /// Fake one key event. Every op goes through here, so the whitelist is
    /// the constants above, not the callers.
    fn fake_key(&self, type_: u8, keycode: u8) -> bool {
        // deviceid is ignored for core key events: the server picks the
        // XTest keyboard from the client's pairing.
        let sent = self
            .conn
            .xtest_fake_input(type_, keycode, 0, self.root, 0, 0, 0);
        // Flush now or the requests leave in one burst when the buffer
        // fills, defeating the pacing sleeps between events.
        sent.and_then(|_| self.conn.flush()).is_ok()
    }
}

impl BackendOps for XtestBackend {
    fn kind(&self) -> Kind {
        Kind::XTest
    }

    fn available(&self) -> bool {
        // Construction succeeded only after the extension query; a server
        // that dies later shows up as the ops returning false instead.
        true
    }

    fn backspaces(&self, n: u32, gap_ms: u64) -> bool {
        for _ in 0..n {
            if !self.fake_key(KEY_PRESS, KEYCODE_BACKSPACE)
                || !self.fake_key(KEY_RELEASE, KEYCODE_BACKSPACE)
            {
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
            if !self.fake_key(KEY_PRESS, KEYCODE_DELETE)
                || !self.fake_key(KEY_RELEASE, KEYCODE_DELETE)
            {
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
    fn xtest_constructor_requires_display() {
        // Both degraded shapes must yield None instead of panicking — a
        // Wayland-only machine or a headless CI just denies the kind. Calling
        // in a normal test fn is the no-panic assertion. The env is
        // process-global and the preference tests mutate the same variables
        // under the parallel runner, so hold the shared test-env lock (a
        // parallel restore once re-set DISPLAY between the unset and the
        // probe and the constructor reached the live display).
        let _lock = crate::transport::test_env::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _env = crate::transport::test_env::EnvGuard::save(&["DISPLAY"]);
        std::env::remove_var("DISPLAY");
        let unset = constructors(Kind::XTest);
        // Loopback TCP: no default X server listens there, so this is the
        // "set but unreachable" shape.
        std::env::set_var("DISPLAY", "localhost:0");
        let unreachable = constructors(Kind::XTest);
        assert!(unset.is_none());
        assert!(unreachable.is_none());
    }
}
