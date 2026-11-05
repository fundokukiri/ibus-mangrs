//! Hotkey parsing and matching, the two hotkey actions in the key path
//! (toggle Vietnamese, cycle mode), and the config mtime reload gate.

use super::*;

impl EngineHandler {
    /// `ProcessKeyEvent`: hot-reload the config, check the mode-cycle hotkey,
    /// then feed the key to the engine. Signals go into `out`.
    pub(super) fn process_key_call(&mut self, msg: &Message, out: &mut Vec<Message>) -> Message {
        let Ok((keyval, _keycode, state)) = msg.read3::<u32, u32, u32>() else {
            return msg.method_return().append1(false);
        };

        self.maybe_reload_config(out);

        // The mode-cycle hotkey is handled before any typing: it changes the
        // config and refreshes the panel menu.
        if self.try_toggle_hotkey(keyval, state, out) {
            return msg.method_return().append1(true);
        }
        if self.try_cycle_hotkey(keyval, state, out) {
            return msg.method_return().append1(true);
        }

        let mut engine = lock_engine(&self.engine);
        let consumed = process_key(&mut engine, keyval, state, out);
        msg.method_return().append1(consumed)
    }

    /// Cycle NonPreedit -> Preedit -> Surrounding -> NonPreedit when `keyval`
    /// with the configured modifiers matches `cycle_mode_hotkey`.
    /// Returns whether the key was consumed.
    /// Flip Vietnamese on/off when `keyval` with the configured modifiers
    /// matches `enabled_hotkey`. Same plumbing as the mode-cycle hotkey:
    /// save, apply, then the per-item panel update so the check mark follows.
    fn try_toggle_hotkey(&mut self, keyval: u32, state: u32, out: &mut Vec<Message>) -> bool {
        let Some((mods, expected)) = self.enabled_hotkey else {
            return false;
        };
        if !hotkey_matches(mods, expected, keyval, state) {
            return false;
        }

        let now = Instant::now();
        if self
            .last_toggle_fire
            .is_some_and(|t| now.duration_since(t) < HOTKEY_REPEAT_GAP)
        {
            return true; // autorepeat of the combo key: consume, do nothing
        }
        self.last_toggle_fire = Some(now);

        let mut config = Config::load();
        config.enabled = !config.enabled;
        if config.save().is_err() {
            return true; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.last_config_mtime = Config::path()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        update_property_group(&config, ibus_prop::KEY_ENABLED, out);
        true
    }

    fn try_cycle_hotkey(&mut self, keyval: u32, state: u32, out: &mut Vec<Message>) -> bool {
        let Some((mods, expected)) = self.cycle_hotkey else {
            return false;
        };
        if !hotkey_matches(mods, expected, keyval, state) {
            return false;
        }

        let now = Instant::now();
        if self
            .last_cycle_fire
            .is_some_and(|t| now.duration_since(t) < HOTKEY_REPEAT_GAP)
        {
            return true; // autorepeat of the combo key: consume, do nothing
        }
        self.last_cycle_fire = Some(now);

        let mut config = Config::load();
        config.mode = match config.mode {
            ModeConfig::NonPreedit => ModeConfig::Preedit,
            ModeConfig::Preedit => ModeConfig::Surrounding,
            ModeConfig::Surrounding => ModeConfig::Smooth,
            ModeConfig::Smooth => ModeConfig::SuperSmooth,
            ModeConfig::SuperSmooth => ModeConfig::Select,
            ModeConfig::Select => ModeConfig::ForwardKeyEvent,
            ModeConfig::ForwardKeyEvent => ModeConfig::ForwardSelect,
            ModeConfig::ForwardSelect => ModeConfig::Off,
            ModeConfig::Off => ModeConfig::DefaultTyping,
            ModeConfig::DefaultTyping => ModeConfig::NonPreedit,
        };
        if config.save().is_err() {
            return true; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.last_config_mtime = Config::path()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        update_property_group(&config, "display", out);
        true
    }

    /// Re-parse the cycle hotkey from an already-loaded config.
    pub(super) fn refresh_hotkey_cache(&mut self, config: &Config) {
        self.cycle_hotkey = parse_hotkey(&config.cycle_mode_hotkey).filter(|(mods, _)| *mods != 0);
        self.enabled_hotkey = parse_hotkey(&config.enabled_hotkey).filter(|(mods, _)| *mods != 0);
    }

    /// The focused window's WM_CLASS from the focus extension. The
    /// session-bus connection is created lazily and kept for the handler's
    /// lifetime; a short timeout keeps a dead extension off the focus path.
    /// Returns `None` when the extension is absent or names nothing (the
    /// caller then falls back to the X active window).
    pub(super) fn focus_extension_app(&mut self) -> Option<String> {
        if self.focus_ext.is_none() {
            // The engine inherits the session environment from ibus-daemon;
            // without a session bus there is no extension either.
            self.focus_ext = Connection::get_private(BusType::Session)
                .and_then(|conn| conn.register().map(|_| conn))
                .ok();
        }
        let conn = self.focus_ext.as_ref()?;
        // Test-only unwraps: the names are this file's own constants.
        let msg = Message::method_call(
            &BusName::new(FOCUS_EXT_NAME).unwrap(),
            &DbusPath::new(FOCUS_EXT_PATH).unwrap(),
            &Interface::new(FOCUS_EXT_IFACE).unwrap(),
            &Member::new("GetFocusedWindow").unwrap(),
        );
        let reply = conn.send_with_reply_and_block(msg, 300).ok()?;
        let (class, _title) = reply.read2::<String, String>().ok()?;
        (!class.is_empty()).then_some(class)
    }

    /// Re-apply the config if the file changed since the last check.
    fn maybe_reload_config(&mut self, out: &mut Vec<Message>) {
        if self.last_config_check.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_config_check = Instant::now();
        let mtime = Config::path()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
        if mtime.is_some() && mtime != self.last_config_mtime {
            self.last_config_mtime = mtime;
            let config = Config::load();
            debug_log::set_enabled(config.debug_log);
            self.refresh_hotkey_cache(&config);
            let mut engine = lock_engine(&self.engine);
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
/// Minimum gap between two fires of the same hotkey: key autorepeat
/// re-sends the combo tens of times a second while it is held, and a
/// deliberate second press is never this fast.
const HOTKEY_REPEAT_GAP: Duration = Duration::from_millis(250);

pub(crate) fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut key = None;
    for part in spec.split('+') {
        match part.trim().to_lowercase().as_str() {
            "control" | "ctrl" => mods |= keymap::CONTROL_MASK,
            "alt" => mods |= keymap::MOD1_MASK,
            "super" | "meta" => mods |= keymap::MOD4_MASK,
            "shift" => mods |= keymap::SHIFT_MASK,
            "space" => key = Some(0x20),
            f if f.len() == 2
                && f.starts_with('f')
                && f[1..].chars().all(|c| c.is_ascii_digit())
                && f[1..]
                    .parse::<u8>()
                    .map(|n| (1..=12).contains(&n))
                    .unwrap_or(false) =>
            {
                let n: u8 = f[1..].parse().unwrap();
                key = Some(0xffbe + (n as u32 - 1));
            }
            s if s.chars().count() == 1 => {
                let c = s.chars().next().unwrap();
                let lower = c.to_ascii_lowercase();
                key = Some(if lower.is_ascii_graphic() {
                    lower as u32
                } else {
                    return None;
                });
            }
            _ => return None,
        }
    }
    // A modifier-only combo ("Control+Shift") completes on the last
    // modifier: `key` stays 0 and matching keys off that. At least two
    // modifiers, or the hotkey would eat every press of a lone modifier.
    let key = match key {
        Some(k) => k,
        None if mods.count_ones() >= 2 => 0,
        _ => return None,
    };
    if mods == 0 || mods & !keymap::SHIFT_MASK == 0 {
        // A bare or Shift-only hotkey would eat normal typing.
        return None;
    }
    Some((mods, key))
}

/// Does this key event trigger the hotkey? `key == 0` marks a
/// modifier-only combo: the pressed key must be one of the combo's own
/// modifiers (either side), with every OTHER combo modifier already held —
/// the just-pressed modifier is not in the event state yet.
pub(super) fn hotkey_matches(mods: u32, key: u32, keyval: u32, state: u32) -> bool {
    // Releases never trigger: a press-release pair would fire the hotkey
    // twice.
    if state & keymap::RELEASE_MASK != 0 {
        return false;
    }
    let held = state & keymap::HOTKEY_STATE_MASK;
    if key == 0 {
        // Modifier-only combo: the pressed key must be one of the combo's
        // own modifiers (either side), and held-plus-pressed must be
        // exactly the combo — the just-pressed modifier may or may not be
        // in the event state yet, depending on the toolkit.
        let Some(pressed) = keymap::modifier_of_keyval(keyval) else {
            return false;
        };
        mods & pressed != 0 && held | pressed == mods
    } else {
        // Regular combo: exactly the combo's modifiers, no extras — and
        // case-insensitive, because Shift turns the event's keysym
        // uppercase ('Z' vs the stored 'z').
        keymap::keyval_to_lower(keyval) == keymap::keyval_to_lower(key) && held == mods
    }
}
