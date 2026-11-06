//! GTK4 + libadwaita settings window, opened from the popup ("Cấu hình") or
//! with `-gui`.
//!
//! Every control writes straight to `config.toml` (and `macros.toml` /
//! `dictionary.txt`); the engine watches the config file mtime, so changes
//! apply within a second without restarting ibus-daemon.
//!
//! Layout: GNOME Settings style — a navigation sidebar on the left, one
//! preferences page per section on the right. Eight pages: Chung /
//! Dấu & tự động / Gõ tắt / Từ điển / Luật / Menu panel / Tinh chỉnh /
//! Giới thiệu.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::{
    BracketConfig, CharsetConfig, Config, MenuNode, MethodConfig, ModeConfig, W2uConfig,
};
use crate::macros::{Dictionary, Macros};
use gtk4::prelude::*;
// The crate ships as `libadwaita`; every GNOME project calls it `adw`.
use adw::prelude::*;
use gtk4::{Entry, Orientation, StringList, TextView};
use libadwaita as adw;

mod content;
mod hotkey;
mod pages;
mod text;
mod widgets;

use text::t;

use content::*;
use hotkey::*;
use widgets::*;
mod build;
mod menu_editor;
mod rules_editor;

#[cfg(test)]
mod tests;

use self::build::*;
use self::menu_editor::*;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Adwaita styles (StyleManager, dark mode, accent color) must be
    // initialized before any Adw widget exists.
    libadwaita::init().map_err(|e| format!("libadwaita init failed: {e}"))?;
    // The engine spawns us with `--page=<stack name>` to open a specific
    // page (menu "Mở bảng gõ tắt…"); GTK parses argv itself and rejects
    // these arguments, so read them here and hand GTK an empty argv.
    let page = std::env::args().find_map(|a| a.strip_prefix("--page=").map(str::to_string));
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs")
        .build();
    // GApplication is single-instance: a second launch activates the first
    // one, which must present its existing window instead of building
    // another (one control panel at a time).
    app.connect_activate(move |app| {
        if let Some(window) = app.windows().first() {
            window.present();
            return;
        }
        build(app, page.as_deref());
    });
    app.run_with_args::<&str>(&[]);
    Ok(())
}

const METHODS: &[&str] = &[
    "Telex",
    "Telex 2",
    "Telex W",
    "Telex + VNI",
    "Telex + VNI + VIQR",
    "VNI",
    "VNI bàn phím Pháp",
    "VIQR",
    "Microsoft layout",
];
const MODES: &[&str] = &[
    "Tự động",
    "Không gạch chân",
    "Gạch chân (preedit)",
    "Surrounding text",
    "Smooth (uinput)",
    "Super Smooth (uinput)",
    "Select (uinput)",
    "ForwardKeyEvent (Backspace)",
    "ForwardKeyEvent II (Shift+Left)",
    "OFF",
];
const CHARSETS: &[&str] = &[
    "Unicode",
    "Unicode tổ hợp",
    "TCVN3",
    "VNI",
    "VIQR",
    "VISCII",
    "VPS",
    "Windows 1258",
    "BKHCM",
    "Vietware X",
    "Vietware Full",
];
const ON_OFF_MODES: &[&str] = &["Tắt", "Trừ đầu từ", "Mọi vị trí"];
const CHANNELS: &[&str] = &["CommitText (an toàn)", "Clipboard (Ctrl+V)"];

/// The custom-shortcut functions, in `config::CUSTOM_HOTKEY_ACTIONS` order:
/// (action key, gui-content key) plus the code fallback labels — the
/// add-row dropdown and the row subtitles read these.
const CUSTOM_ACTION_KEYS: &[(&str, &str)] = &[
    ("enabled", "actions.hotkey_enabled"),
    ("mode", "actions.hotkey_mode"),
    ("rule", "actions.hotkey_rule"),
    ("emoji", "actions.hotkey_emoji"),
];
const CUSTOM_ACTION_DEFAULTS: &[&str] = &[
    "Bật/tắt tiếng Việt",
    "Đổi chế độ gõ",
    "Chọn luật theo app",
    "Mở bảng emoji",
];

