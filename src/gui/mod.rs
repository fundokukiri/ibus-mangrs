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
    BackspaceToneConfig, BracketConfig, CharsetConfig, Config, MenuNode, MethodConfig, ModeConfig,
    W2uConfig,
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

use text::{is_english, set_language, t};

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
    // Before any widget text resolves: t() reads the per-process language.
    set_language(&Config::load().language);
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

/// (Vietnamese, English) display labels for the combo item lists. POSITION
/// is the index contract between the item list and refresh/read — pairs are
/// never reordered, only rendered in the window's language via `labels`.
const METHODS: &[(&str, &str)] = &[
    ("Telex", "Telex"),
    ("Telex 2", "Telex 2"),
    ("Telex W", "Telex W"),
    ("Telex + VNI", "Telex + VNI"),
    ("Telex + VNI + VIQR", "Telex + VNI + VIQR"),
    ("VNI", "VNI"),
    ("VNI bàn phím Pháp", "VNI (French keyboard)"),
    ("VIQR", "VIQR"),
    ("Microsoft layout", "Microsoft layout"),
];
const MODES: &[(&str, &str)] = &[
    ("Không gạch chân", "No underline"),
    ("Gạch chân (preedit)", "Underline (preedit)"),
    ("Surrounding text", "Surrounding text"),
    ("Flow (uinput)", "Flow (uinput)"),
    ("Turbo (uinput)", "Turbo (uinput)"),
    ("Select (uinput)", "Select (uinput)"),
    ("Forward Backspace", "Forward Backspace"),
    ("Forward Selection", "Forward Selection"),
    ("OFF", "OFF"),
];
const CHARSETS: &[(&str, &str)] = &[
    ("Unicode", "Unicode"),
    ("Unicode tổ hợp", "Unicode (combining)"),
    ("TCVN3", "TCVN3"),
    ("VNI", "VNI"),
    ("VIQR", "VIQR"),
    ("VISCII", "VISCII"),
    ("VPS", "VPS"),
    ("Windows 1258", "Windows 1258"),
    ("BKHCM", "BKHCM"),
    ("Vietware X", "Vietware X"),
    ("Vietware Full", "Vietware Full"),
];
const ON_OFF_MODES: &[(&str, &str)] = &[
    ("Tắt", "Off"),
    ("Trừ đầu từ", "Not at word start"),
    ("Mọi vị trí", "Everywhere"),
];
const BACKSPACE_TONE_MODES: &[(&str, &str)] = &[
    ("Xóa dấu trước", "Tone first"),
    ("Xóa cả dấu và chữ", "Whole character"),
    ("Unikey (lùi từng phím)", "Unikey (walk back)"),
];
/// The experimental nav-key behaviors while a preedit commit is pending, in
/// `nav_key_while_preedit` config-value order (swallow/forward/replay); the
/// combo index is the mapping contract for `refresh`/`read`.
const NAV_KEY_MODES: &[(&str, &str)] = &[
    ("Nuốt phím đầu (mặc định)", "Swallow first press (default)"),
    (
        "Chuyển caret ngay (kiểu bamboo)",
        "Move the caret at once (bamboo-style)",
    ),
    ("Phát lại phím qua tín hiệu", "Replay the key as a signal"),
];
const CHANNELS: &[(&str, &str)] = &[
    ("CommitText (an toàn)", "CommitText (safe)"),
    ("Clipboard (Ctrl+V)", "Clipboard (Ctrl+V)"),
];

/// The combo item labels for `pairs` in the window's language.
pub(crate) fn labels<'a>(pairs: &'a [(&'a str, &'a str)]) -> Vec<&'a str> {
    let en = is_english();
    pairs.iter().map(|p| if en { p.1 } else { p.0 }).collect()
}
/// The $TIME/$DATE strftime presets, the shipped defaults first; a format
/// the old free-text rows saved that matches none of these is appended as
/// its own item by `select_format` instead of being rewritten.
const TIME_FORMATS: &[&str] = &["%H:%M", "%H:%M:%S", "%Hh%M", "%I:%M %p"];
const DATE_FORMATS: &[&str] = &["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%Y-%m-%d", "%d/%m/%y"];
/// The interface languages, in combo order. The labels are proper names —
/// they stay the same in every language.
const LANGUAGES: &[&str] = &["Tiếng Việt", "English"];

