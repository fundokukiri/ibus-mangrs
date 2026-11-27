//! D-Bus server: owns the connection and the engine, dispatches incoming
//! calls and emits signals back to IBus.
//!
//! # How IBus talks to an engine
//!
//! IBus does not use the session bus. `ibus-daemon` starts a private bus and
//! publishes its address in `IBUS_ADDRESS` (or the registry file under
//! `~/.config/ibus/bus/`). An engine must:
//!
//! 1. Connect to that address.
//! 2. Own the bus name `org.freedesktop.IBus.<name>` from the component XML.
//! 3. Export `/org/freedesktop/IBus/Factory` with `CreateEngine(s) -> o`.
//!    ibus-daemon calls this when the user switches to the engine; the
//!    returned object path is where the engine object lives.
//! 4. Answer `org.freedesktop.IBus.Engine` calls on that path and emit the
//!    signals below.
//!
//! Introspection is disabled on these objects, so IBus matches calls by
//! interface + member name only.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use dbus::ffidisp::{BusType, Connection, NameFlag};
use dbus::strings::{BusName, Interface, Member, Path as DbusPath};
use dbus::{Message, MessageType};

use crate::config::{CharsetConfig, Config, MethodConfig, ModeConfig, PreferredBackendConfig};
use crate::debug_log::{self, brief};
use crate::engine::{Action, Engine, NavKeyWhilePreedit};
use crate::ibus_prop::{self, Prop};
use crate::ibus_text::{plain_text, text_from_variant, text_variant, Attr};
use crate::keymap;
use crate::transport::{constructors, preference_order, Injection, Kind};

mod hotkey;
mod prop;
mod x11;

#[cfg(test)]
mod tests;

// The GUI hotkey recorder validates specs through crate::server::parse_hotkey;
mod emoji_picker;
mod picker;
#[cfg(feature = "gui")]
pub(crate) use hotkey::focus_extension_answers;
#[cfg(feature = "gui")]
pub(crate) use picker::focus_extension_path;

// the lib build resolves the name inside hotkey itself, hence the allow.
use self::emoji_picker::EmojiPickerState;
#[allow(unused_imports)]
pub(crate) use self::hotkey::parse_hotkey;
use self::hotkey::{hotkey_registry, HotkeyAction, HotkeyBinding};
use self::picker::PickerState;

use self::prop::*;
use self::x11::*;

pub const BUS_NAME: &str = "org.freedesktop.IBus.mangrs";
pub const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
pub const ENGINE_PATH: &str = "/org/freedesktop/IBus/Engine/1";
pub const ENGINE_IFACE: &str = "org.freedesktop.IBus.Engine";
pub const FACTORY_IFACE: &str = "org.freedesktop.IBus.Factory";

/// The MăngRS focus extension (`data/extension`): a session-bus
/// service naming the focused window. Wayland-native apps reach IBus only as
/// a mutter relay, and GNOME offers no other name source for them (Shell
/// Introspect is denied; the wlroots protocol ibus-bamboo uses is absent).
/// The settings window asks the same service for its focused-app chip.
/// How long the focus path waits for the shell extension. Short enough that a
/// missing extension costs one focus change nothing; the picker path, which is
/// user-initiated, waits longer (`picker::PICKER_EXT_TIMEOUT_MS`).
pub(crate) const FOCUS_EXT_TIMEOUT_MS: i32 = 300;
pub(crate) const FOCUS_EXT_NAME: &str = "io.github.fundokukiri.Mangrs";
pub(crate) const FOCUS_EXT_PATH: &str = "/io/github/fundokukiri/Mangrs";
pub(crate) const FOCUS_EXT_IFACE: &str = "io.github.fundokukiri.Mangrs";

/// Shared engine state: the handler mutates it while answering calls, the
/// main thread reads it when the GUI changes settings.
pub type SharedEngine = Arc<Mutex<Engine>>;

/// Lock the engine, recovering a poisoned mutex instead of dying: a panic
/// while holding the lock must degrade one keystroke, not kill typing
/// desktop-wide until IBus restarts.
fn lock_engine(engine: &SharedEngine) -> std::sync::MutexGuard<'_, Engine> {
    engine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Server {
    conn: Connection,
    engine: SharedEngine,
    /// Shared with the message callback: the config hot-reload also runs on
    /// the run loop's own passes, so a GUI edit reaches the panel menu
    /// without waiting for a key event.
    handler: std::rc::Rc<std::cell::RefCell<EngineHandler>>,
}

impl Server {
    /// Connect to the IBus daemon's bus and claim the engine name.
    pub fn connect(engine: SharedEngine) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open_private(&ibus_address()?)?;
        // open_private skips the Hello handshake that get_private does for the
        // well-known buses; without it the bus rejects every call with
        // AccessDenied.
        conn.register()?;

        let reply = conn.register_name(BUS_NAME, NameFlag::ReplaceExisting.value())?;
        if reply != dbus::ffidisp::RequestNameReply::PrimaryOwner {
            return Err(format!("bus name {BUS_NAME} is already owned: {reply:?}").into());
        }

        // Calls are answered from the message filter, not from a MsgHandler or
        // an object path: libdbus answers any method call on an unregistered
        // path with UnknownMethod before the filter returns, so the reply must
        // be sent and the message claimed (return `true`) inside the filter.
        let handler = std::rc::Rc::new(std::cell::RefCell::new(EngineHandler {
            engine: Arc::clone(&engine),
            injection: Injection::new(),
            last_config_check: Instant::now(),
            last_config_mtime: None,
            hotkeys: Vec::new(),
            last_fire: Vec::new(),
            picker: None,
            emoji_picker: None,
            burst: BurstWatch::default(),
            burst_buffer: BurstBuffer::default(),
            bounce_deadline: None,
            stash: None,
            focus_ext: None,
            props: Vec::new(),
        }));
        handler.borrow_mut().last_config_mtime = Config::path()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        let initial = Config::load();
        handler.borrow_mut().refresh_hotkey_cache(&initial);
        handler
            .borrow_mut()
            .injection
            .set_preferred(preferred_kind(&initial));
        // Startup: pick the injection backend before the first key; the
        // engine's availability flag is fed from the holder's answer, not
        // from a static device probe.
        handler.borrow_mut().injection.select(&preference_order());
        let selected = handler.borrow().injection.current();
        if debug_log::enabled() {
            debug_log::log(&backend_selection_log(selected, preferred_kind(&initial)));
        }
        lock_engine(&engine).set_uinput_available(selected.is_some());
        // A compositor-routed backend delivers its keys through the
        // compositor itself, so the relay ban on injection does not apply.
        lock_engine(&engine).set_injection_relay_safe(matches!(
            selected,
            Some(Kind::Libei | Kind::WlrVirtualKeyboard)
        ));
        let handler_for_cb = std::rc::Rc::clone(&handler);
        conn.replace_message_callback(Some(Box::new(move |conn, msg| {
            if msg.msg_type() != MessageType::MethodCall {
                return false;
            }
            match handler_for_cb.borrow_mut().handle(conn, &msg) {
                Some(()) => true,
                None => false,
            }
        })));

        Ok(Self {
            conn,
            engine,
            handler,
        })
    }

    /// Run the main loop until the bus goes away.
    pub fn run(&self) {
        loop {
            // Wake at the next engine deadline (the preedit idle commit)
            // instead of sleeping a fixed second past it. lock_engine, not
            // .ok(): a poisoned mutex must degrade one keystroke, not stop
            // the loop's stale-echo flushes for the rest of the session.
            let timeout = {
                let e = lock_engine(&self.engine);
                let handler = self.handler.borrow();
                // The burst watchdog's deadline joins the engine's own
                // deadlines: a lost burst must wake the loop on its own
                // clock, not the 1s poll. The bounce deadline folds the same
                // way: an expired bounce window aborts promptly.
                let engine_ms = e.ms_until_tick().map_or(1000, |ms| ms.min(1000));
                let watch_ms = handler.burst.ms_until().map_or(1000, |ms| ms.min(1000));
                let bounce_ms = handler.bounce_deadline.map_or(1000, |d| {
                    d.saturating_duration_since(Instant::now())
                        .as_millis()
                        .max(1) as u64
                });
                (engine_ms.min(watch_ms).min(bounce_ms)) as i32
            };
            for item in self.conn.iter(timeout) {
                if matches!(item, dbus::ffidisp::ConnectionItem::Nothing) {
                    break;
                }
            }
            let mut out = Vec::new();
            {
                let mut e = lock_engine(&self.engine);
                let mut handler = self.handler.borrow_mut();
                // Split into disjoint field borrows: the burst state, the
                // bounce state and the key buffer are separate, and a RefMut
                // reborrow per field would trip the borrow checker.
                let EngineHandler {
                    injection,
                    burst,
                    burst_buffer,
                    bounce_deadline,
                    stash,
                    ..
                } = &mut *handler;
                // Bounce resolution first: an expired window aborts the
                // deferred reset before the watchdog or the tick can deliver
                // the parked commits (they belong to the stash now).
                if bounce_deadline.is_some_and(|d| Instant::now() >= d) {
                    *bounce_deadline = None;
                    abort_deferred_focus_out(&mut e, burst, burst_buffer);
                }
                // Inside the bounce window the reset is deferred: neither
                // the watchdog nor the tick may deliver anything and the
                // held keys do not replay — every parked commit belongs to
                // the bounce and is already copied into the stash, and a
                // burst that completes mid-window (its echo debt settles
                // with no parked commit left) must not send the held keys
                // to the wire while the focus is still in flight.
                if bounce_deadline.is_none() {
                    // The per-burst echo watchdog runs beside the tick: a
                    // burst whose echoes stopped arriving is declared lost on
                    // its deadline and its parked commits flush before the
                    // engine's own stale pass looks at the counters.
                    burst_watch_pass(&mut e, burst, &mut out);
                    let actions = e.tick();
                    let injected = emit(&mut e, injection, &actions, &mut out);
                    add_injected_batch(&mut e, burst, injected);
                    // Both burst completion routes converge on this pass: the
                    // watchdog expiry completes inside burst_watch_pass above,
                    // the echo-settle route when the tick delivers the last
                    // parked commit. One flush point replays the held keys.
                    replay_buffered_keys(&mut e, injection, burst, burst_buffer, &mut out);
                }
                // The stash expires without a reconciling report: drop, log
                // once (the drop is the gate).
                if let Some(s) = stash {
                    if Instant::now() >= s.deadline {
                        debug_log::log("st stash=drop why=no_report");
                        *stash = None;
                    }
                }
            }
            for msg in out {
                send(&self.conn, msg);
            }
            // Config hot-reload between messages, not only on key events:
            // a GUI edit (a feature toggle, a language pick) reaches the
            // panel menu within a second, without typing or a focus change.
            // The handler's own 1s gate keeps this to one stat call per pass.
            let mut out = Vec::new();
            self.handler.borrow_mut().maybe_reload_config(&mut out);
            for msg in out {
                send(&self.conn, msg);
            }
        }
    }
}