struct Ui {
    enabled: adw::SwitchRow,
    method: adw::ComboRow,
    mode: adw::ComboRow,
    charset: adw::ComboRow,
    /// Built-in shortcut rows: the value is the stored spec, the display
    /// renders it as keycaps (see `ShortcutDisplay`).
    hotkey_display: ShortcutDisplay,
    hotkey_value: Rc<RefCell<String>>,
    enabled_hotkey_display: ShortcutDisplay,
    enabled_hotkey_value: Rc<RefCell<String>>,
    app_rule_hotkey_display: ShortcutDisplay,
    app_rule_hotkey_value: Rc<RefCell<String>>,
    modern: adw::SwitchRow,
    free: adw::SwitchRow,
    auto_correct: adw::SwitchRow,
    auto_restore: adw::SwitchRow,
    abbrev: adw::SwitchRow,
    bypass_url: adw::SwitchRow,
    w2u: adw::ComboRow,
    bracket: adw::ComboRow,
    em_dash: adw::SwitchRow,
    auto_capitalize: adw::SwitchRow,
    double_space: adw::SwitchRow,
    macro_enabled: adw::SwitchRow,
    capitalize_macro: adw::SwitchRow,
    macro_in_off_mode: adw::SwitchRow,
    time_format: Entry,
    date_format: Entry,
    /// Emoji shortcodes ("Emoji" tab): the on/off switch, the picker
    /// hotkey (the row itself lives on "Phím tắt", one row per shortcut)
    /// and the raw user table `name=glyph` lines of `emoji.toml`.
    emoji_enabled: adw::SwitchRow,
    emoji_hotkey_display: ShortcutDisplay,
    emoji_hotkey_value: Rc<RefCell<String>>,
    emoji_text: TextView,
    /// Custom shortcuts ("Phím tắt" page): the design vector is the source
    /// of truth (spec, action key), the boxed list is its view, rebuilt on
    /// every edit — the menu editor's shape. The add row opens the shared
    /// capture dialog.
    custom: Rc<RefCell<Vec<(String, String)>>>,
    custom_box: gtk4::ListBox,
    /// The page's toast host: the delete confirmation with its undo button.
    hotkey_overlay: adw::ToastOverlay,
    /// Macro editor: one boxed-list row per entry (key, value); GObject
    /// clones share the widgets, so reading them reads what the user sees.
    /// The rows live in a plain ListBox — a PreferencesGroup's first_child
    /// is its internal header, so a remove-everything loop would never end.
    /// The macro table as raw `khóa=nội dung` lines, dictionary-style —
    /// the only editor for it (a row list would duplicate it).
    macros_text: TextView,
    dictionary_enabled: adw::SwitchRow,
    dictionary_text: TextView,
    /// App Rules editor: one boxed-list row per rule. The rows hold the
    /// live values — GObject clones share the object — so reading them
    /// reads what the user sees. A plain ListBox, not a PreferencesGroup:
    /// the group's first_child is its internal header, so a
    /// remove-everything loop would never end.
    rules_box: gtk4::ListBox,
    /// Shown by `fill_rules` when no rule exists: an empty group reads as
    /// a bug otherwise (compact StatusPage, no icon).
    rules_status: adw::StatusPage,
    /// Blocks row save handlers while `fill_rules` rebuilds the rows: the
    /// mode dropdown fires `selected_notify` on a programmatic set too.
    rules_loading: Rc<RefCell<bool>>,
    /// The save funnel, wired in `build` once the window exists; rows are
    /// created in `refresh`, so they fire edits through this instead of a
    /// closure captured at build time. `None` in headless tests.
    save_cb: SharedSave,
    /// Blocks the save funnel while `external_reload` pushes fresh file
    /// contents into the widgets: the programmatic updates fire the same
    /// change handlers a user edit does, and saving them would write the
    /// half-updated state back over the change being picked up.
    suppress_save: Cell<bool>,
    /// "Thử luật" input and its verdict row.
    rule_preview_input: adw::EntryRow,
    rule_preview: adw::ActionRow,
    /// The merged add-rule form ("Thêm luật"): the pill (fill + tooltip),
    /// the frameless name entry whose placeholder mirrors the focused app,
    /// the copy/apply pair shown only while an app is detected, and Thêm.
    rule_pill: gtk4::Box,
    rule_name_entry: gtk4::Entry,
    rule_copy_btn: gtk4::Button,
    rule_apply_btn: gtk4::Button,
    rule_add_btn: gtk4::Button,
    /// The last good focused-app id: the hint Thêm falls back to when the
    /// field is empty. The pill tooltip carries the window title or the
    /// detection error.
    rule_focus_value: Rc<RefCell<String>>,
    hide_underline: adw::SwitchRow,
    debug_log: adw::SwitchRow,
    /// App-grid icon visibility ("Cài đặt khác"). Filesystem state, not a
    /// config key: the override file under ~/.local/share/applications.
    desktop_icon: adw::SwitchRow,
    /// Guards the programmatic `desktop_icon` state set in `refresh`.
    desktop_icon_loading: Rc<RefCell<bool>>,
    x11_delete: adw::SwitchRow,
    preedit_xim: adw::SwitchRow,
    commit_interval: adw::SpinRow,
    backspace_interval: adw::SpinRow,
    post_delay: adw::SpinRow,
    unicode_channel: adw::ComboRow,
    /// The panel-menu editor: the design vector is the source of truth
    /// (`read` clones it, so the round-trip holds without walking rows);
    /// the boxed list is its view, rebuilt on every edit. Folders nest one
    /// level at most, matching `config::MenuNode`.
    menu: Rc<RefCell<Vec<MenuNode>>>,
    menu_box: gtk4::ListBox,
    /// Which folder/radio rows are expanded, by design path — the list is
    /// rebuilt after every edit, so the state must outlive the widgets.
    menu_open: Rc<RefCell<HashSet<Vec<usize>>>>,
    /// The page's toast host: delete and add confirmations.
    menu_overlay: adw::ToastOverlay,
    /// The design-path key of the row just added — the next rebuild focuses
    /// its row menu, so keyboard users land on the new item.
    menu_focus_target: Rc<RefCell<Option<String>>>,
}