/// The custom-shortcut functions, in `config::CUSTOM_HOTKEY_ACTIONS` order:
/// (action key, gui-content key) plus the code fallback labels — the
/// add-row dropdown and the row subtitles read these. "enabled" and "rule"
/// have their own default rows on this page; the small toggles the owner
/// trimmed stay assignable only from old configs.
const CUSTOM_ACTION_KEYS: &[(&str, &str)] = &[
    ("mode", "actions.hotkey_mode"),
    ("emoji", "actions.hotkey_emoji"),
    ("charset", "actions.hotkey_charset"),
    ("method", "actions.hotkey_method"),
    ("auto_capitalize", "actions.hotkey_auto_capitalize"),
    ("abbreviations", "actions.hotkey_abbrev"),
    ("auto_restore", "actions.hotkey_auto_restore"),
    ("auto_correct", "actions.hotkey_auto_correct"),
    ("bypass_url", "actions.hotkey_bypass_url"),
    ("dictionary", "actions.hotkey_dictionary"),
    ("double_space", "actions.hotkey_double_space"),
    ("em_dash", "actions.hotkey_em_dash"),
    ("macro_editor", "actions.hotkey_macro_editor"),
    ("settings", "actions.hotkey_settings"),
];
const CUSTOM_ACTION_DEFAULTS: &[&str] = &[
    "Đổi chế độ gõ",
    "Mở bảng emoji",
    "Xoay bảng mã",
    "Xoay kiểu gõ",
    "Tự viết hoa",
    "Viết tắt",
    "Khôi phục từ nước ngoài",
    "Từ sai trả phím gốc",
    "Không dấu sau URL",
    "Từ điển của tôi",
    "2 dấu cách = chấm",
    "Gạch ngang dài",
    "Mở trang Gõ tắt",
    "Mở Cài đặt",
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
    extra_url_chars_switch: adw::SwitchRow,
    extra_url_chars_entry: adw::EntryRow,
    w2u: adw::ComboRow,
    backspace_tone: adw::ComboRow,
    /// Experimental: what the first navigation key does while a preedit
    /// commit is pending (the "Thử nghiệm" tab).
    nav_key_while_preedit: adw::ComboRow,
    bracket: adw::ComboRow,
    em_dash: adw::SwitchRow,
    auto_capitalize: adw::SwitchRow,
    auto_capitalize_after_switch: adw::SwitchRow,
    auto_capitalize_after_entry: adw::EntryRow,
    double_space: adw::SwitchRow,
    macro_enabled: adw::SwitchRow,
    capitalize_macro: adw::SwitchRow,
    macro_in_off_mode: adw::SwitchRow,
    macro_valid_words: adw::SwitchRow,
    time_format: adw::ComboRow,
    date_format: adw::ComboRow,
    /// The interface language; picking it rebuilds the window (content.rs).
    language: adw::ComboRow,
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
    /// The page's toast host: add/copy confirmations and delete-undo. It is
    /// the wrapper the rules page returns, so toasts are actually visible.
    rules_overlay: adw::ToastOverlay,
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
    /// Programmatic page jump: select_row drives the same row_selected
    /// handler the clicks use. `None` headless.
    sidebar_select: RefCell<Option<PageJump>>,
    /// "Thử luật" input, its verdict row and the verdict's status icon.
    rule_preview_input: adw::EntryRow,
    rule_preview: adw::ActionRow,
    rule_preview_icon: gtk4::Image,
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
    /// Second log tier: typed-text lines (verify diffs, surrounding content).
    /// Only effective while `debug_log` is on — see `Config::debug_log_text`.
    debug_log_text: adw::SwitchRow,
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
    clipboard_restore_switch: adw::SwitchRow,
    clipboard_restore_spin: adw::SpinRow,
    echo_stale_switch: adw::SwitchRow,
    echo_stale_spin: adw::SpinRow,
    idle_commit_switch: adw::SwitchRow,
    idle_commit_spin: adw::SpinRow,
    smooth_per_key_switch: adw::SwitchRow,
    smooth_per_key_spin: adw::SpinRow,
    smooth_backspace_retrack: adw::SwitchRow,
    bypass_hyphen: adw::SwitchRow,
    experimental_features: adw::SwitchRow,
    show_toasts: adw::SwitchRow,
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
/// A programmatic jump to a settings page by its stack name — the closure
/// lives in `content` where the sidebar rows do; the gate uses it to land
/// the user somewhere sane after closing the tab they stood on.
type PageJump = Rc<dyn Fn(&str)>;

/// The combo-row index for a `nav_key_while_preedit` value — shared by
/// refresh, the gate's defaults display and its restore.
fn nav_key_selected(v: &str) -> u32 {
    match v {
        "forward" => 1,
        "replay" => 2,
        _ => 0,
    }
}

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
        ModeConfig::NonPreedit => 0,
        ModeConfig::Preedit => 1,
        ModeConfig::Surrounding => 2,
        ModeConfig::Smooth => 3,
        ModeConfig::SuperSmooth => 4,
        ModeConfig::Select => 5,
        ModeConfig::ForwardKeyEvent => 6,
        ModeConfig::ForwardSelect => 7,
        ModeConfig::Off => 8,
    }
}

