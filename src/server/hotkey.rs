//! Hotkey parsing and matching, the shortcut registry with its actions in
//! the key path (toggle Vietnamese, cycle mode), and the config mtime
//! reload gate.

use super::*;

/// What a keyboard shortcut does. One row in the "Phím tắt" settings page
/// and one entry in the registry each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HotkeyAction {
    /// Flip `config.enabled` — the Vietnamese on/off switch.
    ToggleEnabled,
    /// Cycle the display mode list.
    CycleMode,
    /// Cycle the charset list.
    CycleCharset,
    /// Cycle the typing method list.
    CycleMethod,
    /// Open the per-app rule picker for the focused application.
    AppRulePicker,
    /// Open the emoji picker window (`-emoji`).
    OpenEmoji,
    /// Flip one quick toggle, by the panel registry's key.
    QuickToggle(&'static str),
    /// Open the settings window on one page.
    OpenSettings(&'static str),
}

/// A parsed shortcut: modifier mask, keysym, action.
pub(super) type HotkeyBinding = (u32, u32, HotkeyAction);

/// The shortcut registry from a config: one row per assigned hotkey, in
/// page order. New shortcuts add a config field, a row here and a GUI row.
pub(super) fn hotkey_registry(config: &Config) -> Vec<HotkeyBinding> {
    let mut out = Vec::new();
    if let Some((mods, key)) = parse_hotkey(&config.enabled_hotkey).filter(|(mods, _)| *mods != 0) {
        out.push((mods, key, HotkeyAction::ToggleEnabled));
    }
    if let Some((mods, key)) =
        parse_hotkey(&config.cycle_mode_hotkey).filter(|(mods, _)| *mods != 0)
    {
        out.push((mods, key, HotkeyAction::CycleMode));
    }
    if let Some((mods, key)) = parse_hotkey(&config.app_rule_hotkey).filter(|(mods, _)| *mods != 0)
    {
        out.push((mods, key, HotkeyAction::AppRulePicker));
    }
    if let Some((mods, key)) = parse_hotkey(&config.emoji_hotkey).filter(|(mods, _)| *mods != 0) {
        out.push((mods, key, HotkeyAction::OpenEmoji));
    }
    // The user's own shortcuts, after the four built-ins: the same parse,
    // the action key picked from the fixed table (quick toggles reuse the
    // panel registry's keys). Unknown actions or unusable specs register
    // nothing; a spec colliding with an earlier entry loses (the first
    // match fires).
    for custom in &config.custom_hotkeys {
        let Some(action) = (match custom.action.as_str() {
            "enabled" => Some(HotkeyAction::ToggleEnabled),
            "mode" => Some(HotkeyAction::CycleMode),
            "charset" => Some(HotkeyAction::CycleCharset),
            "method" => Some(HotkeyAction::CycleMethod),
            "rule" => Some(HotkeyAction::AppRulePicker),
            "emoji" => Some(HotkeyAction::OpenEmoji),
            "macro_editor" => Some(HotkeyAction::OpenSettings("macros")),
            "settings" => Some(HotkeyAction::OpenSettings("general")),
            "about" => Some(HotkeyAction::OpenSettings("about")),
            other => ibus_prop::quick_toggle(other).map(|t| HotkeyAction::QuickToggle(t.key)),
        }) else {
            continue;
        };
        if let Some((mods, key)) = parse_hotkey(&custom.spec).filter(|(mods, _)| *mods != 0) {
            out.push((mods, key, action));
        }
    }
    out
}

impl EngineHandler {
    /// `ProcessKeyEvent`: hot-reload the config, check the shortcut
    /// registry, then feed the key to the engine. Signals go into `out`.
    pub(super) fn process_key_call(&mut self, msg: &Message, out: &mut Vec<Message>) -> Message {
        let Ok((keyval, _keycode, state)) = msg.read3::<u32, u32, u32>() else {
            return msg.method_return().append1(false);
        };

        self.maybe_reload_config(out);

        // Shortcuts are handled before any typing: they change the config
        // and refresh the panel state.
        let (skip_menu_keys, consumed) = self.try_hotkey(keyval, state, out);
        if consumed {
            return msg.method_return().append1(true);
        }

        // The second tap of a Shift+punctuation combo just closed the menus
        // the first tap opened — the key belongs to the app, so the menu
        // key handlers must not see it either.
        if !skip_menu_keys {
            // While the app-rule picker is up, every key belongs to the
            // menu: navigate, pick, or close-and-type-on.
            if self.picker.is_some() && self.picker_key(keyval, state, out) {
                return msg.method_return().append1(true);
            }

            // Same for the emoji menu: digits pick, letters search, other
            // keys close it and type on.
            if self.emoji_picker.is_some() && self.emoji_picker_key(keyval, state, out) {
                return msg.method_return().append1(true);
            }
        }

        let mut engine = lock_engine(&self.engine);
        let consumed = process_key(&mut engine, keyval, state, out);
        msg.method_return().append1(consumed)
    }

    /// Run the first registry entry matching this key event. Returns
    /// `(skip_menu_keys, consumed)`. A Shift+punctuation combo is the only
    /// shape that collides with typing: the single tap fires (and the menu
    /// stays up), while a second tap within the repeat gap closes the menu
    /// again and passes the key through — "Shift+~+~" types "~".
    fn try_hotkey(&mut self, keyval: u32, state: u32, out: &mut Vec<Message>) -> (bool, bool) {
        let Some(&(mods, key, action)) = self
            .hotkeys
            .iter()
            .find(|(mods, key, _)| hotkey_matches(*mods, *key, keyval, state))
        else {
            return (false, false);
        };
        // Autorepeat of the combo key: consume, do nothing (a deliberate
        // second press is never this fast) — unless it is the typing tap of
        // a Shift+punctuation combo.
        let now = Instant::now();
        if self
            .last_fire
            .iter()
            .any(|(a, t)| *a == action && now.duration_since(*t) < HOTKEY_REPEAT_GAP)
        {
            if hotkey_passes_key(mods, key) {
                let had_menu = self.picker.take().is_some();
                let had_menu = had_menu || self.emoji_picker.take().is_some();
                if had_menu {
                    out.push(signal("HideLookupTable"));
                    out.push(signal("HideAuxiliaryText"));
                }
                return (true, false);
            }
            return (false, true);
        }
        match self.last_fire.iter_mut().find(|(a, _)| *a == action) {
            Some((_, t)) => *t = now,
            None => self.last_fire.push((action, now)),
        }
        match action {
            HotkeyAction::ToggleEnabled => self.toggle_enabled(out),
            HotkeyAction::CycleMode => self.cycle_mode(out),
            HotkeyAction::CycleCharset => self.cycle_charset(out),
            HotkeyAction::CycleMethod => self.cycle_method(out),
            HotkeyAction::AppRulePicker => self.open_app_rule_picker(out),
            HotkeyAction::OpenEmoji => self.open_emoji_picker(out),
            HotkeyAction::QuickToggle(key) => self.quick_toggle_key(key, out),
            HotkeyAction::OpenSettings(page) => prop::open_settings(page),
        }
        (false, true)
    }

    /// Cycle the charset list: save, apply, per-item panel update.
    fn cycle_charset(&mut self, out: &mut Vec<Message>) {
        let mut config = Config::load();
        let Some(next) =
            cycle_next(ibus_prop::CHARSETS, config.charset.key()).and_then(CharsetConfig::from_key)
        else {
            return;
        };
        config.charset = next;
        if config.save().is_err() {
            return; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.rebaseline_config_mtime();
        update_property_group(&config, "charset", out);
    }

    /// Cycle the typing method list: save, apply, per-item panel update.
    fn cycle_method(&mut self, out: &mut Vec<Message>) {
        let mut config = Config::load();
        let Some(next) =
            cycle_next(ibus_prop::METHODS, config.method.key()).and_then(MethodConfig::from_key)
        else {
            return;
        };
        config.method = next;
        if config.save().is_err() {
            return; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.rebaseline_config_mtime();
        update_property_group(&config, "method", out);
    }

    /// Flip one quick toggle from a hotkey: save, apply, panel update —
    /// the same round a panel click does, state read from the config.
    fn quick_toggle_key(&mut self, key: &str, out: &mut Vec<Message>) {
        let Some(toggle) = ibus_prop::quick_toggle(key) else {
            return;
        };
        let mut config = Config::load();
        let checked = !(toggle.get)(&config);
        (toggle.set)(&mut config, checked);
        if config.save().is_err() {
            return; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.rebaseline_config_mtime();
        update_property_group(&config, key, out);
    }

    /// Flip Vietnamese on/off: save, apply, then the per-item panel update
    /// so the check mark follows.
    fn toggle_enabled(&mut self, out: &mut Vec<Message>) {
        let mut config = Config::load();
        config.enabled = !config.enabled;
        if config.save().is_err() {
            return; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.rebaseline_config_mtime();
        update_property_group(&config, ibus_prop::KEY_ENABLED, out);
    }

    /// Cycle the display mode list: save, apply, per-item panel update.
    fn cycle_mode(&mut self, out: &mut Vec<Message>) {
        let mut config = Config::load();
        config.mode = match config.mode {
            ModeConfig::NonPreedit => ModeConfig::Preedit,
            ModeConfig::Preedit => ModeConfig::Surrounding,
            ModeConfig::Surrounding => ModeConfig::Flow,
            ModeConfig::Flow => ModeConfig::Turbo,
            ModeConfig::Turbo => ModeConfig::Select,
            ModeConfig::Select => ModeConfig::ForwardKeyEvent,
            ModeConfig::ForwardKeyEvent => ModeConfig::ForwardSelect,
            ModeConfig::ForwardSelect => ModeConfig::Off,
            ModeConfig::Off => ModeConfig::NonPreedit,
        };
        if config.save().is_err() {
            return; // consumed anyway; do not type with a hotkey press
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
        }
        self.rebaseline_config_mtime();
        update_property_group(&config, "display", out);
    }

    /// The engine's own save must not look like an external edit to the
    /// mtime watcher.
    fn rebaseline_config_mtime(&mut self) {
        self.last_config_mtime = Config::path()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok());
    }

    /// Rebuild the shortcut registry from an already-loaded config. The
    /// debounce history goes too: the specs may have changed.
    pub(super) fn refresh_hotkey_cache(&mut self, config: &Config) {
        self.hotkeys = hotkey_registry(config);
        self.last_fire.clear();
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
    /// Runs on key events and on the run loop's own passes (a GUI edit must
    /// reach the panel without waiting for typing). The refresh is per-item
    /// `UpdateProperty`, not a full `RegisterProperties` — GNOME applies
    /// only the former mid-session (see prop.rs).
    pub(super) fn maybe_reload_config(&mut self, out: &mut Vec<Message>) {
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
            debug_log::set_text_enabled(config.debug_log_text);
            self.refresh_hotkey_cache(&config);
            let mut engine = lock_engine(&self.engine);
            let actions = engine.apply_config(&config);
            engine.set_enabled(config.enabled);
            emit(&mut engine, &actions, out);
            update_all_properties(&config, out);
        }
    }
}

/// Parse a hotkey spec like `Control+Alt+v` into (modifier mask, keysym).
/// Returns `None` for specs it does not understand. The key may be a single
/// letter/digit/punctuation character, `space`, or `f1`..`f12`.
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
            f if f.len() >= 2
                && f.starts_with('f')
                && f[1..].chars().all(|c| c.is_ascii_digit())
                && f[1..]
                    .parse::<u8>()
                    .map(|n| (1..=12).contains(&n))
                    .unwrap_or(false) =>
            {
                // f1..f12: the len==2 check here used to reject f10..f12.
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
    if mods == 0 {
        // A bare key would fire on every press of that character.
        return None;
    }
    // A Shift-only combo is fine as long as its key is not a letter: the
    // key itself disambiguates (Shift+~ never collides with plain typing),
    // while Shift+a would fire on every capital A the user types. ASCII
    // letters are their own keysyms.
    if mods == keymap::SHIFT_MASK && char::from_u32(key).is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((mods, key))
}

/// A Shift+punctuation combo types its key on the SECOND tap within the
/// repeat gap: Control/Alt block the character and Shift+letters are
/// refused by the parser, so this is the only colliding shape.
pub(super) fn hotkey_passes_key(mods: u32, key: u32) -> bool {
    mods == keymap::SHIFT_MASK && char::from_u32(key).is_some_and(|c| c.is_ascii_graphic())
}

/// The next entry's key after `current`, wrapping around — the same list
/// order the panel radios show.
pub(super) fn cycle_next<'a>(
    list: &'a [(&'a str, &'a str, &'a str)],
    current: &str,
) -> Option<&'a str> {
    let i = list.iter().position(|(k, ..)| *k == current)?;
    Some(list[(i + 1) % list.len()].0)
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

/// The prgname part of a client id (`gtk4-im:chrome` → `chrome`). Fixed ids
/// without a prgname (`xim`, `fake`) return None — the picker then shows
/// the unnamed state.
pub(super) fn client_prgname(client: &str) -> Option<String> {
    client.split_once(':').map(|(_, p)| p.to_string())
}