/// Locate the IBus private bus.
///
/// `IBUS_ADDRESS` is set in the environment of processes ibus-daemon launches;
/// the registry file is the fallback for engines started by hand.
fn ibus_address() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(addr) = std::env::var("IBUS_ADDRESS") {
        if !addr.is_empty() {
            return Ok(addr);
        }
    }

    let dir = dirs::config_dir()
        .ok_or("no config dir")?
        .join("ibus")
        .join("bus");
    address_from_dir(&dir).ok_or_else(|| {
        "IBUS_ADDRESS not set and no registry file found; is ibus-daemon running?".into()
    })
}

/// The address from the newest registry file. A machine with several (display
/// change, X11 vs Wayland, a stale file from a previous session) must reach
/// the running daemon, which rewrites its file on every start; `read_dir`
/// order is arbitrary, so mtime is the only signal available.
fn address_from_dir(dir: &std::path::Path) -> Option<String> {
    let mut newest: Option<(SystemTime, String)> = None;
    for entry in std::fs::read_dir(dir).ok()? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(addr) = text.lines().find_map(|l| l.strip_prefix("IBUS_ADDRESS=")) else {
            continue;
        };
        let at = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if newest.as_ref().is_none_or(|(t, _)| at > *t) {
            newest = Some((at, addr.trim().to_string()));
        }
    }
    newest.map(|(_, addr)| addr)
}