/// Inverse of `mode_index`, shared by the main mode dropdown and the
/// App Rules rows.
fn mode_at(i: u32) -> ModeConfig {
    match i {
        0 => ModeConfig::NonPreedit,
        1 => ModeConfig::Preedit,
        2 => ModeConfig::Surrounding,
        3 => ModeConfig::Smooth,
        4 => ModeConfig::SuperSmooth,
        5 => ModeConfig::Select,
        6 => ModeConfig::ForwardKeyEvent,
        7 => ModeConfig::ForwardSelect,
        8 => ModeConfig::Off,
        // Out of range: the factory default.
        _ => ModeConfig::Preedit,
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

/// Select `value` in a format combo: the matching preset's index, or — for
/// a format stored by the old free-text rows — a temporary extra item
/// appended to the end, so no stored format is rewritten on save.
fn select_format(row: &adw::ComboRow, presets: &[&str], value: &str) {
    let list = StringList::new(presets);
    let index = match presets.iter().position(|p| *p == value) {
        Some(i) => i as u32,
        None => {
            list.append(value);
            list.n_items() - 1
        }
    };
    row.set_model(Some(&list));
    row.set_selected(index);
}

/// The pattern the combo currently shows: presets and appended values read
/// back the same way, the model being the single source of truth.
pub(crate) fn format_selected(row: &adw::ComboRow) -> String {
    row.selected_item()
        .and_downcast::<gtk4::StringObject>()
        .map(|o| o.string().into())
        .unwrap_or_default()
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
        // Same for the language: read() always produces "vi"/"en", so a
        // hand-edited value must normalize here or the round-trip assert
        // below fails.
        if config.language != "en" {
            config.language = "vi".into();
        }
        // Same for the nav-key choice: read() always produces a known
        // value, so a hand-edited one normalizes to the shipped swallow.
        if !matches!(
            config.nav_key_while_preedit.as_str(),
            "swallow" | "forward" | "replay"
        ) {
            config.nav_key_while_preedit = "swallow".into();
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
        self.extra_url_chars_switch
            .set_active(config.extra_url_chars_enabled);
        self.extra_url_chars_entry
            .set_sensitive(config.extra_url_chars_enabled);
        // The typed characters stay saved even while the switch is off —
        // they reapply when the switch returns; the off state itself
        // applies the built-in set only.
        self.extra_url_chars_entry.set_text(&config.extra_url_chars);
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
        self.backspace_tone
            .set_selected(match config.backspace_tone {
                BackspaceToneConfig::ToneFirst => 0,
                BackspaceToneConfig::WholeChar => 1,
                BackspaceToneConfig::UnikeyWalk => 2,
            });
        self.nav_key_while_preedit
            .set_selected(nav_key_selected(&config.nav_key_while_preedit));
        self.em_dash.set_active(config.em_dash);
        self.auto_capitalize.set_active(config.auto_capitalize);
        self.auto_capitalize_after_switch
            .set_active(config.auto_capitalize_after_enabled);
        self.auto_capitalize_after_entry
            .set_sensitive(config.auto_capitalize_after_enabled);
        self.auto_capitalize_after_entry
            .set_text(&config.auto_capitalize_after);
        self.double_space.set_active(config.double_space_period);
        self.macro_enabled.set_active(config.macro_enabled);
        self.capitalize_macro.set_active(config.capitalize_macro);
        self.macro_in_off_mode.set_active(config.macro_in_off_mode);
        self.macro_valid_words
            .set_active(config.macro_expand_valid_words);
        select_format(&self.time_format, TIME_FORMATS, &config.time_format);
        select_format(&self.date_format, DATE_FORMATS, &config.date_format);
        self.language
            .set_selected(u32::from(config.language == "en"));
        self.emoji_enabled.set_active(config.emoji_enabled);
        *self.emoji_hotkey_value.borrow_mut() = config.emoji_hotkey.clone();
        self.emoji_hotkey_display.render(&config.emoji_hotkey);
        self.dictionary_enabled
            .set_active(config.dictionary_enabled);
        self.debug_log.set_active(config.debug_log);
        self.debug_log_text.set_active(config.debug_log_text);
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
        self.clipboard_restore_switch
            .set_active(config.clipboard_restore_delay_enabled);
        self.clipboard_restore_spin
            .set_sensitive(config.clipboard_restore_delay_enabled);
        // The typed value stays saved even while the switch is off — it
        // reapplies when the switch returns; the off state itself applies
        // the built-in default (the engine's flag-aware getters).
        self.clipboard_restore_spin
            .set_value(config.clipboard_restore_delay_ms as f64);
        self.echo_stale_switch.set_active(config.echo_stale_enabled);
        self.echo_stale_spin
            .set_sensitive(config.echo_stale_enabled);
        self.echo_stale_spin.set_value(config.echo_stale_ms as f64);
        self.idle_commit_switch
            .set_active(config.idle_commit_enabled);
        self.idle_commit_spin
            .set_sensitive(config.idle_commit_enabled);
        self.idle_commit_spin
            .set_value(config.idle_commit_ms as f64);
        self.smooth_per_key_switch.set_active(config.smooth_per_key);
        // Same shape as the other switch+spin pairs: the stored value stays
        // visible and locked while the switch is off.
        self.smooth_per_key_spin
            .set_sensitive(config.smooth_per_key);
        self.smooth_per_key_spin
            .set_value(config.smooth_per_key_delay_ms as f64);
        self.smooth_backspace_retrack
            .set_active(config.smooth_backspace_retrack);
        self.bypass_hyphen.set_active(config.bypass_hyphen);
        self.experimental_features
            .set_active(config.experimental_features);
        self.show_toasts.set_active(config.show_toasts);
        // The master gate: off locks every experimental row and displays
        // the shipped defaults — config.toml keeps the user's values (the
        // save funnel skips the experimental fields) and the engine bakes
        // the defaults in, so what is on screen is what typing does.
        self.set_experimental_sensitive(config.experimental_features);
        if !config.experimental_features {
            self.show_experimental_defaults();
        }
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
        // The gate is off: the experimental widgets display the shipped
        // defaults, and the user's saved values must survive any save — the
        // file's own values win over the widgets for those fields.
        let gate_on = self.experimental_features.is_active();
        let saved_experimental = config.clone();
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
        config.debug_log_text = self.debug_log_text.is_active();
        config.x11_delete_without_report = self.x11_delete.is_active();
        config.preedit_for_xim = self.preedit_xim.is_active();
        config.modern_tone_style = self.modern.is_active();
        config.free_tone_marking = self.free.is_active();
        config.auto_correct = self.auto_correct.is_active();
        config.auto_restore = self.auto_restore.is_active();
        config.abbreviations = self.abbrev.is_active();
        config.hide_underline = self.hide_underline.is_active();
        config.bypass_url = self.bypass_url.is_active();
        config.extra_url_chars_enabled = self.extra_url_chars_switch.is_active();
        config.extra_url_chars = self.extra_url_chars_entry.text().trim().to_string();
        config.w2u = match self.w2u.selected() {
            1 => W2uConfig::NonStart,
            2 => W2uConfig::Everywhere,
            _ => W2uConfig::Disabled,
        };
        config.backspace_tone = match self.backspace_tone.selected() {
            1 => BackspaceToneConfig::WholeChar,
            2 => BackspaceToneConfig::UnikeyWalk,
            _ => BackspaceToneConfig::ToneFirst,
        };
        config.nav_key_while_preedit = match self.nav_key_while_preedit.selected() {
            1 => "forward".into(),
            2 => "replay".into(),
            _ => "swallow".into(),
        };
        config.bracket = match self.bracket.selected() {
            1 => BracketConfig::NonStart,
            2 => BracketConfig::Everywhere,
            _ => BracketConfig::Disabled,
        };
        config.em_dash = self.em_dash.is_active();
        config.auto_capitalize = self.auto_capitalize.is_active();
        config.auto_capitalize_after_enabled = self.auto_capitalize_after_switch.is_active();
        config.auto_capitalize_after = self.auto_capitalize_after_entry.text().trim().to_string();
        config.double_space_period = self.double_space.is_active();
        config.macro_enabled = self.macro_enabled.is_active();
        config.capitalize_macro = self.capitalize_macro.is_active();
        config.macro_in_off_mode = self.macro_in_off_mode.is_active();
        config.macro_expand_valid_words = self.macro_valid_words.is_active();
        config.time_format = format_selected(&self.time_format);
        config.date_format = format_selected(&self.date_format);
        config.language = if self.language.selected() == 1 {
            "en"
        } else {
            "vi"
        }
        .into();
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
        config.clipboard_restore_delay_enabled = self.clipboard_restore_switch.is_active();
        config.clipboard_restore_delay_ms = self.clipboard_restore_spin.value() as u64;
        config.echo_stale_enabled = self.echo_stale_switch.is_active();
        config.echo_stale_ms = self.echo_stale_spin.value() as u64;
        config.idle_commit_enabled = self.idle_commit_switch.is_active();
        config.idle_commit_ms = self.idle_commit_spin.value() as u64;
        config.smooth_per_key = self.smooth_per_key_switch.is_active();
        config.smooth_per_key_delay_ms = self.smooth_per_key_spin.value() as u64;
        config.smooth_backspace_retrack = self.smooth_backspace_retrack.is_active();
        config.bypass_hyphen = self.bypass_hyphen.is_active();
        config.experimental_features = self.experimental_features.is_active();
        config.show_toasts = self.show_toasts.is_active();
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
        if !gate_on {
            config.extra_url_chars_enabled = saved_experimental.extra_url_chars_enabled;
            config.extra_url_chars = saved_experimental.extra_url_chars;
            config.auto_capitalize_after_enabled = saved_experimental.auto_capitalize_after_enabled;
            config.auto_capitalize_after = saved_experimental.auto_capitalize_after;
            config.macro_expand_valid_words = saved_experimental.macro_expand_valid_words;
            config.clipboard_restore_delay_enabled =
                saved_experimental.clipboard_restore_delay_enabled;
            config.clipboard_restore_delay_ms = saved_experimental.clipboard_restore_delay_ms;
            config.echo_stale_enabled = saved_experimental.echo_stale_enabled;
            config.echo_stale_ms = saved_experimental.echo_stale_ms;
            config.idle_commit_enabled = saved_experimental.idle_commit_enabled;
            config.idle_commit_ms = saved_experimental.idle_commit_ms;
            config.nav_key_while_preedit = saved_experimental.nav_key_while_preedit;
            config.smooth_per_key_delay_ms = saved_experimental.smooth_per_key_delay_ms;
            config.smooth_backspace_retrack = saved_experimental.smooth_backspace_retrack;
            config.bypass_hyphen = saved_experimental.bypass_hyphen;
        }
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

    /// Lock or unlock every experimental row for the master gate — the tab
    /// itself always stays visible now. Locking is display only: the saved
    /// switch states and values stay in config.toml and come back verbatim
    /// when the gate reopens; the engine forces the features off while the
    /// gate is closed.
    pub(crate) fn set_experimental_sensitive(&self, on: bool) {
        let rows: [&gtk4::Widget; 15] = [
            self.macro_valid_words.upcast_ref(),
            self.extra_url_chars_switch.upcast_ref(),
            self.extra_url_chars_entry.upcast_ref(),
            self.auto_capitalize_after_switch.upcast_ref(),
            self.auto_capitalize_after_entry.upcast_ref(),
            self.clipboard_restore_switch.upcast_ref(),
            self.clipboard_restore_spin.upcast_ref(),
            self.echo_stale_switch.upcast_ref(),
            self.echo_stale_spin.upcast_ref(),
            self.idle_commit_switch.upcast_ref(),
            self.idle_commit_spin.upcast_ref(),
            self.smooth_per_key_spin.upcast_ref(),
            self.smooth_backspace_retrack.upcast_ref(),
            self.bypass_hyphen.upcast_ref(),
            self.nav_key_while_preedit.upcast_ref(),
        ];
        for row in rows {
            row.set_sensitive(on);
        }
        if on {
            // The entry/spin twins follow their own switch again — a twin
            // stays locked while its switch is off, always inside the gate.
            let gate = self.experimental_features.is_active();
            self.clipboard_restore_spin
                .set_sensitive(gate && self.clipboard_restore_switch.is_active());
            self.echo_stale_spin
                .set_sensitive(gate && self.echo_stale_switch.is_active());
            self.idle_commit_spin
                .set_sensitive(gate && self.idle_commit_switch.is_active());
            self.smooth_per_key_spin
                .set_sensitive(gate && self.smooth_per_key_switch.is_active());
            self.extra_url_chars_entry
                .set_sensitive(gate && self.extra_url_chars_switch.is_active());
            self.auto_capitalize_after_entry
                .set_sensitive(gate && self.auto_capitalize_after_switch.is_active());
        }
    }

    /// Display the shipped defaults on the experimental rows while the gate
    /// is off — what the user sees is what typing does (the engine bakes the
    /// defaults in too). The swap runs under suppress_save: these are display
    /// values, and config.toml keeps the user's saved ones (read() reverts
    /// the experimental fields to the file while the gate is off).
    pub(crate) fn show_experimental_defaults(&self) {
        let d = crate::config::Config::default();
        self.suppress_save.set(true);
        self.macro_valid_words
            .set_active(d.macro_expand_valid_words);
        self.extra_url_chars_switch
            .set_active(d.extra_url_chars_enabled);
        self.extra_url_chars_entry.set_text(&d.extra_url_chars);
        self.auto_capitalize_after_switch
            .set_active(d.auto_capitalize_after_enabled);
        self.auto_capitalize_after_entry
            .set_text(&d.auto_capitalize_after);
        self.clipboard_restore_switch
            .set_active(d.clipboard_restore_delay_enabled);
        self.clipboard_restore_spin
            .set_value(d.clipboard_restore_delay_ms as f64);
        self.echo_stale_switch.set_active(d.echo_stale_enabled);
        self.echo_stale_spin.set_value(d.echo_stale_ms as f64);
        self.idle_commit_switch.set_active(d.idle_commit_enabled);
        self.idle_commit_spin.set_value(d.idle_commit_ms as f64);
        self.smooth_per_key_spin
            .set_value(d.smooth_per_key_delay_ms as f64);
        self.smooth_backspace_retrack
            .set_active(d.smooth_backspace_retrack);
        self.bypass_hyphen.set_active(d.bypass_hyphen);
        self.nav_key_while_preedit
            .set_selected(nav_key_selected(&d.nav_key_while_preedit));
        self.suppress_save.set(false);
    }

    /// Put the user's saved values back on the experimental rows when the
    /// gate reopens — from the file, which read() never let the displayed
    /// defaults overwrite. Same suppress_save discipline as the swap out.
    pub(crate) fn restore_experimental_saved(&self) {
        let c = crate::config::Config::load();
        self.suppress_save.set(true);
        self.macro_valid_words
            .set_active(c.macro_expand_valid_words);
        self.extra_url_chars_switch
            .set_active(c.extra_url_chars_enabled);
        self.extra_url_chars_entry.set_text(&c.extra_url_chars);
        self.auto_capitalize_after_switch
            .set_active(c.auto_capitalize_after_enabled);
        self.auto_capitalize_after_entry
            .set_text(&c.auto_capitalize_after);
        self.clipboard_restore_switch
            .set_active(c.clipboard_restore_delay_enabled);
        self.clipboard_restore_spin
            .set_value(c.clipboard_restore_delay_ms as f64);
        self.echo_stale_switch.set_active(c.echo_stale_enabled);
        self.echo_stale_spin.set_value(c.echo_stale_ms as f64);
        self.idle_commit_switch.set_active(c.idle_commit_enabled);
        self.idle_commit_spin.set_value(c.idle_commit_ms as f64);
        self.smooth_per_key_spin
            .set_value(c.smooth_per_key_delay_ms as f64);
        self.smooth_backspace_retrack
            .set_active(c.smooth_backspace_retrack);
        self.bypass_hyphen.set_active(c.bypass_hyphen);
        self.nav_key_while_preedit
            .set_selected(nav_key_selected(&c.nav_key_while_preedit));
        self.suppress_save.set(false);
    }

    /// True while the "Thông báo nhanh" switch is on — the single gate
    /// every toast goes through. Off silences the settings window
    /// completely, the delete-undo toasts included; error dialogs are not
    /// toasts and always show.
    fn toasts_on(&self) -> bool {
        self.show_toasts.is_active()
    }

    /// The one funnel for plain toast notifications: shows `text` unless
    /// the user silenced toasts.
    fn fire_toast(&self, overlay: &adw::ToastOverlay, text: &str) {
        if self.toasts_on() {
            overlay.add_toast(adw::Toast::new(text));
        }
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

/// Is the focus extension already installed (its directory exists)? The
/// extension row shows the status and says "Cài lại" instead of "Cài đặt".
pub(super) fn focus_extension_installed() -> bool {
    dirs::data_dir().is_some_and(|d| d.join(FOCUS_EXT_DIR).join("extension.js").exists())
}

/// The focus extension only makes sense inside GNOME: hide the installer
/// group on other desktops.
pub(super) fn session_is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_ascii_uppercase().contains("GNOME"))
}

/// A minimal STORED (uncompressed) zip wrapping the extension files: the
/// official channel, `gnome-extensions install FILE.zip`, only takes zips,
/// and hand-rolling the container keeps the install copy-free (GNOME unpacks
/// it itself) without a zip crate for two small files.
fn extension_zip(entries: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }
    fn put16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn put32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    // A fixed DOS stamp (2026-11-08 12:00): the format demands one, the
    // content is what matters, and a constant keeps the zip deterministic.
    const TIME: u16 = 12 << 11;
    const DATE: u16 = (46 << 9) | (11 << 5) | 8;

    let mut out = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offset = 0u32;
    for (name, body) in entries {
        let crc = crc32(body.as_bytes());
        let size = body.len() as u32;
        let name = name.as_bytes();
        let header_at = offset;
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 20);
        put16(&mut out, 0);
        put16(&mut out, 0); // stored, no compression
        put16(&mut out, TIME);
        put16(&mut out, DATE);
        put32(&mut out, crc);
        put32(&mut out, size);
        put32(&mut out, size);
        put16(&mut out, name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(body.as_bytes());

        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20);
        put16(&mut central, 20);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, TIME);
        put16(&mut central, DATE);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, name.len() as u16);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, header_at);
        central.extend_from_slice(name);
        offset += 30 + name.len() as u32 + size;
    }
    let central_at = out.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, entries.len() as u16);
    put16(&mut out, entries.len() as u16);
    put32(&mut out, central.len() as u32);
    put32(&mut out, central_at);
    put16(&mut out, 0);
    Ok(out)
}

