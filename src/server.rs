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

use dbus::ffidisp::{Connection, NameFlag};
use dbus::{Message, MessageType};

use crate::config::{CharsetConfig, Config, MethodConfig, ModeConfig};
use crate::debug_log::{self, brief};
use crate::engine::{Action, Engine};
use crate::ibus_prop::{self, Prop};
use crate::ibus_text::{plain_text, text_from_variant, text_variant, Attr};
use crate::keymap;
use crate::uinput::Uinput;

pub const BUS_NAME: &str = "org.freedesktop.IBus.mangrs";
pub const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
pub const ENGINE_PATH: &str = "/org/freedesktop/IBus/Engine/1";
pub const ENGINE_IFACE: &str = "org.freedesktop.IBus.Engine";
pub const FACTORY_IFACE: &str = "org.freedesktop.IBus.Factory";

/// Shared engine state: the handler mutates it while answering calls, the
/// main thread reads it when the GUI changes settings.
pub type SharedEngine = Arc<Mutex<Engine>>;

pub struct Server {
    conn: Connection,
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
            engine,
            last_config_check: Instant::now(),
            last_config_mtime: None,
        };
        handler.last_config_mtime = Config::path().and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        conn.replace_message_callback(Some(Box::new(move |conn, msg| {
            if msg.msg_type() != MessageType::MethodCall {
                return false;
            }
            match handler.handle(conn, &msg) {
                Some(()) => true,
                None => false,
            }
        })));

        Ok(Self { conn })
    }

    /// Run the main loop until the bus goes away.
    pub fn run(&self) {
        // The filter callback does all the work; this only keeps libdbus
        // reading from the socket.
        for item in self.conn.iter(1000) {
            if matches!(item, dbus::ffidisp::ConnectionItem::Nothing) {
                continue;
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
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            if let Some(addr) = line.strip_prefix("IBUS_ADDRESS=") {
                return Ok(addr.trim().to_string());
            }
        }
    }
    Err("IBUS_ADDRESS not set and no registry file found; is ibus-daemon running?".into())
}

struct EngineHandler {
    engine: SharedEngine,
    /// Config file hot-reload: check the mtime at most once a second, on key
    /// events, so GUI/menu edits apply without a focus change or restart.
    last_config_check: Instant,
    last_config_mtime: Option<SystemTime>,
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
                send(conn, msg.method_return().append1(dbus::Path::new(ENGINE_PATH).unwrap()));
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
                        "FocusId" => msg.method_return().append1(dbus::arg::Variant(Box::new(true))),
                        "ActiveSurroundingText" => {
                            msg.method_return().append1(dbus::arg::Variant(Box::new(false)))
                        }
                        _ => msg.method_return().append1(dbus::arg::Variant(Box::new(0u32))),
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

