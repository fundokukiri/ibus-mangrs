//! The per-app rule picker, shown as an IBus lookup table at the caret —
//! the ibus-bamboo Shift+` flow, no GTK window involved. The engine opens
//! it on the app-rule hotkey with the rule target it resolved at press
//! time (once the popup is up, the focused "app" would be the app itself).
//! One flat page of the ten display modes: the highlight starts on the
//! mode in force — GNOME draws the lookup-table cursor as the blue hover —
//! and the auxiliary line names the app and its mode. Up/Down move the
//! highlight, Enter picks it, a digit 1..9/0 picks that entry directly,
//! Escape or any other key closes the menu (other keys type on). Clicking
//! elsewhere resets the client, which closes it too. The choice updates
//! the highest-priority rule the app currently rides on
//! (`Config::set_rule_mode_for_target`).

use super::hotkey::client_prgname;
use super::*;

/// One page holds every mode: the menu is static, selection is by digit.
pub(super) const PICKER_PAGE_SIZE: u32 = 10;

/// The picker's entries: the display modes in panel-menu order, the entry
/// currently in force for the target prefixed with a dot marker.
pub(super) fn picker_entries(current: ModeConfig) -> Vec<String> {
    ibus_prop::MODES
        .iter()
        .map(|(key, label)| {
            if ModeConfig::from_key(key) == Some(current) {
                format!("● {label}")
            } else {
                label.to_string()
            }
        })
        .collect()
}

/// Keys the picker consumes while open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerKey {
    Select(usize),
    /// Move the highlight by this many entries (wrapping).
    Move(i32),
    Cancel,
    /// Any other key: close the picker and let the key reach the engine.
    Ignore,
}

pub(super) struct PickerState {
    /// The rule pattern the choice applies to — the focused app's bare
    /// class name (transport prefix stripped, like the GUI adds rules).
    pub target: String,
    /// The highlighted entry. Starts on the mode in force (the popup's
    /// blue hover marks the current choice), Up/Down move it, Enter picks
    /// it; digits still select their entry directly.
    pub(super) cursor: usize,
}

impl PickerState {
    pub fn new(target: String, cursor: usize) -> Self {
        Self { target, cursor }
    }

    /// Map one key event onto the picker. Pure, so tests can drive it.
    /// Digits 1..9 pick entries 1..9 and 0 picks the tenth; Enter picks
    /// the highlighted entry; Up/Down move the highlight.
    pub fn on_key(&self, keyval: u32) -> PickerKey {
        match keyval {
            65293 | 65421 => PickerKey::Select(self.cursor), // Return / KP_Enter
            65307 => PickerKey::Cancel,                      // Escape
            65362 => PickerKey::Move(-1),                    // Up
            65364 => PickerKey::Move(1),                     // Down
            k @ 0x31..=0x39 => PickerKey::Select((k - 0x31) as usize),
            0x30 => PickerKey::Select(9), // 0 picks the tenth entry
            _ => PickerKey::Ignore,
        }
    }
}

