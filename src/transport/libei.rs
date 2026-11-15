//! libei backend: key injection through mutter's RemoteDesktop EIS socket.
//!
//! Acquisition runs the mutter-direct round-trip the Task 6 spike measured
//! end to end on this machine (DEVLOG 15/11): on ONE session-bus connection —
//! `CreateSession()` -> introspect until the Session interface is exported ->
//! `Start()` -> `ConnectToEIS()` -> EI socket fd -> event-driven device
//! discovery. The backend holds both the D-Bus connection (the mutter session
//! dies when it drops) and the EI context for its lifetime, so dropping the
//! backend tears the whole session down. The libei symbols load at runtime
//! (see `ei_sys`): a machine without `libei.so.1` denies the kind before any
//! D-Bus traffic, through the same `None` path as every other check here.
//!
//! NOT unit-tested here: the key-emission ops against a live mutter (the
//! libei send calls return void and need a desktop to observe; the spike
//! verified delivery — 20 paced Backspaces reached the focused window in
//! 206 ms). The tests cover the constructor's deny/succeed shapes and the
//! raw-evdev key table. Per-op failures (mutter disconnect/removal) surface
//! as `false` through the event pump.

use std::collections::HashMap;
use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use dbus::arg::{OwnedFd, RefArg, Variant};
use dbus::blocking::{BlockingSender, SyncConnection};
use dbus::strings::{BusName, Interface, Member, Path};
use dbus::Message;

use super::ei_sys;
use super::{BackendOps, Kind};

// Key table: RAW evdev codes (linux/input-event-codes.h). libei adds the EI
// wire offset (+8) itself, unlike the XTest/wlr_vk tables, which carry the
// +8. Identical whitelist: no op can reach a keycode beyond these.
const KEY_BACKSPACE: u32 = 14;
const KEY_DELETE: u32 = 111;
const KEY_LEFT: u32 = 105;
const KEY_SHIFT: u32 = 42;
const KEY_V: u32 = 47;
const KEY_CTRL: u32 = 29;

// The whole CreateSession -> fd -> resumed-device round-trip is bounded by
// this budget; mutter answers each step in single-digit ms when healthy.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
// Retry pacing while the Session interface has not appeared yet (it is
// exported ~1.5 ms after CreateSession; the probe used the same interval).
const INTROSPECT_RETRY: Duration = Duration::from_millis(300);
// Discovery poll slice: short, so the deadline is honored closely.
const DISCOVERY_POLL_MS: i32 = 100;

const MUTTER_NAME: &str = "org.gnome.Mutter.RemoteDesktop";
const MUTTER_PATH: &str = "/org/gnome/Mutter/RemoteDesktop";
const MUTTER_IFACE: &str = "org.gnome.Mutter.RemoteDesktop";
const SESSION_IFACE: &str = "org.gnome.Mutter.RemoteDesktop.Session";

pub struct LibeiBackend {
    // Held for its lifetime only — no calls go through it after acquisition,
    // but dropping it ends the mutter session. SyncConnection because the
    // backend must be Send for `BackendOps`.
    #[allow(dead_code)]
    conn: SyncConnection,
    // The runtime-resolved libei symbols; `&'static` because the
    // process-wide table in `ei_sys` outlives the backend.
    sys: &'static ei_sys::Table,
    // EI context and the ref'd keyboard device. AtomicPtr rather than raw
    // pointers so the backend is Send for `BackendOps`; libei itself is
    // driven only from the server's single key-path thread. The device
    // pointer doubles as the alive flag, cleared on removal/disconnect.
    ei: AtomicPtr<ei_sys::Ei>,
    keyboard: AtomicPtr<ei_sys::EiDevice>,
}

/// Build for the constructor table; `None` when the session bus, mutter
/// RemoteDesktop, the EIS socket or libei device discovery fails within the
/// 5 s budget, so the kind is denied instead of failing per keystroke.
pub fn construct() -> Option<Box<dyn BackendOps>> {
    // The runtime dlopen first: a machine without libei denies the kind
    // before any D-Bus traffic, like every other environment check here.
    let sys = ei_sys::table()?;
    Some(Box::new(LibeiBackend::acquire(sys)?))
}

/// A method-call message; every name is a constant at the call sites, so a
/// construction failure here is unreachable in practice.
fn dbus_message(dest: &str, path: &str, iface: &str, member: &str) -> Option<Message> {
    Some(Message::method_call(
        &BusName::new(dest).ok()?,
        &Path::new(path).ok()?,
        &Interface::new(iface).ok()?,
        &Member::new(member).ok()?,
    ))
}

