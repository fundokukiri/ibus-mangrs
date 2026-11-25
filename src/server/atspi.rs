//! Focused-app names through AT-SPI — the last naming source, for Wayland
//! sessions where the focus extension is absent and no X11 window answers.
//!
//! Nothing of the accessibility tree is read: the a11y registry's window
//! activation event carries the sender's bus name, and the process behind it
//! is the application. fcitx5-lotus and ArecaIME name apps the same cheap way,
//! and it works for every toolkit that publishes accessibility at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use dbus::ffidisp::{BusType, Connection, ConnectionItem};
use dbus::strings::{BusName, Interface, Member, Path};
use dbus::Message;

use crate::debug_log;

const A11Y_BUS: &str = "org.a11y.Bus";
const A11Y_BUS_PATH: &str = "/org/a11y/bus";
const REGISTRY: &str = "org.a11y.atspi.Registry";
const REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
const WINDOW_IFACE: &str = "org.a11y.atspi.Event.Window";
const WINDOW_MATCH: &str = "type='signal',interface='org.a11y.atspi.Event.Window'";
const DBUS_IFACE: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const ACTIVATE: &str = "window:activate";
const DEACTIVATE: &str = "window:deactivate";
/// Safety net only: the name normally ends when the app reports that its
/// window lost focus (`window:deactivate`). Expiring it while the user is
/// still typing in that window closed the app-rule picker for no reason.
const FRESH: Duration = Duration::from_secs(300);
const CALL_TIMEOUT_MS: i32 = 200;
const POLL_MS: i32 = 200;
const RETRY: Duration = Duration::from_secs(2);
/// Window owners that are not the app the user types into.
const SHELLS: [&str; 7] = [
    "gnome-shell",
    "mutter",
    "kwin_wayland",
    "kwin_x11",
    "plasmashell",
    "Xwayland",
    "xdg-desktop-por",
];

/// A background watcher: window activations in, one program name out.
pub(super) struct Tracker {
    /// Name, the bus that reported it, and when: the sender is how a
    /// deactivation is matched back to the activation that named it.
    latest: Arc<Mutex<Option<(String, String, Instant)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Tracker {
    pub(super) fn start() -> Self {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let latest = Arc::clone(&latest);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("atspi-watch".into())
                .spawn(move || watch(&latest, &stop))
                .ok()
        };
        Self {
            latest,
            stop,
            thread,
        }
    }