        let mut engine = self.engine.lock().unwrap();

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
                if let Ok((text, cursor, _anchor)) =
                    msg.read3::<dbus::arg::Variant<Box<dyn dbus::arg::RefArg>>, u32, u32>()
                {
                    if let Some(s) = text_from_variant(&text) {
                        debug_log::log(&format!(
                            "surrounding cursor={cursor} tail={:?} text={:?}",
                            brief(engine.tail()),
                            brief(&s),
                        ));
                        engine.set_surrounding(s, cursor);
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
                // Pick up settings changed in the GUI since the last focus.
                let config = Config::load();
                debug_log::set_enabled(config.debug_log);
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
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "RequireSurroundingText")
                        .expect("valid signal"),
                );
                msg.method_return()
            }
            "FocusOut" | "Disable" | "FocusOutId" => {
                // Commits still waiting for their Backspace echoes: echoes will
                // not follow a focus change, so deliver them now (late but not
                // never).
                for text in engine.flush_deferred() {
                    out.push(
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                            .expect("valid signal")
                            .append1(plain_text(&text)),
                    );
                }
                let actions = engine.reset();
                emit(&mut engine, &actions, out);
                engine.set_enabled(false);
                engine.set_client(None);
                // Drop the context's buffer data: the next focus may be a
                // context whose buffer mutter never saw. Caps are KEPT — the
                // daemon only re-sends SetCapabilities when they actually
                // change, and on a quick unfocus/refocus of the same context
                // (1ms apart in the logs) they are never re-sent; zeroing
                // them here made every later rewrite take the broken
                // injection path ("xin chaà" session).
                engine.clear_buffer_state();
                debug_log::log("focus-out");
                msg.method_return()
            }
            "Reset" => {
                for text in engine.flush_deferred() {
                    out.push(
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                            .expect("valid signal")
                            .append1(plain_text(&text)),
                    );
                }
                let actions = engine.reset();
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

impl EngineHandler {
    /// `ProcessKeyEvent`: hot-reload the config, check the mode-cycle hotkey,
    /// then feed the key to the engine. Signals go into `out`.
    fn process_key_call(&mut self, msg: &Message, out: &mut Vec<Message>) -> Message {
        let Ok((keyval, _keycode, state)) = msg.read3::<u32, u32, u32>() else {
            return msg.method_return().append1(false);
        };

        self.maybe_reload_config(out);

        // The mode-cycle hotkey is handled before any typing: it changes the
        // config and refreshes the panel menu.
        if self.try_cycle_hotkey(keyval, state, out) {
            return msg.method_return().append1(true);
        }

        let mut engine = self.engine.lock().unwrap();
        let consumed = process_key(&mut engine, keyval, state, out);
        msg.method_return().append1(consumed)
    }

    /// Cycle NonPreedit -> Preedit -> Surrounding -> NonPreedit when `keyval`
    /// with the configured modifiers matches `cycle_mode_hotkey`.
    /// Returns whether the key was consumed.
    fn try_cycle_hotkey(&mut self, keyval: u32, state: u32, out: &mut Vec<Message>) -> bool {
        let config = Config::load();
        if config.cycle_mode_hotkey.is_empty() {
            return false;
        }
        let Some((mods, expected)) = parse_hotkey(&config.cycle_mode_hotkey) else {
            return false;
        };
        if mods == 0 || keyval != expected || state & mods != mods {
            return false;
        }

        let mut config = Config::load();
        config.mode = match config.mode {
            ModeConfig::NonPreedit => ModeConfig::Preedit,
            ModeConfig::Preedit => ModeConfig::Surrounding,
            ModeConfig::Surrounding => ModeConfig::Smooth,
            ModeConfig::Smooth => ModeConfig::SuperSmooth,
            ModeConfig::SuperSmooth => ModeConfig::Select,
            ModeConfig::Select => ModeConfig::Off,
            ModeConfig::Off => ModeConfig::DefaultTyping,
            ModeConfig::DefaultTyping => ModeConfig::NonPreedit,
        };
        if config.save().is_err() {
            return true; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = self.engine.lock().unwrap();
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.last_config_mtime = Config::path().and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        register_properties(&config, out);
        true
    }

    /// Re-apply the config if the file changed since the last check.
    fn maybe_reload_config(&mut self, out: &mut Vec<Message>) {
        if self.last_config_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_config_check = Instant::now();
        let mtime = Config::path().and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        if mtime.is_some() && mtime != self.last_config_mtime {
            self.last_config_mtime = mtime;
            let config = Config::load();
            debug_log::set_enabled(config.debug_log);
            let mut engine = self.engine.lock().unwrap();
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
            register_properties(&config, out);
        }
    }
}

/// Parse a hotkey spec like `Control+Alt+v` into (modifier mask, keysym).
/// Returns `None` for specs it does not understand. The key may be a single
/// letter/digit, `space`, or `f1`..`f12`.
fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut key = None;
    for part in spec.split('+') {
        match part.trim().to_lowercase().as_str() {
            "control" | "ctrl" => mods |= keymap::CONTROL_MASK,
            "alt" => mods |= keymap::MOD1_MASK,
            "super" | "meta" => mods |= keymap::MOD4_MASK,
            "shift" => mods |= keymap::SHIFT_MASK,
            "space" => key = Some(0x20),
            f if f.len() == 2 && f.starts_with('f') && f[1..].chars().all(|c| c.is_ascii_digit())
                && f[1..].parse::<u8>().map(|n| (1..=12).contains(&n)).unwrap_or(false) =>
            {
                let n: u8 = f[1..].parse().unwrap();
                key = Some(0xffbe + (n as u32 - 1));
            }
            s if s.chars().count() == 1 => {
                let c = s.chars().next().unwrap();
                let lower = c.to_ascii_lowercase();
                key = Some(if lower.is_ascii_graphic() { lower as u32 } else { return None });
            }
            _ => return None,
        }
    }
    let key = key?;
    if mods == 0 || mods & !keymap::SHIFT_MASK == 0 {
        // A bare or Shift-only hotkey would eat normal typing.
        return None;
    }
    Some((mods, key))
}

/// Send the full property list to ibus-daemon. `RegisterProperties` is a
/// signal `(v)`, not a method: the daemon's engine proxy relays it to the
/// panel (GNOME Shell's input-source popup).
fn register_properties(config: &Config, out: &mut Vec<Message>) {
    let props = ibus_prop::properties(config);
    out.push(
        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "RegisterProperties")
            .expect("valid signal")
            .append1(ibus_prop::prop_list_variant(&props)),
    );
}

