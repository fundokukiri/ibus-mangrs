//! The per-app rule picker, shown as an IBus lookup table at the caret —
//! the ibus-bamboo Shift+` flow, no GTK window involved. The engine opens
//! it on the app-rule hotkey with the rule target it resolved at press
//! time (once the popup is up, the focused "app" would be the app itself).
//! One flat page of the visible display modes: the highlight starts on the
//! mode in force — GNOME draws the lookup-table cursor as the blue hover —
//! and the auxiliary line names the app and its mode. Up/Down move the
//! highlight, Enter picks it, a digit 1..9 picks that entry directly, 0 picks
//! the last, Escape or any other key closes the menu (other keys type on).
//! Clicking elsewhere resets the client, which closes it too. The choice
//! updates the highest-priority rule the app currently rides on
//! (`Config::set_rule_mode_for_target`).

use super::hotkey::client_prgname;
use super::*;

/// One page holds every mode: the menu is static, selection is by digit.
pub(super) const PICKER_PAGE_SIZE: u32 = 9;

/// The picker's entries: the visible display modes in dropdown order, the
/// entry currently in force for the target prefixed with a dot marker.
pub(super) fn picker_entries(visible: &[ModeConfig], current: ModeConfig, en: bool) -> Vec<String> {
    visible
        .iter()
        .map(|mode| {
            let label = ibus_prop::mode_label(*mode, en);
            if *mode == current {
                format!("● {label}")
            } else {
                label.to_string()
            }
        })
        .collect()
}

/// The mode a picker row resolves to right now: the rule/app mode, else the
/// global mode.
fn picker_current(config: &Config, target: &str) -> ModeConfig {
    let probe = (target != crate::config::UNNAMED_PATTERN).then_some(target);
    crate::config::app_rule_mode(&config.app_rules, probe).unwrap_or(config.mode)
}

/// Keys the picker consumes while open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerKey {
    Select(usize),
    /// The `0` key: the last visible entry.
    SelectLast,
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
    /// Nothing can be picked: the list is a single warning row. Without the
    /// focus extension no Wayland app can be named, so any choice would be
    /// written against the wrong one.
    warn: bool,
}

impl PickerState {
    pub fn new(target: String, cursor: usize) -> Self {
        Self {
            target,
            cursor,
            warn: false,
        }
    }

    /// The state with nothing to pick — see the field docs.
    fn warning() -> Self {
        Self {
            target: String::new(),
            cursor: 0,
            warn: true,
        }
    }

    pub(super) fn warnings_only(&self) -> bool {
        self.warn
    }

    /// Map one key event onto the picker. Pure, so tests can drive it.
    /// Digits 1..9 pick entries 1..9 and 0 picks the last; Enter picks
    /// the highlighted entry; Up/Down move the highlight.
    pub fn on_key(&self, keyval: u32) -> PickerKey {
        if self.warn {
            // Enter, a digit or a click must never write a rule for an app
            // that cannot be named: any key just closes the menu.
            return PickerKey::Ignore;
        }
        match keyval {
            65293 | 65421 => PickerKey::Select(self.cursor), // Return / KP_Enter
            65307 => PickerKey::Cancel,                      // Escape
            65362 => PickerKey::Move(-1),                    // Up
            65364 => PickerKey::Move(1),                     // Down
            k @ 0x31..=0x39 => PickerKey::Select((k - 0x31) as usize),
            0x30 => PickerKey::SelectLast,
            _ => PickerKey::Ignore,
        }
    }
}

/// Where the focus extension's own file is, when it is installed at all:
/// GIO's extension directories, the system one included.
pub(crate) fn focus_extension_path() -> Option<std::path::PathBuf> {
    const REL: &str = "gnome-shell/extensions/mangrs-focus@fundokukiri/extension.js";
    dirs::data_dir()
        .map(|dir| dir.join(REL))
        .filter(|path| path.exists())
        .or_else(|| {
            let system = std::path::Path::new("/usr/share").join(REL);
            system.exists().then_some(system)
        })
}