/// Unpack the purpose out of a ContentType value — GVariant "(uu)" boxed in
/// a variant. The dbus crate hands us either the demarshalled tuple or an
/// opaque structure; read the first element either way. A wrong-shaped
/// value yields None and the gate stays where it was.
fn content_type_purpose(value: &(dyn dbus::arg::RefArg + 'static)) -> Option<u32> {
    if let Some((purpose, _hints)) = value.as_any().downcast_ref::<(u32, u32)>() {
        return Some(*purpose);
    }
    let mut iter = value.as_iter()?;
    let purpose = iter.next()?.as_u64()?;
    u32::try_from(purpose).ok()
}

struct EngineHandler {
    engine: SharedEngine,
    /// The selected injection backend: `emit`'s transports route through it,
    /// and the engine's availability flag is fed from `current()`. Selected
    /// at startup; re-selected with denials cleared on config apply/focus-in;
    /// the uinput denial is additionally retried per keystroke (the
    /// cheap-probe self-heal).
    injection: Injection,
    /// Config file hot-reload: check the mtime at most once a second, on key
    /// events, so GUI/menu edits apply without a focus change or restart.
    last_config_check: Instant,
    last_config_mtime: Option<SystemTime>,
    /// The shortcut registry, rebuilt wherever the config is already
    /// loaded (startup, mtime reload, focus-in). Loading the config file on
    /// every key event just to compare a hotkey put disk I/O on the key path.
    hotkeys: Vec<HotkeyBinding>,
    /// Last fire per action: autorepeat of the combo key would re-trigger
    /// it tens of times a second while held.
    last_fire: Vec<(HotkeyAction, Instant)>,
    /// The per-app rule picker while it is open (an IBus lookup table at
    /// the caret; see picker.rs). Keys route here first, so nothing types
    /// while the menu is up.
    picker: Option<PickerState>,
    /// The emoji picker while it is open (an IBus lookup table at the
    /// caret; see emoji_picker.rs) — the same menu shape, digits pick,
    /// typing searches.
    emoji_picker: Option<EmojiPickerState>,
    /// Per-burst echo watchdog: the server's view of the rewrite burst in
    /// flight (expected echoes + their deadline). Armed beside every
    /// `add_injected`, re-armed per forwarded echo, expired on the tick
    /// path — see `BurstWatch`.
    burst: BurstWatch,
    /// Keystrokes typed while that burst was in flight, held for the
    /// ordered replay — see `BurstBuffer`.
    burst_buffer: BurstBuffer,
    /// Deadline while a FocusOut during a burst has deferred the reset: a
    /// FocusIn inside the window resumes the burst, expiry aborts it (see
    /// `defer_focus_out_during_burst`).
    bounce_deadline: Option<Instant>,
    /// The rewrite stashed by an aborted burst, reconciled against the next
    /// client's surrounding-text report — see `StashedRewrite`.
    stash: Option<StashedRewrite>,
    /// Session-bus connection to the focus extension, created lazily.
    focus_ext: Option<Connection>,
    /// The property list pushed to the panel at the last config reload: a
    /// reload sends only what differs, so a knob no menu shows sends nothing.
    props: Vec<ibus_prop::Prop>,
}

impl EngineHandler {
    /// Answer `msg` if it is ours. Returns `None` to let the message fall
    /// through to libdbus (and so to `iter()`).
    fn handle(&mut self, conn: &Connection, msg: &Message) -> Option<()> {
        let iface = &*msg.interface()?;
        let member = &*msg.member()?;

        match (iface, member) {
            (FACTORY_IFACE, "CreateEngine") => {
                // The engine name from the component XML is accepted but there
                // is only one engine implementation, so it is ignored.
                // The return type is `o` (object path), not `s`: ibus-daemon
                // fails to parse a string here and the engine never starts.
                send(
                    conn,
                    msg.method_return()
                        .append1(dbus::Path::new(ENGINE_PATH).unwrap()),
                );
            }
            (ENGINE_IFACE, m) => {
                let mut out: Vec<Message> = Vec::new();
                let reply = self.engine_call(msg, m, &mut out);
                for m in out {
                    send(conn, m);
                }
                send(conn, reply);
            }
            ("org.freedesktop.DBus.Introspectable", "Introspect") => {
                send(conn, msg.method_return().append1(INTROSPECT_XML));
            }
            ("org.freedesktop.DBus.Peer", "Ping") => send(conn, msg.method_return()),
            // ibus-daemon reads these IBusEngine properties before FocusIn.
            // `FocusId` and `ActiveSurroundingText` are booleans on the wire:
            // the daemon calls g_variant_get_boolean() on them and, on any
            // other type, warns and falls back to the plain `FocusIn` signal
            // (engineproxy.c `_get_has_focus_id_cb`). Answering `true` for
            // FocusId is what makes the daemon send `FocusInId(path, client)`,
            // which is the only way an engine learns which app has focus.
            ("org.freedesktop.DBus.Properties", m) => {
                match self.properties_call(msg, m) {
                    Some(reply) => send(conn, reply),
                    // An unreadable Properties message still answers — the
                    // daemon waits on every Properties call.
                    None => send(conn, msg.method_return()),
                }
            }
            _ => return None,
        }
        Some(())
    }

    /// `org.freedesktop.DBus.Properties` calls on the engine object.
    /// Returns the reply; `None` on an unreadable message (the caller still
    /// answers with an empty return — the daemon waits on every call).
    fn properties_call(&mut self, msg: &Message, member: &str) -> Option<Message> {
        match member {
            "Get" => {
                // `FocusId` and `ActiveSurroundingText` are booleans on the
                // wire: the daemon calls g_variant_get_boolean() on them
                // and, on any other type, warns and falls back to the plain
                // `FocusIn` signal (engineproxy.c `_get_has_focus_id_cb`).
                // Answering `true` for FocusId is what makes the daemon
                // send `FocusInId(path, client)`, which is the only way an
                // engine learns which app has focus.
                let reply = match msg.read2::<String, String>() {
                    Ok((_, prop)) => match prop.as_str() {
                        "FocusId" => msg
                            .method_return()
                            .append1(dbus::arg::Variant(Box::new(true))),
                        "ActiveSurroundingText" => msg
                            .method_return()
                            .append1(dbus::arg::Variant(Box::new(false))),
                        _ => msg
                            .method_return()
                            .append1(dbus::arg::Variant(Box::new(0u32))),
                    },
                    Err(_) => msg.method_return(),
                };
                Some(reply)
            }
            "GetAll" => {
                let props: dbus::arg::PropMap = Default::default();
                Some(msg.method_return().append1(props))
            }
            "Set" => {
                // ibus-daemon delivers the focused field's content type as
                // the engine's write-only ContentType property, (purpose,
                // hints) — the only channel a purpose reaches an engine
                // through (libibus's engine interface has no InputPurpose
                // member; the interface XML lives inside libibus itself).
                // PASSWORD/PIN close the strip and the learning files;
                // every other purpose — TERMINAL included — opens them.
                if let Ok((interface, prop, value)) =
                    msg.read3::<String, String, dbus::arg::Variant<Box<dyn dbus::arg::RefArg>>>()
                {
                    if prop == "ContentType" && interface == ENGINE_IFACE {
                        if let Some(purpose) = content_type_purpose(value.0.as_ref()) {
                            if let Ok(mut engine) = self.engine.lock() {
                                engine.set_content_purpose(purpose);
                            }
                        }
                    }
                }
                Some(msg.method_return())
            }
            _ => None,
        }
    }

    /// Handle one `org.freedesktop.IBus.Engine` call: push any signals into
    /// `out` and return the method reply. The engine mutex is taken lazily —
    /// `ProcessKeyEvent` re-checks the config first, which locks by itself.
    fn engine_call(&mut self, msg: &Message, member: &str, out: &mut Vec<Message>) -> Message {
        if member == "ProcessKeyEvent" {
            return self.process_key_call(msg, out);
        }
        if member == "CandidateClicked" {
            // A mouse click on a picker entry (GNOME's candidate popup
            // forwards it here): same as pressing the entry's digit. The
            // strip is checked first — it owns the table when its word is
            // in flight, and picker/emoji cannot coexist with it.
            if strip_open() {
                if let Ok((index, _button, _state)) = msg.read3::<u32, u32, u32>() {
                    let mut engine = lock_engine(&self.engine);
                    let actions = engine.accept_candidate(index as usize);
                    if !actions.is_empty() {
                        let injected = emit(&mut engine, &self.injection, &actions, out);
                        add_injected_batch(&mut engine, &mut self.burst, injected);
                    }
                }
                return msg.method_return();
            }
            if self.emoji_picker.is_some() {
                if let Ok((index, _button, _state)) = msg.read3::<u32, u32, u32>() {
                    self.apply_emoji_pick(index as usize, out);
                }
                return msg.method_return();
            }
            if self.picker.is_some() {
                if let Ok((index, _button, _state)) = msg.read3::<u32, u32, u32>() {
                    self.apply_picker_entry(index as usize, out);
                }
            }
            return msg.method_return();
        }
        if member == "PageUp" || member == "PageDown" {
            // The candidate popup's pager buttons; the suggestion strip
            // pages by them too when it is up, the emoji menu only when it
            // is open. Server-side state — no engine lock needed for the
            // strip.
            if strip_open() {
                strip_move_page(if member == "PageDown" { 1 } else { -1 });
                strip_emit_table(out);
            }
            if self.emoji_picker.is_some() {
                if let Some(picker) = self.emoji_picker.as_mut() {
                    if member == "PageDown" {
                        picker.page_down();
                    } else {
                        picker.page_up();
                    }
                }
                self.emit_emoji_table(out);
            }
            return msg.method_return();
        }

        // The extension round trip must not run under the engine mutex; the
        // naming step below decides whether the answer is actually used.
        let (ext_answered, ext_name) = if matches!(member, "FocusIn" | "FocusInId") {
            self.focus_extension_app(FOCUS_EXT_TIMEOUT_MS)
        } else {
            (false, None)
        };

        let mut engine = lock_engine(&self.engine);

        match member {
            "PropertyActivate" => {
                if let Ok((name, state)) = msg.read2::<String, u32>() {
                    activate_property(&mut engine, &self.injection, &name, state, out);
                } else {
                    eprintln!("ibus-mangrs: PropertyActivate read2 FAILED");
                }
                msg.method_return()
            }
            // `SetSurroundingText(v text, u cursor, u anchor)` — the client's
            // report of the text before the cursor, used by the surrounding
            // display mode to verify what was actually committed.
            "SetSurroundingText" => {
                if let Ok((text, cursor, anchor)) =
                    msg.read3::<dbus::arg::Variant<Box<dyn dbus::arg::RefArg>>, u32, u32>()
                {
                    if let Some(s) = text_from_variant(&text) {
                        // The report content is typed text — text tier only;
                        // `set_surrounding_with_anchor` logs cursor/len on the
                        // decision tier.
                        debug_log::log_text(&format!(
                            "st text cursor={cursor} tail={:?} text={:?}",
                            brief(engine.tail()),
                            brief(&s)
                        ));
                        // The text before the caret, taken before the report
                        // moves into the engine: the stashed rewrite
                        // reconciles against exactly this view.
                        let prefix_end = s
                            .char_indices()
                            .nth(cursor as usize)
                            .map_or(s.len(), |(byte, _)| byte);
                        let actual = s[..prefix_end].to_string();
                        // The report can resolve a deferred click-Reset
                        // (park mode): the resulting HidePreedit must go out.
                        let actions = engine.set_surrounding_with_anchor(s, cursor, anchor);
                        emit(&mut engine, &self.injection, &actions, out);
                        // The stashed rewrite reconciles against this report —
                        // only a caret-aligned one (a selection says nothing
                        // about the word, the same rule the engine's own
                        // repair applies) and only after the bounce window
                        // closed: while it is open the burst is still live
                        // and the stash is just the abort's fallback.
                        if anchor == cursor && self.bounce_deadline.is_none() {
                            reconcile_stashed_rewrite(
                                &mut engine,
                                &self.injection,
                                &mut self.burst,
                                &mut self.stash,
                                &actual,
                                out,
                            );
                        }
                    }
                }
                msg.method_return()
            }
            // `FocusInId(s path, s client)` replaces `FocusIn` once the
            // FocusId property is true. `client` looks like `gtk4-im:chrome`
            // and is the only way to know which app is typing.
            "FocusIn" | "Enable" | "FocusInId" => {
                // The incoming client id, read before set_client moves the
                // engine's view: the bounce resume compares it against the
                // context that left.
                let incoming_client = if member == "FocusInId" {
                    msg.read2::<String, String>().ok().map(|(_, client)| client)
                } else {
                    None
                };
                // Bounce resolution before any reset work: the same context
                // back inside the window resumes the deferred burst untouched;
                // anything else aborts it now (the stash survives for the
                // reconcile at this focus's own ST report).
                let resumed = resolve_focus_bounce_on_focus_in(
                    &mut engine,
                    &mut self.burst,
                    &mut self.burst_buffer,
                    &mut self.bounce_deadline,
                    &mut self.stash,
                    incoming_client.as_deref(),
                );
                if member == "FocusInId" {
                    if let Some(client) = incoming_client {
                        engine.set_client(Some(client));
                        note_client(msg);
                    }
                }
                // A new context may not support surrounding text at all; data
                // left over from the previous client would let the engine send
                // protocol deletes to it (mutter segfaults on those). Skipped
                // when a bounce resumed the same context: its buffer state
                // (the delete-dropped proof) survives the round trip.
                if !resumed {
                    engine.clear_buffer_state();
                }
                // Name the focused app beyond the IBus client string: the
                // focus extension covers Wayland-native apps (they arrive as
                // a mutter relay or unnamed), xprop covers X11 when the
                // extension is absent. The name feeds rule matching only —
                // transport decisions keep using `client`, so relayed
                // contexts keep their mutter-safe delete handling. Focus
                // path only, never on keys.
                // Naming is skipped only when the IBus client string already
                // carries the application (`gtk3-im:sublime_text`). Relayed
                // Wayland clients arrive as `gnome-shell` and every XIM client
                // as the hard-coded `xim`, which names no application at all —
                // without this no App Rule can target a Java/XIM app.
                engine.set_extension_answered(ext_answered);
                engine.set_shell_ui_focused(ext_answered && ext_name.is_none());
                if engine.relay_context() || engine.client_name_is_opaque() {
                    let resolved = ext_name
                        .map(|class| (class.clone(), format!("wayland:{class}")))
                        .or_else(|| {
                            x11_active_window_name()
                                .map(|class| (class.clone(), format!("x11:{class}")))
                        });
                    match resolved {
                        Some((class, named)) => {
                            engine.set_app_name(Some(named));
                            note_prgname(&class);
                        }
                        None => engine.set_app_name(None),
                    }
                } else {
                    engine.set_app_name(None);
                }
                // Pick up settings changed in the GUI since the last focus.
                let config = Config::load();
                debug_log::set_enabled(config.debug_log);
                debug_log::set_text_enabled(config.debug_log_text);
                debug_log::set_keys_enabled(config.debug_log_keys);
                // Direct field assignment: the engine guard held by this
                // branch borrows part of self, so the &mut self helper
                // cannot be called here.
                self.hotkeys = hotkey_registry(&config);
                self.last_fire.clear();
                if debug_log::enabled() {
                    debug_log::log(&format!(
                        "ctx focus=in client={:?} app={:?}",
                        engine.client(),
                        engine.app_name()
                    ));
                }
                // Re-select the injection backend next to the config apply:
                // a new focus may follow an environment change (device ACL,
                // session type), so denials clear and every kind is
                // re-attempted once — their constructor failures are fast
                // and this is a rare event (the per-keystroke self-heal
                // stays uinput-only). emit's inject gate reads the flag fed
                // from the holder's answer.
                self.injection.re_request();
                // The preferred backend rides the same re-select: a fresh
                // config value leads the next walk.
                self.injection.set_preferred(preferred_kind(&config));
                self.injection.select(&preference_order());
                if debug_log::enabled() {
                    debug_log::log(&backend_selection_log(
                        self.injection.current(),
                        preferred_kind(&config),
                    ));
                }
                engine.set_uinput_available(self.injection.current().is_some());
                // A compositor-routed backend delivers its keys through the
                // compositor itself, so the relay ban on injection does not
                // apply.
                engine.set_injection_relay_safe(matches!(
                    self.injection.current(),
                    Some(Kind::Libei | Kind::WlrVirtualKeyboard)
                ));
                let actions = engine.apply_config(&config);
                engine.set_enabled(config.enabled);
                emit(&mut engine, &self.injection, &actions, out);
                // The panel shows the menu; rebuild it from the saved config so
                // radio/toggle check marks survive engine restarts.
                register_properties(&config, out);
                // Ask ibus-daemon to have the client provide surrounding text.
                // Without this the GTK IM modules never send
                // SetSurroundingText, so the surrounding display mode would
                // run blind (this is how ibus-bamboo gets its data too).
                out.push(signal("RequireSurroundingText"));
                msg.method_return()
            }
            "FocusOut" | "Disable" | "FocusOutId" => {
                // The candidate popup belongs to the old input context: hide
                // it, or it lingers over the next window. (Direct field access:
                // the engine guard below holds a borrow of self.)
                let had_menu = self.picker.take().is_some();
                let had_menu = had_menu || self.emoji_picker.take().is_some();
                if had_menu {
                    out.push(signal("HideLookupTable"));
                    out.push(signal("HideAuxiliaryText"));
                }
                // The suggestion strip rides the same teardown — the table
                // belongs to the outgoing context, even when a burst defers
                // the reset below (then the engine's own close never fires).
                // The engine's HideCandidates from focus_out stays silent
                // behind the taken state instead of doubling the hide.
                strip_hide(out);
                // Focus-bounce tolerance: a burst in flight defers the whole
                // reset for the bounce window. A quick unfocus/refocus of the
                // same context resumes it untouched in FocusIn; a real switch
                // aborts on the loop pass with the rewrite stashed for
                // reconcile. The branch emits nothing: the parked commits
                // must not reach the wire while the focus is in flight.
                if member != "Disable"
                    && defer_focus_out_during_burst(
                        &mut engine,
                        &mut self.burst,
                        &mut self.bounce_deadline,
                        &mut self.stash,
                    )
                {
                    return msg.method_return();
                }
                // Commits still waiting for their Backspace echoes: echoes will
                // not follow a focus change, so deliver them now (late but not
                // never).
                for text in engine.flush_deferred() {
                    out.push(signal("CommitText").append1(plain_text(&text)));
                }
                // The preedit carries IBUS_ENGINE_PREEDIT_COMMIT, so the client
                // has already committed the pending word on the way out;
                // committing it again here would duplicate it — only the
                // internal composition is discarded. No HidePreedit either:
                // the client clears its own preedit display, and in clients
                // that handle FocusOut before committing, our HidePreedit would
                // arrive first and wipe the word (same as ibus-bamboo's no-op
                // FocusOut).
                let actions = engine.focus_out();
                emit(&mut engine, &self.injection, &actions, out);
                // Only `Disable` (the panel toggle) switches the wrapper off.
                // FocusOut must not: focus events get coalesced (a quick
                // unfocus/refocus of one context never re-sends them — see the
                // SetCapabilities note below), so a FocusOut whose FocusIn
                // never arrives would leave every later key PassThrough:
                // raw Latin typing until the engine is toggled or restarted.
                if member == "Disable" {
                    engine.set_enabled(false);
                    // A pending bounce dies with the Disable: the parks
                    // flush below and nothing is left to defer or reconcile.
                    self.bounce_deadline = None;
                    self.stash = None;
                }
                engine.set_client(None);
                // Drop the context's buffer data: the next focus may be a
                // context whose buffer mutter never saw. Caps are KEPT — the
                // daemon only re-sends SetCapabilities when they actually
                // change, and on a quick unfocus/refocus of the same context
                // (1ms apart in the logs) they are never re-sent; zeroing
                // them here made every later rewrite take the broken
                // injection path ("xin chaà" session).
                engine.clear_buffer_state();
                // Keys held for the burst belonged to the context going
                // away: replaying them after the next FocusIn would type
                // them into the wrong window. Disable keeps the context —
                // the held keys still replay there, raw once the engine
                // is off. Non-Disable reaches here only with NO burst live
                // (the bounce branch above returns first): a burst's keys
                // drop in `abort_deferred_focus_out` when its window
                // expires, and survive a bounced focus.
                if member != "Disable" {
                    self.burst_buffer.clear();
                }
                debug_log::log(if member == "Disable" {
                    "ctx focus=out cause=disable"
                } else {
                    "ctx focus=out cause=blur"
                });
                msg.method_return()
            }
            "Reset" => {
                // A client reset (click, button press) closes the popup —
                // this is what makes a click elsewhere dismiss the menu.
                // (Direct field access: the engine guard holds a borrow.)
                let had_menu = self.picker.take().is_some();
                let had_menu = had_menu || self.emoji_picker.take().is_some();
                if had_menu {
                    out.push(signal("HideLookupTable"));
                    out.push(signal("HideAuxiliaryText"));
                }
                // The suggestion strip rides the same teardown; the engine's
                // own HideCandidates from on_client_reset stays silent behind
                // the taken state instead of doubling the hide.
                strip_hide(out);
                // The preedit is sent with IBUS_ENGINE_PREEDIT_COMMIT, so the
                // client has already committed the word by the time this
                // arrives (GTK sends it on a mouse click, a focus change and a
                // button press).
                debug_log::log("ctx reset cause=client");
                // A reset is the click signal: nothing else tells us the app
                // may have changed, so the cached delete verdicts go too.
                engine.forget_delete_verdicts();
                for text in engine.flush_deferred() {
                    out.push(signal("CommitText").append1(plain_text(&text)));
                }
                // A click repositioned the caret (and the client already
                // committed the word): the held keys belonged to the
                // pre-click word — replaying them would type at the new
                // caret.
                self.burst_buffer.clear();
                let actions = engine.on_client_reset();
                let injected = emit(&mut engine, &self.injection, &actions, out);
                add_injected_batch(&mut engine, &mut self.burst, injected);
                msg.method_return()
            }
            "SetCapabilities" => {
                if let Ok(caps) = msg.read1::<u32>() {
                    debug_log::log(&format!("ctx caps={caps:#b}"));
                    engine.set_capabilities(caps);
                }
                msg.method_return()
            }
            // Candidate placement queries are not used: non-preedit rewriting
            // goes through DeleteSurroundingText, which needs no knowledge of
            // the text itself.
            "SetCursorLocation" => msg.method_return(),
            _ => msg.method_return(),
        }
    }
}

/// Backspace scancode as it goes on the wire. Every ibus client adds 8 to get
/// its own keycode (`ibusimcontext.c`, Qt's input context and the X11/XIM
/// client all do `keycode += 8`), so this must be the evdev code, NOT the X
/// keycode: sending the X keycode 22 made clients use 30, i.e. evdev KEY_U,
/// and every forwarded Backspace typed a `u` ("tùng" came out "tunguuuùng").
const BACKSPACE_KEYCODE: u32 = 14;

/// Left-arrow scancode, same convention as `BACKSPACE_KEYCODE` (X keycode 113
/// minus 8). ibus-bamboo forwards the same value for its ForwardKeyEvent II.
const LEFT_KEYCODE: u32 = 105;

/// Evdev scancode of a navigation key, same convention — the experimental
/// nav-key replay re-emits the swallowed key with its original keyval and one
/// of these (kernel input-event-codes.h values). `None` keys fall back to the
/// swallow.
fn nav_evdev_keycode(keyval: u32) -> Option<u32> {
    match keyval {
        keymap::LEFT => Some(LEFT_KEYCODE),
        keymap::UP => Some(103),
        keymap::RIGHT => Some(106),
        keymap::DOWN => Some(108),
        keymap::PAGE_UP => Some(104),
        keymap::PAGE_DOWN => Some(109),
        keymap::HOME => Some(110),
        keymap::END => Some(115),
        _ => None,
    }
}

/// One forwarded key as press + release. The release carries no modifier (the
/// app has already acted on the press); ibus-bamboo's ForwardKeyEvent modes
/// send the same pair.
fn push_forward_keys(out: &mut Vec<Message>, keyval: u32, keycode: u32, state: u32) {
    out.push(
        signal("ForwardKeyEvent")
            .append1(keyval)
            .append1(keycode)
            .append1(state),
    );
    out.push(
        signal("ForwardKeyEvent")
            .append1(keyval)
            .append1(keycode)
            .append1(keymap::RELEASE_MASK),
    );
}

/// Push both engine backend flags from the holder's current state — the
/// self-heal and the paced Flow re-request share it, so every re-select
/// refreshes both (a stale relay-safe mark would let injection back onto
/// a relay the new backend does not serve).
fn refresh_backend_flags(engine: &mut Engine, injection: &Injection) {
    engine.set_uinput_available(injection.current().is_some());
    engine.set_injection_relay_safe(matches!(
        injection.current(),
        Some(Kind::Libei | Kind::WlrVirtualKeyboard)
    ));
    engine.set_transport_types_text(injection.supports_text());
}

/// The config's preferred injection backend, `None` for `auto`: the
/// experimental switch's value rides the existing selection walk.
fn preferred_kind(config: &Config) -> Option<Kind> {
    match config.preferred_backend {
        PreferredBackendConfig::Auto => None,
        PreferredBackendConfig::Uinput => Some(Kind::Uinput),
        PreferredBackendConfig::XTest => Some(Kind::XTest),
        PreferredBackendConfig::WlrVirtualKeyboard => Some(Kind::WlrVirtualKeyboard),
        PreferredBackendConfig::Libei => Some(Kind::Libei),
    }
}

/// The debug-log line naming the backend actually held after a re-select,
/// with the role: `preferred` when it is the config's choice, `fallback` when
/// that choice was denied and the matrix answered, `matrix` when no choice
/// was set. Reads only — never decides anything.
fn backend_selection_log(selected: Option<Kind>, preferred: Option<Kind>) -> String {
    let held = selected.map_or("none".to_string(), |k| k.label().to_string());
    // `denied` is the one selection a reader has to explain: the preferred
    // backend is installed but the environment refused it.
    let (wanted, why) = match preferred {
        None => ("none".to_string(), "matrix"),
        Some(p) if Some(p) == selected => (p.label().to_string(), "preferred"),
        Some(p) => (p.label().to_string(), "denied"),
    };
    format!("ctx backend={held} pref={wanted} why={why}")
}

/// One key event as `ProcessKeyEvent` carries it: keysym, evdev keycode,
/// modifier state. The burst buffer stores the same triple, so a replayed
/// key reconstructs the event the user actually made.
struct KeyEvent {
    keyval: u32,
    keycode: u32,
    state: u32,
}

/// Handle one key event, pushing resulting signals into `out`.
/// Returns whether the engine consumed the key.
fn process_key(
    engine: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    buffer: &mut BurstBuffer,
    ev: KeyEvent,
    out: &mut Vec<Message>,
) -> bool {
    let KeyEvent {
        keyval,
        keycode,
        state,
    } = ev;
    // Hottest log site (every key event, every mode): the format! would
    // allocate per keystroke even with the log off, so gate it first —
    // on the keys tier (default off), which log_keys then double-checks.
    if debug_log::keys_enabled() {
        debug_log::log_keys(&format!("key keyval={keyval:#x} state={state:#x}"));
    }
    // Key releases are never ours.
    if state & keymap::RELEASE_MASK != 0 {
        // The experimental Backspace correction may only inject once the key
        // is up — injected Backspaces sent while it is down never reach the
        // application. The release is the engine's only signal for that.
        if keyval == keymap::BACKSPACE {
            engine.note_backspace_released();
        }
        // alt+digit, the pick waits for the modifier it was asked with to go
        // up: the Backspaces it injects otherwise arrive at the application
        // as Alt+Backspace, which deletes nothing ("xin chao" plus an Alt+1
        // pick left "xin chaochào"). The key itself stays the app's.
        if keymap::is_alt_key(keyval) {
            if let Some(index) = engine.take_pending_pick() {
                strip_pick(engine, injection, burst, index, out);
            }
        }
        return false;
    }
    // The engine's flag is pulled mid-session when an injection fails on a
    // delete-swallowing client; the holder still owns the backend it built,
    // so its answer — not a static device probe — decides when to re-mark
    // it. Backends the holder could not build stay denied (see `select`)
    // and never reach this re-mark — except uinput: the old static probe
    // picked a /dev/uinput grant up mid-session (logind's ACL can land
    // minutes after engine spawn), and losing that self-heal is a
    // regression the holder must not reintroduce.
    if !engine.uinput_available() {
        if injection.current().is_some() {
            refresh_backend_flags(engine, injection);
        } else {
            // Only the uinput denial clears here: its constructor is a
            // cheap device probe, while the other backends' failures are
            // environment-fixed (no mutter, no compositor global, no
            // DISPLAY) and retrying them per keystroke would pay their
            // full acquisition cost for nothing.
            injection.retry_denied(Kind::Uinput);
            // select_with, not select: this retry stays uinput-only and
            // must not rebuild a promoted list per keystroke (every other
            // kind here is denied, so promotion could not change the
            // outcome anyway).
            if injection
                .select_with(&[Kind::Uinput], &constructors)
                .is_some()
            {
                debug_log::log("ctx backend=uinput why=late");
                refresh_backend_flags(engine, injection);
            }
        }
    }
    // An injected Backspace echo arrives with the modifiers of the key it
    // serves: an Alt+1 pick still holds Alt when its own deletes come back
    // (state=MASK_MOD1). The shortcut branch below would take it for an
    // application shortcut, reset the engine mid-rewrite and leave the echo
    // uncounted, so the pick's parked commit went out ahead of its deletes
    // ("xin chao" + Alt+1 left "xin chaoc"). Echoes are decided before any
    // branch that reads modifiers.
    if keyval == keymap::BACKSPACE && note_burst_echo(engine, burst) {
        return false;
    }
    // Ctrl/Alt/Super combinations are application shortcuts.
    if state & keymap::SHORTCUT_MASK != 0 {
        // The strip's `Ctrl+,`/`Ctrl+.` paging owns the combo while it is
        // up — a page must not commit the word the box is showing (the
        // reset below would). Any other shortcut, or a closed strip, stays
        // an application shortcut.
        if strip_open() && state & keymap::CONTROL_MASK != 0 {
            match keyval {
                keymap::COMMA => {
                    strip_move_page(-1);
                    strip_emit_table(out);
                    return true;
                }
                keymap::PERIOD => {
                    strip_move_page(1);
                    strip_emit_table(out);
                    return true;
                }
                _ => {}
            }
        }
        // Alt+1..5 picks a slot. The bare digits are content again: a digit
        // typed inside a word reaches the composition, which matters in the
        // presets that treat digits as tone keys and whenever a number
        // belongs to the word.
        if strip_open() && state & keymap::MOD1_MASK != 0 {
            if let Some(digit) = strip_digit(keyval) {
                let index = strip_page() * STRIP_PAGE_SIZE + digit - 1;
                if !engine.pick_needs_modifier_release() {
                    return strip_pick(engine, injection, burst, index, out);
                }
                // Erasing with key events: held for Alt's release (see the
                // release arm), where they no longer carry the modifier.
                engine.ask_pick(index);
                return true;
            }
        }
        // Held like any keystroke: replaying the shortcut after the keys
        // already held keeps the user's sequence — resetting the engine
        // now would orphan them past the caret move the shortcut makes.
        // Echoes never reach this branch (they carry no modifier mask).
        if buffer.holds_keystroke(engine, burst) {
            buffer.push(keyval, keycode, state);
            return true;
        }
        if engine.take_stale_injection(engine.echo_stale()) || !engine.pending_echoes() {
            for text in engine.flush_deferred() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
        }
        let actions = engine.reset();
        let injected = emit(engine, injection, &actions, out);
        add_injected_batch(engine, burst, injected);
        return false;
    }

    if keyval == keymap::BACKSPACE {
        // Echoes that never came back (injection lost) must not eat this real
        // Backspace.
        if engine.take_stale_injection(engine.echo_stale()) {
            for text in engine.flush_deferred() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
        }
        // Echoes are already decided above, before the modifier branches.
        // A real Backspace typed mid-burst: held like any keystroke. The
        // check sits after the echo test on purpose — an echo must never
        // hold, or the burst would never settle.
        if buffer.holds_keystroke(engine, burst) {
            buffer.push(keyval, keycode, state);
            return true;
        }
        return match engine.backspace() {
            Some(actions) => {
                let injected = emit(engine, injection, &actions, out);
                add_injected_batch(engine, burst, injected);
                !actions.contains(&Action::Forward)
            }
            None => false,
        };
    }

    // A real key (never an echo): while the burst is in flight it must not
    // process against the mid-rewrite screen — hold it for the replay. The
    // check is the hot path's only cost when nothing is armed.
    if buffer.holds_keystroke(engine, burst) {
        buffer.push(keyval, keycode, state);
        return true;
    }
    // Deliver deferred commits whose Backspace echoes are all in, or whose
    // echoes have gone stale (apps that receive injected keys without
    // routing them through ibus would otherwise eat every later Backspace
    // and never see the commit).
    if engine.take_stale_injection(engine.echo_stale()) || !engine.pending_echoes() {
        for text in engine.flush_deferred() {
            out.push(signal("CommitText").append1(plain_text(&text)));
        }
    }

    // The suggestion strip's keys: digits pick, `Ctrl+,`/`Ctrl+.` page —
    // the only keys an open strip owns (the popup's ◀▶ buttons page by
    // mouse). Sits before the editing-key dispatch so a pick or page never
    // reaches the composition; a closed strip changes nothing here, and
    // every other key — arrows, `,`/`.` without Ctrl — types or navigates
    // exactly as with the strip off.
    if strip_key(engine, injection, burst, keyval, out) {
        return true;
    }

    // Arrows, Tab, Escape, Delete... end the word and belong to the app.
    if keymap::is_editing_key(keyval) {
        if engine.take_stale_injection(engine.echo_stale()) || !engine.pending_echoes() {
            for text in engine.flush_deferred() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
        }
        // A preedit commit and the forwarded navigation key travel different
        // transports on relayed contexts; a burst of arrows moves the caret
        // before the commit lands and the word is inserted mid-text
        // ("em tên tùng" → "etùngm tên"). The default `forward` trades that
        // for an immediate caret move — the commit signals above are already
        // in `out`, so the forwarded key cannot overtake them; `swallow`
        // consumes the first press instead so the commit lands with the
        // caret still in place.
        let nav_pending = keymap::is_navigation_key(keyval)
            && engine.relay_context()
            && engine.preedit_commit_pending();
        let nav_behavior = engine.nav_key_while_preedit();
        let actions = engine.reset();
        let injected = emit(engine, injection, &actions, out);
        add_injected_batch(engine, burst, injected);
        if nav_pending {
            match nav_behavior {
                NavKeyWhilePreedit::Forward => {
                    // The commit signals above are already in `out`, so the
                    // reply-forwarded key cannot overtake them on the wire.
                    debug_log::log("ins nav=forward reply=key_and_commit");
                    return false;
                }
                // The relay drops modifier state on forwarded keys (the
                // Shift+Left selection defect), so a modified key must not be
                // replayed as a plain one; an unknown keyval has no evdev
                // keycode to replay with. Both fall back to the swallow.
                NavKeyWhilePreedit::Replay if state & !keymap::RELEASE_MASK == 0 => {
                    if let Some(keycode) = nav_evdev_keycode(keyval) {
                        debug_log::log(&format!(
                            "key replay=1 keyval={keyval:#x} keycode={keycode} after=commit"
                        ));
                        push_forward_keys(out, keyval, keycode, 0);
                        return true;
                    }
                }
                _ => {}
            }
            debug_log::log("key nav=consumed why=commit_first");
            return true;
        }
        return false;
    }

    let Some(c) = keymap::keysym_to_char(keyval) else {
        return false;
    };
    let actions = engine.process_char(c);
    let consumed = !actions.contains(&Action::Forward);
    let injected = emit(engine, injection, &actions, out);
    add_injected_batch(engine, burst, injected);
    // Char path only, after the actions are emitted: a backend arriving
    // must not disturb the word in flight (its lock serves the rest of it;
    // the next word resolves through the walk) — a downgraded Flow word
    // discovered via Backspace re-requests on the next char key.
    flow_fallback_re_request(engine, injection);
    consumed
}

/// The Flow fallback's paced backend re-request: the mode walk downgrades
/// Flow for lack of an injection backend and raises `flow_needs_backend`;
/// each later word then retries the constructor table — paced by
/// `re_request_paced` (a mutter/Libei acquisition costs seconds, the
/// uinput probe is cheap) — and the engine flags follow the holder's
/// answer.
fn flow_fallback_re_request(engine: &mut Engine, injection: &mut Injection) {
    if engine.flow_needs_backend() && injection.re_request_paced() {
        injection.select(&preference_order());
        refresh_backend_flags(engine, injection);
        if debug_log::enabled() {
            debug_log::log(&backend_selection_log(
                injection.current(),
                injection.preferred(),
            ));
        }
    }
}

/// Built-in grace of the per-burst echo watchdog: after the burst arms
/// (or its latest echo), the remaining echoes get this long before the
/// burst is declared lost. Derived from the engine's built-in lost-echo
/// window so the watchdog and the stale flush cannot drift; a custom
/// window (`echo_stale_enabled`) replaces it with the user's value.
pub const ECHO_WATCHDOG_GRACE_MS: u64 = Engine::DEFAULT_ECHO_STALE_MS;

/// The watchdog window: how long the remaining echoes of an injected
/// burst survive past the burst arming or its latest echo. The next echo
/// of a batch paced `backspace_interval_ms` apart should arrive within
/// one gap, so the window is the gap plus a grace; a custom lost-echo
/// window (`echo_stale_enabled`) replaces the grace with the user's
/// value — the same fold `Engine::echo_stale` applies to the stale
/// flushes.
fn burst_watch_window(engine: &Engine) -> Duration {
    let grace = if engine.echo_stale_custom() {
        engine.echo_stale()
    } else {
        Duration::from_millis(ECHO_WATCHDOG_GRACE_MS)
    };
    Duration::from_millis(engine.backspace_interval_ms()) + grace
}

/// Per-burst echo watchdog: the server's view of the rewrite burst in
/// flight — how many echoes it still expects and when they are declared
/// lost. The engine counts the same echoes (`pending_injected`); the
/// deadline is the server's addition, re-armed by every arriving echo,
/// so an app that echoes slowly keeps its burst alive while one that
/// stops echoing gets its burst aborted on the tick path (the engine's
/// stale window measures from the batch arming, not from the last echo).
#[derive(Default)]
struct BurstWatch {
    /// Echoes still expected from the armed burst(s).
    expected: u32,
    /// When the remaining echoes are declared lost; `None` = no burst
    /// armed.
    deadline: Option<Instant>,
}

impl BurstWatch {
    /// Arm/extend the watchdog for a batch that injected `n` keys.
    fn arm(&mut self, engine: &Engine, n: u32) {
        if n > 0 {
            self.expected += n;
            self.deadline = Some(Instant::now() + burst_watch_window(engine));
        }
    }

    /// One echo was forwarded: the burst shrinks and its deadline moves
    /// out; the last echo settles the burst.
    fn note_echo(&mut self, engine: &Engine) {
        self.expected = self.expected.saturating_sub(1);
        self.deadline = if self.expected > 0 {
            Some(Instant::now() + burst_watch_window(engine))
        } else {
            None
        };
    }

    /// Whether a rewrite burst is still in flight — echoes are still
    /// expected. The key-buffering consumer reads this alongside the
    /// engine's `has_deferred()` (parked commits are the other in-flight
    /// half).
    fn in_flight(&self) -> bool {
        self.expected > 0
    }

    /// Echoes the armed burst still expects — the readable count for the
    /// key-buffering consumer.
    fn expected_echoes(&self) -> u32 {
        self.expected
    }

    /// Whether the armed burst's echoes are declared lost.
    fn expired(&self) -> bool {
        self.in_flight() && self.deadline.is_some_and(|d| Instant::now() >= d)
    }

    /// Milliseconds until the watchdog deadline — folded into the main
    /// loop's wake timeout so a lost burst is its own wake-up, not a 1 s
    /// poll away.
    fn ms_until(&self) -> Option<u64> {
        self.deadline.map(|d| {
            d.saturating_duration_since(Instant::now())
                .as_millis()
                .max(1) as u64
        })
    }

    fn disarm(&mut self) {
        self.expected = 0;
        self.deadline = None;
    }
}

/// Capacity of the per-burst key buffer. A burst whose echoes never come
/// back holds keys only until the watchdog aborts it (about one window), so
/// 64 is far past anything real typing produces in that time; the bound
/// keeps a stuck state from accumulating without limit.
const BURST_BUFFER_CAP: usize = 64;

/// Keystrokes held while a rewrite burst was in flight, replayed in order
/// once it completes (see `replay_buffered_keys`). Presses only: releases
/// carry no composition ("key releases are never ours") and keep flowing,
/// so the application's key-state tracking stays live during the burst.
#[derive(Default)]
struct BurstBuffer {
    /// `(keyval, keycode, state)` of each held press, arrival order.
    keys: Vec<(u32, u32, u32)>,
    /// Log-once gate for the overflow: one line per overflow episode,
    /// reset when the buffer drains.
    overflow_logged: bool,
    /// Set while `replay_buffered_keys` drains: the replayed keys are
    /// consecutive user keystrokes and must process even though each one
    /// parks its own commit (the parked half of the in-flight state) —
    /// re-holding them here would stall the drain to one key per
    /// completion pass.
    replaying: bool,
}

impl BurstBuffer {
    /// Hold one key press. Past the cap the NEWEST key is the drop: the
    /// first keys are the ones the engine's word state was built around,
    /// so the replay must keep them and lose the latest presses instead.
    fn push(&mut self, keyval: u32, keycode: u32, state: u32) {
        if self.keys.len() >= BURST_BUFFER_CAP {
            if !self.overflow_logged {
                self.overflow_logged = true;
                debug_log::log(&format!("buf ok=0 why=burst_full cap={BURST_BUFFER_CAP}"));
            }
            return;
        }
        self.keys.push((keyval, keycode, state));
    }

    /// Whether a keystroke must wait for the rewrite burst: echoes of the
    /// injected batch are still in flight, a parked commit has not landed
    /// yet (the parked half of the in-flight state), or keys are already
    /// held — they must replay first, order being the point of the buffer.
    /// Idle typing pays a few field reads and never holds.
    fn holds_keystroke(&self, engine: &Engine, burst: &BurstWatch) -> bool {
        !self.replaying && (burst.in_flight() || engine.has_deferred() || !self.keys.is_empty())
    }

    /// Drain in arrival order for the replay; the overflow gate resets so
    /// a fresh episode logs again.
    fn take(&mut self) -> Vec<(u32, u32, u32)> {
        self.overflow_logged = false;
        std::mem::take(&mut self.keys)
    }

    /// Drop everything: the held keys belonged to a context or a word that
    /// is going away (focus out, client reset) — replaying them would type
    /// into the wrong place.
    fn clear(&mut self) {
        self.overflow_logged = false;
        self.keys.clear();
    }
}

/// How long a FocusOut during a burst waits for the context to come back
/// before the deferred reset runs as a real switch. Quick unfocus/refocus
/// of one context lands well inside this window (the logs show the pair
/// 1ms apart); a user switching apps takes longer.
const FOCUS_BOUNCE_MS: u64 = 100;

/// How long the rewrite stashed by an aborted burst waits for a focus-in
/// whose client report can reconcile it. lotus's
/// `reconcileAbortedUinputReplacement` semantics: the report still showing
/// the stashed state means the rewrite never landed and is re-applied;
/// anything else means it is unsafe or pointless to rewrite.
const RECONCILE_WINDOW: Duration = Duration::from_secs(2);

/// The smallest anchor the reconcile accepts. A one-character anchor is
/// satisfiable by a single stray keystroke in ANY context — alt-tab away
/// mid-burst, type one matching letter, and the repair would fire its
/// deletes and commit into the wrong document — so the match must require
/// two ordered characters to coincide. The cost is bounded: a refused
/// one-character rewrite stays raw (the pre-bounce outcome), never corrupts.
const MIN_ANCHOR_CHARS: usize = 2;

/// The rewrite a burst had in flight when a real focus switch aborted it:
/// the parked commit text and the screen states needed to reconcile it
/// against the next client report.
struct StashedRewrite {
    /// The rewrite text the burst still owed the screen (the parked commit).
    text: String,
    /// The state the client's buffer should still show for the repair to be
    /// safe — what the application certainly held when the batch armed
    /// (nothing landed), or the deletes-landed state when the anchor falls
    /// back to the tail without the parked insert. The reconcile matches the
    /// report's text before the caret against this.
    expected_tail: String,
    /// The believed text before the caret once the rewrite lands — the
    /// re-anchor value for the engine's tail after the repair.
    tail: String,
    /// The client the burst belonged to (`FocusInId`'s string): a live
    /// client naming a DIFFERENT context refuses the reconcile, so a
    /// confirmed foreign report can never drive the repair.
    client: Option<String>,
    /// When the stash expires; the drop is the log-once gate.
    deadline: Instant,
}

/// The burst-completion flush: when the rewrite burst has fully completed
/// (no echoes expected, nothing parked) replay the held keys through
/// `process_key` in arrival order, then the buffer clears. One flush point
/// for both completion routes — the watchdog expiry completes inside
/// `burst_watch_pass` (its flush delivers the parked commits), the
/// echo-settle route when the tick delivers the last parked commit — and
/// the run loop's post-tick pass reaches both.
///
/// The replay respects the engine's word-mode lock: `key_mode()` returns
/// the locked mode for the whole word and only a word end, a reset or a
/// focus drop releases it (the burst flushes never do), so replayed
/// character keys continue the same word the burst was rewriting.
fn replay_buffered_keys(
    engine: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    buffer: &mut BurstBuffer,
    out: &mut Vec<Message>,
) {
    if buffer.replaying || buffer.keys.is_empty() || burst.in_flight() || engine.has_deferred() {
        return;
    }
    buffer.replaying = true;
    for (keyval, keycode, state) in buffer.take() {
        let ev = KeyEvent {
            keyval,
            keycode,
            state,
        };
        if !process_key(engine, injection, burst, buffer, ev, out) {
            // The engine passed the key through (an editing key, a disabled
            // engine, plain Latin): the original ProcessKeyEvent reply would
            // have handed it to the application, but the replay rides the
            // tick path — forward it explicitly.
            push_forward_keys(out, keyval, keycode, state);
        }
    }
    buffer.replaying = false;
}

/// One pass of the per-burst echo watchdog on the tick path. A burst
/// whose echoes stopped arriving is declared lost on its deadline: the
/// parked commits flush through the same path the stale windows use
/// (`flush_deferred` — the mutter-safe parked order), the burst counters
/// clear through the stale machinery, and the expiry logs once (the
/// disarm is the gate — a deadline cannot fire twice). The pass also
/// reconciles a burst the engine's own stale flush already ended on a
/// key path or an earlier tick, keeping the in-flight view honest for
/// the key-buffering consumer.
fn burst_watch_pass(engine: &mut Engine, burst: &mut BurstWatch, out: &mut Vec<Message>) {
    // Reconcile first: the engine's stale pass may have consumed the
    // burst already — a flush here would be empty, but the state must
    // settle.
    if burst.in_flight() && !engine.pending_echoes() {
        burst.disarm();
        return;
    }
    if !burst.expired() {
        return;
    }
    debug_log::log(&format!(
        "ins ok=0 why=burst_lost echoes={}",
        burst.expected_echoes()
    ));
    // Clear the burst counters through the existing stale machinery and
    // deliver what the burst parked — the same flush FocusOut and Reset
    // run.
    engine.take_stale_injection(Duration::ZERO);
    for text in engine.flush_deferred() {
        out.push(signal("CommitText").append1(plain_text(&text)));
    }
    burst.disarm();
}

/// One injected-backspace echo arriving through ProcessKeyEvent: the key
/// goes to the application untouched (that key press IS the deletion
/// mechanism) and the burst watchdog re-arms. Once the last echo of a
/// rewrite is forwarded, its deferred commit can go — but the reply
/// carrying the forwarded key must not also carry the commit: on a
/// mutter relay the commit channel beats the key events and the commit
/// overtook the last deletes ("titiến"). Park it for the main loop to
/// emit after the app has applied the deletes. Returns whether the key
/// was an echo.
fn note_burst_echo(engine: &mut Engine, burst: &mut BurstWatch) -> bool {
    if !engine.take_injected_echo() {
        return false;
    }
    burst.note_echo(engine);
    for text in engine.note_echo_forwarded() {
        engine.park_commit(text, Duration::from_millis(Engine::ECHO_APPLY_GRACE_MS));
    }
    true
}

/// Account one injected batch: the engine counts the echoes it must
/// forward (they arrive as ProcessKeyEvent and must be forwarded, not
/// processed) and the burst watchdog arms its deadline beside it — the
/// two counters must never drift apart.
fn add_injected_batch(engine: &mut Engine, burst: &mut BurstWatch, n: u32) {
    engine.add_injected(n);
    burst.arm(engine, n);
}

/// The FocusOut arm's bounce branch, called only for a non-`Disable` focus
/// loss. A burst in flight (echoes expected or commits parked — the same
/// pair the key buffer holds on) defers the whole reset: the engine keeps
/// its composition, tail, buffer state and parked commits, the held keys
/// stay in the buffer (the branch deliberately clears nothing), and the
/// in-flight rewrite is captured for the reconcile in case this turns out
/// to be a real switch. Returns whether the branch took the focus-out.
fn defer_focus_out_during_burst(
    engine: &mut Engine,
    burst: &mut BurstWatch,
    bounce_deadline: &mut Option<Instant>,
    stash: &mut Option<StashedRewrite>,
) -> bool {
    // The burst half alone is not enough — the echo counters may already
    // have settled while the commit is still parked behind its grace.
    if !(engine.has_deferred() || burst.in_flight()) {
        return false;
    }
    *stash = stash_in_flight_rewrite(engine);
    *bounce_deadline = Some(Instant::now() + Duration::from_millis(FOCUS_BOUNCE_MS));
    debug_log::log("ctx focus=out defer=reset why=burst");
    true
}

/// Capture the rewrite a live burst owes the screen, for the reconcile
/// after a real-switch abort. Only a single parked text is reconcilable
/// (stacked parks from fast multi-key edits have no single rewrite text);
/// and the anchor must carry evidence — an empty expected tail cannot
/// distinguish the stashed state in a foreign report.
fn stash_in_flight_rewrite(engine: &Engine) -> Option<StashedRewrite> {
    let parked = engine.parked_commits();
    let [insert] = parked.as_slice() else {
        return None;
    };
    let tail = engine.tail().to_string();
    // The anchor: what the client certainly holds. The per-key batch's
    // landed state is exact while one is parked; otherwise the tail without
    // the parked insert is the deletes-landed state of the echo-gated
    // transports — valid only while the parked text really is the tail's
    // suffix: a park from another producer (a macro/emoji timed park) may
    // not be, and stripping it would manufacture a garbage anchor from an
    // unrelated prefix.
    let expected_tail = match engine.in_flight_landed() {
        Some(h) => h.to_string(),
        None if tail.ends_with(insert) => tail
            .chars()
            .take(tail.chars().count() - insert.chars().count())
            .collect(),
        None => return None,
    };
    // A one-character anchor cannot evidence the departed screen (see
    // MIN_ANCHOR_CHARS).
    if expected_tail.chars().count() < MIN_ANCHOR_CHARS {
        return None;
    }
    Some(StashedRewrite {
        text: insert.to_string(),
        expected_tail,
        tail,
        client: engine.client().map(str::to_string),
        deadline: Instant::now() + RECONCILE_WINDOW,
    })
}

/// The FocusIn arm's bounce resolution, before any reset work. Inside the
/// window and the same context (a `FocusInId` naming another client is a
/// real switch; `FocusIn`/`Enable` carry no id and are indistinguishable
/// from the bounce), the burst resumes untouched and the fallback stash
/// drops; anything else aborts the deferred reset now — the stash survives
/// for the reconcile at this focus's own ST report. Returns whether a
/// pending bounce resumed.
fn resolve_focus_bounce_on_focus_in(
    engine: &mut Engine,
    burst: &mut BurstWatch,
    buffer: &mut BurstBuffer,
    bounce_deadline: &mut Option<Instant>,
    stash: &mut Option<StashedRewrite>,
    incoming_client: Option<&str>,
) -> bool {
    let Some(deadline) = *bounce_deadline else {
        return false;
    };
    *bounce_deadline = None;
    // The bounce belongs to the context that left: a same-client refocus
    // resumes it. XIM names every client `xim`, so a same-string comparison
    // cannot distinguish two XIM apps — accepted, the window is 100ms.
    let same_client = match (incoming_client, engine.client()) {
        (Some(incoming), Some(held)) => incoming == held,
        _ => true,
    };
    if Instant::now() < deadline && same_client {
        *stash = None;
        debug_log::log("ctx focus=back why=bounce");
        return true;
    }
    abort_deferred_focus_out(engine, burst, buffer);
    false
}

/// The deferred FocusOut reset, run when a bounce window expired without a
/// refocus (or a different context refocused inside it): a real switch.
/// Emits nothing — a commit here would land in whatever context focused
/// since; the parked commits go to the stash instead (dropped when it holds
/// nothing reconcilable), and the held keys die with the context.
fn abort_deferred_focus_out(engine: &mut Engine, burst: &mut BurstWatch, buffer: &mut BurstBuffer) {
    // The echoes of the aborted batch will never come back — clear the
    // counters the same way the watchdog's expiry does.
    engine.take_stale_injection(Duration::ZERO);
    // Drop the parks without emitting: the stash holds the rewrite text.
    for _ in engine.flush_deferred() {}
    // The deferred reset itself: focus_out emits nothing on a focus change
    // by contract (no HidePreedit — the client owns the display).
    let _ = engine.focus_out();
    engine.clear_buffer_state();
    // The departed client's identity dies with the abort: a plain-FocusIn
    // (non-FocusInId) next client would otherwise inherit the stale string,
    // which feeds the relay ban and app-rule resolution (the injection-
    // onto-relay class, "xin chaà"). Safe here: the FocusIn resolution
    // compares the incoming client BEFORE this runs, and the loop-pass
    // expiry needs no client.
    engine.set_client(None);
    burst.disarm();
    // Keys held for the burst belonged to the context going away: replaying
    // them after the next FocusIn would type them into the wrong window.
    // Task 12's drop moved here from the FocusOut arm when the bounce
    // window landed: a bounced focus keeps the keys, a real switch drops
    // them — the coordination note from Task 12's round.
    buffer.clear();
    debug_log::log("ctx focus=lost why=bounce stash=rewrite");
}

/// Chars of the common prefix of two strings — the same derivation
/// `per_key_coalesce` applies to split a rewrite into an erase and an
/// insert.
fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Reconcile the stashed rewrite against the client's report (the text
/// before the caret). Match: the rewrite never completed — repair by
/// re-emitting it (the deletes the batch still owes plus the stashed text)
/// and re-anchor the engine's belief. Mismatch: unsafe or pointless to
/// rewrite — drop, log once (the drop is the gate).
fn reconcile_stashed_rewrite(
    engine: &mut Engine,
    injection: &Injection,
    burst: &mut BurstWatch,
    stash: &mut Option<StashedRewrite>,
    actual: &str,
    out: &mut Vec<Message>,
) {
    let Some(s) = stash else {
        return;
    };
    if Instant::now() > s.deadline {
        debug_log::log("st stash=drop why=no_report");
        *stash = None;
        return;
    }
    // The stash belongs to the context that left: a live client naming a
    // DIFFERENT one refuses it — the report describes a foreign buffer, and
    // the repair would fire its deletes and commit into the wrong document.
    if let (Some(held), Some(live)) = (&s.client, engine.client()) {
        if held != live {
            debug_log::log("st stash=drop why=other_context");
            *stash = None;
            return;
        }
    }
    // The report must still show the stashed state — the rewrite never
    // completed, so re-emitting it is safe. The capture already guarantees
    // the anchor carries evidence (MIN_ANCHOR_CHARS).
    if actual.ends_with(&s.expected_tail) {
        // The erase the batch still owes: the same split per_key_coalesce
        // applies. When the deletes already landed (the anchor is the tail
        // without the insert), the common prefix covers the anchor and this
        // is zero — the repair is a plain commit.
        let erase = s.expected_tail.chars().count() - common_prefix_len(&s.expected_tail, &s.tail);
        let insert = s.text.clone();
        let mut actions = Vec::new();
        if erase > 0 {
            actions.push(Action::InjectBackspaces(erase as u32));
        }
        actions.push(Action::Commit(insert.clone()));
        let injected = emit(engine, injection, &actions, out);
        add_injected_batch(engine, burst, injected);
        // The engine's belief follows the repair: the screen will hold the
        // stashed tail once it lands.
        engine.reanchor_tail(s.tail.clone());
        engine.note_committed(&insert);
        debug_log::log("st stash=repair why=matches");
    } else {
        debug_log::log("st stash=drop why=mismatch");
    }
    *stash = None;
}

/// Turn engine actions into the signals IBus expects.
///
/// Only the `IBusText` argument is a variant: `CommitText(v)` and
/// `UpdatePreeditText(v u b u)`. Wrapping every argument in a variant (the
/// old `(v...)` helper) makes ibus-daemon drop the call, which is why preedit
/// text never appeared until the word was committed.
fn emit(
    engine: &mut Engine,
    injection: &Injection,
    actions: &[Action],
    out: &mut Vec<Message>,
) -> u32 {
    let transports = Transports {
        backspaces: &|n, gap| injection.backspaces(n, gap),
        type_text: &|text| injection.type_text(text),
    };
    emit_with(engine, actions, out, &transports)
}

/// Key transports shaped like the selected backend's ops. Production builds
/// them over the server's `Injection`, which forwards to the backend it
/// holds; tests build them over stubs to exercise the injection paths
/// without a writable `/dev/uinput`. `backspaces(n, gap)` presses Backspace
/// `n` times `gap` ms apart.
struct Transports<'a> {
    backspaces: &'a dyn Fn(u32, u64) -> bool,
    type_text: &'a dyn Fn(&str) -> bool,
}

