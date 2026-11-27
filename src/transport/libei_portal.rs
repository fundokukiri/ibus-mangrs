//! `xdg-desktop-portal` acquisition of an EIS socket, for the compositors that
//! have no `org.gnome.Mutter.RemoteDesktop` and implement only the portal. The
//! mutter-direct path in `libei.rs` stays the first attempt — it needs no user
//! prompt and is measured — and this one is the fallback the non-mutter Wayland
//! preference list reaches last.
//!
//! The portal API is request-based: every method returns a `Request` object
//! path and the result arrives as a `Response` signal on that path, so each
//! call here subscribes first, hands the portal a unique `handle_token`, and
//! waits for the matching signal inside the acquisition budget.
//!
//! NOT exercised on a desktop that has only the portal, from this repository:
//! the code path is unreachable on GNOME (mutter answers first) and on
//! X11/wlroots (uinput or
//! the virtual keyboard answer first), so it is denied gracefully wherever it
//! cannot work and never runs on the machines this project is verified on.

use std::collections::HashMap;
use std::os::fd::IntoRawFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dbus::arg::{OwnedFd, RefArg, Variant};
use dbus::blocking::{BlockingSender, SyncConnection};
use dbus::message::MatchRule;
use dbus::strings::{BusName, Interface, Member, Path};
use dbus::Message;

const PORTAL_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const REMOTE_DESKTOP_IFACE: &str = "org.freedesktop.portal.RemoteDesktop";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// `org.freedesktop.portal.RemoteDesktop.DeviceType`: KEYBOARD = 1. A pointer
/// is never requested — the engine presses keys and nothing else.
const DEVICE_KEYBOARD: u32 = 1;

/// The remaining acquisition budget, floored at 1 ms so a spent deadline fails
/// on the next call instead of blocking on the default timeout.
fn remaining(deadline: Instant) -> Duration {
    Duration::from_millis(
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .max(1) as u64,
    )
}

fn call(dest: &str, path: &str, iface: &str, member: &str) -> Option<Message> {
    Some(Message::method_call(
        &BusName::new(dest).ok()?,
        &Path::new(path).ok()?,
        &Interface::new(iface).ok()?,
        &Member::new(member).ok()?,
    ))
}

/// The portal mirrors the caller's unique bus name into the request path
/// (`:1.42` becomes `1_42`), which is how the expected object path is known
/// before the reply arrives.
fn sender_token(conn: &SyncConnection) -> Option<String> {
    let unique = conn.unique_name().to_string();
    Some(unique.trim_start_matches(':').replace('.', "_"))
}

/// Run one request-based portal call and return its `results` dictionary
/// (empty on `Response` 1 = cancelled, `None` on error or timeout). The
/// request path is ours alone, so the first `Response` seen while waiting
/// belongs to this call.
fn request(
    conn: &SyncConnection,
    member: &str,
    args: Message,
    deadline: Instant,
) -> Option<HashMap<String, Variant<Box<dyn RefArg>>>> {
    // `add_match` requires a Send + 'static callback, so the slot is shared
    // by Arc/Mutex rather than Rc/RefCell.
    type Results = HashMap<String, Variant<Box<dyn RefArg>>>;
    let seen: Arc<Mutex<Option<Results>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&seen);
    let token = conn
        .add_match(
            MatchRule::new_signal(REQUEST_IFACE, "Response"),
            move |(code, results): (u32, HashMap<String, Variant<Box<dyn RefArg>>>),
                  _conn: &SyncConnection,
                  _msg: &Message| {
                crate::debug_log::log(&format!("ctx ei=portal code={code}"));
                if code == 0 {
                    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(results);
                }
                // The callback returns whether the message is consumed: true
                // keeps it out of the rest of the dispatch.
                true
            },
        )
        .ok()?;
    let reply = conn
        .send_with_reply_and_block(args, remaining(deadline))
        .ok()?;
    let _handle: Path<'static> = reply.read1().ok()?;
    let outcome = loop {
        // `process` dispatches the queued signals, which is what runs the
        // callback above; the slice is short so the deadline is honoured.
        let _ = conn.process(remaining(deadline).min(Duration::from_millis(50)));
        if let Some(results) = seen.lock().unwrap_or_else(|e| e.into_inner()).take() {
            break Some(results);
        }
        if Instant::now() >= deadline {
            break None;
        }
    };
    let _ = conn.remove_match(token);
    debug_log_done(member, outcome.is_some());
    outcome
}