/// Is the focus extension installed at all (whatever its enabled state)?
pub(crate) fn focus_extension_installed() -> bool {
    focus_extension_path().is_some()
}

/// The warning shown instead of the mode list while no app can be named: the
/// user is told which of the two causes to fix.
fn warning_text(english: bool, installed: bool) -> &'static str {
    match (english, installed) {
        (false, false) => "Chưa cài extension MăngRS Focus",
        (false, true) => "Extension MăngRS Focus chưa bật",
        (true, false) => "The MăngRS Focus extension is not installed",
        (true, true) => "The MăngRS Focus extension is off",
    }
}

/// The rule target for the focused context, read at press time: the
/// extension/xprop name, else the client's own prgname, else — only while the
/// compositor's own UI holds the keyboard — the bare relay client.
/// `None` when nothing names the app.
fn rule_target(engine: &Engine) -> Option<String> {
    engine
        .app_name()
        .map(|name| crate::config::strip_transport_prefix(name).to_string())
        .or_else(|| engine.client().and_then(client_prgname))
        .or_else(|| {
            // A compositor-owned context reports its own name with no toolkit
            // prefix (`gnome-shell` while its search field or Alt+F2 box holds
            // the keyboard), and rule matching already uses that string — so
            // it is a target in its own right. Without the extension the same
            // string is every relayed Wayland app, so it only counts when the
            // extension answered with no window.
            engine
                .shell_ui_focused()
                .then(|| engine.client())
                .flatten()
                .filter(|client| !matches!(*client, "xim" | "fake"))
                .map(str::to_string)
        })
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
        let (target, extension_answered) = {
            let engine = lock_engine(&self.engine);
            (rule_target(&engine), engine.extension_answered())
        };
        // Nothing names this app and there is no extension to ask: with the
        // extension off no Wayland app can be named at all, so the menu warns
        // instead of offering a choice that would hit the wrong app.
        if target.is_none() && !extension_answered {
            debug_log::log("ext picker=hotkey why=no_extension");
            self.picker = Some(PickerState::warning());
            self.emit_picker_table(out);
            return;
        }
        // An app that names nothing (a terminal window under Wayland, any
        // winit/Electron app) still needs its mode reachable: the pick writes
        // the unnamed pattern, which matches only clients with no name.
        let target = target
            .filter(|target| !target.is_empty())
            .unwrap_or_else(|| {
                debug_log::log("ext picker=hotkey target=none rule=none");
                crate::config::UNNAMED_PATTERN.to_string()
            });
        // The in-flight word ends here, before the menu shows: a pick must
        // never apply_config over a live composition (that desynced the
        // tail and the wrapper — the lost-letters/duplicated-word report).
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.end_word_for_picker();
            emit(&mut engine, &self.injection, &actions, out);
        }
        // The highlight starts on the mode in force: GNOME draws the
        // lookup-table cursor as the blue hover, so the current choice is
        // visible the moment the menu opens.
        let config = Config::load();
        let current = picker_current(&config, &target);
        let cursor = ModeConfig::ALL
            .iter()
            .position(|mode| *mode == current)
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
        if state.warnings_only() {
            // Text only: a candidate row would be one the user can click, and
            // there is nothing to pick.
            out.push(signal("HideLookupTable"));
            out.push(signal("UpdateAuxiliaryText").append2(
                crate::ibus_text::plain_text(warning_text(
                    config.language == "en",
                    focus_extension_installed(),
                )),
                true,
            ));
            return;
        }
        let current = picker_current(&config, &state.target);
        let entries = picker_entries(&ModeConfig::ALL, current, config.language == "en");
        let current_label = ibus_prop::mode_label(current, config.language == "en").to_string();
        let table =
            ibus_prop::lookup_table_variant(PICKER_PAGE_SIZE, state.cursor as u32, &entries);
        out.push(signal("UpdateLookupTable").append2(table, true));
        // One short line: the app, the mode in force, and the keys the menu
        // answers to (nothing else advertises them).
        let hint = if config.language == "en" {
            "↑↓ then Enter"
        } else {
            "↑↓ rồi Enter"
        };
        let label = if state.target == crate::config::UNNAMED_PATTERN {
            if config.language == "en" {
                "unnamed app".to_string()
            } else {
                "app không tên".to_string()
            }
        } else {
            crate::appname::display_name(&state.target)
        };
        out.push(signal("UpdateAuxiliaryText").append2(
            crate::ibus_text::plain_text(&format!("{label} · {current_label} · {hint}")),
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
            PickerKey::SelectLast => {
                let len = ModeConfig::ALL.len();
                self.apply_picker_entry(len.saturating_sub(1), out);
                true
            }
            PickerKey::Move(delta) => {
                let len = ModeConfig::ALL.len();
                if len > 0 {
                    if let Some(p) = self.picker.as_mut() {
                        p.cursor = (p.cursor as i32 + delta).rem_euclid(len as i32) as usize;
                    }
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
        let mut config = Config::load();
        let Some(mode) = ModeConfig::ALL.get(index).copied() else {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
            return;
        };
        debug_log::log(&format!(
            "ext picker target={:?} class={:?} mode={}",
            crate::appname::display_name(&state.target),
            state.target,
            mode.key()
        ));
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
            emit(&mut engine, &self.injection, &actions, out);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MethodConfig, ModeConfig};

    fn engine(client: &str) -> Engine {
        let mut engine = Engine::new(MethodConfig::Telex.into(), ModeConfig::Preedit.into());
        engine.set_client(Some(client.to_string()));
        engine
    }

    #[test]
    fn the_shell_is_a_target_only_while_the_extension_names_no_window() {
        // Without the extension a relayed Wayland app reports exactly this
        // string, so it names no app.
        let mut e = engine("gnome-shell");
        assert_eq!(rule_target(&e), None);

        // With the extension answering empty, the compositor's own UI really
        // does hold the keyboard.
        e.set_shell_ui_focused(true);
        assert_eq!(rule_target(&e).as_deref(), Some("gnome-shell"));

        // A named app always wins over the client string.
        e.set_app_name(Some("wayland:Alacritty".into()));
        assert_eq!(rule_target(&e).as_deref(), Some("Alacritty"));
    }

    #[test]
    fn the_warning_says_which_half_is_missing() {
        // Not installed and installed-but-off need different advice, in both
        // languages, and both stay one short line.
        for english in [false, true] {
            let missing = warning_text(english, false);
            let off = warning_text(english, true);
            assert_ne!(missing, off);
            assert!(missing.contains("MăngRS Focus") && off.contains("MăngRS Focus"));
            let long = |text: &str| text.chars().count() > 44;
            assert!(!long(missing) && !long(off), "{missing} / {off}");
            assert!(!missing.contains(','), "{missing}");
        }
        assert!(warning_text(false, false).contains("Chưa cài"));
        assert!(warning_text(false, true).contains("chưa bật"));
    }

    #[test]
    fn a_warning_only_menu_cannot_pick_anything() {
        // Without the extension no Wayland app is named, so Enter, a digit or
        // Escape must all just close the menu: nothing is written.
        let state = PickerState::warning();
        assert!(state.warnings_only());
        for key in [65293u32, 65421, 0x31, 0x30, 65307, 65362] {
            assert_eq!(state.on_key(key), PickerKey::Ignore, "key {key}");
        }
    }

    #[test]
    fn a_toolkit_client_names_its_app_without_the_extension() {
        let e = engine("gtk3-im:sublime_text");
        assert_eq!(rule_target(&e).as_deref(), Some("sublime_text"));
    }
}