/// The constructor's per-call D-Bus timeout: the remaining acquisition
/// budget, floored at 1 ms so a spent deadline fails fast on the next call
/// instead of hanging on a long default timeout.
fn remaining(deadline: Instant) -> Duration {
    let ms = deadline
        .saturating_duration_since(Instant::now())
        .as_millis() as u64;
    Duration::from_millis(ms.max(1))
}

/// Whether the EI socket has data within the slice; only paces the discovery
/// loop (`ei_dispatch` itself never blocks).
fn poll_readable(fd: i32, timeout_ms: i32) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: poll reads/writes only pfd, which outlives the call.
    let ready = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    ready > 0 && (pfd.revents & libc::POLLIN) != 0
}

impl LibeiBackend {
    /// The mutter-direct round-trip from the Task 6 spike. Failure at any
    /// step returns None; the kind is denied by the caller.
    fn acquire(sys: &'static ei_sys::Table) -> Option<Self> {
        let deadline = Instant::now() + ACQUIRE_TIMEOUT;
        let conn = SyncConnection::new_session().ok()?;

        // CreateSession() takes no arguments and returns the session path.
        let reply = conn
            .send_with_reply_and_block(
                dbus_message(MUTTER_NAME, MUTTER_PATH, MUTTER_IFACE, "CreateSession")?,
                remaining(deadline),
            )
            .ok()?;
        let session: Path<'static> = reply.read1().ok()?;

        // The Session interface is exported a beat after CreateSession
        // (~1.5 ms measured): introspect until it shows.
        loop {
            let reply = conn
                .send_with_reply_and_block(
                    dbus_message(
                        MUTTER_NAME,
                        &session,
                        "org.freedesktop.DBus.Introspectable",
                        "Introspect",
                    )?,
                    remaining(deadline),
                )
                .ok()?;
            let xml: String = reply.read1().ok()?;
            if xml.contains(SESSION_IFACE) {
                break;
            }
            if Instant::now() >= deadline {
                return None;
            }
            sleep(INTROSPECT_RETRY);
        }

        conn.send_with_reply_and_block(
            dbus_message(MUTTER_NAME, &session, SESSION_IFACE, "Start")?,
            remaining(deadline),
        )
        .ok()?;

        // ConnectToEIS(a{sv}) with an empty options dict; the reply carries
        // the EI socket fd as 'h'.
        let options = HashMap::<String, Variant<Box<dyn RefArg>>>::new();
        let reply = conn
            .send_with_reply_and_block(
                dbus_message(MUTTER_NAME, &session, SESSION_IFACE, "ConnectToEIS")?
                    .append1(options),
                remaining(deadline),
            )
            .ok()?;
        // SAFETY: ei_setup_backend_fd takes ownership of the fd and closes it
        // at teardown, so into_fd() forgets the dbus wrapper instead of
        // closing it (a close here would race libei's own).
        let fd = reply.read1::<OwnedFd>().ok()?.into_fd();

        // SAFETY: ei_new returns a fresh sender context or NULL, checked
        // before any further use.
        let ei = unsafe { (sys.ei_new)(ptr::null_mut()) };
        if ei.is_null() {
            // into_fd() detached the fd from the dbus wrapper above and it
            // never reached libei, so close it here or every denial leaks
            // one. SAFETY: the fd is owned only by this function at this
            // point (libei never took it).
            unsafe { libc::close(fd) };
            return None;
        }
        // SAFETY: ei is valid; the name must be configured before backend
        // setup (header contract).
        unsafe { (sys.ei_configure_name)(ei, c"ibus-engine-mangrs".as_ptr()) };
        // SAFETY: ei is valid; the fd ownership moves to libei per the header.
        if unsafe { (sys.ei_setup_backend_fd)(ei, fd) } < 0 {
            // libei took the fd even on failure (header: ownership transfers
            // at the call), so releasing the context is the whole cleanup.
            // SAFETY: ei was never wired to a backend; no other refs exist.
            unsafe { (sys.ei_unref)(ei) };
            return None;
        }
        // SAFETY: ei is valid; the fd is only polled, never read or closed.
        let ei_fd = unsafe { (sys.ei_get_fd)(ei) };

        let backend = Self {
            conn,
            sys,
            ei: AtomicPtr::new(ei),
            keyboard: AtomicPtr::new(ptr::null_mut()),
        };
        if backend.discover(ei_fd, deadline) {
            Some(backend)
        } else {
            // Drop runs here: releases the device ref and the context, and
            // the connection drop ends the mutter session.
            None
        }
    }