/// The save funnel handle shared between `Ui` and the rule-row closures.
type SharedSave = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

fn method_index(m: MethodConfig) -> u32 {
    match m {
        MethodConfig::Telex => 0,
        MethodConfig::Telex2 => 1,
        MethodConfig::TelexW => 2,
        MethodConfig::TelexVni => 3,
        MethodConfig::TelexVniViqr => 4,
        MethodConfig::Vni => 5,
        MethodConfig::VniFrench => 6,
        MethodConfig::Viqr => 7,
        MethodConfig::Microsoft => 8,
    }
}

fn mode_index(m: ModeConfig) -> u32 {
    match m {
        ModeConfig::Auto => 0,
        ModeConfig::NonPreedit => 1,
        ModeConfig::Preedit => 2,
        ModeConfig::Surrounding => 3,
        ModeConfig::Smooth => 4,
        ModeConfig::SuperSmooth => 5,
        ModeConfig::Select => 6,
        ModeConfig::ForwardKeyEvent => 7,
        ModeConfig::ForwardSelect => 8,
        ModeConfig::Off => 9,
    }
}

/// Inverse of `mode_index`, shared by the main mode dropdown and the
/// App Rules rows.
fn mode_at(i: u32) -> ModeConfig {
    match i {
        1 => ModeConfig::NonPreedit,
        2 => ModeConfig::Preedit,
        3 => ModeConfig::Surrounding,
        4 => ModeConfig::Smooth,
        5 => ModeConfig::SuperSmooth,
        6 => ModeConfig::Select,
        7 => ModeConfig::ForwardKeyEvent,
        8 => ModeConfig::ForwardSelect,
        9 => ModeConfig::Off,
        _ => ModeConfig::Auto,
    }
}

fn charset_index(c: CharsetConfig) -> u32 {
    match c {
        CharsetConfig::Unicode => 0,
        CharsetConfig::Nfd => 1,
        CharsetConfig::Tcvn3 => 2,
        CharsetConfig::Vni => 3,
        CharsetConfig::Viqr => 4,
        CharsetConfig::Viscii => 5,
        CharsetConfig::Vps => 6,
        CharsetConfig::Cp1258 => 7,
        CharsetConfig::Bkhcm => 8,
        CharsetConfig::VietwareX => 9,
        CharsetConfig::VietwareFull => 10,
    }
}