    /// The app named by the last window activation, while it is still current.
    pub(super) fn focused_app(&self) -> Option<String> {
        let guard = self.latest.lock().ok()?;
        let (name, _, at) = guard.as_ref()?;
        (at.elapsed() < FRESH).then(|| name.clone())
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn watch(latest: &Mutex<Option<(String, String, Instant)>>, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        listen(latest, stop);
        if stop.load(Ordering::Relaxed) {
            return;
        }
        // No accessibility bus (or it went away): back off, the session may
        // turn it on later without an engine restart.
        thread::sleep(RETRY);
    }
}

fn listen(latest: &Mutex<Option<(String, String, Instant)>>, stop: &AtomicBool) {
    let Some((events, lookup)) = connect() else {
        // Once per attempt, never per key: a session without accessibility
        // retries every RETRY seconds for its whole life.
        debug_log::log("atspi: no bus or no registry");
        return;
    };
    for item in events.iter(POLL_MS) {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let ConnectionItem::Signal(msg) = item else {
            continue;
        };
        let iface = msg.interface();
        let member = msg.member();
        if iface.as_deref() != Some(WINDOW_IFACE) {
            continue;
        }
        let Some(sender) = msg.sender() else {
            continue;
        };
        match member.as_deref() {
            Some("Activate") => {}
            Some("Deactivate") => {
                // Only the app that named the entry may end it: a window of
                // another app deactivating says nothing about ours.
                if let Ok(mut slot) = latest.lock() {
                    if slot.as_ref().is_some_and(|(_, who, _)| *who == *sender) {
                        *slot = None;
                    }
                }
                continue;
            }
            _ => continue,
        }
        let Some(pid) = sender_pid(&lookup, &sender) else {
            debug_log::log("atspi activate: no pid");
            continue;
        };
        if pid == std::process::id() {
            // Our own settings window is not an app to write rules for.
            continue;
        }
        let Some(name) = comm_of(pid) else {
            debug_log::log(&format!("atspi activate: pid={pid} has no app name"));
            continue;
        };
        debug_log::log(&format!("atspi activate: pid={pid} name={name}"));
        if let Ok(mut slot) = latest.lock() {
            *slot = Some((name, sender.to_string(), Instant::now()));
        }
    }
}

/// Open the accessibility bus, ask for activation events, return the event
/// connection plus a second one for calls (a blocking call inside the event
/// loop would dispatch on the same connection).
fn connect() -> Option<(Connection, Connection)> {
    let session = Connection::get_private(BusType::Session).ok()?;
    // Test-only unwraps: this file's own constants are valid names.
    let msg = Message::method_call(
        &BusName::new(A11Y_BUS).unwrap(),
        &Path::new(A11Y_BUS_PATH).unwrap(),
        &Interface::new(A11Y_BUS).unwrap(),
        &Member::new("GetAddress").unwrap(),
    );
    let address = session
        .send_with_reply_and_block(msg, CALL_TIMEOUT_MS)
        .ok()?
        .read1::<String>()
        .ok()?;
    // A private connection is unregistered until Hello goes out; until then
    // every AddMatch/RegisterEvent is refused with AccessDenied.
    let events = Connection::open_private(&address)
        .ok()
        .and_then(|conn| conn.register().ok().map(|_| conn))?;
    let lookup = Connection::open_private(&address)
        .ok()
        .and_then(|conn| conn.register().ok().map(|_| conn))?;
    events.add_match(WINDOW_MATCH).ok()?;
    // Without this the registry emits nothing to us, whatever we match on.
    let msg = Message::method_call(
        &BusName::new(REGISTRY).unwrap(),
        &Path::new(REGISTRY_PATH).unwrap(),
        &Interface::new(REGISTRY).unwrap(),
        &Member::new("RegisterEvent").unwrap(),
    )
    .append1(ACTIVATE);
    let deactivate = Message::method_call(
        &BusName::new(REGISTRY).unwrap(),
        &Path::new(REGISTRY_PATH).unwrap(),
        &Interface::new(REGISTRY).unwrap(),
        &Member::new("RegisterEvent").unwrap(),
    )
    .append1(DEACTIVATE);
    let registered = events
        .send_with_reply_and_block(msg, CALL_TIMEOUT_MS)
        .is_ok()
        && events
            .send_with_reply_and_block(deactivate, CALL_TIMEOUT_MS)
            .is_ok();
    debug_log::log(&format!("atspi: bus ok, register={registered}"));
    registered.then_some((events, lookup))
}

fn sender_pid(conn: &Connection, sender: &BusName) -> Option<u32> {
    let msg = Message::method_call(
        &BusName::new(DBUS_IFACE).unwrap(),
        &Path::new(DBUS_PATH).unwrap(),
        &Interface::new(DBUS_IFACE).unwrap(),
        &Member::new("GetConnectionUnixProcessID").unwrap(),
    )
    .append1(&**sender);
    conn.send_with_reply_and_block(msg, CALL_TIMEOUT_MS)
        .ok()?
        .read1::<u32>()
        .ok()
}

fn is_shell(name: &str) -> bool {
    SHELLS.contains(&name)
}

/// The program behind `pid` — what App Rules match.
///
/// `/proc/<pid>/comm` truncates at 15 bytes (`gnome-text-edit`), so the
/// executable's own name is preferred and comm is only the fallback.
fn comm_of(pid: u32) -> Option<String> {
    if pid <= 1 {
        return None;
    }
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    let name = exe
        .as_deref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| {
            std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .map(|raw| raw.trim().to_string())
        })?;
    (!name.is_empty() && !is_shell(&name)).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comm_names_the_process_that_owns_it() {
        let name = comm_of(std::process::id()).expect("name of the test process");
        assert!(!name.is_empty() && !name.contains('\n') && !name.contains('/'));
        assert_eq!(comm_of(0), None);
    }

    fn tracker_naming(name: &str, at: Instant) -> Tracker {
        Tracker {
            latest: Arc::new(Mutex::new(Some((name.to_string(), ":1.7".to_string(), at)))),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    #[test]
    fn a_name_survives_long_typing_and_expires_only_as_a_safety_net() {
        // Minutes in the same window: the picker asks long after the
        // activation, and must still get the app it is open in.
        let late = tracker_naming("firefox", Instant::now());
        assert_eq!(late.focused_app().as_deref(), Some("firefox"));
        let stale = Instant::now()
            .checked_sub(FRESH + Duration::from_secs(1))
            .expect("freshness window is inside the process lifetime");
        assert_eq!(tracker_naming("firefox", stale).focused_app(), None);
    }

    #[test]
    fn shell_processes_are_not_apps() {
        assert!(is_shell("gnome-shell"));
        assert!(is_shell("kwin_wayland"));
        assert!(!is_shell("firefox"));
    }
}