/// `emit` with injectable transports. Tests use it to exercise the injection
/// paths without requiring a writable `/dev/uinput`.
fn emit_with(
    engine: &mut Engine,
    actions: &[Action],
    out: &mut Vec<Message>,
    t: &Transports,
) -> u32 {
    let hide_underline = engine.hide_underline();
    // The protocol delete is the primary transport whenever the client has
    // reported surrounding text (mutter then knows the buffer; on relayed
    // contexts it is also the ONLY working transport — injected keys never
    // arrive there). Injection takes over for non-relayed clients without a
    // report or proven to swallow the protocol delete. Actions can only
    // reach emit after `deletes_allowed()` in the engine, so the protocol
    // arms below additionally guard on the report.
    let inject = engine.uinput_available() && engine.may_inject();
    let bs_gap = engine.backspace_interval_ms();
    let mut injected_since_commit = 0u32;
    let mut injected_total = 0u32;
    // Set when a rewrite delete could not be delivered to a client proven
    // to swallow deletes: the paired commit must be suppressed too, or the
    // replacement lands on top of the raw keys and duplicates the word.
    let mut delete_failed = false;
    for action in actions {
        match action {
            Action::Commit(text) => push_commit(
                engine,
                t,
                out,
                text.clone(),
                &mut delete_failed,
                &mut injected_since_commit,
            ),
            Action::DeleteBefore(n) if inject => {
                // No usable protocol delete here; press real Backspace keys
                // at the cursor instead. If the device cannot be opened
                // (permissions lost mid-session), fall back to the protocol
                // delete rather than dropping the deletion and leaving a
                // commit deferred forever.
                if (t.backspaces)(*n, bs_gap) {
                    debug_log::log(&format!("del mode=protocol n={n} op=inject"));
                    injected_since_commit += *n;
                    injected_total += *n;
                } else if engine.delete_dropped() {
                    // The device is gone for the session (the handle is
                    // opened once) and the client swallows the protocol
                    // delete: neither transport works. Keep the raw text,
                    // suppress the paired commit, and let the engine know —
                    // the mode walk serves the following words through the
                    // forwarded-Backspace transport.
                    debug_log::log("del ok=0 mode=protocol why=inject_failed keep=raw");
                    engine.set_uinput_available(false);
                    delete_failed = true;
                } else {
                    debug_log::log(&format!("del ok=0 mode=protocol why=inject_failed n={n}"));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                }
            }
            Action::DeleteBefore(n) => {
                if !engine.protocol_delete_ok() {
                    // Defensive: the engine gates deletes on a surrounding
                    // report (or the direct-client opt-in); never let one slip
                    // to a buffer mutter never saw.
                    debug_log::log(&format!("del mode=protocol dropped=1 why=no_report n={n}"));
                } else {
                    debug_log::log(&format!("del mode=protocol n={n}"));
                    // Signature is (iu): a plain signed offset, not a variant.
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                }
            }
            Action::DeleteSurrounding { offset, n } if inject => {
                // Real Backspaces must remove everything from the deletion
                // start up to the cursor (the deleted range and the part the
                // commit will re-type), i.e. |offset| presses — n only says
                // how much of that range the protocol delete covered.
                let presses = presses(*offset);
                if (t.backspaces)(presses, bs_gap) {
                    debug_log::log(&format!(
                        "del mode=surrounding offset={offset} presses={presses}"
                    ));
                    injected_since_commit += presses;
                    injected_total += presses;
                } else {
                    debug_log::log(&format!(
                        "del ok=0 mode=surrounding why=inject_failed offset={offset} n={n}"
                    ));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(*offset).append1(*n);
                    out.push(m);
                }
            }
            Action::DeleteSurrounding { offset, n } => {
                if !engine.surrounding_data_ok() {
                    debug_log::log(&format!(
                        "del mode=surrounding dropped=1 why=no_report offset={offset} n={n}"
                    ));
                } else {
                    debug_log::log(&format!("del mode=surrounding offset={offset} n={n}"));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(*offset).append1(*n);
                    out.push(m);
                }
            }
            Action::RewriteByBackspace { backspaces, commit } => {
                if (t.backspaces)(*backspaces, bs_gap) {
                    debug_log::log(&format!("del mode=backspace n={backspaces}"));
                    injected_since_commit += *backspaces;
                    injected_total += *backspaces;
                } else if engine.delete_dropped() {
                    // Device gone on a delete-swallowing client: the
                    // protocol fallback pair would land nowhere and
                    // duplicate. Keep the raw text (the commit rides inside
                    // this action, so skipping the branch skips both).
                    debug_log::log("del ok=0 mode=backspace why=inject_failed keep=raw");
                    engine.set_uinput_available(false);
                } else {
                    debug_log::log(&format!(
                        "del ok=0 mode=backspace why=inject_failed n={backspaces}"
                    ));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*backspaces as i32)).append1(*backspaces);
                    out.push(m);
                    out.push(signal("CommitText").append1(plain_text(commit)));
                }
            }
            Action::UpdatePreedit { text, cursor } => {
                {
                    // Both display choices keep the classic preedit beside
                    // the table; a batch without candidates sends it alone.
                    let len = text.chars().count() as u32;
                    let attrs: &[Attr] = if hide_underline {
                        &[]
                    } else {
                        &[Attr::underline(0, len)]
                    };
                    out.push(
                        signal("UpdatePreeditText")
                            .append1(text_variant(text, attrs))
                            .append1(*cursor)
                            .append1(true)
                            // IBUS_ENGINE_PREEDIT_COMMIT: when the client resets
                            // the input context (a mouse click, a focus change) it
                            // commits this preedit itself instead of clearing it,
                            // so a pending word is never lost. ibus-bamboo sends
                            // this same mode.
                            .append1(1u32),
                    );
                }
                // The box's input line is not shaped here: the word rides
                // `UpdateCandidates` (the inline display has no preedit
                // signal to convert) and `strip_show` writes it.
            }
            Action::HidePreedit => {
                out.push(
                    signal("UpdatePreeditText")
                        .append1(plain_text(""))
                        .append1(0u32)
                        .append1(false) // not visible = hide
                        .append1(0u32),
                );
            }
            // uinput modes' word-end rewrite: real Backspaces, paced.
            Action::InjectBackspaces(n) => {
                if (t.backspaces)(*n, bs_gap) {
                    debug_log::log(&format!("del mode=flow n={n} op=inject"));
                    injected_since_commit += *n;
                    injected_total += *n;
                } else if engine.surrounding_data_ok() {
                    // Device lost mid-session: protocol delete as fallback.
                    debug_log::log(&format!("del ok=0 mode=flow why=inject_failed n={n}"));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                } else {
                    debug_log::log(&format!("del ok=0 mode=flow why=no_report dropped=1 n={n}"));
                }
            }
            // ForwardBackspace mode's delete transport: Backspace key events
            // sent through IBus, so the client's own IM context applies them
            // like real presses. They ride the same connection as the Commit
            // that follows, which keeps them ordered; no echo comes back
            // (the key goes to the application, not to the engine).
            // ForwardBackspace (mode I) commits with its deletes in one
            // batch, as shipped: pacing here made JetBrains' XIM bridge
            // apply the commit into a stale view (the mark-coded vowel
            // vanished from screen yet Backspace revealed it — the buffer
            // and the display desynced). The pacing stays on the injected
            // Backspace transport only.
            // With a transport that types the word too, the deletes go through
            // it as well: one channel means no forwarded pair the client may
            // drop, and nothing left to wait for.
            Action::ForwardBackspaces(n) if inject && engine.transport_types_text() => {
                if (t.backspaces)(*n, bs_gap) {
                    debug_log::log(&format!("del mode=forward n={n} op=inject"));
                    injected_since_commit += *n;
                    injected_total += *n;
                } else {
                    debug_log::log(&format!("del mode=forward n={n}"));
                    for _ in 0..*n {
                        push_forward_keys(out, keymap::BACKSPACE, BACKSPACE_KEYCODE, 0);
                    }
                }
            }
            Action::ForwardBackspaces(n) => {
                debug_log::log(&format!("del mode=forward n={n}"));
                for _ in 0..*n {
                    push_forward_keys(out, keymap::BACKSPACE, BACKSPACE_KEYCODE, 0);
                }
            }
            // The per-key batch's parked commit: its deletes were injected
            // and their echoes accounted when the batch armed, so the
            // normal deferral would park it behind a stale count. Emit
            // directly.
            Action::CommitNow(text) => {
                out.push(signal("CommitText").append1(plain_text(text)));
            }
            Action::PostDelay(ms) => {
                if *ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(*ms));
                }
            }
            Action::Forward | Action::Consumed => {}
            // The strip renders server-side, the picker's lookup-table
            // precedent: the candidate actions carry the word-in-flight and
            // the engine's full list, the server pages and labels the list
            // (below). The word's only surface is the classic inline preedit
            // beside the table, so the box carries no input line.
            Action::UpdateCandidates { word, items } => strip_show(false, word, items, out),
            Action::HideCandidates => strip_hide(out),
        }
    }
    injected_total
}