/// Handle a panel click. The daemon sends `PropertyActivate(s name, u state)`.
///
/// GNOME Shell sends one call per item of a clicked radio group: the picked
/// item with state CHECKED and every sibling with state UNCHECKED (in menu
/// order, `keyboard.js::_buildPropSubMenu`). The UNCHECKED calls must be
/// ignored — honoring them makes the last sibling of the group always win
/// (`VIQR` for the method menu, `preedit` for the display menu).
fn activate_property(engine: &mut Engine, name: &str, state: u32, out: &mut Vec<Message>) {
    if name == ibus_prop::KEY_ABOUT {
        open_repo_page();
        return;
    }
    if name == ibus_prop::KEY_SHORTCUTS {
        open_settings("general");
        return;
    }

    // Radio items arrive as `group:value`; only the CHECKED call counts.
    if let Some((group, value)) = name.split_once(':') {
        if state != ibus_prop::STATE_CHECKED {
            return;
        }
        let mut config = Config::load();
        match group {
            "charset" => match CharsetConfig::from_key(value) {
                Some(c) => config.charset = c,
                None => return,
            },
            "method" => match MethodConfig::from_key(value) {
                Some(m) => config.method = m,
                None => return,
            },
            "display" => match ModeConfig::from_key(value) {
                Some(m) => config.mode = m,
                None => return,
            },
            _ => return,
        }
        // Do not move the check mark if the setting could not be persisted.
        if config.save().is_err() {
            return;
        }
        let actions = engine.apply_config(&config);
        engine.set_enabled(config.enabled);
        emit(engine, &actions, out);
        // Radios need their whole group resent so the old one loses the check.
        update_group(&ibus_prop::properties(&config), group, out);
        return;
    }

    // Toggles carry their new state directly.
    let checked = state == ibus_prop::STATE_CHECKED;
    let mut config = Config::load();
    match name {
        ibus_prop::KEY_SPELL_CHECK => config.auto_correct = checked,
        ibus_prop::KEY_ENABLED => config.enabled = checked,
        ibus_prop::KEY_MACROS => config.macro_enabled = checked,
        ibus_prop::KEY_MACRO_OFF => config.macro_in_off_mode = checked,
        ibus_prop::KEY_AUTO_CAPITALIZE => config.auto_capitalize = checked,
        ibus_prop::KEY_DICTIONARY => config.dictionary_enabled = checked,
        ibus_prop::KEY_FREE_TONE => config.free_tone_marking = checked,
        ibus_prop::KEY_MODERN_TONE => config.modern_tone_style = checked,
        ibus_prop::KEY_HIDE_UNDERLINE => config.hide_underline = checked,
        ibus_prop::KEY_AUTO_RESTORE => config.auto_restore = checked,
        ibus_prop::KEY_EM_DASH => config.em_dash = checked,
        ibus_prop::KEY_DOUBLE_SPACE => config.double_space_period = checked,
        ibus_prop::KEY_MACRO_EDITOR => {
            open_settings("macros");
            return;
        }
        _ => return,
    }
    if config.save().is_err() {
        return;
    }
    let actions = engine.apply_config(&config);
    engine.set_enabled(config.enabled);
    emit(engine, &actions, out);
    update_group(&ibus_prop::properties(&config), name, out);
}