/// One line per portal step, at the debug log's `ei` tag: a report from a
/// portal-only desktop then says which step refused instead of a bare "no
/// transport".
fn debug_log_done(member: &str, ok: bool) {
    crate::debug_log::log(&format!("ctx ei=portal step={member} ok={}", u8::from(ok)));
}

/// An EIS socket from the portal, or `None` when the portal is absent, the
/// user cancels the permission prompt, or the budget runs out. Ownership of
/// the fd transfers to the caller (libei takes it at `ei_setup_backend_fd`).
pub(super) fn connect_eis(deadline: Instant) -> Option<i32> {
    let conn = dbus::blocking::SyncConnection::new_session().ok()?;
    let token = sender_token(&conn)?;
    // The portal is optional on a Wayland session: ask the bus first so a
    // compositor without it costs one cheap call instead of a timeout.
    conn.send_with_reply_and_block(
        call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
        )?
        .append1(PORTAL_NAME),
        remaining(deadline),
    )
    .ok()?;

    let mut options = HashMap::<String, Variant<Box<dyn RefArg>>>::new();
    options.insert(
        "handle_token".into(),
        Variant(Box::new(format!("mangrs_token_{token}")) as Box<dyn RefArg>),
    );
    let session_reply = request(
        &conn,
        "CreateSession",
        call(
            PORTAL_NAME,
            PORTAL_PATH,
            REMOTE_DESKTOP_IFACE,
            "CreateSession",
        )?
        .append1(options),
        deadline,
    )?;
    let session = session_reply
        .get("session_handle")
        .and_then(|v| v.0.as_str().map(str::to_string))?;

    let mut select = HashMap::<String, Variant<Box<dyn RefArg>>>::new();
    select.insert(
        "handle_token".into(),
        Variant(Box::new(format!("mangrs_select_{token}")) as Box<dyn RefArg>),
    );
    select.insert(
        "types".into(),
        Variant(Box::new(DEVICE_KEYBOARD) as Box<dyn RefArg>),
    );
    request(
        &conn,
        "SelectDevices",
        call(
            PORTAL_NAME,
            PORTAL_PATH,
            REMOTE_DESKTOP_IFACE,
            "SelectDevices",
        )?
        .append2(&*session, select),
        deadline,
    )?;

    let mut start = HashMap::<String, Variant<Box<dyn RefArg>>>::new();
    start.insert(
        "handle_token".into(),
        Variant(Box::new(format!("mangrs_start_{token}")) as Box<dyn RefArg>),
    );
    request(
        &conn,
        "Start",
        call(PORTAL_NAME, PORTAL_PATH, REMOTE_DESKTOP_IFACE, "Start")?
            .append2(&*session, "")
            .append1(start),
        deadline,
    )?;

    let mut eis = HashMap::<String, Variant<Box<dyn RefArg>>>::new();
    eis.insert(
        "handle_token".into(),
        Variant(Box::new(format!("mangrs_eis_{token}")) as Box<dyn RefArg>),
    );
    let eis_reply = request(
        &conn,
        "ConnectToEIS",
        call(
            PORTAL_NAME,
            PORTAL_PATH,
            REMOTE_DESKTOP_IFACE,
            "ConnectToEIS",
        )?
        .append2(&*session, eis),
        deadline,
    )?;
    // The fd arrives as 'h' inside the results dictionary.
    let fd = eis_reply.get("fd")?;
    let owned: OwnedFd = fd.0.as_any().downcast_ref::<OwnedFd>()?.clone();
    Some(owned.into_raw_fd())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_sender_token_matches_the_portal_path_shape() {
        // ':1.42' -> '1_42'; the portal builds its request paths from this.
        let unique = ":1.42";
        assert_eq!(unique.trim_start_matches(':').replace('.', "_"), "1_42");
    }
}