fn on_off_index<T: PartialEq>(value: &T, disabled: T, nonstart: T) -> u32 {
    if *value == disabled {
        0
    } else if *value == nonstart {
        1
    } else {
        2
    }
}

impl Ui {
    /// Push a config (plus the two text lists) into every widget. This is the
    /// only config → widget mapping: the initial build and the import both go
    /// through it, so an import cannot leave a stale widget that the next
    /// save would write back over the imported file. Takes `&Rc<Self>`
    /// because the rule rows it rebuilds capture the handle in their
    /// reorder/delete handlers.
    fn refresh(self: &Rc<Self>, config: &Config, macros: &Macros, dictionary: &Dictionary) {
        // Normalize the menu design first: an absent or empty design edits
        // the shipped shape. The round-trip assert at the end compares
        // `read()` — which always produces a design — against this copy.
        let mut config = config.clone();
        if config.menu.as_ref().is_none_or(|m| m.is_empty()) {
            config.menu = Some(config.default_menu());
        }
        self.enabled.set_active(config.enabled);
        self.method.set_selected(method_index(config.method));
        self.mode.set_selected(mode_index(config.mode));
        self.charset.set_selected(charset_index(config.charset));
        *self.hotkey_value.borrow_mut() = config.cycle_mode_hotkey.clone();
        self.hotkey_display.render(&config.cycle_mode_hotkey);
        *self.enabled_hotkey_value.borrow_mut() = config.enabled_hotkey.clone();
        self.enabled_hotkey_display.render(&config.enabled_hotkey);
        *self.app_rule_hotkey_value.borrow_mut() = config.app_rule_hotkey.clone();
        self.app_rule_hotkey_display.render(&config.app_rule_hotkey);
        self.modern.set_active(config.modern_tone_style);
        self.free.set_active(config.free_tone_marking);
        self.auto_correct.set_active(config.auto_correct);
        self.auto_restore.set_active(config.auto_restore);
        self.abbrev.set_active(config.abbreviations);
        self.bypass_url.set_active(config.bypass_url);
        self.w2u.set_selected(on_off_index(
            &config.w2u,
            W2uConfig::Disabled,
            W2uConfig::NonStart,
        ));
        self.bracket.set_selected(on_off_index(
            &config.bracket,
            BracketConfig::Disabled,
            BracketConfig::NonStart,
        ));
        self.em_dash.set_active(config.em_dash);
        self.auto_capitalize.set_active(config.auto_capitalize);
        self.double_space.set_active(config.double_space_period);
        self.macro_enabled.set_active(config.macro_enabled);
        self.capitalize_macro.set_active(config.capitalize_macro);
        self.macro_in_off_mode.set_active(config.macro_in_off_mode);
        self.time_format.set_text(&config.time_format);
        self.date_format.set_text(&config.date_format);
        self.emoji_enabled.set_active(config.emoji_enabled);
        *self.emoji_hotkey_value.borrow_mut() = config.emoji_hotkey.clone();
        self.emoji_hotkey_display.render(&config.emoji_hotkey);
        self.dictionary_enabled
            .set_active(config.dictionary_enabled);
        self.debug_log.set_active(config.debug_log);
        // Not a config key: the app-grid override file on disk decides.
        *self.desktop_icon_loading.borrow_mut() = true;
        self.desktop_icon.set_active(!desktop_icon_hidden());
        *self.desktop_icon_loading.borrow_mut() = false;
        self.desktop_icon
            .set_sensitive(system_desktop_entry_exists());
        self.x11_delete.set_active(config.x11_delete_without_report);
        self.preedit_xim.set_active(config.preedit_for_xim);
        self.commit_interval
            .set_value(config.commit_interval_ms as f64);
        self.backspace_interval
            .set_value(config.backspace_interval_ms as f64);
        self.post_delay.set_value(config.post_delay_ms as f64);
        self.unicode_channel
            .set_selected(match config.unicode_channel {
                crate::config::UnicodeChannel::Clipboard => 1,
                crate::config::UnicodeChannel::Commit => 0,
            });
        let menu = config.menu.clone().unwrap_or_else(|| config.default_menu());
        *self.menu.borrow_mut() = normalize_menu(&menu);
        self.fill_menu_list();
        set_text(
            &self.macros_text,
            &macros
                .entries()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        set_text(
            &self.dictionary_text,
            &dictionary.words().collect::<Vec<_>>().join("\n"),
        );
        set_text(
            &self.emoji_text,
            &crate::emoji::UserEmoji::load()
                .entries()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        self.fill_rules(&config.app_rules);
        *self.custom.borrow_mut() = config
            .custom_hotkeys
            .iter()
            .map(|c| (c.spec.clone(), c.action.clone()))
            .collect();
        self.fill_custom();
        self.hide_underline.set_active(config.hide_underline);
        // A field added to Config and Ui but forgotten here would leave a
        // stale widget that the next save writes back (the import bug).
        #[cfg(debug_assertions)]
        debug_assert_eq!(&self.read(), &config, "refresh missed a widget");
    }

    fn read(&self) -> Config {
        // Start from the file so options this window does not show are not
        // overwritten.
        let mut config = Config::load();
        config.enabled = self.enabled.is_active();
        config.method = match self.method.selected() {
            0 => MethodConfig::Telex,
            1 => MethodConfig::Telex2,
            2 => MethodConfig::TelexW,
            3 => MethodConfig::TelexVni,
            4 => MethodConfig::TelexVniViqr,
            5 => MethodConfig::Vni,
            6 => MethodConfig::VniFrench,
            7 => MethodConfig::Viqr,
            _ => MethodConfig::Microsoft,
        };
        config.mode = mode_at(self.mode.selected());
        config.charset = match self.charset.selected() {
            1 => CharsetConfig::Nfd,
            2 => CharsetConfig::Tcvn3,
            3 => CharsetConfig::Vni,
            4 => CharsetConfig::Viqr,
            5 => CharsetConfig::Viscii,
            6 => CharsetConfig::Vps,
            7 => CharsetConfig::Cp1258,
            8 => CharsetConfig::Bkhcm,
            9 => CharsetConfig::VietwareX,
            10 => CharsetConfig::VietwareFull,
            _ => CharsetConfig::Unicode,
        };
        config.cycle_mode_hotkey = self.hotkey_value.borrow().clone();
        config.enabled_hotkey = self.enabled_hotkey_value.borrow().clone();
        config.app_rule_hotkey = self.app_rule_hotkey_value.borrow().clone();
        config.debug_log = self.debug_log.is_active();
        config.x11_delete_without_report = self.x11_delete.is_active();
        config.preedit_for_xim = self.preedit_xim.is_active();
        config.modern_tone_style = self.modern.is_active();
        config.free_tone_marking = self.free.is_active();
        config.auto_correct = self.auto_correct.is_active();
        config.auto_restore = self.auto_restore.is_active();
        config.abbreviations = self.abbrev.is_active();
        config.hide_underline = self.hide_underline.is_active();
        config.bypass_url = self.bypass_url.is_active();
        config.w2u = match self.w2u.selected() {
            1 => W2uConfig::NonStart,
            2 => W2uConfig::Everywhere,
            _ => W2uConfig::Disabled,
        };
        config.bracket = match self.bracket.selected() {
            1 => BracketConfig::NonStart,
            2 => BracketConfig::Everywhere,
            _ => BracketConfig::Disabled,
        };
        config.em_dash = self.em_dash.is_active();
        config.auto_capitalize = self.auto_capitalize.is_active();
        config.double_space_period = self.double_space.is_active();
        config.macro_enabled = self.macro_enabled.is_active();
        config.capitalize_macro = self.capitalize_macro.is_active();
        config.macro_in_off_mode = self.macro_in_off_mode.is_active();
        config.time_format = self.time_format.text().trim().to_string();
        config.date_format = self.date_format.text().trim().to_string();
        config.emoji_enabled = self.emoji_enabled.is_active();
        config.emoji_hotkey = self.emoji_hotkey_value.borrow().clone();
        config.dictionary_enabled = self.dictionary_enabled.is_active();
        config.commit_interval_ms = self.commit_interval.value() as u64;
        config.backspace_interval_ms = self.backspace_interval.value() as u64;
        config.post_delay_ms = self.post_delay.value() as u64;
        config.unicode_channel = match self.unicode_channel.selected() {
            1 => crate::config::UnicodeChannel::Clipboard,
            _ => crate::config::UnicodeChannel::Commit,
        };
        // The designed menu tree; never empty (the editor refuses to remove
        // the last entry), so a save always pins the design down.
        config.menu = Some(self.read_menu());
        // Read here, not in `save_all`: `refresh` asserts that `read` returns
        // everything it just pushed into the widgets, and the rule rows are
        // among those widgets.
        config.app_rules = self.rules_from_rows();
        config.custom_hotkeys = self
            .custom
            .borrow()
            .iter()
            .map(|(spec, action)| crate::config::CustomHotkey {
                spec: spec.clone(),
                action: action.clone(),
            })
            .collect();
        config
    }

    /// The macros as the raw table shows them: one `khóa=nội dung` line
    /// per pair; an empty key or value drops the pair (the old text list
    /// skipped such lines too).
    fn macros_from_text(&self) -> Vec<(String, String)> {
        text_of(&self.macros_text)
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect()
    }

    /// The emoji user table as the raw editor shows it: one `name=glyph`
    /// line per pair, empty names or glyphs dropped (the macros rule).
    fn emoji_from_text(&self) -> Vec<(String, String)> {
        text_of(&self.emoji_text)
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect()
    }

    // --- panel-menu editor (the design vector is the source of truth) ---

    /// The design as the user sees it: `read()` clones this, so the
    /// round-trip holds without walking the widgets.
    fn read_menu(&self) -> Vec<MenuNode> {
        self.menu.borrow().clone()
    }

    fn read_dictionary(&self) -> Vec<String> {
        text_of(&self.dictionary_text)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect()
    }

    /// Run the save funnel if the window wired it (headless tests skip it).
    fn fire_save(&self) {
        if self.suppress_save.get() {
            return;
        }
        let save = self.save_cb.borrow().clone();
        if let Some(save) = save {
            save();
        }
    }

    fn save_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.read();
        config.save()?;

        let mut macros = Macros::load();
        macros.set_entries(self.macros_from_text().into_iter());
        macros.save()?;

        let mut dict = Dictionary::load();
        dict.set_words(self.read_dictionary().into_iter());
        dict.save()?;

        let mut emoji = crate::emoji::UserEmoji::load();
        emoji.set_entries(self.emoji_from_text().into_iter());
        emoji.save()?;
        Ok(())
    }

    /// Re-read every settings file and push its contents into the widgets
    /// without saving. Called when the files changed under the window —
    /// the engine's hotkey toggles write config.toml directly. Without
    /// this the stale widget values would be saved back over the change
    /// on the next user edit.
    pub(super) fn external_reload(self: &Rc<Self>) {
        self.suppress_save.set(true);
        let config = Config::load();
        let macros = Macros::load();
        let dictionary = Dictionary::load();
        self.refresh(&config, &macros, &dictionary);
        self.suppress_save.set(false);
    }
}

/// The shipped focus extension, embedded so the install button works on a
/// dev run and needs no separate data file next to the binary.
const FOCUS_EXTENSION_JS: &str =
    include_str!("../../data/gnome-shell-extension/mangrs-focus@fundokukiri/extension.js");
const FOCUS_EXTENSION_META: &str =
    include_str!("../../data/gnome-shell-extension/mangrs-focus@fundokukiri/metadata.json");
const FOCUS_EXT_DIR: &str = "gnome-shell/extensions/mangrs-focus@fundokukiri";

/// Copy the embedded focus extension into the user extension directory and
/// try to enable it. GNOME 45+ caches extension ES modules, so a fresh copy
/// only loads after logout/login — every outcome says so.
pub(super) fn install_focus_extension() -> Result<String, String> {
    use std::fmt::Write as _;
    let Some(base) = dirs::data_dir() else {
        return Err("Không tìm thấy thư mục dữ liệu người dùng (~/.local/share)".into());
    };
    let dir = base.join(FOCUS_EXT_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Không tạo được {}: {e}", dir.display()))?;
    for (name, body) in [
        ("extension.js", FOCUS_EXTENSION_JS),
        ("metadata.json", FOCUS_EXTENSION_META),
    ] {
        std::fs::write(dir.join(name), body)
            .map_err(|e| format!("Không ghi được {}: {e}", name))?;
    }
    let mut note = String::from("Đã cài extension vào ~/.local/share/gnome-shell/extensions. ");
    match std::process::Command::new("gnome-extensions")
        .args(["enable", "mangrs-focus@fundokukiri"])
        .status()
    {
        Ok(s) if s.success() => {
            let _ = writeln!(
                note,
                "Đã bật. Đăng xuất rồi đăng nhập lại để nó chạy (GNOME giữ module cũ trong bộ nhớ)."
            );
        }
        _ => {
            let _ = writeln!(
                note,
                "Chưa bật được từ đây — đăng xuất/đăng nhập rồi bật trong ứng dụng Extensions, hoặc chạy: gnome-extensions enable mangrs-focus@fundokukiri"
            );
        }
    }
    Ok(note)
}

const DESKTOP_ENTRY: &str = include_str!("../../data/org.freedesktop.IBus.mangrs.desktop");

/// The user-level override that hides the settings app from the grid.
fn desktop_override_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("applications/org.freedesktop.IBus.mangrs.desktop"))
}