    /// Event-driven device discovery until the keyboard device is resumed;
    /// the events may already be queued, so pump before waiting. False on
    /// timeout (the kind is denied, but FocusIn's `re_request` re-attempts
    /// it — since Task 8 every re-selection retries every kind. Libei must
    /// not re-enter the preference list while acquisition can block: this
    /// path burns up to the full budget synchronously on startup/FocusIn).
    fn discover(&self, ei_fd: i32, deadline: Instant) -> bool {
        loop {
            // A device resumed and removed inside one pump must not count as
            // success: the end state is a live claimed device.
            if self.pump() && !self.keyboard.load(Ordering::Relaxed).is_null() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            poll_readable(ei_fd, DISCOVERY_POLL_MS);
        }
    }

    /// Dispatch once and drain every queued event. Seat-added binds the
    /// keyboard capability, device-added claims the first keyboard-capable
    /// device, removal/disconnect releases it so the ops fail instead of
    /// sending into a dead socket. Returns whether the claimed device is
    /// resumed (the discovery end state).
    fn pump(&self) -> bool {
        let mut resumed = false;
        let ei = self.ei.load(Ordering::Relaxed);
        // SAFETY: ei is a live context; dispatch drains what the socket has
        // already delivered and never blocks.
        unsafe { (self.sys.ei_dispatch)(ei) };
        loop {
            // SAFETY: ei is live; the returned event is unref'd before the
            // next get_event (header: one event ref held at a time).
            let ev = unsafe { (self.sys.ei_get_event)(ei) };
            if ev.is_null() {
                break;
            }
            // SAFETY: ev is valid until its unref below.
            let ev_type = unsafe { (self.sys.ei_event_get_type)(ev) };
            match ev_type {
                ei_sys::EI_EVENT_SEAT_ADDED => {
                    // SAFETY: the seat is borrowed from ev.
                    let seat = unsafe { (self.sys.ei_event_get_seat)(ev) };
                    // SAFETY: the bind entry is the real
                    // ei_seat_bind_capabilities called through the fixed
                    // arity documented on the Table field, terminated by the
                    // NULL sentinel.
                    unsafe {
                        (self.sys.ei_seat_bind_keyboard)(
                            seat,
                            ei_sys::EI_DEVICE_CAP_KEYBOARD,
                            ptr::null_mut::<c_void>(),
                        )
                    };
                }
                ei_sys::EI_EVENT_DEVICE_ADDED => {
                    // SAFETY: the device is borrowed from ev.
                    let dev = unsafe { (self.sys.ei_event_get_device)(ev) };
                    // Claim only when our slot is free: a device swap must
                    // not double-ref the same object.
                    if self.keyboard.load(Ordering::Relaxed).is_null()
                        // SAFETY: dev is valid until the event's unref below.
                        && unsafe {
                            (self.sys.ei_device_has_capability)(
                                dev,
                                ei_sys::EI_DEVICE_CAP_KEYBOARD,
                            )
                        }
                    {
                        // SAFETY: dev is valid; our own ref keeps it alive
                        // past this event's unref.
                        let claimed = unsafe { (self.sys.ei_device_ref)(dev) };
                        self.keyboard.store(claimed, Ordering::Relaxed);
                    }
                }
                ei_sys::EI_EVENT_DEVICE_RESUMED => {
                    // SAFETY: the device is borrowed from ev.
                    let dev = unsafe { (self.sys.ei_event_get_device)(ev) };
                    if dev == self.keyboard.load(Ordering::Relaxed) {
                        resumed = true;
                    }
                }
                ei_sys::EI_EVENT_DEVICE_REMOVED => {
                    // SAFETY: the device is borrowed from ev.
                    let dev = unsafe { (self.sys.ei_event_get_device)(ev) };
                    if dev == self.keyboard.load(Ordering::Relaxed) {
                        self.release_device();
                    }
                }
                ei_sys::EI_EVENT_DISCONNECT => {
                    // libei emulates DEVICE_REMOVED before DISCONNECT, so
                    // the ref is usually already released; this covers a
                    // bare disconnect.
                    self.release_device();
                }
                _ => {}
            }
            // SAFETY: event ownership returns to libei (header contract).
            unsafe { (self.sys.ei_event_unref)(ev) };
        }
        resumed
    }