impl EngineHandler {
    /// Open the picker for the focused application. The target is the same
    /// string the rules will match against: the focus extension/xprop name
    /// (`wayland:<class>`, prefix stripped here) or the client's prgname.
    pub(super) fn open_app_rule_picker(&mut self, out: &mut Vec<Message>) {
        // Second press of the hotkey while the menu is up closes it — the
        // combo is a toggle, like bamboo's.
        if self.picker.take().is_some() {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
            return;
        }
        let target = {
            let engine = lock_engine(&self.engine);
            engine
                .app_name()
                .map(|name| {
                    name.strip_prefix("wayland:")
                        .or_else(|| name.strip_prefix("x11:"))
                        .unwrap_or(name)
                        .to_string()
                })
                .or_else(|| engine.client().and_then(client_prgname))
        };
        let Some(target) = target.filter(|t| !t.is_empty()) else {
            // No name, no rule that could ever match: stay closed.
            debug_log::log("picker hotkey: focused app unnamed, staying closed");
            return;
        };
        // The in-flight word ends here, before the menu shows: a pick must
        // never apply_config over a live composition (that desynced the
        // tail and the wrapper — the lost-letters/duplicated-word report).
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.end_word_for_picker();
            emit(&mut engine, &actions, out);
        }
        // The highlight starts on the mode in force: GNOME draws the
        // lookup-table cursor as the blue hover, so the current choice is
        // visible the moment the menu opens.
        let config = Config::load();
        let current =
            crate::config::app_rule_mode(&config.app_rules, Some(&target)).unwrap_or(config.mode);
        let cursor = ibus_prop::MODES
            .iter()
            .position(|(key, _)| ModeConfig::from_key(key) == Some(current))
            .unwrap_or(0);
        self.picker = Some(PickerState::new(target, cursor));
        self.emit_picker_table(out);
    }

    /// Push the lookup table plus the auxiliary line naming the app and the
    /// mode it currently rides on.
    fn emit_picker_table(&self, out: &mut Vec<Message>) {
        let Some(state) = self.picker.as_ref() else {
            return;
        };
        let config = Config::load();
        let current = crate::config::app_rule_mode(&config.app_rules, Some(&state.target))
            .unwrap_or(config.mode);
        let entries = picker_entries(current);
        let current_label = ibus_prop::MODES
            .iter()
            .find(|(key, _)| ModeConfig::from_key(key) == Some(current))
            .map(|(_, label)| label.to_string())
            .unwrap_or_default();
        let table =
            ibus_prop::lookup_table_variant(PICKER_PAGE_SIZE, state.cursor as u32, &entries);
        out.push(signal("UpdateLookupTable").append2(table, true));
        out.push(signal("UpdateAuxiliaryText").append2(
            crate::ibus_text::plain_text(&format!("{} — {}", state.target, current_label)),
            true,
        ));
    }

    /// Handle a key while the picker is open. Returns whether the key was
    /// consumed; `Ignore` closes the picker first so the key types on.
    pub(super) fn picker_key(&mut self, keyval: u32, state: u32, out: &mut Vec<Message>) -> bool {
        // Releases never act: the combo's own keys are released the moment
        // the menu opens (which used to close it instantly). Swallow them.
        if state & keymap::RELEASE_MASK != 0 {
            return true;
        }
        let action = match self.picker.as_ref() {
            Some(p) => p.on_key(keyval),
            None => return false,
        };
        match action {
            PickerKey::Select(i) => {
                self.apply_picker_entry(i, out);
                true
            }
            PickerKey::Move(delta) => {
                if let Some(p) = self.picker.as_mut() {
                    p.cursor =
                        (p.cursor as i32 + delta).rem_euclid(PICKER_PAGE_SIZE as i32) as usize;
                }
                // Re-render so the popup's highlight follows the cursor.
                self.emit_picker_table(out);
                true
            }
            PickerKey::Cancel => {
                self.close_picker(out);
                true
            }
            PickerKey::Ignore => {
                self.close_picker(out);
                false
            }
        }
    }

    /// Write the chosen mode into the rule the app currently rides on and
    /// apply it at once.
    pub(super) fn apply_picker_entry(&mut self, index: usize, out: &mut Vec<Message>) {
        let Some(state) = self.picker.take() else {
            return;
        };
        let Some((key, _)) = ibus_prop::MODES.get(index) else {
            return;
        };
        let Some(mode) = ModeConfig::from_key(key) else {
            return;
        };
        debug_log::log(&format!("picker: {} → {}", state.target, mode.key()));
        let mut config = Config::load();
        config.set_rule_mode_for_target(&state.target, mode);
        if config.save().is_err() {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
            return;
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
        out.push(signal("HideLookupTable"));
        out.push(signal("HideAuxiliaryText"));
    }

    /// Close the picker without writing anything.
    pub(super) fn close_picker(&mut self, out: &mut Vec<Message>) {
        if self.picker.take().is_some() {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
        }
    }
}
