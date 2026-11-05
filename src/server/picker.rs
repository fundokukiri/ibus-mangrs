//! The per-app rule picker, shown as an IBus lookup table at the caret —
//! the ibus-bamboo Shift+` flow, no GTK window involved. The engine opens
//! it on the app-rule hotkey with the rule target it resolved at press
//! time (once the popup is up, the focused "app" would be the app itself);
//! arrow keys navigate, Enter or a digit picks, Escape or any other key
//! closes it. The choice upserts an `[[app_rules]]` entry for the target
//! and the engine applies it immediately.

use super::hotkey::client_prgname;
use super::*;

/// Lookup-table page size: GNOME draws the candidate bar compactly and the
/// number keys 1..9 must reach every entry of a page.
pub(super) const PICKER_PAGE_SIZE: u32 = 9;

/// The entries the picker offers, in panel-menu mode order, plus the rule
/// removal entry. Index `i < len-1` selects `MODES[i]`; the last entry
/// clears the target's rule.
pub(super) fn picker_entries() -> Vec<String> {
    let mut entries: Vec<String> = ibus_prop::MODES
        .iter()
        .map(|(_, label)| label.to_string())
        .collect();
    entries.push("— Xóa luật của app này —".to_string());
    entries
}

/// Keys the picker consumes while open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerKey {
    Down,
    Up,
    PageDown,
    PageUp,
    Select(usize),
    Cancel,
    /// Any other key: close the picker and let the key reach the engine.
    Ignore,
}

pub(super) struct PickerState {
    /// The rule pattern the choice applies to — the focused app's bare
    /// class name (transport prefix stripped, like the GUI adds rules).
    pub target: String,
    pub cursor: usize,
}

impl PickerState {
    pub fn new(target: String) -> Self {
        Self { target, cursor: 0 }
    }

    /// Map one key event onto the picker. Pure, so tests can drive it.
    pub fn on_key(&self, keyval: u32, len: usize) -> PickerKey {
        match keyval {
            65364 => PickerKey::Down, // Down
            65362 => PickerKey::Up,   // Up
            65366 => PickerKey::PageDown,
            65365 => PickerKey::PageUp,
            65293 | 65421 => PickerKey::Select(self.cursor), // Return / KP_Enter
            65307 => PickerKey::Cancel,                      // Escape
            k @ 0x31..=0x39 => {
                // A digit selects the entry at that position of the current
                // page, exactly what the label numbering shows.
                let page_first =
                    (self.cursor / PICKER_PAGE_SIZE as usize) * PICKER_PAGE_SIZE as usize;
                let idx = page_first + (k - 0x31) as usize;
                if idx < len {
                    PickerKey::Select(idx)
                } else {
                    PickerKey::Ignore
                }
            }
            _ => PickerKey::Ignore,
        }
    }

    pub(super) fn step(&mut self, delta: isize, len: usize) {
        let len = len.max(1);
        self.cursor = (self.cursor as isize + delta).rem_euclid(len as isize) as usize;
    }
}

impl EngineHandler {
    /// Open the picker for the focused application. The target is the same
    /// string the rules will match against: the focus extension/xprop name
    /// (`wayland:<class>`, prefix stripped here) or the client's prgname.
    pub(super) fn open_app_rule_picker(&mut self, out: &mut Vec<Message>) {
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
        self.picker = Some(PickerState::new(target));
        self.emit_picker_table(out);
    }

    /// Push the lookup table reflecting the current cursor.
    fn emit_picker_table(&self, out: &mut Vec<Message>) {
        let entries: Vec<String> = ibus_prop::MODES
            .iter()
            .map(|(_, l)| l.to_string())
            .chain(std::iter::once("— Xóa luật của app này —".to_string()))
            .collect();
        let table = ibus_prop::lookup_table_variant(
            PICKER_PAGE_SIZE,
            self.picker.as_ref().map_or(0, |p| p.cursor as u32),
            &entries,
        );
        out.push(signal("UpdateLookupTable").append2(table, true));
    }

    /// Handle a key while the picker is open. Returns whether the key was
    /// consumed; `Ignore` closes the picker first so the key types on.
    pub(super) fn picker_key(&mut self, keyval: u32, out: &mut Vec<Message>) -> bool {
        let len = picker_entries().len();
        let Some(state) = self.picker.as_ref() else {
            return false;
        };
        match state.on_key(keyval, len) {
            PickerKey::Down => {
                if let Some(state) = self.picker.as_mut() {
                    state.step(1, len);
                }
                self.emit_picker_table(out);
                true
            }
            PickerKey::Up => {
                if let Some(state) = self.picker.as_mut() {
                    state.step(-1, len);
                }
                self.emit_picker_table(out);
                true
            }
            PickerKey::PageDown => {
                if let Some(state) = self.picker.as_mut() {
                    state.step(PICKER_PAGE_SIZE as isize, len);
                }
                self.emit_picker_table(out);
                true
            }
            PickerKey::PageUp => {
                if let Some(state) = self.picker.as_mut() {
                    state.step(-(PICKER_PAGE_SIZE as isize), len);
                }
                self.emit_picker_table(out);
                true
            }
            PickerKey::Select(i) => {
                self.apply_picker_entry(i, out);
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

    /// Write the chosen entry as the target's rule and apply it at once.
    fn apply_picker_entry(&mut self, index: usize, out: &mut Vec<Message>) {
        let Some(state) = self.picker.take() else {
            return;
        };
        let entries = picker_entries();
        let mut config = Config::load();
        if index + 1 < entries.len() {
            // A mode entry: upsert on top so the explicit choice outranks
            // the built-in safety net.
            let Some((key, _)) = ibus_prop::MODES.get(index) else {
                return;
            };
            let Some(mode) = ModeConfig::from_key(key) else {
                return;
            };
            debug_log::log(&format!("picker: rule {} → {}", state.target, mode.key()));
            config.upsert_app_rule(&state.target, mode);
        } else {
            debug_log::log(&format!("picker: rule {} removed", state.target));
            config.remove_app_rule(&state.target);
        }
        if config.save().is_err() {
            out.push(signal("HideLookupTable"));
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
    }

    /// Close the picker without writing anything.
    pub(super) fn close_picker(&mut self, out: &mut Vec<Message>) {
        if self.picker.take().is_some() {
            out.push(signal("HideLookupTable"));
        }
    }
}
