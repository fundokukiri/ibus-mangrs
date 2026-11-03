//! GTK4 settings window, opened from the popup ("Cấu hình") or with
//! `-gui`.
//!
//! Every control writes straight to `config.toml` (and `macros.toml` /
//! `dictionary.txt`); the engine watches the config file mtime, so changes
//! apply within a second without restarting ibus-daemon.
//!
//! Layout: header bar with [search] [pill tabs] [close], five pages
//! (Kiểu gõ / Dấu & Sửa lỗi / Gõ tắt / Dữ liệu / Giới thiệu), grouped cards.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::{
    BracketConfig, CharsetConfig, Config, MenuNode, MethodConfig, ModeConfig, W2uConfig,
};
use crate::macros::{Dictionary, Macros};
use gtk4::prelude::*;
// TreeView/TreeStore/ComboBoxText are deprecated since GTK 4.10 in favor of
// ListView/DropDown, but they are the pragmatic editor widgets here: a tree
// editor on ListView+TreeListModel would triple the code for no user gain.
#[allow(deprecated)]
use gtk4::{
    Application, ApplicationWindow, CellRendererText, ComboBoxText, DropDown, Entry, Orientation,
    Revealer, StringList, Switch, TextView, TreeIter, TreeStore, TreeView, TreeViewColumn,
};

thread_local! {
    /// Every settings row (lowercase "title subtitle", widget) so the search
    /// bar can hide the ones that do not match.
    static ROWS: RefCell<Vec<(String, gtk4::Widget)>> = const { RefCell::new(Vec::new()) };
    /// Each collapsible section's revealer plus its arrow label: the search
    /// opens all of them while a query is active (rows inside a collapsed
    /// section could never be found) and closes them again afterwards.
    static COLLAPSIBLES: RefCell<Vec<(Revealer, gtk4::Label)>> =
        const { RefCell::new(Vec::new()) };
}

// Panel-menu editor store columns.
const MENU_COL_KIND: u32 = 0;
const MENU_COL_KEY: u32 = 1;
const MENU_COL_LABEL: u32 = 2;
const MENU_COL_KIND_LABEL: u32 = 3;

/// (kind id, key, display label, Vietnamese kind label) for one node.
fn menu_row(node: &MenuNode) -> (String, String, String, String) {
    match node {
        MenuNode::Toggle { key } => (
            "toggle".into(),
            key.clone(),
            menu_toggle_label(key),
            "Bật/tắt".into(),
        ),
        MenuNode::Radios { group, .. } => (
            "radios".into(),
            group.clone(),
            menu_radios_label(group),
            "Nhóm chọn".into(),
        ),
        MenuNode::Action { key } => (
            "action".into(),
            key.clone(),
            menu_action_label(key),
            "Chức năng".into(),
        ),
        MenuNode::Separator => (
            "sep".into(),
            String::new(),
            "— phân cách —".into(),
            "Gạch ngang".into(),
        ),
        MenuNode::Folder { label, .. } => (
            "folder".into(),
            String::new(),
            label.clone(),
            "Thư mục".into(),
        ),
    }
}