/// Page size of the suggestion strip's lookup table: five labeled slots,
/// `Tab` and `Alt`+`1`-`5` pick them (slots shared with the engine's page-1
/// builder), `Ctrl+,`/`Ctrl+.` and PageUp/PageDown turn the page — the
/// panel's ◀▶ buttons page the same way.
use crate::suggest::STRIP_PAGE_SIZE;

/// The server's view of the suggestion strip — the full candidate list the
/// last `UpdateCandidates` carried and the page the pagers moved to (the
/// `-`/`=`/PageUp/PageDown keys and the panel's pager methods; the arrows
/// stay caret navigation). It lives beside `emit` instead of on
/// `EngineHandler` because those two actions reach the server only through
/// `emit`, whose call sites span the key path, the tick, focus/reset and
/// the config applies across five files; riding the funnel keeps the state
/// in step with the table on screen no matter which of them fired. The
/// server's message loop is single-threaded (the callback runs inside
/// `conn.iter()` on the main thread) and `EngineHandler` is `!Send`
/// anyway, so a thread-local is sound.
struct StripState {
    items: Vec<String>,
    page: usize,
}

thread_local! {
    static STRIP: std::cell::RefCell<Option<StripState>> = const { std::cell::RefCell::new(None) };
}

/// Show the strip: rewrite the box's input line with the word-in-flight,
/// store the engine's full list and render page 0. Every `UpdateCandidates`
/// is a word change, so the input line is rewritten and the pager's page
/// never survives it. An empty list would render an empty table — hide
/// instead, keeping the state and the screen in step.
fn strip_show(aux: bool, word: &str, items: &[String], out: &mut Vec<Message>) {
    if items.is_empty() {
        strip_hide(out);
        return;
    }
    // The floating input line: the word-in-flight shows in the candidate
    // window (ibus intelligent pinyin's aux line); the cursor arg carries
    // no meaning there. The action carries the word because the inline
    // modes have no preedit signal to convert — this is the only source.
    // The inline display keeps the classic preedit as the word's only
    // surface, so no line at all.
    if aux {
        out.push(
            signal("UpdateAuxiliaryText")
                .append1(plain_text(word))
                .append1(true),
        );
    }
    STRIP.with(|cell| {
        *cell.borrow_mut() = Some(StripState {
            items: items.to_vec(),
            page: 0,
        });
    });
    strip_emit_table(out);
}