    /// Drop the backend's device ref and mark the device gone.
    fn release_device(&self) {
        let keyboard = self.keyboard.swap(ptr::null_mut(), Ordering::Relaxed);
        // SAFETY: keyboard is our own ref from ei_device_ref at claim time;
        // the header directs clients to unref it after DEVICE_REMOVED or
        // disconnect.
        if !keyboard.is_null() {
            unsafe { (self.sys.ei_device_unref)(keyboard) };
        }
    }

    /// Send one key event as its own frame. `ei_device_keyboard_key` queues
    /// a half-event; the frame after each event is what mutter acts on
    /// (spike-verified pacing). Every op's whitelist is the key-table
    /// constants above, not the callers.
    fn key(&self, keycode: u32, press: bool) {
        let ei = self.ei.load(Ordering::Relaxed);
        let keyboard = self.keyboard.load(Ordering::Relaxed);
        // SAFETY: keyboard is the claimed, resumed device (non-null checked
        // by op_begin); ei is its live context providing the monotonic frame
        // timestamp. No call unwinds.
        unsafe {
            (self.sys.ei_device_keyboard_key)(keyboard, keycode, press);
            (self.sys.ei_device_frame)(keyboard, (self.sys.ei_now)(ei));
        }
    }

    /// Start an op batch: pump stale events, refuse a dead socket, and open
    /// the emulation transaction the header requires around sent events.
    fn op_begin(&self) -> bool {
        self.pump();
        let keyboard = self.keyboard.load(Ordering::Relaxed);
        if keyboard.is_null() {
            return false;
        }
        // SAFETY: keyboard is a claimed, resumed device; sending outside an
        // emulation transaction is a client bug per the header.
        unsafe { (self.sys.ei_device_start_emulating)(keyboard, 1) };
        true
    }

    /// Close the emulation transaction opened by `op_begin`.
    fn op_end(&self) {
        let keyboard = self.keyboard.load(Ordering::Relaxed);
        // SAFETY: matches the start_emulating in op_begin; the pointer is
        // cleared to null before any release, so a live value is valid.
        if !keyboard.is_null() {
            unsafe { (self.sys.ei_device_stop_emulating)(keyboard) };
        }
    }
}

impl BackendOps for LibeiBackend {
    fn kind(&self) -> Kind {
        Kind::Libei
    }

    fn available(&self) -> bool {
        // Cleared by the ops' pump on removal/disconnect.
        !self.keyboard.load(Ordering::Relaxed).is_null()
    }

    fn backspaces(&self, n: u32, gap_ms: u64) -> bool {
        if !self.op_begin() {
            return false;
        }
        for _ in 0..n {
            self.key(KEY_BACKSPACE, true);
            self.key(KEY_BACKSPACE, false);
            if gap_ms > 0 {
                sleep(Duration::from_millis(gap_ms));
            }
        }
        self.op_end();
        true
    }

    fn select_left(&self, n: u32, gap_ms: u64) -> bool {
        if !self.op_begin() {
            return false;
        }
        self.key(KEY_SHIFT, true);
        for _ in 0..n {
            self.key(KEY_LEFT, true);
            self.key(KEY_LEFT, false);
            if gap_ms > 0 {
                sleep(Duration::from_millis(gap_ms));
            }
        }
        self.key(KEY_SHIFT, false);
        self.op_end();
        true
    }

    fn delete(&self, n: u32) -> bool {
        if !self.op_begin() {
            return false;
        }
        for _ in 0..n {
            self.key(KEY_DELETE, true);
            self.key(KEY_DELETE, false);
        }
        self.op_end();
        true
    }

    fn paste(&self) -> bool {
        if !self.op_begin() {
            return false;
        }
        self.key(KEY_CTRL, true);
        self.key(KEY_V, true);
        self.key(KEY_V, false);
        self.key(KEY_CTRL, false);
        self.op_end();
        true
    }
}