fn desktop_icon_hidden() -> bool {
    desktop_override_path().is_some_and(|p| p.exists())
}

/// Is the desktop entry installed system-wide? A dev run without
/// `install.sh` has nothing to override, so the row goes insensitive.
fn system_desktop_entry_exists() -> bool {
    ["/usr/share/applications", "/usr/local/share/applications"]
        .iter()
        .any(|d| {
            std::path::Path::new(d)
                .join("org.freedesktop.IBus.mangrs.desktop")
                .exists()
        })
}

/// Hide/show "MăngRS Cấu hình" in the application grid by writing or
/// removing a user-level override of the installed desktop entry. The
/// override is a full copy plus `Hidden=true` — the highest-priority entry
/// file replaces the system one, the spec does not merge keys.
fn set_desktop_icon_hidden(hide: bool) -> Result<(), String> {
    let Some(path) = desktop_override_path() else {
        return Err("Không tìm thấy thư mục ~/.local/share/applications".into());
    };
    let outcome = if hide {
        let mut text = DESKTOP_ENTRY.to_string();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("Hidden=true\n");
        std::fs::write(&path, text).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    };
    outcome.map_err(|e| format!("Không ghi được file override: {e}"))?;
    // Best effort: gio keeps a per-directory desktop-file cache.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::process::Command::new("update-desktop-database")
            .arg(parent)
            .spawn();
    }
    Ok(())
}