/// Hide the strip and drop its state. The signals ride only when something
/// was shown: the engine's close racing the server's own teardown (focus
/// out) must not push a duplicate hide. The floating input line dies with
/// the table — they share the one candidate window.
fn strip_hide(out: &mut Vec<Message>) {
    let shown = STRIP.with(|cell| cell.borrow_mut().take().is_some());
    if shown {
        out.push(signal("HideAuxiliaryText"));
        out.push(signal("HideLookupTable"));
    }
}

/// Whether the strip is showing.
fn strip_open() -> bool {
    STRIP.with(|cell| cell.borrow().is_some())
}

/// The page the pagers moved to, `0` while page 0 is the newest update.
fn strip_page() -> usize {
    STRIP.with(|cell| cell.borrow().as_ref().map(|s| s.page).unwrap_or(0))
}

/// Move the page by `delta`, clamped at both ends — the strip never wraps
/// (a wrap past the last page would re-show page 0 under the user's hand).
fn strip_move_page(delta: i32) {
    STRIP.with(|cell| {
        let mut s = cell.borrow_mut();
        if let Some(s) = s.as_mut() {
            let last = s.items.len().saturating_sub(1) / STRIP_PAGE_SIZE;
            s.page = (s.page as i32 + delta).clamp(0, last as i32) as usize;
        }
    });
}