impl Drop for LibeiBackend {
    fn drop(&mut self) {
        let keyboard = self.keyboard.load(Ordering::Relaxed);
        let ei = self.ei.load(Ordering::Relaxed);
        // SAFETY: the device pointer is a live ref (cleared to null before
        // any release, so a non-null value is valid) and the context owns no
        // external refs; ei_unref runs last and closes the EI socket fd that
        // libei owns — never closed from Rust. No unwinding crosses FFI.
        unsafe {
            if !keyboard.is_null() {
                (self.sys.ei_device_unref)(keyboard);
            }
            (self.sys.ei_unref)(ei);
        }
        // conn drops after this body: the mutter session dies with it.
    }
}

/// Whether mutter RemoteDesktop is owned on the session bus; the success
/// test skips cleanly on machines without it (the deny test covers that
/// shape), while this GNOME machine always reaches the assert.
#[cfg(test)]
fn mutter_reachable() -> bool {
    let Ok(conn) = SyncConnection::new_session() else {
        return false;
    };
    let Some(msg) = dbus_message(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
    ) else {
        return false;
    };
    conn.send_with_reply_and_block(msg.append1(MUTTER_NAME.to_string()), Duration::from_secs(1))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use crate::transport::{constructors, Kind};

    /// Marker telling the re-spawned copy of this test binary to run the
    /// deny probe below instead of skipping it.
    const DENY_CHILD_ENV: &str = "MANGRS_EI_DENY_CHILD_PROBE";
    /// A unix socket nothing listens on: libdbus fails the connect inside
    /// the 5 s acquisition budget instead of hanging.
    const DEAD_BUS_ADDRESS: &str = "unix:path=/tmp/ibus-mangrs-test-no-bus";

    #[test]
    fn constructor_denies_without_mutter() {
        // The probe must run in a FRESH process: libdbus resolves the
        // well-known bus address once per process and caches the result for
        // its whole lifetime (measured: after any prior new_session, a
        // second one succeeds even with a nonexistent-socket address), so an
        // in-process env fixture would depend on test scheduling. The
        // engine's constructor always runs in its own fresh process, which
        // is exactly the shape the probe re-creates. No test in this process
        // mutates the environment, so sibling tests cannot be poisoned by a
        // failed restore.
        let exe = std::env::current_exe().expect("test binary path");
        let status = std::process::Command::new(exe)
            .args([
                "--quiet",
                "--exact",
                "transport::libei::tests::deny_child_probe",
            ])
            .env(DENY_CHILD_ENV, "1")
            .env("DBUS_SESSION_BUS_ADDRESS", DEAD_BUS_ADDRESS)
            .status()
            .expect("spawn deny probe child");
        assert!(status.success(), "deny probe child failed: {status}");
    }

    #[test]
    fn deny_child_probe() {
        // Skipped in the normal suite (no marker): the parent above runs it
        // as a separate process against the dead bus address. Run directly
        // it passes vacuously, and on a libei-less machine it exercises the
        // dlopen denial rather than the dbus denial — the parent's process
        // re-spawn is what makes the dbus-cache rationale hold.
        if std::env::var_os(DENY_CHILD_ENV).is_none() {
            return;
        }
        assert!(
            constructors(Kind::Libei).is_none(),
            "constructor connected despite an unreachable session bus address"
        );
    }

    #[test]
    fn constructor_succeeds_on_this_machine() {
        // This test acquires a REAL mutter session and the test process's
        // exit drops it — one vanish event per cargo test run, and mutter 51
        // segfaults on EI client vanish (15/11: test runs alone put vanish
        // events on the live shell). Opt in with MANGRS_REQUIRE_LIBEI=1 on
        // a machine where a session churn is acceptable; machines without
        // mutter skip either way, and the deny probe covers the None shape.
        if std::env::var_os("MANGRS_REQUIRE_LIBEI").is_none() || !super::mutter_reachable() {
            return;
        }
        let backend = constructors(Kind::Libei)
            .expect("mutter RemoteDesktop reachable but the constructor denied Libei");
        assert_eq!(backend.kind(), Kind::Libei);
        assert!(backend.available());
    }

    #[test]
    fn keycodes_are_raw_evdev() {
        // The C API takes raw evdev codes; libei adds the EI wire offset (+8)
        // internally — unlike the XTest/wlr_vk tables, which carry the +8.
        assert_eq!(super::KEY_BACKSPACE, 14);
        assert_eq!(super::KEY_DELETE, 111);
        assert_eq!(super::KEY_LEFT, 105);
        assert_eq!(super::KEY_SHIFT, 42);
        assert_eq!(super::KEY_V, 47);
        assert_eq!(super::KEY_CTRL, 29);
    }
}