/// Install the embedded focus extension through the official channel —
/// `gnome-extensions install FILE.zip`, the same tool the Extensions apps
/// call underneath — never a manual copy into the extension directory.
/// GNOME 45+ caches extension ES modules, so a fresh install only loads
/// after logout/login; the caller surfaces the logout button on first
/// install.
pub(super) fn install_focus_extension() -> Result<String, String> {
    use std::fmt::Write as _;
    let zip = extension_zip(&[
        ("extension.js", FOCUS_EXTENSION_JS),
        ("metadata.json", FOCUS_EXTENSION_META),
    ])?;
    let tmp = std::env::temp_dir().join(format!("mangrs-focus-{}.zip", std::process::id()));
    std::fs::write(&tmp, zip).map_err(|e| format!("Không ghi được {}: {e}", tmp.display()))?;
    let installed = std::process::Command::new("gnome-extensions")
        .args(["install", "--force"])
        .arg(&tmp)
        .output();
    let _ = std::fs::remove_file(&tmp);
    match installed {
        Ok(s) if s.status.success() => {}
        Ok(s) => {
            return Err(format!(
                "gnome-extensions install thất bại: {}",
                String::from_utf8_lossy(&s.stderr).trim()
            ))
        }
        Err(e) => return Err(format!("Không chạy được gnome-extensions: {e}")),
    }
    let mut note = String::from("Đã cài qua gnome-extensions (kênh chính thức của GNOME). ");
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
                "Chưa bật được từ đây — mở ứng dụng Extensions và bật, hoặc chạy: gnome-extensions enable mangrs-focus@fundokukiri"
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
        crate::emoji::UserEmoji::load()
            .entries()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
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
    // Version 1 files carry no emoji section: restoring one must leave the
    // user's `emoji.toml` alone instead of wiping it with an empty table.
    if backup.version >= 2 {
        let mut emoji = crate::emoji::UserEmoji::load();
        emoji.set_entries(backup.emoji.into_iter());
        emoji.save().map_err(|e| e.to_string())?;
    }
    // Every widget, not just the visible ones: a stale switch is written
    // back over the imported file by the next save. Suppressed like
    // `external_reload` — programmatic updates fire the row handlers, and
    // an unsuppressed save mid-refresh would write back the pre-import
    // values still held by the not-yet-refreshed widgets.
    let config = Config::load();
    ui.suppress_save.set(true);
    ui.refresh(&config, &macros, &dict);
    ui.suppress_save.set(false);
    Ok(())
}