/// Render the strip: the full list travels at once with the cursor on the
/// page head — the panel's page count then renders the ◀▶ buttons (mouse
/// paging) — horizontal with the `1.`-`5.` labels, one digit label per
/// slot, so the key that picks an entry is visible on it.
fn strip_emit_table(out: &mut Vec<Message>) {
    STRIP.with(|cell| {
        let s = cell.borrow();
        let Some(s) = s.as_ref() else {
            return;
        };
        let cursor = (s.page * STRIP_PAGE_SIZE) as u32;
        let table = ibus_prop::lookup_table_strip(STRIP_PAGE_SIZE as u32, cursor, &s.items);
        out.push(signal("UpdateLookupTable").append2(table, true));
    });
}

/// The selection digit a keysym names, `1`-`5` (keypad digits included
/// through their chars); `None` for every other key.
fn strip_digit(keyval: u32) -> Option<usize> {
    match keymap::keysym_to_char(keyval) {
        Some(c @ '1'..='5') => Some(c as usize - '0' as usize),
        _ => None,
    }
}

/// The strip's server-side keys, tried between the burst hold and the
/// editing-key dispatch: `Tab` picks the first slot — the only key the
/// strip owns here. `Alt`+digits pick (in the shortcut branch above, where
/// the modifier is still readable); the bare digits stayed content, so the
/// strip no longer eats a number typed inside a word. `Ctrl+,`/`Ctrl+.` paging lives in the shortcut branch above
/// (a combo must be intercepted before the branch's reset commits the
/// word the box is showing), and the popup's ◀▶ buttons page through
/// their PageUp/PageDown methods. Returns whether the key was consumed.
///
/// A pick maps page + digit to the global index and hands it to
/// `accept_candidate`, whose returned actions ride the normal emit — the
/// commit, the composition reset and the strip's own close all travel the
/// regular paths. The engine validates the index: an over-range label or a
/// strip the word has moved past returns empty, and the key falls through
/// to the normal dispatch (a refused digit types as content, exactly the
/// no-strip behavior). The strip never injects on its own; a pick's commit
/// rides the word's own delete transports (protocol delete, forwarded or
/// injected Backspaces), and the returned count feeds `add_injected_batch`
/// like any word end.
fn strip_key(
    engine: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    keyval: u32,
    out: &mut Vec<Message>,
) -> bool {
    if !strip_open() || !matches!(keyval, keymap::GRAVE | keymap::ASCII_TILDE) {
        return false;
    }
    // The first slot of the visible page. A page the word moved past, or an
    // empty slot, returns empty and the key keeps its app meaning.
    strip_pick(
        engine,
        injection,
        burst,
        strip_page() * STRIP_PAGE_SIZE,
        out,
    )
}