fn menu_toggle_label(key: &str) -> String {
    if key == "enabled" {
        return "Bật tiếng Việt".into();
    }
    crate::ibus_prop::quick_toggle(key)
        .map(|t| t.label.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn menu_radios_label(group: &str) -> String {
    match group {
        "charset" => "Bảng mã".into(),
        "method" => "Kiểu gõ".into(),
        "display" => "Chế độ gõ".into(),
        other => other.into(),
    }
}

fn menu_action_label(key: &str) -> String {
    match key {
        "macro_editor" => "Mở bảng gõ tắt…".into(),
        "settings" => "Cấu hình".into(),
        "about" => "Giới thiệu (About)".into(),
        other => other.into(),
    }
}

/// (display label, kind label) for a picker id "kind:key".
fn menu_item_label(kind: &str, key: &str) -> (String, String) {
    match kind {
        "toggle" => (menu_toggle_label(key), "Bật/tắt".into()),
        "radios" => (menu_radios_label(key), "Nhóm chọn".into()),
        "action" => (menu_action_label(key), "Chức năng".into()),
        _ => ("— phân cách —".into(), "Gạch ngang".into()),
    }
}

/// Canonical combo string: modifiers first (Shift/Control/Alt/Super), then
/// the key name when one is given. Matches `server::parse_hotkey`.
fn hotkey_spec(full: gtk4::gdk::ModifierType, key_name: Option<String>) -> String {
    let mut spec = String::new();
    for (mask, name) in [
        (gtk4::gdk::ModifierType::SHIFT_MASK, "Shift"),
        (gtk4::gdk::ModifierType::CONTROL_MASK, "Control"),
        (gtk4::gdk::ModifierType::ALT_MASK, "Alt"),
        (gtk4::gdk::ModifierType::SUPER_MASK, "Super"),
        (gtk4::gdk::ModifierType::HYPER_MASK, "Super"),
        (gtk4::gdk::ModifierType::META_MASK, "Super"),
    ] {
        if full.contains(mask) && !spec.contains(&format!("{name}+")) {
            spec.push_str(name);
            spec.push('+');
        }
    }
    if let Some(k) = key_name {
        spec.push_str(&k);
    }
    spec
}

/// The modifier a pressed/released key belongs to, in gdk mask terms.
fn gdk_modifier_of_key(key: gtk4::gdk::Key) -> Option<gtk4::gdk::ModifierType> {
    Some(match key {
        gtk4::gdk::Key::Shift_L | gtk4::gdk::Key::Shift_R => gtk4::gdk::ModifierType::SHIFT_MASK,
        gtk4::gdk::Key::Control_L | gtk4::gdk::Key::Control_R => {
            gtk4::gdk::ModifierType::CONTROL_MASK
        }
        gtk4::gdk::Key::Alt_L | gtk4::gdk::Key::Alt_R => gtk4::gdk::ModifierType::ALT_MASK,
        gtk4::gdk::Key::Super_L | gtk4::gdk::Key::Super_R => gtk4::gdk::ModifierType::SUPER_MASK,
        gtk4::gdk::Key::Hyper_L | gtk4::gdk::Key::Hyper_R => gtk4::gdk::ModifierType::HYPER_MASK,
        gtk4::gdk::Key::Meta_L | gtk4::gdk::Key::Meta_R => gtk4::gdk::ModifierType::META_MASK,
        _ => return None,
    })
}

/// Show a hotkey value on its capture button; empty means unassigned.
fn render_hotkey(btn: &gtk4::Button, value: &str) {
    btn.set_label(if value.is_empty() {
        "Chưa gán"
    } else {
        value
    });
}

/// Turn a hotkey button into a recorder: click it, press a combination —
/// the combo is written into `value` and saved. Escape cancels, plain
/// Backspace clears, unusable combinations keep the recorder open. The
/// combo string matches `server::parse_hotkey` (the same parser re-reads
/// it from config).
fn hotkey_wire(btn: &gtk4::Button, value: &Rc<RefCell<String>>, save: impl Fn() + Clone + 'static) {
    let capturing = Rc::new(Cell::new(false));
    // Modifiers held during the capture, as a gdk mask. The combo is
    // completed by a regular key press (any number of modifiers) or by
    // releasing one of two-or-more held modifiers (modifier-only combos) —
    // both stay recordable.
    let pending = Rc::new(Cell::new(0u32));

    {
        let capturing = capturing.clone();
        let pending = pending.clone();
        let btn_c = btn.clone();
        btn.connect_clicked(move |_| {
            if capturing.get() {
                return;
            }
            capturing.set(true);
            pending.set(0);
            btn_c.set_label("Nhấn tổ hợp phím… (Esc hủy)");
        });
    }

    let controller = gtk4::EventControllerKey::new();
    {
        let btn_c = btn.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        let save = save.clone();
        controller.connect_key_pressed(move |_, key, _keycode, state| {
            use gtk4::glib::Propagation;

            if !capturing.get() {
                return Propagation::Proceed;
            }
            // A modifier already held before the button was clicked is in
            // the event state, so it joins the pending mask here.
            let held = state
                & (gtk4::gdk::ModifierType::SHIFT_MASK
                    | gtk4::gdk::ModifierType::CONTROL_MASK
                    | gtk4::gdk::ModifierType::ALT_MASK
                    | gtk4::gdk::ModifierType::SUPER_MASK
                    | gtk4::gdk::ModifierType::HYPER_MASK
                    | gtk4::gdk::ModifierType::META_MASK);
            if let Some(mask) = gdk_modifier_of_key(key) {
                pending.set(pending.get() | mask.bits() | held.bits());
                return Propagation::Stop;
            }
            // Escape cancels the capture; the stored value stays as it was.
            if key == gtk4::gdk::Key::Escape {
                capturing.set(false);
                pending.set(0);
                render_hotkey(&btn_c, &value.borrow());
                return Propagation::Stop;
            }
            // Plain Backspace clears the assignment.
            if key == gtk4::gdk::Key::BackSpace && pending.get() == 0 {
                capturing.set(false);
                *value.borrow_mut() = String::new();
                render_hotkey(&btn_c, "");
                save();
                return Propagation::Stop;
            }
            let spec = hotkey_spec(
                gtk4::gdk::ModifierType::from_bits_truncate(pending.get() | held.bits()),
                Some(key.name().unwrap_or_default().to_string()),
            );
            // Rejected combos keep the recorder open: the engine's parser
            // would silently drop them, so try another key instead.
            if crate::server::parse_hotkey(&spec).is_none() {
                btn_c.set_label("Phím này không dùng được — thử phím khác");
                return Propagation::Stop;
            }
            capturing.set(false);
            pending.set(0);
            *value.borrow_mut() = spec.clone();
            btn_c.set_label(&spec);
            save();
            Propagation::Stop
        });
    }
    btn.add_controller(controller);

    // A modifier-only combo completes on release: hold Control, tap Shift,
    // done. Releasing the only held modifier keeps the recorder open.
    let released = gtk4::EventControllerKey::new();
    {
        let btn_c = btn.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        let save = save.clone();
        released.connect_key_released(move |_, key, _keycode, _state| {
            if !capturing.get() {
                return;
            }
            let Some(mask) = gdk_modifier_of_key(key) else {
                return;
            };
            let before = pending.get();
            pending.set(before & !mask.bits());
            if before.count_ones() >= 2 {
                let spec = hotkey_spec(gtk4::gdk::ModifierType::from_bits_truncate(before), None);
                if crate::server::parse_hotkey(&spec).is_some() {
                    capturing.set(false);
                    pending.set(0);
                    *value.borrow_mut() = spec.clone();
                    btn_c.set_label(&spec);
                    save();
                }
            }
        });
    }
    btn.add_controller(released);

    // Leaving the button mid-capture cancels, like Escape.
    let focus = gtk4::EventControllerFocus::new();
    {
        let btn_c = btn.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        focus.connect_leave(move |_| {
            if capturing.get() {
                capturing.set(false);
                pending.set(0);
                render_hotkey(&btn_c, &value.borrow());
            }
        });
    }
    btn.add_controller(focus);
}

/// The display label of a radio-group entry ("vni" in charset → "VNI").
fn menu_entry_label(group: &str, key: &str) -> String {
    let entries: &[(&str, &str)] = match group {
        "charset" => crate::ibus_prop::CHARSETS,
        "method" => crate::ibus_prop::METHODS,
        "display" => crate::ibus_prop::MODES,
        _ => &[],
    };
    entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| key.to_string())
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // The engine spawns us with `--page=<stack name>` to open a specific
    // page (menu "Mở bảng gõ tắt…"); GTK parses argv itself and rejects
    // these arguments, so read them here and hand GTK an empty argv.
    let page = std::env::args().find_map(|a| a.strip_prefix("--page=").map(str::to_string));
    let app = Application::builder()
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

const METHODS: &[&str] = &["Telex", "VNI", "VIQR", "Microsoft layout"];
const MODES: &[&str] = &[
    "Không gạch chân",
    "Gạch chân (preedit)",
    "Surrounding text",
    "Smooth (uinput)",
    "Super Smooth (uinput)",
    "Select (uinput)",
    "ForwardKeyEvent (Backspace)",
    "ForwardKeyEvent II (Shift+Left)",
    "OFF",
    "Gõ mặc định",
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

#[allow(deprecated)] // TreeStore/TreeView/ComboBoxText, see the import above
struct Ui {
    enabled: Switch,
    method: DropDown,
    mode: DropDown,
    charset: DropDown,
    hotkey_btn: gtk4::Button,
    hotkey_value: Rc<RefCell<String>>,
    enabled_hotkey_btn: gtk4::Button,
    enabled_hotkey_value: Rc<RefCell<String>>,
    modern: Switch,
    free: Switch,
    auto_correct: Switch,
    auto_restore: Switch,
    abbrev: Switch,
    bypass_url: Switch,
    w2u: DropDown,
    bracket: DropDown,
    em_dash: Switch,
    auto_capitalize: Switch,
    double_space: Switch,
    macro_enabled: Switch,
    capitalize_macro: Switch,
    macro_in_off_mode: Switch,
    time_format: Entry,
    date_format: Entry,
    macro_text: TextView,
    dictionary_enabled: Switch,
    dictionary_text: TextView,
    rules_text: TextView,
    hide_underline: Switch,
    debug_log: Switch,
    x11_delete: Switch,
    preedit_xim: Switch,
    commit_interval: Entry,
    backspace_interval: Entry,
    post_delay: Entry,
    unicode_channel: DropDown,
    /// The panel-menu editor: a 4-column store (kind id, key, label, kind
    /// label), its view, the "add" type picker and the folder-name entry.
    /// Folders nest one level at most, matching `config::MenuNode`.
    menu_store: TreeStore,
    menu_view: TreeView,
    menu_kind: ComboBoxText,
    menu_name: Entry,
}

fn method_index(m: MethodConfig) -> u32 {
    match m {
        MethodConfig::Telex => 0,
        MethodConfig::Vni => 1,
        MethodConfig::Viqr => 2,
        MethodConfig::Microsoft => 3,
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
        ModeConfig::DefaultTyping => 9,
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

#[allow(deprecated)]
impl Ui {
    /// Push a config (plus the two text lists) into every widget. This is the
    /// only config → widget mapping: the initial build and the import both go
    /// through it, so an import cannot leave a stale widget that the next
    /// save would write back over the imported file.
    fn refresh(&self, config: &Config, macros: &Macros, dictionary: &Dictionary) {
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
        render_hotkey(&self.hotkey_btn, &config.cycle_mode_hotkey);
        *self.enabled_hotkey_value.borrow_mut() = config.enabled_hotkey.clone();
        render_hotkey(&self.enabled_hotkey_btn, &config.enabled_hotkey);
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
        self.dictionary_enabled
            .set_active(config.dictionary_enabled);
        self.debug_log.set_active(config.debug_log);
        self.x11_delete.set_active(config.x11_delete_without_report);
        self.preedit_xim.set_active(config.preedit_for_xim);
        self.commit_interval
            .set_text(&config.commit_interval_ms.to_string());
        self.backspace_interval
            .set_text(&config.backspace_interval_ms.to_string());
        self.post_delay.set_text(&config.post_delay_ms.to_string());
        self.unicode_channel
            .set_selected(match config.unicode_channel {
                crate::config::UnicodeChannel::Clipboard => 1,
                crate::config::UnicodeChannel::Commit => 0,
            });
        self.fill_menu_store(config.menu.as_deref().unwrap_or(&config.default_menu()));
        set_text(
            &self.macro_text,
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
            &self.rules_text,
            &config
                .app_rules
                .iter()
                .map(|r| format!("{}={}", r.pattern, r.mode.key()))
                .collect::<Vec<_>>()
                .join("\n"),
        );
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
            1 => MethodConfig::Vni,
            2 => MethodConfig::Viqr,
            3 => MethodConfig::Microsoft,
            _ => MethodConfig::Telex,
        };
        config.mode = match self.mode.selected() {
            1 => ModeConfig::Preedit,
            2 => ModeConfig::Surrounding,
            3 => ModeConfig::Smooth,
            4 => ModeConfig::SuperSmooth,
            5 => ModeConfig::Select,
            6 => ModeConfig::ForwardKeyEvent,
            7 => ModeConfig::ForwardSelect,
            8 => ModeConfig::Off,
            9 => ModeConfig::DefaultTyping,
            _ => ModeConfig::NonPreedit,
        };
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
        config.dictionary_enabled = self.dictionary_enabled.is_active();
        config.commit_interval_ms = self.commit_interval.text().trim().parse().unwrap_or(20);
        config.backspace_interval_ms = self.backspace_interval.text().trim().parse().unwrap_or(5);
        config.post_delay_ms = self.post_delay.text().trim().parse().unwrap_or(10);
        config.unicode_channel = match self.unicode_channel.selected() {
            1 => crate::config::UnicodeChannel::Clipboard,
            _ => crate::config::UnicodeChannel::Commit,
        };
        // The designed menu tree; never empty (the editor refuses to remove
        // the last entry), so a save always pins the design down.
        config.menu = Some(self.read_menu());
        // Read here, not in `save_all`: `refresh` asserts that `read` returns
        // everything it just pushed into the widgets, and the rules text view
        // is one of those widgets.
        config.app_rules = self
            .read_rules()
            .into_iter()
            .map(|(pattern, mode)| crate::config::AppRule { pattern, mode })
            .collect();
        config
    }

    fn read_macros(&self) -> Vec<(String, String)> {
        let text = text_of(&self.macro_text);
        text.lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect()
    }

    // --- panel-menu editor (store columns: kind id, key, label, kind label) ---

    fn menu_value(&self, iter: &TreeIter, col: u32) -> String {
        self.menu_store
            .get_value(iter, col as i32)
            .get::<String>()
            .unwrap_or_default()
    }

    fn menu_selected(&self) -> Option<(TreeIter, usize)> {
        let (_, iter) = self.menu_view.selection().selected()?;
        Some((iter, self.menu_store.iter_depth(&iter) as usize))
    }

    fn fill_menu_store(&self, nodes: &[MenuNode]) {
        self.menu_store.clear();
        for node in nodes {
            self.append_menu_node(None, node);
        }
    }

    fn append_menu_node(&self, parent: Option<&TreeIter>, node: &MenuNode) {
        let (kind, key, label, kind_label) = menu_row(node);
        let iter = self.menu_store.insert_with_values(
            parent,
            None,
            &[
                (MENU_COL_KIND, &kind),
                (MENU_COL_KEY, &key),
                (MENU_COL_LABEL, &label),
                (MENU_COL_KIND_LABEL, &kind_label),
            ],
        );
        match node {
            MenuNode::Folder { children, .. } => {
                for child in children {
                    self.append_menu_node(Some(&iter), child);
                }
            }
            // A radio group's entries are plain keys in the model.
            MenuNode::Radios { group, children } => {
                for entry in children {
                    let label = menu_entry_label(group, entry);
                    self.menu_store.insert_with_values(
                        Some(&iter),
                        None,
                        &[
                            (MENU_COL_KIND, &"entry".to_string()),
                            (MENU_COL_KEY, entry),
                            (MENU_COL_LABEL, &label),
                            (MENU_COL_KIND_LABEL, &"Mục chọn".to_string()),
                        ],
                    );
                }
            }
            _ => {}
        }
    }

    fn read_menu(&self) -> Vec<MenuNode> {
        let mut out = Vec::new();
        if let Some(iter) = self.menu_store.iter_first() {
            loop {
                out.push(self.menu_node_from(&iter, 0));
                if !self.menu_store.iter_next(&iter) {
                    break;
                }
            }
        }
        out
    }

    fn menu_node_from(&self, iter: &TreeIter, depth: usize) -> MenuNode {
        let kind = self.menu_value(iter, MENU_COL_KIND);
        let key = self.menu_value(iter, MENU_COL_KEY);
        let label = self.menu_value(iter, MENU_COL_LABEL);
        match kind.as_str() {
            "toggle" => MenuNode::Toggle { key },
            "radios" => MenuNode::Radios {
                group: key,
                children: if depth == 0 {
                    self.menu_children_keys(iter)
                } else {
                    Vec::new()
                },
            },
            "action" => MenuNode::Action { key },
            "sep" => MenuNode::Separator,
            // Folders only exist at the root; deeper ones cannot be created
            // by this editor, so they are not read back either.
            _ => MenuNode::Folder {
                label,
                children: if depth == 0 {
                    self.menu_children(iter, depth + 1)
                } else {
                    Vec::new()
                },
            },
        }
    }

    /// The entry keys under a radio group node.
    fn menu_children_keys(&self, parent: &TreeIter) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(iter) = self.menu_store.iter_children(Some(parent)) {
            loop {
                if self.menu_value(&iter, MENU_COL_KIND) == "entry" {
                    out.push(self.menu_value(&iter, MENU_COL_KEY));
                }
                if !self.menu_store.iter_next(&iter) {
                    break;
                }
            }
        }
        out
    }

    fn menu_children(&self, parent: &TreeIter, depth: usize) -> Vec<MenuNode> {
        let mut out = Vec::new();
        if let Some(iter) = self.menu_store.iter_children(Some(parent)) {
            loop {
                out.push(self.menu_node_from(&iter, depth));
                if !self.menu_store.iter_next(&iter) {
                    break;
                }
            }
        }
        out
    }

    /// Insert the picked item at the selected position: into the selected
    /// folder / radio group (at its end), or right after the selected row at
    /// its own level; appends at the root when nothing is selected.
    fn menu_insert(&self, kind: &str, key: &str) {
        let sel = self.menu_selected().map(|(iter, depth)| {
            (
                iter,
                depth,
                self.menu_value(&iter, MENU_COL_KIND),
                self.menu_value(&iter, MENU_COL_KEY),
            )
        });

        // Placement: a selected folder / radio group takes the item at its
        // end; anything else inserts right AFTER the selected row, at its
        // own level. A container picked while a child is selected cannot go
        // inside — containers live at the root, so it lands after the
        // child's parent instead.
        let (parent, position, label, kind_label) = match sel {
            Some((iter, _, k, _)) if k == "folder" => {
                let (label, kind_label) = menu_item_label(kind, key);
                (Some(iter), None, label, kind_label)
            }
            // The picker offers a radio group only its own entries; this
            // check is the belt to its braces.
            Some((iter, _, k, group)) if k == "radios" && kind == "entry" => (
                Some(iter),
                None,
                menu_entry_label(&group, key),
                "Mục chọn".into(),
            ),
            Some((iter, depth, _, _)) => {
                let parent = self.menu_store.iter_parent(&iter);
                let pkind = parent
                    .as_ref()
                    .map(|p| self.menu_value(p, MENU_COL_KIND))
                    .unwrap_or_default();
                let invalid_inside = depth == 1
                    && ((pkind == "radios" && kind != "entry")
                        || (pkind == "folder" && kind == "radios"));
                let (parent, anchor) = if invalid_inside {
                    (None, parent)
                } else if depth == 1 {
                    (parent, Some(iter))
                } else {
                    (None, Some(iter))
                };
                let position = anchor
                    .as_ref()
                    .and_then(|a| self.menu_position_after(parent.as_ref(), a));
                let (label, kind_label) = menu_item_label(kind, key);
                (parent, position, label, kind_label)
            }
            None => {
                let (label, kind_label) = menu_item_label(kind, key);
                (None, None, label, kind_label)
            }
        };
        self.menu_store.insert_with_values(
            parent.as_ref(),
            position,
            &[
                (MENU_COL_KIND, &kind.to_string()),
                (MENU_COL_KEY, &key.to_string()),
                (MENU_COL_LABEL, &label),
                (MENU_COL_KIND_LABEL, &kind_label),
            ],
        );
    }

    /// The insert position right AFTER `target` among its siblings.
    fn menu_position_after(&self, parent: Option<&TreeIter>, target: &TreeIter) -> Option<u32> {
        let target_path = self.menu_store.path(target);
        self.menu_siblings(parent)
            .iter()
            .position(|i| self.menu_store.path(i) == target_path)
            .map(|p| p as u32 + 1)
    }

    /// Refill the type picker for the current selection: a radio group only
    /// offers its own entries, everything else offers the generic items.
    fn menu_fill_picker(&self) {
        self.menu_kind.remove_all();
        let selected = self
            .menu_selected()
            .filter(|(_, depth)| *depth == 0)
            .map(|(iter, _)| {
                (
                    self.menu_value(&iter, MENU_COL_KIND),
                    self.menu_value(&iter, MENU_COL_KEY),
                )
            });
        match selected.as_ref().map(|(k, g)| (k.as_str(), g.as_str())) {
            Some(("radios", "charset")) => {
                for (key, label) in crate::ibus_prop::CHARSETS {
                    self.menu_kind.append(Some(&format!("entry:{key}")), label);
                }
            }
            Some(("radios", "method")) => {
                for (key, label) in crate::ibus_prop::METHODS {
                    self.menu_kind.append(Some(&format!("entry:{key}")), label);
                }
            }
            Some(("radios", "display")) => {
                for (key, label) in crate::ibus_prop::MODES {
                    self.menu_kind.append(Some(&format!("entry:{key}")), label);
                }
            }
            _ => {
                self.menu_kind
                    .append(Some("toggle:enabled"), "Bật tiếng Việt");
                for t in crate::ibus_prop::QUICK_TOGGLES {
                    self.menu_kind
                        .append(Some(&format!("toggle:{}", t.key)), t.label);
                }
                self.menu_kind
                    .append(Some("radios:charset"), "Bảng mã (nhóm chọn)");
                self.menu_kind
                    .append(Some("radios:method"), "Kiểu gõ (nhóm chọn)");
                self.menu_kind
                    .append(Some("radios:display"), "Chế độ gõ (nhóm chọn)");
                self.menu_kind
                    .append(Some("action:macro_editor"), "Mở bảng gõ tắt…");
                self.menu_kind.append(Some("action:settings"), "Cấu hình");
                self.menu_kind
                    .append(Some("action:about"), "Giới thiệu (About)");
                self.menu_kind.append(Some("sep:"), "Gạch ngang phân cách");
            }
        }
        self.menu_kind.set_active(Some(0));
    }

    /// Folders live at the root only — the popup nests one level.
    fn menu_add_folder(&self, label: &str) {
        let label = label.trim();
        let label = if label.is_empty() {
            "Thư mục mới"
        } else {
            label
        };
        // Folders live at the root: right after the selected root row, or at
        // the end when the selection is inside a folder or missing.
        let position = self.menu_selected().and_then(|(iter, depth)| {
            let anchor = if depth == 0 {
                Some(iter)
            } else {
                self.menu_store.iter_parent(&iter)
            };
            anchor.and_then(|a| self.menu_position_after(None, &a))
        });
        self.menu_store.insert_with_values(
            None,
            position,
            &[
                (MENU_COL_KIND, &"folder".to_string()),
                (MENU_COL_KEY, &String::new()),
                (MENU_COL_LABEL, &label.to_string()),
                (MENU_COL_KIND_LABEL, &"Thư mục".to_string()),
            ],
        );
    }

    fn menu_rename(&self, label: &str) {
        let Some((iter, 0)) = self.menu_selected() else {
            return;
        };
        if self.menu_value(&iter, MENU_COL_KIND) != "folder" {
            return;
        }
        let label = label.trim();
        if label.is_empty() {
            return;
        }
        self.menu_store
            .set_value(&iter, MENU_COL_LABEL, &label.to_value());
    }

    fn menu_delete(&self) {
        let Some((iter, depth)) = self.menu_selected() else {
            return;
        };
        // The popup must never end up empty: refuse to remove the last
        // top-level entry.
        if depth == 0 && self.menu_root_len() <= 1 {
            return;
        }
        let parent = self.menu_store.iter_parent(&iter);
        self.menu_store.remove(&iter);
        // An emptied folder goes away with its last child.
        if let Some(p) = parent {
            if self.menu_store.iter_children(Some(&p)).is_none() {
                self.menu_store.remove(&p);
            }
        }
    }

    fn menu_root_len(&self) -> usize {
        self.menu_siblings(None).len()
    }

    fn menu_siblings(&self, parent: Option<&TreeIter>) -> Vec<TreeIter> {
        let mut out = Vec::new();
        let first = match parent {
            Some(p) => self.menu_store.iter_children(Some(p)),
            None => self.menu_store.iter_first(),
        };
        if let Some(iter) = first {
            loop {
                out.push(iter);
                if !self.menu_store.iter_next(&iter) {
                    break;
                }
            }
        }
        out
    }

    fn menu_move(&self, delta: isize) {
        let Some((iter, _)) = self.menu_selected() else {
            return;
        };
        let parent = self.menu_store.iter_parent(&iter);
        let sibs = self.menu_siblings(parent.as_ref());
        let sel_path = self.menu_store.path(&iter);
        let Some(pos) = sibs
            .iter()
            .position(|i| self.menu_store.path(i) == sel_path)
        else {
            return;
        };
        let target = if delta < 0 {
            pos.checked_sub(1)
        } else {
            Some(pos + 1).filter(|&p| p < sibs.len())
        };
        let Some(target) = target else {
            return;
        };
        self.menu_store.swap(&sibs[pos], &sibs[target]);
    }

    /// Move the selected folder child up to the root (after the folder).
    /// Radio-group entries cannot exist at the root, so they stay put.
    fn menu_out(&self) {
        let Some((iter, 1)) = self.menu_selected() else {
            return;
        };
        let parent = self.menu_store.iter_parent(&iter);
        let in_folder = parent
            .as_ref()
            .is_some_and(|p| self.menu_value(p, MENU_COL_KIND) == "folder");
        if !in_folder {
            return;
        }
        let kind = self.menu_value(&iter, MENU_COL_KIND);
        let key = self.menu_value(&iter, MENU_COL_KEY);
        let label = self.menu_value(&iter, MENU_COL_LABEL);
        let kind_label = self.menu_value(&iter, MENU_COL_KIND_LABEL);
        self.menu_store.insert_with_values(
            None,
            None,
            &[
                (MENU_COL_KIND, &kind),
                (MENU_COL_KEY, &key),
                (MENU_COL_LABEL, &label),
                (MENU_COL_KIND_LABEL, &kind_label),
            ],
        );
        self.menu_store.remove(&iter);
    }

    fn menu_reset(&self) {
        self.fill_menu_store(&Config::load().default_menu());
    }

    fn read_dictionary(&self) -> Vec<String> {
        text_of(&self.dictionary_text)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect()
    }

    fn read_rules(&self) -> Vec<(String, ModeConfig)> {
        text_of(&self.rules_text)
            .lines()
            .filter_map(|l| l.split_once('='))
            .filter_map(|(p, m)| {
                let mode = ModeConfig::from_key(m.trim())?;
                let pattern = p.trim().to_string();
                (!pattern.is_empty()).then_some((pattern, mode))
            })
            .collect()
    }

    fn save_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.read();
        config.save()?;

        let mut macros = Macros::load();
        macros.set_entries(self.read_macros().into_iter());
        macros.save()?;

        let mut dict = Dictionary::load();
        dict.set_words(self.read_dictionary().into_iter());
        dict.save()?;
        Ok(())
    }
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

/// The widgets on their own, so a test can drive `refresh`/`read` without a
/// window; `build` composes them into pages and presents them.
#[allow(deprecated)]
fn widgets() -> Rc<Ui> {
    let enabled = Switch::new();
    let method = drop_down(METHODS, 0);
    let mode = drop_down(MODES, 0);
    let charset = drop_down(CHARSETS, 0);
    let hotkey_btn = gtk4::Button::new();
    hotkey_btn.set_tooltip_text(Some(
        "Bấm rồi nhấn tổ hợp phím mới. Esc hủy, Backspace xóa.",
    ));
    let hotkey_value = Rc::new(RefCell::new(String::new()));
    let enabled_hotkey_btn = gtk4::Button::new();
    enabled_hotkey_btn.set_tooltip_text(Some(
        "Bấm rồi nhấn tổ hợp phím mới. Esc hủy, Backspace xóa.",
    ));
    let enabled_hotkey_value = Rc::new(RefCell::new(String::new()));
    let modern = Switch::new();
    let free = Switch::new();
    let auto_correct = Switch::new();
    let auto_restore = Switch::new();
    let abbrev = Switch::new();
    let bypass_url = Switch::new();
    let w2u = drop_down(ON_OFF_MODES, 0);
    let bracket = drop_down(ON_OFF_MODES, 0);
    let em_dash = Switch::new();
    let auto_capitalize = Switch::new();
    let double_space = Switch::new();
    let macro_enabled = Switch::new();
    let capitalize_macro = Switch::new();
    let macro_in_off_mode = Switch::new();
    let time_format = Entry::builder().width_chars(14).build();
    let date_format = Entry::builder().width_chars(14).build();
    let macro_text = text_view("");
    let dictionary_enabled = Switch::new();
    let dictionary_text = text_view("");
    let rules_text = text_view("");
    let hide_underline = Switch::new();
    let debug_log = Switch::new();
    let x11_delete = Switch::new();
    let preedit_xim = Switch::new();
    let commit_interval = Entry::builder().width_chars(6).build();
    let backspace_interval = Entry::builder().width_chars(6).build();
    let post_delay = Entry::builder().width_chars(6).build();
    let unicode_channel = drop_down(CHANNELS, 0);

    // Panel-menu editor: kind id, key, label, Vietnamese kind label.
    let menu_store = TreeStore::new(&[
        gtk4::glib::Type::STRING,
        gtk4::glib::Type::STRING,
        gtk4::glib::Type::STRING,
        gtk4::glib::Type::STRING,
    ]);
    let menu_view = TreeView::with_model(&menu_store);
    menu_view.set_headers_visible(true);
    menu_view.set_enable_tree_lines(true);
    for (title, col) in [("Loại", MENU_COL_KIND_LABEL), ("Tên", MENU_COL_LABEL)] {
        let column = TreeViewColumn::new();
        column.set_title(title);
        let cell = CellRendererText::new();
        column.pack_start(&cell, true);
        column.add_attribute(&cell, "text", col as i32);
        menu_view.append_column(&column);
    }
    // The type picker is filled dynamically from the selection
    // (menu_fill_picker): a radio group offers its own entries, anything
    // else the generic items.
    let menu_kind = ComboBoxText::new();
    let menu_name = Entry::builder().placeholder_text("Tên thư mục").build();

    Rc::new(Ui {
        enabled,
        method,
        mode,
        charset,
        hotkey_btn,
        hotkey_value,
        enabled_hotkey_btn,
        enabled_hotkey_value,
        modern,
        free,
        auto_correct,
        auto_restore,
        abbrev,
        bypass_url,
        w2u,
        bracket,
        em_dash,
        auto_capitalize,
        double_space,
        macro_enabled,
        capitalize_macro,
        macro_in_off_mode,
        time_format,
        date_format,
        macro_text,
        dictionary_enabled,
        dictionary_text,
        rules_text,
        hide_underline,
        debug_log,
        x11_delete,
        preedit_xim,
        commit_interval,
        backspace_interval,
        post_delay,
        unicode_channel,
        menu_store,
        menu_view,
        menu_kind,
        menu_name,
    })
}

#[allow(deprecated)]
fn build(app: &Application, initial_page: Option<&str>) {
    ROWS.with(|r| r.borrow_mut().clear());
    COLLAPSIBLES.with(|c| c.borrow_mut().clear());

    let config = Config::load();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let ui = widgets();
    ui.refresh(&config, &macros, &dictionary);
    // The type picker follows the editor selection: a radio group offers its
    // own entries, anything else the generic items.
    ui.menu_fill_picker();
    {
        let ui = ui.clone();
        ui.menu_view.selection().connect_changed(move |_| {
            ui.menu_fill_picker();
        });
    }

    // --- pages: grouped-card settings style (section header + cards) ---

    // Quick add for App Rules: pick a client the engine has seen, pick a
    // mode, append one rule line.
    // Quick-add targets: real clients the engine has seen (GTK IM apps only —
    // Wayland relays show up as gnome-shell, X11/XIM apps as none) plus the
    // two pseudo-targets for those nameless groups.
    let mut app_targets: Vec<(String, String)> = crate::config::recent_clients()
        .into_iter()
        .map(|n| {
            let label = n.clone();
            (label, n)
        })
        .collect();
    app_targets.push((
        "gnome-shell (app Wayland)".to_string(),
        "gnome-shell".to_string(),
    ));
    app_targets.push(("(app không báo tên)".to_string(), "none".to_string()));
    let combo_labels: Vec<&str> = app_targets
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();
    let client_combo = drop_down(&combo_labels, 0);
    let rule_mode = drop_down(
        &crate::ibus_prop::MODES
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        1,
    );
    let add_btn = gtk4::Button::with_label("Thêm luật");
    let add_row = gtk4::Box::new(Orientation::Horizontal, 8);
    add_row.set_margin_start(10);
    add_row.set_margin_end(10);
    add_row.set_margin_top(8);
    add_row.set_margin_bottom(8);
    add_row.append(&client_combo);
    add_row.append(&rule_mode);
    add_row.append(&add_btn);
    // The clicked handler is connected after the window exists, so a failed
    // save can surface in the window (see the save block below).

    let export_btn = gtk4::Button::with_label("Xuất…");
    let import_btn = gtk4::Button::with_label("Nhập…");

    // ===== Page 1: Chung =====
    // The everyday tab — what users actually touch. One-off customization
    // lives on the Tinh chỉnh tab, so this stays short.
    let typing = gtk4::Box::new(Orientation::Vertical, 6);
    section(
        &typing,
        "Trạng thái",
        "Bật tiếng Việt và phím tắt đổi chế độ",
    );
    typing.append(&card(vec![
        pref_row(
            "Bật tiếng Việt",
            "Tắt để gõ thẳng mọi phím (gõ tắt vẫn chạy nếu bật ở tab Gõ tắt)",
            &ui.enabled,
        ),
        pref_row(
            "Phím tắt đổi chế độ",
            "Ví dụ: Control+Alt+v. Để trống là chưa gán",
            &ui.hotkey_btn,
        ),
        pref_row(
            "Phím tắt bật/tắt tiếng Việt",
            "Ví dụ: Control+Alt+space. Để trống là chưa gán",
            &ui.enabled_hotkey_btn,
        ),
    ]));
    hint(
        &typing,
        "Thứ tự phím tắt đổi chế độ: Không gạch chân → Gạch chân → Surrounding → Smooth → Super Smooth → Select → ForwardKeyEvent → ForwardKeyEvent II → OFF → Gõ mặc định.",
    );
    section(&typing, "Cách gõ", "Kiểu gõ, bảng mã và chế độ gõ");
    typing.append(&card(vec![
        pref_row("Kiểu gõ", "Telex, VNI, VIQR, Microsoft layout", &ui.method),
        pref_row("Bảng mã", "Unicode và 10 bảng mã cũ", &ui.charset),
        pref_row(
            "Chế độ gõ",
            "Gạch chân (mặc định), Không gạch chân, Surrounding, 3 mode uinput, 2 mode ForwardKeyEvent, OFF, Gõ mặc định",
            &ui.mode,
        ),
    ]));
    section(&typing, "Hiển thị", "Hình thức của chữ đang gõ");
    typing.append(&card(vec![pref_row(
        "Ẩn gạch chân",
        "Chữ đang gõ không có gạch chân (tùy app)",
        &ui.hide_underline,
    )]));

    // ===== Page 2: Dấu & tự động =====
    let tone = gtk4::Box::new(Orientation::Vertical, 6);
    // Row titles match the quick-menu registry labels (ibus_prop
    // ::QUICK_TOGGLES): a toggle ticked in the panel menu must be findable
    // under the same name here.
    section(&tone, "Dấu thanh", "Kiểu đặt dấu trên nguyên âm");
    tone.append(&card(vec![
        pref_row(
            "Kiểu dấu mới",
            "hòa (chuẩn mới) thay vì hoà (chuẩn cũ)",
            &ui.modern,
        ),
        pref_row(
            "Dấu tự do",
            "Dấu đặt anywhere trong từ: hoangf → hoàng",
            &ui.free,
        ),
    ]));
    section(&tone, "Chính tả", "Sửa lỗi và khôi phục từ");
    tone.append(&card(vec![
        pref_row(
            "Từ sai trả phím gốc",
            "Âm tiết không hợp lệ trả về đúng phím đã bấm",
            &ui.auto_correct,
        ),
        pref_row(
            "Khôi phục từ nước ngoài",
            "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
            &ui.auto_restore,
        ),
        pref_row(
            "Không dấu sau URL",
            "Từ gõ ngay sau / : @ . _ ~ = # & % ? giữ nguyên",
            &ui.bypass_url,
        ),
        pref_row(
            "Viết tắt",
            "Giữ đ trong từ viết tắt: ddc → đc, ddk → đk, ddt → đt",
            &ui.abbrev,
        ),
    ]));
    section(&tone, "Tự động", "Viết hoa và dấu câu tự động");
    tone.append(&card(vec![
        pref_row("Tự viết hoa", "Sau . ! ? và Enter", &ui.auto_capitalize),
        pref_row(
            "2 dấu cách = chấm",
            "Thử nghiệm: hai dấu cách thành dấu chấm",
            &ui.double_space,
        ),
    ]));
    section(&tone, "Ký tự đặc biệt", "Chuyển đổi phím tắt ký tự");
    tone.append(&card(vec![
        pref_row("W thành ư", "Tắt / trừ đầu từ / mọi vị trí", &ui.w2u),
        pref_row(
            "[ ] thành ơ ư",
            "Tắt / trừ đầu từ / mọi vị trí",
            &ui.bracket,
        ),
        pref_row(
            "Gạch ngang dài",
            "Hai dấu trừ liên tiếp thành gạch dài (-- thành —)",
            &ui.em_dash,
        ),
    ]));

    // ===== Page 3: Gõ tắt =====
    let macro_box = gtk4::Box::new(Orientation::Vertical, 6);
    section(&macro_box, "Gõ tắt", "Mở rộng cụm tắt thành câu đầy đủ");
    macro_box.append(&card(vec![
        pref_row(
            "Gõ tắt",
            "Khóa gõ tắt mở rộng thành cụm khi hết từ",
            &ui.macro_enabled,
        ),
        pref_row(
            "Viết hoa gõ tắt",
            "Khóa viết hoa → nội dung viết hoa theo",
            &ui.capitalize_macro,
        ),
        pref_row(
            "Gõ tắt khi OFF",
            "Vẫn mở rộng khi tiếng Việt đang tắt",
            &ui.macro_in_off_mode,
        ),
        pref_row("Định dạng giờ ($TIME)", "Ví dụ %H:%M", &ui.time_format),
        pref_row("Định dạng ngày ($DATE)", "Ví dụ %d/%m/%Y", &ui.date_format),
    ]));
    section(
        &macro_box,
        "Bảng gõ tắt",
        "Mỗi dòng: khóa=Nội dung ($TIME, $DATE)",
    );
    macro_box.append(&card_widget(&scrolled(&ui.macro_text, 160)));

    // ===== Page 4: Dữ liệu =====
    let data_box = gtk4::Box::new(Orientation::Vertical, 6);
    section(
        &data_box,
        "Từ điển của tôi",
        "Từ trong từ điển không bị khôi phục về chuỗi phím",
    );
    data_box.append(&card(vec![pref_row(
        "Từ điển của tôi",
        "Bật: từ trong từ điển không bị khôi phục về chuỗi phím (~/.config/ibus-mangrs/dictionary.txt)",
        &ui.dictionary_enabled,
    )]));
    data_box.append(&card_widget(&scrolled(&ui.dictionary_text, 160)));

    section(
        &data_box,
        "Sao lưu & khôi phục",
        "Một JSON chứa cấu hình, gõ tắt và từ điển — chuyển máy hoặc dự phòng",
    );
    data_box.append(&card(vec![
        pref_row(
            "Xuất cấu hình ra JSON",
            "Lưu ra file để mang đi",
            &export_btn,
        ),
        pref_row(
            "Nhập cấu hình từ JSON",
            "Đọc lại từ file đã xuất",
            &import_btn,
        ),
    ]));

    // ===== Page 5: Tinh chỉnh =====
    // Everything touched once (or only when debugging) lives here so the
    // everyday tabs stay short: the panel-menu editor, the uinput pacing,
    // the app-specific X11/XIM workarounds, the log switch.
    let advanced = gtk4::Box::new(Orientation::Vertical, 6);

    // The panel popup is the user's own design: folders (one level deep),
    // toggles, radio groups, actions and separators, freely ordered. The
    // editor rows below get their handlers once `save` exists.
    let menu_add_btn = gtk4::Button::with_label("Thêm");
    let menu_folder_btn = gtk4::Button::with_label("Thêm thư mục");
    let menu_rename_btn = gtk4::Button::with_label("Đổi tên");
    let menu_up_btn = gtk4::Button::with_label("Lên");
    let menu_down_btn = gtk4::Button::with_label("Xuống");
    let menu_out_btn = gtk4::Button::with_label("Ra ngoài");
    let menu_del_btn = gtk4::Button::with_label("Xoá");
    let menu_reset_btn = gtk4::Button::with_label("Đặt lại mặc định");
    let menu_row1 = gtk4::Box::new(Orientation::Horizontal, 8);
    menu_row1.set_margin_start(10);
    menu_row1.set_margin_end(10);
    menu_row1.set_margin_top(8);
    menu_row1.set_margin_bottom(4);
    menu_row1.append(&ui.menu_kind);
    menu_row1.append(&menu_add_btn);
    menu_row1.append(&ui.menu_name);
    menu_row1.append(&menu_folder_btn);
    menu_row1.append(&menu_rename_btn);
    let menu_row2 = gtk4::Box::new(Orientation::Horizontal, 8);
    menu_row2.set_margin_start(10);
    menu_row2.set_margin_end(10);
    menu_row2.set_margin_top(4);
    menu_row2.set_margin_bottom(8);
    menu_row2.append(&menu_up_btn);
    menu_row2.append(&menu_down_btn);
    menu_row2.append(&menu_out_btn);
    menu_row2.append(&menu_del_btn);
    menu_row2.append(&menu_reset_btn);
    let menu_view_row = gtk4::Box::new(Orientation::Vertical, 0);
    menu_view_row.set_margin_start(10);
    menu_view_row.set_margin_end(10);
    menu_view_row.set_margin_top(0);
    menu_view_row.set_margin_bottom(8);
    let menu_scroll = gtk4::ScrolledWindow::new();
    menu_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    menu_scroll.set_min_content_height(220);
    menu_scroll.set_child(Some(&ui.menu_view));
    menu_view_row.append(&menu_scroll);
    collapsible_section(
        &advanced,
        "Menu trên panel",
        "Tự thiết kế menu popup: thêm thư mục, thêm chức năng, bỏ vào thư mục, sắp xếp thứ tự. Bảng mã / Kiểu gõ / Chế độ gõ cũng chỉnh được mục bên trong.",
        vec![menu_row1, menu_row2, menu_view_row],
    );
    section(
        &advanced,
        "Luật theo ứng dụng",
        "Mỗi dòng: tên-ứng-dụng=chế độ (nonpreedit, preedit, surrounding, smooth, supersmooth, select, off, defaulttyping). Tên đặc biệt: gnome-shell = app Wayland, none = app không báo tên",
    );
    advanced.append(&card_widget(&add_row));
    advanced.append(&card_widget(&scrolled(&ui.rules_text, 160)));
    hint(
        &advanced,
        "Chỉ app dùng GTK IM module mới báo tên riêng cho engine; app Wayland hiện là gnome-shell, app X11/khác là none. Cài extension MăngRS Focus (data/gnome-shell-extension) để engine nhận tên app Wayland-native: Chrome, Firefox, … Danh sách là các app engine đã từng thấy, không phải mọi app đang chạy.",
    );
    section(
        &advanced,
        "Tốc độ gõ (uinput)",
        "Chỉ áp dụng cho Smooth / Super Smooth / Select",
    );
    advanced.append(&card(vec![
        pref_row(
            "Nhịp gõ chữ",
            "Khoảng cách giữa hai lần gửi chữ (ms). Mặc định: 20",
            &ui.commit_interval,
        ),
        pref_row(
            "Nhịp xoá lùi",
            "Khoảng cách giữa hai lần Backspace (ms). Mặc định: 5",
            &ui.backspace_interval,
        ),
        pref_row(
            "Độ trễ sau từ",
            "Chờ sau khi viết lại từ trước khi gõ tiếp (ms). Mặc định: 10",
            &ui.post_delay,
        ),
        pref_row(
            "Kênh gửi từ tiếng Việt",
            "CommitText an toàn, hoặc Clipboard (Ctrl+V, tạm đè clipboard)",
            &ui.unicode_channel,
        ),
    ]));
    section(
        &advanced,
        "Tương thích X11/XIM",
        "Cho app X11 trực tiếp và app XIM (Java/JetBrains)",
    );
    advanced.append(&card(vec![
        pref_row(
            "Xoá đuôi từ ở app X11",
            "App X11 trực tiếp xoá đuôi từ dù không gửi báo cáo — như ibus-bamboo. Có thể xoá nhầm vài ký tự nếu con trỏ bị đổi chỗ.",
            &ui.x11_delete,
        ),
        pref_row(
            "Preedit cho app XIM",
            "Giữ gạch chân cho app XIM (Java/JetBrains): chữ đang gõ hiện trong cửa sổ riêng rồi commit khi hết từ.",
            &ui.preedit_xim,
        ),
    ]));
    section(
        &advanced,
        "Chẩn đoán",
        "Ghi lại quyết định của engine để gỡ lỗi",
    );
    advanced.append(&card(vec![pref_row(
        "Ghi log gỡ lỗi",
        "Ghi vào ~/.cache/ibus-mangrs/debug.log",
        &ui.debug_log,
    )]));

    // ===== Page 6: Giới thiệu =====
    let about = gtk4::Box::new(Orientation::Vertical, 6);

    // Logo is embedded at compile time: the GUI has no installed asset dir.
    const LOGO: &[u8] = include_bytes!("../data/logo.png");
    let logo = gtk4::gdk::Texture::from_bytes(&gtk4::glib::Bytes::from_static(LOGO)).ok();
    let hero = gtk4::Box::new(Orientation::Vertical, 4);
    hero.set_halign(gtk4::Align::Center);
    hero.set_margin_top(30);
    hero.set_margin_bottom(10);
    if let Some(texture) = &logo {
        let image = gtk4::Image::from_paintable(Some(texture));
        image.set_pixel_size(130);
        image.set_halign(gtk4::Align::Center);
        image.add_css_class("app-icon");
        hero.append(&image);
    }

    let name = gtk4::Label::new(Some("MăngRS IBus"));
    name.add_css_class("app-name");
    name.set_halign(gtk4::Align::Center);
    name.set_margin_top(12);
    hero.append(&name);

    let headline = gtk4::Label::new(Some(concat!(
        "Phiên bản ",
        env!("CARGO_PKG_VERSION"),
        " • Native IBus Engine"
    )));
    headline.add_css_class("app-version");
    headline.add_css_class("dim-label");
    headline.set_halign(gtk4::Align::Center);
    hero.append(&headline);

    let tagline = gtk4::Label::new(Some(
        "Bộ gõ tiếng Việt cho lập trình viên và mọi ứng dụng — Rust + bamboo-core",
    ));
    tagline.add_css_class("app-tagline");
    tagline.set_halign(gtk4::Align::Center);
    tagline.set_wrap(true);
    tagline.set_justify(gtk4::Justification::Center);
    tagline.set_margin_top(6);
    hero.append(&tagline);
    about.append(&hero);

    section(&about, "Đặc điểm nổi bật", "");
    about.append(&card(vec![
        info_row(
            "input-keyboard-symbolic",
            "10 chế độ hiển thị",
            "Preedit, Non-preedit, Surrounding + 3 mode uinput + 2 mode ForwardKeyEvent, OFF, Gõ mặc định",
        ),
        info_row(
            "network-workgroup-symbolic",
            "URL thông minh",
            "Từ ngay sau / : @ . _ ~ = # & % ? giữ nguyên, không thêm dấu",
        ),
        info_row(
            "applications-system-symbolic",
            "App Rules theo ứng dụng",
            "Mỗi ứng dụng một chế độ — đổi bằng menu, hotkey hoặc cửa sổ này",
        ),
        info_row(
            "insert-text-symbolic",
            "Gõ tắt & macro",
            "$TIME/$DATE, viết hoa thông minh, không kích hoạt nhầm từ Việt thật",
        ),
    ]));

    section(&about, "Thông tin dự án", "");
    about.append(&card(vec![
        row_impl(
            Some("web-browser-symbolic"),
            "Mã nguồn GitHub",
            "https://github.com/fundokukiri/ibus-mangrs",
            Some(link_button("Mở", "https://github.com/fundokukiri/ibus-mangrs").upcast_ref()),
        ),
        info_row(
            "avatar-default-symbolic",
            "Tác giả",
            "Phùng Đức Tùng (Fundoku)",
        ),
        row_impl(
            Some("web-browser-symbolic"),
            "Trang chủ",
            "https://mangrs.fundoku.io.vn",
            Some(link_button("Mở", "https://mangrs.fundoku.io.vn").upcast_ref()),
        ),
        info_row(
            "help-about-symbolic",
            "Giấy phép mã nguồn mở",
            "GPL — phiên bản 3, chi tiết trong LICENSE",
        ),
        info_row(
            "emblem-favorite-symbolic",
            "Credits",
            "Lõi gõ bamboo-core (MIT, bên thứ 3)",
        ),
    ]));

    // Content: one scrolled, width-clamped page per section, crossfading.
    // (stack name, pill label, symbolic icon from the icon theme)
    const PAGES: [(&str, &str, &str); 6] = [
        ("typing", "Chung", "input-keyboard-symbolic"),
        ("tone", "Dấu & tự động", "tools-check-spelling-symbolic"),
        ("macros", "Gõ tắt", "insert-text-symbolic"),
        ("data", "Dữ liệu", "folder-documents-symbolic"),
        ("advanced", "Tinh chỉnh", "emblem-system-symbolic"),
        ("about", "Giới thiệu", "help-about-symbolic"),
    ];
    let page_boxes: [&gtk4::Box; 6] = [&typing, &tone, &macro_box, &data_box, &advanced, &about];
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_vexpand(true);
    stack.set_hexpand(true);
    for ((name, title, _icon), content) in PAGES.iter().zip(page_boxes) {
        // One centered column per page, 580px wide at most. GTK has no
        // max-width, so symmetric margins cap it; they are derived from the
        // content's own minimum width so the cap can never squeeze (and thus
        // grow) the window.
        let clamp = gtk4::Box::new(Orientation::Vertical, 0);
        clamp.set_halign(gtk4::Align::Fill);
        clamp.set_hexpand(true);
        clamp.set_valign(gtk4::Align::Start);
        clamp.set_margin_top(6);
        clamp.set_margin_bottom(24);
        clamp.set_margin_start(24);
        clamp.set_margin_end(24);
        content.set_halign(gtk4::Align::Fill);
        content.set_hexpand(true);
        clamp.append(content);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_child(Some(&clamp));
        {
            let clamp = clamp.clone();
            let content = content.clone();
            scroll.add_tick_callback(move |sc, _| {
                let cap = 580.max(content.measure(Orientation::Horizontal, -1).0);
                let margin = ((sc.width() - cap) / 2).max(24);
                if clamp.margin_start() != margin {
                    clamp.set_margin_start(margin);
                    clamp.set_margin_end(margin);
                }
                gtk4::glib::ControlFlow::Continue
            });
        }
        stack.add_titled(&scroll, Some(name), title);
    }

    // Search bar (toggled by the magnifier on the left of the header).
    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Tìm cài đặt…"));
    search_entry.set_width_chars(32);
    let search_bar = gtk4::SearchBar::new();
    search_bar.set_child(Some(&search_entry));
    search_bar.connect_entry(&search_entry);
    search_entry.connect_search_changed(|entry| {
        let q = entry.text().to_lowercase();
        ROWS.with(|rows| {
            for (text, widget) in rows.borrow().iter() {
                widget.set_visible(q.is_empty() || text.contains(&q));
            }
        });
        // Rows inside a collapsed section can never be seen, so a search
        // opens every collapsible and an empty query restores them closed.
        COLLAPSIBLES.with(|c| {
            for (revealer, arrow) in c.borrow().iter() {
                revealer.set_reveal_child(!q.is_empty());
                arrow.set_text(if q.is_empty() { "▸" } else { "▼" });
            }
        });
    });

    let content = gtk4::Box::new(Orientation::Vertical, 0);
    content.append(&search_bar);
    content.append(&stack);

    // The floating restart button. It stays hidden until a change is saved:
    // every control already writes straight to config.toml, so the only
    // thing left for the user to do is restart IBus (a new engine binary or
    // component XML needs it; plain settings changes apply within a second).
    let restart_btn = gtk4::Button::new();
    {
        let b = gtk4::Box::new(Orientation::Horizontal, 8);
        b.append(&gtk4::Image::from_icon_name("system-reboot-symbolic"));
        b.append(&gtk4::Label::new(Some("Khởi động lại IBus")));
        restart_btn.set_child(Some(&b));
    }
    restart_btn.add_css_class("suggested-action");
    restart_btn.add_css_class("pill");
    restart_btn.set_halign(gtk4::Align::Center);
    restart_btn.set_valign(gtk4::Align::End);
    restart_btn.set_margin_bottom(18);
    restart_btn.set_visible(false);

    // The overlay floats the restart button over the page, bottom center.
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&content));
    overlay.add_overlay(&restart_btn);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("MăngRS")
        .default_width(780)
        .default_height(700)
        .child(&overlay)
        .build();
    // X11/XWayland reads the icon from the window; Wayland derives it from
    // the app id and the installed desktop file instead.
    window.set_icon_name(Some("ibus-mangrs"));

    // Header: [search] ... [pill tabs] ... [close]. Window controls are
    // replaced by our own round close button.
    let header = gtk4::HeaderBar::new();
    header.set_show_title_buttons(false);

    let search_btn = gtk4::ToggleButton::new();
    search_btn.set_icon_name("system-search-symbolic");
    search_btn.add_css_class("flat");
    search_btn.add_css_class("circular");
    search_btn
        .bind_property("active", &search_bar, "search-mode-enabled")
        .bidirectional()
        .sync_create()
        .build();
    header.pack_start(&search_btn);

    let close_btn = gtk4::Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("circular");
    close_btn.add_css_class("close-btn");
    {
        let window = window.clone();
        close_btn.connect_clicked(move |_| window.close());
    }
    header.pack_end(&close_btn);

    let pillbar = gtk4::Box::new(Orientation::Horizontal, 4);
    pillbar.set_halign(gtk4::Align::Center);
    let mut pills: Vec<(String, gtk4::ToggleButton)> = Vec::new();
    let mut group: Option<gtk4::ToggleButton> = None;
    for (name, title, icon) in PAGES {
        let pill = gtk4::ToggleButton::new();
        pill.set_has_frame(false);
        let h = gtk4::Box::new(Orientation::Horizontal, 8);
        h.set_halign(gtk4::Align::Center);
        h.append(&gtk4::Image::from_icon_name(icon));
        h.append(&gtk4::Label::new(Some(title)));
        pill.set_child(Some(&h));
        pill.add_css_class("nav-pill");
        // Radio behaviour: exactly one pill active.
        match &group {
            Some(first) => pill.set_group(Some(first)),
            None => group = Some(pill.clone()),
        }
        let stack = stack.clone();
        let target = name.to_string();
        pill.connect_toggled(move |pill| {
            if pill.is_active() {
                stack.set_visible_child_name(&target);
            }
        });
        pills.push((name.to_string(), pill.clone()));
        pillbar.append(&pill);
    }
    header.set_title_widget(Some(&pillbar));
    window.set_titlebar(Some(&header));

    // Open a specific page when spawned with `--page=<name>` (menu items).
    // Old page names from the 7-tab layout are mapped onto the new ones.
    let wanted = match initial_page {
        Some("general") | None => "typing",
        Some("dictionary") | Some("apps") | Some("backup") => "data",
        Some(other) => other,
    };
    let (_, pill) = pills.iter().find(|(n, _)| n == wanted).unwrap_or(&pills[0]);
    pill.set_active(true);

    // Grouped-card look, theme-agnostic (alpha over currentColor adapts to
    // light/dark without libadwaita).
    let css = gtk4::CssProvider::new();
    css.load_from_data(
        "window { font-family: \"Adwaita Sans\", \"Cantarell\", sans-serif; } \
         headerbar { background: none; box-shadow: none; border: none; padding: 8px 12px 4px 12px; } \
         .group-card { background: alpha(currentColor, 0.06); border-radius: 14px; padding: 2px 0; } \
         .group-card separator { background: alpha(currentColor, 0.08); min-height: 1px; } \
         .nav-pill { border-radius: 10px; padding: 8px 14px; min-height: 0; background: none; margin: 0 1px; } \
         .nav-pill:checked { background: alpha(currentColor, 0.10); } \
         .nav-pill label { font-weight: bold; font-size: 16px; } \
         .nav-pill image { -gtk-icon-size: 16px; opacity: 0.75; } \
         .close-btn { background: alpha(currentColor, 0.10); } \
         .group-card textview, .group-card textview text, .group-card scrolledwindow { background-color: transparent; } \
         dropdown button, dropdown button:hover { background: none; box-shadow: none; border: none; } \
         .app-icon { border-radius: 32px; padding: 10px; background: alpha(currentColor, 0.06); } \
         .app-name { font-size: 30px; font-weight: bold; } \
         .app-version { font-size: 17px; font-weight: bold; } \
         .app-tagline { font-size: 16px; } \
         .section-title { font-weight: bold; font-size: 18px; } \
         .section-sub { font-size: 15px; opacity: 0.6; } \
         .row-title { font-size: 16px; } \
         .row-sub { font-size: 14px; opacity: 0.62; } \
         .row-icon { opacity: 0.85; }",
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    {
        let window_for_save = window.clone();
        let ui_for_save = ui.clone();
        let restart_for_save = restart_btn.clone();
        let save = move || {
            match ui_for_save.save_all() {
                // Every saved change floats the restart button up: the
                // settings are already on disk, the button is only there so
                // a restart is one click away.
                Ok(()) => restart_for_save.set_visible(true),
                Err(e) => alert(&window_for_save, &e.to_string()),
            }
        };
        // The quick-add row appends the rule line, then saves through the
        // same error surface as every other control.
        {
            let save = save.clone();
            let ui = ui.clone();
            let client_combo = client_combo.clone();
            let rule_mode = rule_mode.clone();
            add_btn.connect_clicked(move |_| {
                let idx = client_combo.selected() as usize;
                let Some((_, pattern)) = app_targets.get(idx) else {
                    return;
                };
                let pattern = pattern.clone();
                let Some((key, _)) = crate::ibus_prop::MODES.get(rule_mode.selected() as usize)
                else {
                    return;
                };
                let mode = *key;
                let mut text = text_of(&ui.rules_text);
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!("{pattern}={mode}\n"));
                set_text(&ui.rules_text, &text);
                save();
            });
        }
        // The panel-menu editor buttons mutate the tree, then save through
        // the shared funnel — the design is on disk (and the panel, after a
        // reload) before the window loses focus.
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_add_btn.connect_clicked(move |_| {
                let Some(id) = ui.menu_kind.active_id() else {
                    return;
                };
                let Some((kind, key)) = id.split_once(':') else {
                    return;
                };
                ui.menu_insert(kind, key);
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_folder_btn.connect_clicked(move |_| {
                let name = ui.menu_name.text().to_string();
                ui.menu_add_folder(&name);
                ui.menu_name.set_text("");
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_rename_btn.connect_clicked(move |_| {
                let name = ui.menu_name.text().to_string();
                ui.menu_rename(&name);
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_up_btn.connect_clicked(move |_| {
                ui.menu_move(-1);
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_down_btn.connect_clicked(move |_| {
                ui.menu_move(1);
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_out_btn.connect_clicked(move |_| {
                ui.menu_out();
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_del_btn.connect_clicked(move |_| {
                ui.menu_delete();
                save();
            });
        }
        {
            let save = save.clone();
            let ui = ui.clone();
            menu_reset_btn.connect_clicked(move |_| {
                ui.menu_reset();
                save();
            });
        }
        hotkey_wire(&ui.hotkey_btn, &ui.hotkey_value, save.clone());
        hotkey_wire(
            &ui.enabled_hotkey_btn,
            &ui.enabled_hotkey_value,
            save.clone(),
        );
        macro_rules! on_change {
            ($w:expr) => {{
                let s = save.clone();
                $w.connect_active_notify(move |_| s());
            }};
        }
        on_change!(ui.enabled);
        on_change!(ui.modern);
        on_change!(ui.free);
        on_change!(ui.auto_correct);
        on_change!(ui.auto_restore);
        on_change!(ui.abbrev);
        on_change!(ui.bypass_url);
        on_change!(ui.em_dash);
        on_change!(ui.auto_capitalize);
        on_change!(ui.double_space);
        on_change!(ui.macro_enabled);
        on_change!(ui.capitalize_macro);
        on_change!(ui.macro_in_off_mode);
        on_change!(ui.dictionary_enabled);
        on_change!(ui.debug_log);
        on_change!(ui.x11_delete);
        on_change!(ui.preedit_xim);
        {
            let s = save.clone();
            ui.method.connect_selected_notify(move |_| s());
        }
        {
            let s = save.clone();
            ui.mode.connect_selected_notify(move |_| s());
        }
        {
            let s = save.clone();
            ui.charset.connect_selected_notify(move |_| s());
        }
        {
            let s = save.clone();
            ui.w2u.connect_selected_notify(move |_| s());
        }
        {
            let s = save.clone();
            ui.bracket.connect_selected_notify(move |_| s());
        }
        {
            let s = save.clone();
            ui.unicode_channel.connect_selected_notify(move |_| s());
        }
        // Entries save on Enter or when they lose focus.
        for entry in [
            &ui.time_format,
            &ui.date_format,
            &ui.commit_interval,
            &ui.backspace_interval,
            &ui.post_delay,
        ] {
            let s = save.clone();
            entry.connect_activate(move |_| s());
            let s = save.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            entry.add_controller(controller);
        }
        // Text views save when focus leaves the window's text area.
        for view in [&ui.macro_text, &ui.dictionary_text, &ui.rules_text] {
            let s = save.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            view.add_controller(controller);
        }
    }

    {
        let window = window.clone();
        export_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Xuất cấu hình")
                .modal(true)
                .initial_name("ibus-mangrs-backup.json")
                .build();
            // The clicked handler is Fn and keeps its own capture; the
            // callback gets its own clone (the parent is only borrowed for
            // the call, the callback outlives it).
            let callback_window = window.clone();
            dialog.save(
                Some(&window),
                gtk4::gio::Cancellable::NONE,
                // A dismissed dialog is also an Err: nothing to save, no error.
                move |result| {
                    if let Ok(file) = result {
                        if let Err(e) = export_json(file.path()) {
                            alert(&callback_window, &e);
                        }
                    }
                },
            );
        });
    }
    {
        let ui = ui.clone();
        let window = window.clone();
        let restart_for_import = restart_btn.clone();
        import_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Nhập cấu hình")
                .modal(true)
                .build();
            let ui = ui.clone();
            let callback_window = window.clone();
            let restart_for_import = restart_for_import.clone();
            dialog.open(
                Some(&window),
                gtk4::gio::Cancellable::NONE,
                // A dismissed dialog is also an Err: nothing to import.
                move |result| {
                    if let Ok(file) = result {
                        match import_json(file.path(), &ui) {
                            // An import rewrites config.toml wholesale — the
                            // restart button applies to it like any save.
                            Ok(()) => restart_for_import.set_visible(true),
                            Err(e) => alert(&callback_window, &e),
                        }
                    }
                },
            );
        });
    }

    {
        let window = window.clone();
        let btn = restart_btn.clone();
        restart_btn.connect_clicked(move |_| {
            // User-initiated only — the engine never restarts IBus itself.
            match std::process::Command::new("ibus").arg("restart").spawn() {
                Ok(_) => btn.set_visible(false),
                Err(e) => alert(&window, &format!("Không khởi động lại được IBus: {e}")),
            }
        });
    }

    window.present();
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

fn import_json(path: Option<PathBuf>, ui: &Ui) -> Result<(), String> {
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

// --- small builders ---

/// Surface a save/import failure in the window: an `eprintln!` is invisible
/// from a GUI, and a silent failure looks exactly like a broken feature.
fn alert(window: &impl IsA<gtk4::Window>, text: &str) {
    gtk4::AlertDialog::builder()
        .modal(true)
        .message("Không lưu được cấu hình")
        .detail(text)
        .build()
        .show(Some(window));
}

fn drop_down(items: &[&str], selected: u32) -> DropDown {
    let dd = DropDown::new(Some(StringList::new(items)), gtk4::Expression::NONE);
    dd.set_selected(selected);
    dd
}

fn text_view(content: &str) -> TextView {
    let view = TextView::builder()
        .hexpand(true)
        .monospace(true)
        .left_margin(10)
        .right_margin(10)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    view.buffer().set_text(content);
    view
}

/// Wrap a text view in a fixed-height scrolled window so a long list cannot
/// stretch (or collapse) the whole tab.
fn scrolled(view: &TextView, height: i32) -> gtk4::ScrolledWindow {
    gtk4::ScrolledWindow::builder()
        .child(view)
        .height_request(height)
        .hexpand(true)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build()
}

/// A link that opens in the default browser.
fn link_button(label_text: &str, uri: &str) -> gtk4::LinkButton {
    gtk4::LinkButton::builder()
        .label(label_text)
        .uri(uri)
        .build()
}

/// Section header: bold title + dim subtitle (one size larger than rows).
fn section(box_: &gtk4::Box, title: &str, subtitle: &str) {
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("section-title");
    t.set_halign(gtk4::Align::Start);
    t.set_margin_top(20);
    t.set_margin_bottom(2);
    box_.append(&t);
    if !subtitle.is_empty() {
        let s = gtk4::Label::new(Some(subtitle));
        s.add_css_class("section-sub");
        s.set_halign(gtk4::Align::Start);
        s.set_xalign(0.0);
        s.set_wrap(true);
        s.set_margin_bottom(10);
        box_.append(&s);
    }
}

/// A section whose card starts collapsed: clicking the title toggles it.
/// GTK4 dropped Expander, so this is a flat button over a Revealer. Used for
/// the long customization lists — they would otherwise dominate the page.
fn collapsible_section(parent: &gtk4::Box, title: &str, subtitle: &str, rows: Vec<gtk4::Box>) {
    let header = gtk4::Button::new();
    header.add_css_class("flat");
    let h = gtk4::Box::new(Orientation::Horizontal, 8);
    let arrow = gtk4::Label::new(Some("▸"));
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("section-title");
    h.set_halign(gtk4::Align::Start);
    h.append(&arrow);
    h.append(&t);
    header.set_child(Some(&h));
    header.set_halign(gtk4::Align::Start);
    let revealer = Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideDown)
        .transition_duration(150)
        .reveal_child(false)
        .build();
    revealer.set_child(Some(&card(rows)));
    {
        let revealer = revealer.clone();
        let arrow = arrow.clone();
        header.connect_clicked(move |_| {
            // The arrow must follow the revealer, not the click count: the
            // search handler also opens and closes it.
            let open = !revealer.reveals_child();
            revealer.set_reveal_child(open);
            arrow.set_text(if open { "▼" } else { "▸" });
        });
    }
    parent.append(&header);
    if !subtitle.is_empty() {
        let s = gtk4::Label::new(Some(subtitle));
        s.add_css_class("section-sub");
        s.set_halign(gtk4::Align::Start);
        s.set_xalign(0.0);
        s.set_wrap(true);
        s.set_margin_bottom(10);
        parent.append(&s);
    }
    parent.append(&revealer);
    COLLAPSIBLES.with(|c| c.borrow_mut().push((revealer, arrow)));
}

/// A grouped card: rows separated by hairline separators, rounded background.
fn card(rows: Vec<gtk4::Box>) -> gtk4::Box {
    let c = gtk4::Box::new(Orientation::Vertical, 0);
    c.add_css_class("group-card");
    c.set_margin_bottom(6);
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            c.append(&gtk4::Separator::new(Orientation::Horizontal));
        }
        c.append(r);
    }
    c
}

/// A card holding a single control (text views, the quick-add row).
fn card_widget(widget: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let c = gtk4::Box::new(Orientation::Vertical, 0);
    c.add_css_class("group-card");
    c.set_margin_bottom(6);
    c.append(widget);
    c
}

/// Generic settings row: optional leading icon, title + small dim subtitle,
/// optional control on the right. Registered for the search bar.
fn row_impl(
    icon: Option<&str>,
    title: &str,
    subtitle: &str,
    widget: Option<&gtk4::Widget>,
) -> gtk4::Box {
    let h = gtk4::Box::new(Orientation::Horizontal, 12);
    h.set_margin_start(16);
    h.set_margin_end(16);
    h.set_margin_top(10);
    h.set_margin_bottom(10);
    if let Some(name) = icon {
        let img = gtk4::Image::from_icon_name(name);
        img.set_pixel_size(20);
        img.add_css_class("row-icon");
        img.set_valign(gtk4::Align::Center);
        h.append(&img);
    }
    let v = gtk4::Box::new(Orientation::Vertical, 2);
    v.set_valign(gtk4::Align::Center);
    v.set_hexpand(true);
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("row-title");
    t.set_xalign(0.0);
    t.set_wrap(true);
    v.append(&t);
    if !subtitle.is_empty() {
        let sub = gtk4::Label::new(Some(subtitle));
        sub.add_css_class("row-sub");
        sub.set_xalign(0.0);
        sub.set_wrap(true);
        v.append(&sub);
    }
    h.append(&v);
    if let Some(w) = widget {
        w.set_valign(gtk4::Align::Center);
        h.append(w);
    }
    // Hovering anywhere on the row explains the feature.
    h.set_tooltip_text(Some(&format!("{title}: {subtitle}")));
    ROWS.with(|r| {
        r.borrow_mut().push((
            format!("{title} {subtitle}").to_lowercase(),
            h.clone().upcast(),
        ))
    });
    h
}

/// A settings row: title + small dim subtitle on the left, control right.
fn pref_row(title: &str, subtitle: &str, widget: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    row_impl(None, title, subtitle, Some(widget.upcast_ref()))
}

/// Icon + title + subtitle, no control — feature/project lists on About.
fn info_row(icon: &str, title: &str, subtitle: &str) -> gtk4::Box {
    row_impl(Some(icon), title, subtitle, None)
}

fn hint(box_: &gtk4::Box, text: &str) {
    // Small and dimmed: hints explain the row above, they must not compete
    // with it. markup_escape_text because some hints contain < > &.
    let l = gtk4::Label::new(None);
    l.set_use_markup(true);
    l.set_markup(&format!(
        "<small>{}</small>",
        gtk4::glib::markup_escape_text(text)
    ));
    l.set_halign(gtk4::Align::Start);
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.add_css_class("dim-label");
    l.set_margin_start(4);
    box_.append(&l);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B3 regression: the import path pushes a config into every widget through
    /// `refresh`, so a widget `refresh` forgets keeps its old value and the next
    /// save writes that stale value back over the imported settings. Needs a
    /// display; skips where GTK cannot start.
    #[test]
    fn refresh_round_trips_every_widget() {
        if gtk4::init().is_err() {
            eprintln!("gui: SKIP (no display)");
            return;
        }
        let ui = widgets();
        let mut macros = Macros::load();
        macros.set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
        let mut dictionary = Dictionary::load();
        dictionary.set_words(["asl".to_string(), "tr".to_string()].into_iter());

        // `read()` starts from the file, and two fields pass through it
        // untouched now: `quick_menu` (its checkbox list was replaced by the
        // menu editor; the list is only the design default) and anything
        // else without a widget.
        let baseline = ui.read();
        let config = Config {
            enabled: false,
            method: MethodConfig::Vni,
            mode: ModeConfig::Select,
            charset: CharsetConfig::Tcvn3,
            modern_tone_style: false,
            free_tone_marking: true,
            auto_correct: false,
            auto_restore: false,
            bypass_url: false,
            w2u: W2uConfig::NonStart,
            bracket: BracketConfig::Everywhere,
            em_dash: true,
            auto_capitalize: true,
            double_space_period: true,
            macro_enabled: false,
            capitalize_macro: false,
            macro_in_off_mode: true,
            time_format: "%H".into(),
            date_format: "%Y".into(),
            dictionary_enabled: false,
            abbreviations: false,
            cycle_mode_hotkey: "Control+Alt+v".into(),
            enabled_hotkey: "Control+Alt+space".into(),
            hide_underline: true,
            commit_interval_ms: 33,
            backspace_interval_ms: 7,
            post_delay_ms: 12,
            unicode_channel: crate::config::UnicodeChannel::Clipboard,
            debug_log: true,
            x11_delete_without_report: true,
            preedit_for_xim: true,
            app_rules: vec![crate::config::AppRule {
                pattern: "kate".into(),
                mode: ModeConfig::Preedit,
            }],
            // Passes through read() untouched (see baseline above).
            quick_menu: baseline.quick_menu,
            menu_modes: baseline.menu_modes,
            // The editor always produces a design, and refresh normalizes an
            // absent/empty one to the shipped shape; here a small tree that
            // must survive the store round-trip unchanged.
            menu: Some(vec![
                MenuNode::Toggle {
                    key: "enabled".into(),
                },
                MenuNode::Folder {
                    label: "Cá nhân".into(),
                    children: vec![
                        MenuNode::Toggle {
                            key: "em_dash".into(),
                        },
                        MenuNode::Separator,
                        MenuNode::Action {
                            key: "settings".into(),
                        },
                    ],
                },
                MenuNode::Radios {
                    group: "display".into(),
                    children: vec!["preedit".into(), "smooth".into()],
                },
            ]),
        };
        ui.refresh(&config, &macros, &dictionary);

        assert_eq!(ui.read(), config);
        assert_eq!(
            ui.read_rules(),
            vec![("kate".to_string(), ModeConfig::Preedit)]
        );
        assert_eq!(
            ui.read_macros(),
            vec![("gs".to_string(), "giờ sáng".to_string())]
        );
        // A set: the order it hands the text view back in is not the input order.
        let mut words = ui.read_dictionary();
        words.sort();
        assert_eq!(words, vec!["asl".to_string(), "tr".to_string()]);
    }
}