fn text_of(view: &TextView) -> String {
    view.buffer()
        .text(
            &view.buffer().start_iter(),
            &view.buffer().end_iter(),
            false,
        )
        .to_string()
}

fn set_text(view: &TextView, text: &str) {
    view.buffer().set_text(text);
}

// --- backup / restore (Giai đoạn 11) ---

fn export_json(path: Option<PathBuf>) -> Result<(), String> {
    let Some(path) = path else {
        return Err("no file chosen".into());
    };
    let text = crate::backup::to_json(
        Config::load(),
        Macros::load()
            .entries()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
        Dictionary::load().words().map(str::to_string).collect(),
    )?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn import_json(path: Option<PathBuf>, ui: &Rc<Ui>) -> Result<(), String> {
    let Some(path) = path else {
        return Err("no file chosen".into());
    };
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let backup = crate::backup::from_json(&text)?;
    // Nothing is written before this line: a corrupt file changes nothing.
    backup.config.save().map_err(|e| e.to_string())?;
    let mut macros = Macros::load();
    macros.set_entries(backup.macros.into_iter());
    macros.save().map_err(|e| e.to_string())?;
    let mut dict = Dictionary::load();
    dict.set_words(backup.dictionary.into_iter());
    dict.save().map_err(|e| e.to_string())?;
    // Every widget, not just the visible ones: a stale switch is written
    // back over the imported file by the next save.
    let config = Config::load();
    ui.refresh(&config, &macros, &dict);
    Ok(())
}