/// Commit the strip slot at `index` and emit its actions. False when the
/// engine refuses the index, so the caller's key falls through untouched.
fn strip_pick(
    engine: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    index: usize,
    out: &mut Vec<Message>,
) -> bool {
    let actions = engine.accept_candidate(index);
    if actions.is_empty() {
        return false;
    }
    let injected = emit(engine, injection, &actions, out);
    add_injected_batch(engine, burst, injected);
    true
}

/// A signal on the static engine path and interface: `new_signal` cannot
/// fail on those, so the `expect` is unreachable by construction.
fn signal(member: &str) -> Message {
    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, member)
        .expect("static path and interface are valid")
}

/// Emit a commit, honouring the transport deferrals: a commit that would
/// overtake injected keys is parked until their echoes come back ("the
/// accent flashes and reverts"), and one whose rewrite delete never landed
/// is dropped.
fn push_commit(
    engine: &mut Engine,
    t: &Transports,
    out: &mut Vec<Message>,
    text: String,
    delete_failed: &mut bool,
    injected_since_commit: &mut u32,
) {
    if std::mem::take(delete_failed) {
        debug_log::log("buf commit=suppressed why=no_delete");
    } else if *injected_since_commit > 0 && engine.transport_types_text() {
        // One channel: the transport presses the deletes and types the word,
        // in that order, so nothing can overtake anything and no wait is
        // needed after the deletes.
        *injected_since_commit = 0;
        if (t.type_text)(&text) {
            debug_log::log(&format!("buf commit=typed len={}", text.chars().count()));
        } else {
            out.push(signal("CommitText").append1(plain_text(&text)));
        }
    } else if *injected_since_commit > 0 {
        // The injected Backspaces travel through the compositor while this
        // commit would go straight over D-Bus — a synchronous commit here
        // can land BEFORE the deletions and the echoing Backspace then
        // deletes the freshly committed word.
        engine.defer_commit(*injected_since_commit, text);
        *injected_since_commit = 0;
    } else {
        out.push(signal("CommitText").append1(plain_text(&text)));
    }
}

/// Send a reply or signal. A failed send means the peer is gone; there is
/// nothing useful to do about it from inside the filter.
fn send(conn: &Connection, msg: Message) {
    let _ = conn.send(msg);
}

/// Backspace presses needed to delete `n` chars at `offset` before the cursor.
fn presses(offset: i32) -> u32 {
    (-offset as i64).max(0) as u32
}

const INTROSPECT_XML: &str = r#"<node>
  <interface name="org.freedesktop.IBus.Factory">
    <method name="CreateEngine">
      <arg direction="in" type="s" name="name"/>
      <arg direction="out" type="o"/>
    </method>
  </interface>
</node>"#;

/// Fake transports for tests: every key press "succeeds".
#[cfg(test)]
fn fake_transports<'a>(backspaces: &'a dyn Fn(u32, u64) -> bool) -> Transports<'a> {
    Transports {
        backspaces,
        type_text: &|_| false,
    }
}
