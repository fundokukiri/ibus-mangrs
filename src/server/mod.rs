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

use crate::config::{CharsetConfig, Config, MethodConfig, ModeConfig};
use crate::debug_log::{self, brief};
use crate::engine::{Action, Engine};
use crate::ibus_prop::{self, Prop};
use crate::ibus_text::{plain_text, text_from_variant, text_variant, Attr};
use crate::keymap;
use crate::uinput::Uinput;

mod hotkey;
mod prop;
mod x11;

#[cfg(test)]
mod tests;

// The GUI hotkey recorder validates specs through crate::server::parse_hotkey;
mod emoji_picker;
mod picker;

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

/// The MăngRS focus extension (`data/gnome-shell-extension`): a session-bus
/// service naming the focused window. Wayland-native apps reach IBus only as
/// a mutter relay, and GNOME offers no other name source for them (Shell
/// Introspect is denied; the wlroots protocol ibus-bamboo uses is absent).
/// The settings window asks the same service for its focused-app chip.
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
        let mut handler = EngineHandler {
            engine: Arc::clone(&engine),
            last_config_check: Instant::now(),
            last_config_mtime: None,
            hotkeys: Vec::new(),
            last_fire: Vec::new(),
            picker: None,
            emoji_picker: None,
            focus_ext: None,
        };
        handler.last_config_mtime = Config::path()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        let initial = Config::load();
        handler.refresh_hotkey_cache(&initial);
        conn.replace_message_callback(Some(Box::new(move |conn, msg| {
            if msg.msg_type() != MessageType::MethodCall {
                return false;
            }
            match handler.handle(conn, &msg) {
                Some(()) => true,
                None => false,
            }
        })));

        Ok(Self { conn, engine })
    }

    /// Run the main loop until the bus goes away.
    pub fn run(&self) {
        loop {
            // Wake at the next engine deadline (the preedit idle commit)
            // instead of sleeping a fixed second past it. lock_engine, not
            // .ok(): a poisoned mutex must degrade one keystroke, not stop
            // the loop's stale-echo flushes for the rest of the session.
            let timeout = lock_engine(&self.engine)
                .ms_until_tick()
                .map_or(1000, |ms| ms.min(1000)) as i32;
            for item in self.conn.iter(timeout) {
                if matches!(item, dbus::ffidisp::ConnectionItem::Nothing) {
                    break;
                }
            }
            let out = {
                let mut e = lock_engine(&self.engine);
                let actions = e.tick();
                let mut out = Vec::new();
                let injected = emit(&mut e, &actions, &mut out);
                e.add_injected(injected);
                out
            };
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

struct EngineHandler {
    engine: SharedEngine,
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
    /// Session-bus connection to the focus extension, created lazily.
    focus_ext: Option<Connection>,
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
            ("org.freedesktop.DBus.Properties", "Get") => {
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
                send(conn, reply);
            }
            ("org.freedesktop.DBus.Properties", "GetAll") => {
                let props: dbus::arg::PropMap = Default::default();
                send(conn, msg.method_return().append1(props));
            }
            ("org.freedesktop.DBus.Properties", "Set") => send(conn, msg.method_return()),
            _ => return None,
        }
        Some(())
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
            // forwards it here): same as pressing the entry's digit. Handled
            // before the engine lock — the apply locks by itself.
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
            // The candidate popup's pager buttons; only the emoji menu has
            // more than one page. Handled before the engine lock.
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
        let ext_name = if matches!(member, "FocusIn" | "FocusInId") {
            self.focus_extension_app()
        } else {
            None
        };

        let mut engine = lock_engine(&self.engine);

        match member {
            "PropertyActivate" => {
                if let Ok((name, state)) = msg.read2::<String, u32>() {
                    activate_property(&mut engine, &name, state, out);
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
                            "surrounding cursor={cursor} tail={:?} text={:?}",
                            brief(engine.tail()),
                            brief(&s),
                        ));
                        // The report can resolve a deferred click-Reset
                        // (park mode): the resulting HidePreedit must go out.
                        let actions = engine.set_surrounding_with_anchor(s, cursor, anchor);
                        emit(&mut engine, &actions, out);
                    }
                }
                msg.method_return()
            }
            // `FocusInId(s path, s client)` replaces `FocusIn` once the
            // FocusId property is true. `client` looks like `gtk4-im:chrome`
            // and is the only way to know which app is typing.
            "FocusIn" | "Enable" | "FocusInId" => {
                if member == "FocusInId" {
                    if let Ok((_, client)) = msg.read2::<String, String>() {
                        engine.set_client(Some(client));
                        note_client(msg);
                    }
                }
                // A new context may not support surrounding text at all; data
                // left over from the previous client would let the engine send
                // protocol deletes to it (mutter segfaults on those).
                engine.clear_buffer_state();
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
                // Direct field assignment: the engine guard held by this
                // branch borrows part of self, so the &mut self helper
                // cannot be called here.
                self.hotkeys = hotkey_registry(&config);
                self.last_fire.clear();
                debug_log::log(&format!("focus-in client={:?}", engine.client()));
                let actions = engine.apply_config(&config);
                engine.set_enabled(config.enabled);
                emit(&mut engine, &actions, out);
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
                emit(&mut engine, &actions, out);
                // Only `Disable` (the panel toggle) switches the wrapper off.
                // FocusOut must not: focus events get coalesced (a quick
                // unfocus/refocus of one context never re-sends them — see the
                // SetCapabilities note below), so a FocusOut whose FocusIn
                // never arrives would leave every later key PassThrough:
                // raw Latin typing until the engine is toggled or restarted.
                if member == "Disable" {
                    engine.set_enabled(false);
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
                debug_log::log(if member == "Disable" {
                    "disable"
                } else {
                    "focus-out"
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
                // The preedit is sent with IBUS_ENGINE_PREEDIT_COMMIT, so the
                // client has already committed the word by the time this
                // arrives (GTK sends it on a mouse click, a focus change and a
                // button press).
                debug_log::log("reset from client");
                for text in engine.flush_deferred() {
                    out.push(signal("CommitText").append1(plain_text(&text)));
                }
                let actions = engine.on_client_reset();
                let injected = emit(&mut engine, &actions, out);
                engine.add_injected(injected);
                msg.method_return()
            }
            "SetCapabilities" => {
                if let Ok(caps) = msg.read1::<u32>() {
                    debug_log::log(&format!("set-capabilities {caps:#b}"));
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

/// Handle one key event, pushing resulting signals into `out`.
/// Returns whether the engine consumed the key.
fn process_key(engine: &mut Engine, keyval: u32, state: u32, out: &mut Vec<Message>) -> bool {
    // Hottest log site (every key event, every mode): the format! would
    // allocate per keystroke even with the log off, so gate it first.
    if debug_log::enabled() {
        debug_log::log(&format!("key keyval={keyval:#x} state={state:#x}"));
    }
    // Key releases are never ours.
    if state & keymap::RELEASE_MASK != 0 {
        return false;
    }
    // Ctrl/Alt/Super combinations are application shortcuts.
    if state & keymap::SHORTCUT_MASK != 0 {
        // The Ctrl+V of a clipboard-channel paste comes back as a shortcut:
        // it is our own injection and must reach the application untouched.
        if let Some(commits) = engine.take_nav_echo(keymap::V) {
            for text in commits {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
            return false;
        }
        if engine.take_stale_injection(Engine::ECHO_STALE) || !engine.pending_echoes() {
            for text in engine.flush_deferred() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
        }
        let actions = engine.reset();
        let injected = emit(engine, &actions, out);
        engine.add_injected(injected);
        return false;
    }

    if keyval == keymap::BACKSPACE {
        // Echoes that never came back (injection lost) must not eat this real
        // Backspace.
        if engine.take_stale_injection(Engine::ECHO_STALE) {
            for text in engine.flush_deferred() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
        }
        // An echo of a Backspace we injected ourselves: pass it to the
        // application — that key press IS the deletion mechanism. Once the
        // last echo of a rewrite is forwarded, its deferred commit can go.
        if engine.take_injected_echo() {
            for text in engine.note_echo_forwarded() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
            return false;
        }
        return match engine.backspace() {
            Some(actions) => {
                let injected = emit(engine, &actions, out);
                engine.add_injected(injected);
                !actions.contains(&Action::Forward)
            }
            None => false,
        };
    }

    // A real key (never an echo): deliver deferred commits whose Backspace
    // echoes are all in, or whose echoes have gone stale (apps that receive
    // injected keys without routing them through ibus would otherwise eat
    // every later Backspace and never see the commit).
    if engine.take_stale_injection(Engine::ECHO_STALE) || !engine.pending_echoes() {
        for text in engine.flush_deferred() {
            out.push(signal("CommitText").append1(plain_text(&text)));
        }
    }

    // Arrows, Tab, Escape, Delete... end the word and belong to the app.
    if keymap::is_editing_key(keyval) {
        // Echoes of keys the uinput modes injected (Select's Shift+Left and
        // Delete): forward them so the selection/erase actually happens.
        if let Some(commits) = engine.take_nav_echo(keyval) {
            for text in commits {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
            return false;
        }
        if engine.take_stale_injection(Engine::ECHO_STALE) || !engine.pending_echoes() {
            for text in engine.flush_deferred() {
                out.push(signal("CommitText").append1(plain_text(&text)));
            }
        }
        // A preedit commit and the forwarded navigation key travel different
        // transports on relayed contexts; a burst of arrows moves the caret
        // before the commit lands and the word is inserted mid-text
        // ("em tên tùng" → "etùngm tên"). Swallow the first navigation key so
        // the commit lands while the caret is still in place; the next press
        // moves the caret normally.
        let swallow = keymap::is_navigation_key(keyval)
            && engine.relay_context()
            && engine.preedit_commit_pending();
        let actions = engine.reset();
        let injected = emit(engine, &actions, out);
        engine.add_injected(injected);
        if swallow {
            debug_log::log(
                "navigation key consumed: preedit commit must land before the caret moves",
            );
            return true;
        }
        return false;
    }

    let Some(c) = keymap::keysym_to_char(keyval) else {
        return false;
    };
    let actions = engine.process_char(c);
    let consumed = !actions.contains(&Action::Forward);
    let injected = emit(engine, &actions, out);
    engine.add_injected(injected);
    consumed
}

/// Turn engine actions into the signals IBus expects.
///
/// Only the `IBusText` argument is a variant: `CommitText(v)` and
/// `UpdatePreeditText(v u b u)`. Wrapping every argument in a variant (the
/// old `(v...)` helper) makes ibus-daemon drop the call, which is why preedit
/// text never appeared until the word was committed.
fn emit(engine: &mut Engine, actions: &[Action], out: &mut Vec<Message>) -> u32 {
    let transports = Transports {
        backspaces: &|n, gap| Uinput::inject_backspaces(n, gap),
        keys: &|keys, gap| Uinput::press_keys(keys, gap),
        select_left: &|n, gap| Uinput::select_left(n, gap),
        paste: &|text| crate::paste::paste_and_restore(text, &|| Uinput::ctrl_v()),
    };
    emit_with(engine, actions, out, &transports)
}

/// A paced key-press transport: (keycode, shift) pairs and the gap in ms.
type PressKeys<'a> = &'a dyn Fn(&[(u16, bool)], u64) -> bool;

/// Key transports so tests can exercise the injection paths without a
/// writable `/dev/uinput`. `backspaces(n, gap)` presses Backspace `n` times
/// `gap` ms apart; `keys` presses (keycode, shift) pairs; `select_left`
/// Shift+Left `n` times; `paste` puts text on the clipboard (with restore)
/// around a Ctrl+V and reports success.
struct Transports<'a> {
    backspaces: &'a dyn Fn(u32, u64) -> bool,
    keys: PressKeys<'a>,
    select_left: &'a dyn Fn(u32, u64) -> bool,
    paste: &'a dyn Fn(&str) -> bool,
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
    let mut nav_since_commit = 0usize;
    // Set when a rewrite delete could not be delivered to a client proven
    // to swallow deletes: the paired commit must be suppressed too, or the
    // replacement lands on top of the raw keys and duplicates the word.
    let mut delete_failed = false;
    for action in actions {
        match action {
            Action::Commit(text) => push_commit(
                engine,
                out,
                text.clone(),
                &mut delete_failed,
                &mut nav_since_commit,
                &mut injected_since_commit,
            ),
            Action::DeleteBefore(n) if inject => {
                // No usable protocol delete here; press real Backspace keys
                // at the cursor instead. If the device cannot be opened
                // (permissions lost mid-session), fall back to the protocol
                // delete rather than dropping the deletion and leaving a
                // commit deferred forever.
                if (t.backspaces)(*n, bs_gap) {
                    debug_log::log(&format!("delete-before inject={n}"));
                    injected_since_commit += *n;
                    injected_total += *n;
                } else if engine.delete_dropped() {
                    // The device is gone for the session (the handle is
                    // opened once) and the client swallows the protocol
                    // delete: neither transport works. Keep the raw text,
                    // suppress the paired commit, and let the engine know —
                    // its mode resolution moves the following words to
                    // preedit, which needs no delete.
                    debug_log::log(
                        "delete-before inject FAILED on a delete-dropping client → keep raw",
                    );
                    engine.set_uinput_available(false);
                    delete_failed = true;
                } else {
                    debug_log::log(&format!("delete-before inject FAILED → protocol=-{n}"));
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
                    debug_log::log(&format!("delete-before protocol DROPPED (no report) n={n}"));
                } else {
                    debug_log::log(&format!("delete-before protocol=-{n}"));
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
                        "delete-surrounding inject offset={offset} presses={presses}"
                    ));
                    injected_since_commit += presses;
                    injected_total += presses;
                } else {
                    debug_log::log(&format!(
                        "delete-surrounding inject FAILED → protocol offset={offset} n={n}"
                    ));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(*offset).append1(*n);
                    out.push(m);
                }
            }
            Action::DeleteSurrounding { offset, n } => {
                if !engine.surrounding_data_ok() {
                    debug_log::log(&format!(
                        "delete-surrounding protocol DROPPED (no report) offset={offset} n={n}"
                    ));
                } else {
                    debug_log::log(&format!(
                        "delete-surrounding protocol offset={offset} n={n}"
                    ));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(*offset).append1(*n);
                    out.push(m);
                }
            }
            Action::RewriteByBackspace { backspaces, commit } => {
                if (t.backspaces)(*backspaces, bs_gap) {
                    debug_log::log(&format!("rewrite-by-backspace n={backspaces}"));
                    injected_since_commit += *backspaces;
                    injected_total += *backspaces;
                } else if engine.delete_dropped() {
                    // Device gone on a delete-swallowing client: the
                    // protocol fallback pair would land nowhere and
                    // duplicate. Keep the raw text (the commit rides inside
                    // this action, so skipping the branch skips both).
                    debug_log::log(
                        "rewrite-by-backspace inject FAILED on a delete-dropping client → keep raw",
                    );
                    engine.set_uinput_available(false);
                } else {
                    debug_log::log(&format!(
                        "rewrite-by-backspace inject FAILED → protocol delete={backspaces}"
                    ));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*backspaces as i32)).append1(*backspaces);
                    out.push(m);
                    out.push(signal("CommitText").append1(plain_text(commit)));
                }
            }
            Action::UpdatePreedit { text, cursor } => {
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
                    debug_log::log(&format!("smooth rewrite: inject backspaces n={n}"));
                    injected_since_commit += *n;
                    injected_total += *n;
                } else if engine.surrounding_data_ok() {
                    // Device lost mid-session: protocol delete as fallback.
                    debug_log::log(&format!("smooth rewrite inject FAILED → protocol -{n}"));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                } else {
                    debug_log::log(&format!(
                        "smooth rewrite inject FAILED and no report: deletion dropped n={n}"
                    ));
                }
            }
            // ForwardKeyEvent mode's delete transport: Backspace key events
            // sent through IBus, so the client's own IM context applies them
            // like real presses. They ride the same connection as the Commit
            // that follows, which keeps them ordered; no echo comes back
            // (the key goes to the application, not to the engine).
            // ForwardKeyEvent (mode I) commits with its deletes in one
            // batch, as shipped: pacing here made JetBrains' XIM bridge
            // apply the commit into a stale view (the mark-coded vowel
            // vanished from screen yet Backspace revealed it — the buffer
            // and the display desynced). The pacing stays on the
            // select-and-erase mode only.
            Action::ForwardBackspaces(n) => {
                debug_log::log(&format!("forward-backspaces n={n}"));
                for _ in 0..*n {
                    push_forward_keys(out, keymap::BACKSPACE, BACKSPACE_KEYCODE, 0);
                }
            }
            // ForwardSelect: Shift+Left moves the application's own selection
            // over the raw tail, then one forwarded Backspace erases it.
            // A CommitText does NOT replace an application-side selection (the
            // selection lives in the widget, the commit goes through the input
            // context), so without the erase the raw keys stayed and the word
            // was duplicated ("chao" + "chào" = "chàoao"). Backspace over a
            // selection deletes the selection in every toolkit, and it is the
            // transport already proven to land in these clients.
            Action::ForwardSelectLeft(n) => {
                debug_log::log(&format!("forward-select-left n={n} + erase"));
                for _ in 0..*n {
                    push_forward_keys(out, keymap::LEFT, LEFT_KEYCODE, keymap::SHIFT_MASK);
                }
                push_forward_keys(out, keymap::BACKSPACE, BACKSPACE_KEYCODE, 0);
            }
            Action::SelectLeft(n) => {
                if (t.select_left)(*n, bs_gap) {
                    debug_log::log(&format!("select rewrite: shift+left n={n}"));
                    let echoes = vec![keymap::LEFT; *n as usize];
                    engine.add_injected_nav(&echoes);
                    nav_since_commit += *n as usize;
                } else if engine.surrounding_data_ok() {
                    debug_log::log(&format!("select-left inject FAILED → protocol -{n}"));
                    let mut m = signal("DeleteSurroundingText");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                } else {
                    debug_log::log(&format!(
                        "select-left inject FAILED and no report: deletion dropped n={n}"
                    ));
                }
            }
            Action::PressDelete => {
                if (t.keys)(&[(crate::uinput::KEYCODE_DELETE, false)], 0) {
                    debug_log::log("select rewrite: delete");
                    engine.add_injected_nav(&[keymap::DELETE]);
                    nav_since_commit += 1;
                } else if (t.backspaces)(1, bs_gap) {
                    // Every toolkit deletes the current selection on
                    // Backspace, which is exactly what the Delete was for.
                    // The paired commit still has to wait for this key.
                    debug_log::log("press-delete FAILED → backspace over the selection");
                    injected_since_commit += 1;
                    injected_total += 1;
                } else {
                    debug_log::log("press-delete FAILED: selection stays selected");
                }
            }
            Action::Paste(text) => {
                if (t.paste)(text) {
                    // Length only: the word itself is what the user just
                    // typed, and the paste result is visible on screen.
                    debug_log::log(&format!("paste channel len={}", text.chars().count()));
                    // The Ctrl+V we injected comes back as a key event; forward it.
                    engine.add_injected_nav(&[keymap::V]);
                } else {
                    debug_log::log("paste channel FAILED → CommitText fallback");
                    // Same deferrals as a plain commit: this fallback usually
                    // follows injected Backspaces, and overtaking them makes
                    // the accent flash and revert.
                    push_commit(
                        engine,
                        out,
                        text.clone(),
                        &mut delete_failed,
                        &mut nav_since_commit,
                        &mut injected_since_commit,
                    );
                }
            }
            Action::PostDelay(ms) => {
                if *ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(*ms));
                }
            }
            Action::Forward | Action::Consumed => {}
        }
    }
    injected_total
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
    out: &mut Vec<Message>,
    text: String,
    delete_failed: &mut bool,
    nav_since_commit: &mut usize,
    injected_since_commit: &mut u32,
) {
    if std::mem::take(delete_failed) {
        debug_log::log("commit suppressed: its rewrite delete was not deliverable");
    } else if *nav_since_commit > 0 {
        // A commit right after injected selection keys can land before the
        // app erased the selection.
        engine.defer_nav_commit(*nav_since_commit, text);
        *nav_since_commit = 0;
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

/// Fake transports for tests: every key press "succeeds", the clipboard
/// does not exist (Paste falls back to CommitText).
#[cfg(test)]
fn fake_transports<'a>(backspaces: &'a dyn Fn(u32, u64) -> bool) -> Transports<'a> {
    Transports {
        backspaces,
        keys: &|_, _| true,
        select_left: &|_, _| true,
        paste: &|_| false,
    }
}