fn update_group(props: &[Prop], group: &str, out: &mut Vec<Message>) {
    for prop in props {
        if prop.key == group {
            if prop.sub.is_empty() {
                push_update(prop, out);
            } else {
                for child in &prop.sub {
                    push_update(child, out);
                }
            }
        }
        update_group(&prop.sub, group, out);
    }
}

fn push_update(prop: &Prop, out: &mut Vec<Message>) {
    out.push(
        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "UpdateProperty")
            .expect("valid signal")
            .append1(prop.variant()),
    );
}

/// The "Phím tắt" item opens the GTK4 settings window, same as `-gui`.
fn open_settings(page: &str) {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::process::Command::new(exe)
        .arg("-gui")
        .arg(format!("--page={page}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Open the project's GitHub repository in the default browser (the
/// "MăngRS <version>" entry at the top of the menu).
fn open_repo_page() {
    let _ = std::process::Command::new("xdg-open")
        .arg(ibus_prop::HOMEPAGE)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Handle one key event, pushing resulting signals into `out`.
/// Returns whether the engine consumed the key.
fn process_key(engine: &mut Engine, keyval: u32, state: u32, out: &mut Vec<Message>) -> bool {
    debug_log::log(&format!("key keyval={keyval:#x} state={state:#x}"));
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
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                        .expect("valid signal")
                        .append1(plain_text(&text)),
                );
            }
            return false;
        }
        if engine.take_stale_injection(Duration::from_millis(150)) || !engine.pending_echoes() {
            for text in engine.flush_deferred() {
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                        .expect("valid signal")
                        .append1(plain_text(&text)),
                );
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
        if engine.take_stale_injection(Duration::from_millis(150)) {
            for text in engine.flush_deferred() {
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                        .expect("valid signal")
                        .append1(plain_text(&text)),
                );
            }
        }
        // An echo of a Backspace we injected ourselves: pass it to the
        // application — that key press IS the deletion mechanism. Once the
        // last echo of a rewrite is forwarded, its deferred commit can go.
        if engine.take_injected_echo() {
            for text in engine.note_echo_forwarded() {
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                        .expect("valid signal")
                        .append1(plain_text(&text)),
                );
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
    if engine.take_stale_injection(Duration::from_millis(150)) || !engine.pending_echoes() {
        for text in engine.flush_deferred() {
            out.push(
                Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                    .expect("valid signal")
                    .append1(plain_text(&text)),
            );
        }
    }

    // Arrows, Tab, Escape, Delete... end the word and belong to the app.
    if keymap::is_editing_key(keyval) {
        // Echoes of keys the uinput modes injected (Select's Shift+Left and
        // Delete): forward them so the selection/erase actually happens.
        if let Some(commits) = engine.take_nav_echo(keyval) {
            for text in commits {
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                        .expect("valid signal")
                        .append1(plain_text(&text)),
                );
            }
            return false;
        }
        if engine.take_stale_injection(Duration::from_millis(150)) || !engine.pending_echoes() {
            for text in engine.flush_deferred() {
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                        .expect("valid signal")
                        .append1(plain_text(&text)),
                );
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
            debug_log::log("navigation key consumed: preedit commit must land before the caret moves");
            return true;
        }
        return false;
    }

    let Some(c) = keymap::keysym_to_char(keyval) else { return false };
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
    for action in actions {
        match action {
            Action::Commit(text) => {
                if nav_since_commit > 0 {
                    // The same transport race as the Backspace deferral below:
                    // a commit right after injected selection keys can land
                    // before the app erased the selection.
                    engine.defer_nav_commit(nav_since_commit, text.clone());
                    nav_since_commit = 0;
                } else if injected_since_commit > 0 {
                    // The injected Backspaces travel through the compositor
                    // while this commit would go straight over D-Bus — a
                    // synchronous commit here can land BEFORE the deletions
                    // and the echoing Backspace then deletes the freshly
                    // committed word ("the accent flashes and reverts").
                    // Defer: the server flushes the commit once the last
                    // Backspace echo has been forwarded to the client.
                    engine.defer_commit(injected_since_commit, text.clone());
                    injected_since_commit = 0;
                } else {
                    out.push(
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                            .expect("valid signal")
                            .append1(plain_text(text)),
                    );
                }
            }
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
                } else {
                    debug_log::log(&format!("delete-before inject FAILED → protocol=-{n}"));
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                }
            }
            Action::DeleteBefore(n) => {
                if !engine.surrounding_data_ok() {
                    // Defensive: the engine gates deletes on a surrounding
                    // report; never let one slip to a buffer mutter never saw.
                    debug_log::log(&format!("delete-before protocol DROPPED (no report) n={n}"));
                } else {
                    debug_log::log(&format!("delete-before protocol=-{n}"));
                    // Signature is (iu): a plain signed offset, not a variant.
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
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
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
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
                    debug_log::log(&format!("delete-surrounding protocol offset={offset} n={n}"));
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
                    m = m.append1(*offset).append1(*n);
                    out.push(m);
                }
            }
            Action::RewriteByBackspace { backspaces, commit } => {
                if (t.backspaces)(*backspaces, bs_gap) {
                    debug_log::log(&format!("rewrite-by-backspace n={backspaces}"));
                    injected_since_commit += *backspaces;
                    injected_total += *backspaces;
                } else {
                    debug_log::log(&format!(
                        "rewrite-by-backspace inject FAILED → protocol delete={backspaces}"
                    ));
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
                    m = m.append1(-(*backspaces as i32)).append1(*backspaces);
                    out.push(m);
                    out.push(
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                            .expect("valid signal")
                            .append1(plain_text(commit)),
                    );
                }
            }
            Action::UpdatePreedit { text, cursor } => {
                let len = text.chars().count() as u32;
                let attrs: &[Attr] = if hide_underline { &[] } else { &[Attr::underline(0, len)] };
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "UpdatePreeditText")
                        .expect("valid signal")
                        .append1(text_variant(text, attrs))
                        .append1(*cursor)
                        .append1(true)
                        .append1(0u32), // IBUS_ENGINE_PREEDIT_CLEAR
                );
            }
            Action::HidePreedit => {
                out.push(
                    Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "UpdatePreeditText")
                        .expect("valid signal")
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
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
                    m = m.append1(-(*n as i32)).append1(*n);
                    out.push(m);
                } else {
                    debug_log::log(&format!(
                        "smooth rewrite inject FAILED and no report: deletion dropped n={n}"
                    ));
                }
            }
            Action::SelectLeft(n) => {
                if (t.select_left)(*n, bs_gap) {
                    debug_log::log(&format!("select rewrite: shift+left n={n}"));
                    let echoes = vec![keymap::LEFT; *n as usize];
                    engine.add_injected_nav(&echoes);
                    nav_since_commit += *n as usize;
                } else if engine.surrounding_data_ok() {
                    debug_log::log(&format!("select-left inject FAILED → protocol -{n}"));
                    let mut m =
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "DeleteSurroundingText")
                            .expect("valid signal");
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
                } else {
                    debug_log::log("press-delete FAILED: selection stays selected");
                }
            }
            Action::Paste(text) => {
                if (t.paste)(text) {
                    debug_log::log(&format!("paste channel: {:?}", crate::debug_log::brief(text)));
                    // The Ctrl+V we injected comes back as a key event; forward it.
                    engine.add_injected_nav(&[keymap::V]);
                } else {
                    debug_log::log("paste channel FAILED → CommitText fallback");
                    out.push(
                        Message::new_signal(ENGINE_PATH, ENGINE_IFACE, "CommitText")
                            .expect("valid signal")
                            .append1(plain_text(text)),
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

/// Send a reply or signal. A failed send means the peer is gone; there is
/// nothing useful to do about it from inside the filter.
fn send(conn: &Connection, msg: Message) {
    let _ = conn.send(msg);
}

/// Backspace presses needed to delete `n` chars at `offset` before the cursor.
fn presses(offset: i32) -> u32 {
    (-offset as i64).max(0) as u32
}

/// Record a seen client prgname in `~/.cache/ibus-mangrs/clients` so the GUI can
/// offer it as an App Rules target. Best effort; keeps the newest 50.
fn note_client(msg: &Message) {
    let Ok((_, client)) = msg.read2::<String, String>() else { return };
    let Some(prgname) = client.split_once(':').map(|(_, p)| p.to_string()) else { return };
    if prgname == "fake" || prgname.is_empty() {
        return;
    }
    let Some(dir) = dirs::cache_dir().map(|d| d.join("ibus-mangrs")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("clients");
    let mut names: Vec<String> = std::fs::read_to_string(&path)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    names.retain(|n| n != &prgname);
    names.insert(0, prgname);
    names.truncate(50);
    let _ = std::fs::write(path, names.join("\n"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bamboo_wrapper::Method;
    use crate::engine::Mode;
    use dbus::arg::{RefArg, Variant};

    #[test]
    fn parse_hotkey_accepts_modifier_combinations() {
        let (mods, key) = parse_hotkey("Control+Alt+v").expect("hotkey");
        assert_eq!(key, 0x76); // 'v'
        assert_eq!(mods, keymap::CONTROL_MASK | keymap::MOD1_MASK);

        let (mods, key) = parse_hotkey("Ctrl+Shift+space").expect("hotkey");
        assert_eq!(key, 0x20);
        assert_eq!(mods, keymap::CONTROL_MASK | keymap::SHIFT_MASK);

        let (mods, key) = parse_hotkey("Super+F5").expect("hotkey");
        assert_eq!(key, 0xffbe + 4);
        assert_eq!(mods, keymap::MOD4_MASK);
    }

    #[test]
    fn parse_hotkey_rejects_dangerous_or_bad_specs() {
        // A bare letter would swallow normal typing.
        assert_eq!(parse_hotkey("v"), None);
        // Shift-only too: capitals go through Shift.
        assert_eq!(parse_hotkey("Shift+v"), None);
        assert_eq!(parse_hotkey(""), None);
        assert_eq!(parse_hotkey("NotAModifier+q"), None);
    }

    fn emitted(actions: &[Action]) -> Vec<Message> {
        // An engine with surrounding data + capability uses the protocol
        // delete; anything else falls back to key injection (no D-Bus signal).
        let mut e = crate::engine::Engine::new(
            crate::bamboo_wrapper::Method::Telex,
            crate::engine::Mode::NonPreedit,
        );
        e.set_capabilities(32); // CAP_SURROUNDING_TEXT
        e.set_surrounding("word".into(), 4);
        let mut out = Vec::new();
        emit(&mut e, actions, &mut out);
        out
    }

    #[test]
    fn commit_text_is_one_ibus_text_variant() {
        // Regression: the old helper wrapped this variant in a second one, so
        // the daemon received a variant instead of an IBusText struct.
        let msgs = emitted(&[Action::Commit("tiếng".into())]);
        assert_eq!(&*msgs[0].member().unwrap(), "CommitText");
        let text = msgs[0].read1::<Variant<Box<dyn RefArg>>>().expect("IBusText variant");
        assert_eq!(text.0.signature().to_string(), "(sa{sv}sv)");
    }

    #[test]
    fn preedit_signal_is_vubu() {
        // Regression: the old helper produced (vvvv); ibus-daemon dropped it,
        // so preedit text only appeared when the word was committed.
        let msgs = emitted(&[Action::UpdatePreedit { text: "tiếng".into(), cursor: 5 }]);
        assert_eq!(&*msgs[0].member().unwrap(), "UpdatePreeditText");
        let (text, cursor, visible, mode) = msgs[0]
            .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
            .expect("(vubu) arguments");
        assert_eq!(text.0.signature().to_string(), "(sa{sv}sv)");
        assert_eq!((cursor, visible, mode), (5, true, 0));
    }

    #[test]
    fn hide_preedit_reuses_the_preedit_signal() {
        let msgs = emitted(&[Action::HidePreedit]);
        assert_eq!(&*msgs[0].member().unwrap(), "UpdatePreeditText");
        let (_, cursor, visible, _) = msgs[0]
            .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
            .expect("(vubu) arguments");
        assert_eq!((cursor, visible), (0, false));
    }

    #[test]
    fn hide_underline_drops_the_underline_attribute() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config { hide_underline: true, ..Config::default() });
        let mut out = Vec::new();
        emit(&mut e, &[Action::UpdatePreedit { text: "tiếng".into(), cursor: 5 }], &mut out);
        let (text, ..) = out[0]
            .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
            .expect("(vubu) arguments");
        // No IBusAttribute inside the attr list.
        let rendered = format!("{:?}", text.0);
        assert!(!rendered.contains("IBusAttribute"), "{rendered}");
    }

    #[test]
    fn delete_surrounding_text_is_plain_int_uint() {
        let msgs = emitted(&[Action::DeleteBefore(3)]);
        assert_eq!(msgs[0].read2::<i32, u32>().unwrap(), (-3, 3));
    }

    #[test]
    fn no_surrounding_data_routes_deletes_to_injection() {
        // A client without surrounding data must never see a protocol
        // DeleteSurroundingText (mutter segfaults on contexts whose buffer it
        // never saw — the search-bar logout); uinput is the delete transport.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(32); // CAP_SURROUNDING_TEXT, but no data reported
        e.set_uinput_available(true);
        let mut out = Vec::new();
        let bs = |_: u32, _: u64| true;
        let t = fake_transports(&bs);
        let injected = emit_with(
            &mut e,
            &[Action::DeleteBefore(2), Action::DeleteSurrounding { offset: -3, n: 1 }],
            &mut out,
            &t,
        );
        assert_eq!(injected, 5, "2 + |offset| Backspaces");
        assert!(
            out.iter()
                .all(|m| m.member().map(|mem| mem != "DeleteSurroundingText").unwrap_or(true)),
            "{:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unknown_client_routes_deletes_to_injection() {
        // Sublime Text / JetBrains XToolkit never send FocusInId. Their
        // injected keys do arrive, so a client-less context uses the same
        // Backspace transport instead of raw-forwarding the tone keys.
        let mut e = Engine::new(crate::bamboo_wrapper::Method::Telex, crate::engine::Mode::NonPreedit);
        e.set_capabilities(1);
        e.set_uinput_available(true);
        let mut out = Vec::new();
        let bs = |_: u32, _: u64| true;
        let t = fake_transports(&bs);
        let injected = emit_with(&mut e, &[Action::DeleteBefore(2)], &mut out, &t);
        assert_eq!(injected, 2);
        assert!(
            out.iter()
                .all(|m| m.member().map(|mem| mem != "DeleteSurroundingText").unwrap_or(true)),
            "{:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
    }

    #[test]
    fn commit_after_injection_waits_for_backspace_echoes() {
        // Regression: a synchronous CommitText after injected Backspaces
        // outruns the compositor round trip — the echo then deletes the
        // freshly committed word ("accent flashes and reverts"). The commit
        // must be deferred until the last echo is forwarded.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_uinput_available(true); // no surrounding data → injection
        let mut out = Vec::new();
        let bs = |_: u32, _: u64| true;
        let t = fake_transports(&bs);
        let injected = emit_with(
            &mut e,
            &[Action::DeleteBefore(1), Action::Commit("ế".into())],
            &mut out,
            &t,
        );
        e.add_injected(injected);
        assert_eq!(injected, 1);
        assert!(
            !out.iter().any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
            "commit must be deferred, got {:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
        assert!(e.has_deferred());
        // The single Backspace echo is forwarded → the commit is ready.
        assert!(e.take_injected_echo());
        let ready = e.note_echo_forwarded();
        assert_eq!(ready, vec!["ế".to_string()]);
        assert!(!e.has_deferred());
    }

    #[test]
    fn commit_after_selection_waits_for_nav_echoes() {
        // The Select rewrite injects Shift+Left presses and a Delete; a
        // synchronous commit would land before the app erased the selection.
        // The commit must wait until every injected nav-key echo has been
        // forwarded, then flush.
        let mut e = Engine::new(Method::Telex, Mode::Select);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_uinput_available(true);
        e.set_surrounding(String::new(), 0);
        let mut out = Vec::new();
        let bs = |_: u32, _: u64| true;
        let t = fake_transports(&bs);
        emit_with(
            &mut e,
            &[
                Action::SelectLeft(3),
                Action::PressDelete,
                Action::Commit("việt".into()),
            ],
            &mut out,
            &t,
        );
        assert!(
            !out.iter().any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
            "commit must be deferred, got {:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
        assert!(e.has_deferred());
        assert!(e.pending_nav_echoes());
        // The three Left echoes come back, then the Delete echo: the commit
        // is only ready after the last one.
        for _ in 0..3 {
            assert!(e.take_nav_echo(keymap::LEFT).unwrap().is_empty());
        }
        assert!(e.take_nav_echo(keymap::DELETE).unwrap() == vec!["việt".to_string()]);
        assert!(!e.has_deferred());
        assert!(!e.pending_nav_echoes());
    }

    #[test]
    fn paste_failure_falls_back_to_commit() {
        // Without a clipboard tool the Paste transport fails; the word must
        // still reach the application (CommitText fallback).
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_uinput_available(true);
        let mut out = Vec::new();
        let bs = |_: u32, _: u64| true;
        let t = fake_transports(&bs);
        emit_with(&mut e, &[Action::Paste("việt".into())], &mut out, &t);
        assert!(
            out.iter().any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
            "{:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
    }

    #[test]
    fn forced_flush_delivers_lost_echo_commits() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.defer_commit(2, "word".into());
        assert!(e.has_deferred());
        // One echo arrives, one is lost: a focus change forces delivery.
        e.note_echo_forwarded();
        assert!(e.has_deferred(), "threshold not reached yet");
        assert_eq!(e.flush_deferred(), vec!["word".to_string()]);
        assert!(!e.has_deferred());
    }

    #[test]
    fn first_navigation_key_with_pending_preedit_is_swallowed_on_relays() {
        // The preedit commit and the forwarded key travel different
        // transports on mutter-relayed contexts; a burst of arrows used to
        // move the caret before the commit landed ("em tên tùng" → "etùngm
        // tên"). The first navigation key is consumed so the commit lands
        // while the caret is still in place.
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        e.set_client(Some("gnome-shell".into()));
        e.set_capabilities(0b101001);
        for c in "tung".chars() {
            process_key(&mut e, c as u32, 0, &mut Vec::new());
        }
        let mut out = Vec::new();
        assert!(process_key(&mut e, 0xff51, 0, &mut out), "Left must be consumed");
        assert!(
            out.iter().any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
            "the pending word must be committed: {:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
        // The composition is gone: the next arrow is forwarded normally.
        let mut out = Vec::new();
        assert!(!process_key(&mut e, 0xff51, 0, &mut out));
    }

    #[test]
    fn first_navigation_key_commits_preedit_even_after_a_capability_flip() {
        // The daemon flips capabilities mid-word. The preedit is already on
        // screen (word-mode lock), so a navigation key must still commit it
        // and swallow that one press to keep it from landing at the moved
        // caret.
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        e.set_client(Some("gnome-shell".into()));
        e.set_capabilities(0b100001); // PREEDIT|SURROUNDING
        for c in "tung".chars() {
            process_key(&mut e, c as u32, 0, &mut Vec::new());
        }
        e.set_capabilities(0b100000); // preedit bit gone, preedit still visible
        let mut out = Vec::new();
        assert!(process_key(&mut e, 0xff51, keymap::SHIFT_MASK, &mut out));
        assert!(
            out.iter().any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
            "the visible preedit must be committed: {:?}",
            out.iter().map(|m| m.member().map(|m| m.to_string())).collect::<Vec<_>>()
        );
    }

    #[test]
    fn navigation_key_forwards_normally_without_pending_preedit() {
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        e.set_client(Some("gnome-shell".into()));
        e.set_capabilities(0b101001);
        let mut out = Vec::new();
        assert!(!process_key(&mut e, 0xff51, 0, &mut out));
    }

    #[test]
    fn in_process_clients_keep_navigation_keys_forwarded() {
        // Only relayed contexts need the swallow: in-process IM clients
        // apply the commit and the key in order themselves.
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(0b101001);
        for c in "tung".chars() {
            process_key(&mut e, c as u32, 0, &mut Vec::new());
        }
        let mut out = Vec::new();
        assert!(!process_key(&mut e, 0xff51, 0, &mut out));
    }
}
