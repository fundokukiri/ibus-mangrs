//! GTK4 + libadwaita settings window, opened from the popup ("Cấu hình") or
//! with `-gui`.
//!
//! Every control writes straight to `config.toml` (and `macros.toml` /
//! `dictionary.txt`); the engine watches the config file mtime, so changes
//! apply within a second without restarting ibus-daemon.
//!
//! Layout: GNOME Settings style — a navigation sidebar on the left, one
//! preferences page per section on the right. Seven pages: Chung /
//! Dấu & tự động / Gõ tắt / Dữ liệu / Luật / Tinh chỉnh / Giới thiệu.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::{
    BracketConfig, CharsetConfig, Config, MenuNode, MethodConfig, ModeConfig, W2uConfig,
};
use crate::macros::{Dictionary, Macros};
use gtk4::prelude::*;
// The crate ships as `libadwaita`; every GNOME project calls it `adw`.
use adw::prelude::*;
use libadwaita as adw;
// TreeView/TreeStore/ComboBoxText are deprecated since GTK 4.10 in favor of
// ListView/DropDown, but they are the pragmatic editor widgets here: a tree
// editor on ListView+TreeListModel would triple the code for no user gain.
#[allow(deprecated)]
use gtk4::{
    CellRendererText, ComboBoxText, DropDown, Entry, Orientation, StringList, TextView, TreeIter,
    TreeStore, TreeView, TreeViewColumn,
};

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

mod content;
mod hotkey;
mod pages;
mod widgets;

use content::*;
use hotkey::*;
use widgets::*;
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
    enabled: adw::SwitchRow,
    method: adw::ComboRow,
    mode: adw::ComboRow,
    charset: adw::ComboRow,
    hotkey_btn: gtk4::Button,
    hotkey_value: Rc<RefCell<String>>,
    enabled_hotkey_btn: gtk4::Button,
    enabled_hotkey_value: Rc<RefCell<String>>,
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
    /// Macro editor: one boxed-list row per entry (key, value); GObject
    /// clones share the widgets, so reading them reads what the user sees.
    /// The rows live in a plain ListBox — a PreferencesGroup's first_child
    /// is its internal header, so a remove-everything loop would never end.
    macros_list: gtk4::ListBox,
    macros_rows: Rc<RefCell<Vec<MacroRow>>>,
    dictionary_enabled: adw::SwitchRow,
    dictionary_text: TextView,
    /// App Rules editor: one boxed-list row per rule. The rows hold the
    /// live values — GObject clones share the object — so reading them
    /// reads what the user sees. A plain ListBox, not a PreferencesGroup:
    /// the group's first_child is its internal header, so a
    /// remove-everything loop would never end.
    rules_box: gtk4::ListBox,
    rules_rows: Rc<RefCell<Vec<RuleRow>>>,
    /// Blocks row save handlers while `fill_rules` rebuilds the rows: the
    /// mode dropdown fires `selected_notify` on a programmatic set too.
    rules_loading: Rc<RefCell<bool>>,
    /// The save funnel, wired in `build` once the window exists; rows are
    /// created in `refresh`, so they fire edits through this instead of a
    /// closure captured at build time. `None` in headless tests.
    save_cb: SharedSave,
    /// "Thử luật" input and its verdict label.
    rule_preview_input: Entry,
    rule_preview: gtk4::Label,
    /// The focused-app chip: name label plus the last good name (the label
    /// also carries error text, so the usable value is kept separately).
    rule_focus: gtk4::Label,
    rule_focus_value: Rc<RefCell<String>>,
    hide_underline: adw::SwitchRow,
    debug_log: adw::SwitchRow,
    x11_delete: adw::SwitchRow,
    preedit_xim: adw::SwitchRow,
    commit_interval: adw::SpinRow,
    backspace_interval: adw::SpinRow,
    post_delay: adw::SpinRow,
    unicode_channel: adw::ComboRow,
    /// The panel-menu editor: a 4-column store (kind id, key, label, kind
    /// label), its view, the "add" type picker and the folder-name entry.
    /// Folders nest one level at most, matching `config::MenuNode`.
    menu_store: TreeStore,
    menu_view: TreeView,
    menu_kind: ComboBoxText,
    menu_name: Entry,
}

/// One App Rules editor row's widgets.
struct RuleRow {
    pattern: Entry,
    mode: DropDown,
}

/// One macro editor row's widgets.
struct MacroRow {
    key: Entry,
    value: Entry,
}

/// The save funnel handle shared between `Ui` and the rule-row closures.
type SharedSave = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

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

/// Inverse of `mode_index`, shared by the main mode dropdown and the
/// App Rules rows.
fn mode_at(i: u32) -> ModeConfig {
    match i {
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
            .set_value(config.commit_interval_ms as f64);
        self.backspace_interval
            .set_value(config.backspace_interval_ms as f64);
        self.post_delay.set_value(config.post_delay_ms as f64);
        self.unicode_channel
            .set_selected(match config.unicode_channel {
                crate::config::UnicodeChannel::Clipboard => 1,
                crate::config::UnicodeChannel::Commit => 0,
            });
        self.fill_menu_store(config.menu.as_deref().unwrap_or(&config.default_menu()));
        self.fill_macros(
            &macros
                .entries()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<Vec<_>>(),
        );
        set_text(
            &self.dictionary_text,
            &dictionary.words().collect::<Vec<_>>().join("\n"),
        );
        self.fill_rules(&config.app_rules);
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
        config
    }

    /// The macros as the rows currently show them; an empty key or value
    /// drops the row (the old text list skipped such lines too).
    fn macros_from_rows(&self) -> Vec<(String, String)> {
        self.macros_rows
            .borrow()
            .iter()
            .map(|row| {
                (
                    row.key.text().trim().to_string(),
                    row.value.text().trim().to_string(),
                )
            })
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect()
    }

    // --- macro editor (one boxed row per shortcut) ---

    /// Apply one user edit to the macro list: rebuild the rows, then save
    /// through the funnel like every other control.
    fn edit_macros(self: &Rc<Self>, edit: impl FnOnce(&mut Vec<(String, String)>)) {
        let mut entries = self.macros_from_rows();
        edit(&mut entries);
        self.fill_macros(&entries);
        self.fire_save();
    }

    /// Rebuild the macro rows from `entries`.
    fn fill_macros(self: &Rc<Self>, entries: &[(String, String)]) {
        while let Some(child) = self.macros_list.first_child() {
            self.macros_list.remove(&child);
        }
        self.macros_rows.borrow_mut().clear();
        for (idx, (k, v)) in entries.iter().enumerate() {
            self.macro_row(idx, k, v);
        }
    }

    /// One editable macro row: the key entry, the value entry and the
    /// delete button, hosted in the group's boxed list.
    fn macro_row(self: &Rc<Self>, idx: usize, key: &str, value: &str) {
        let h = gtk4::Box::new(Orientation::Horizontal, 8);
        let key_entry = Entry::builder()
            .text(key)
            .width_chars(14)
            .tooltip_text("Khóa gõ tắt (gõ xong sẽ mở rộng)")
            .build();
        let value_entry = Entry::builder()
            .text(value)
            .hexpand(true)
            .tooltip_text("Nội dung mở rộng ($TIME, $DATE)")
            .build();
        for e in [&key_entry, &value_entry] {
            let s = self.save_cb.clone();
            e.connect_activate(move |_| {
                if let Some(f) = s.borrow().as_ref() {
                    f();
                }
            });
            let s = self.save_cb.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| {
                if let Some(f) = s.borrow().as_ref() {
                    f();
                }
            });
            e.add_controller(controller);
        }
        let del = gtk4::Button::from_icon_name("edit-delete-symbolic");
        del.set_tooltip_text(Some("Xoá cặp này"));
        del.add_css_class("destructive-action");
        {
            let ui = self.clone();
            del.connect_clicked(move |_| {
                ui.edit_macros(|entries| {
                    if idx < entries.len() {
                        entries.remove(idx);
                    }
                });
            });
        }
        h.append(&key_entry);
        h.append(&value_entry);
        h.append(&del);
        let row = gtk4::ListBoxRow::new();
        row.set_activatable(false);
        row.set_child(Some(&h));
        self.macros_list.append(&row);
        self.macros_rows.borrow_mut().push(MacroRow {
            key: key_entry,
            value: value_entry,
        });
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

    /// The rules as the rows currently show them; row order is match
    /// priority. An empty pattern is kept (an inert rule the user can
    /// delete), so `refresh`'s round-trip assert holds for hand-edited
    /// configs too.
    fn rules_from_rows(&self) -> Vec<crate::config::AppRule> {
        self.rules_rows
            .borrow()
            .iter()
            .map(|row| crate::config::AppRule {
                pattern: row.pattern.text().trim().to_string(),
                mode: mode_at(row.mode.selected()),
            })
            .collect()
    }

    /// Apply one user edit to the rule list: rebuild the rows, then save
    /// through the funnel like every other control.
    fn edit_rules(self: &Rc<Self>, edit: impl FnOnce(&mut Vec<crate::config::AppRule>)) {
        let mut rules = self.rules_from_rows();
        edit(&mut rules);
        self.fill_rules(&rules);
        self.fire_save();
    }

    /// Rebuild the rule rows from `rules`. Also runs at refresh — that path
    /// must not save; only user edits do (`edit_rules`).
    fn fill_rules(self: &Rc<Self>, rules: &[crate::config::AppRule]) {
        *self.rules_loading.borrow_mut() = true;
        while let Some(child) = self.rules_box.first_child() {
            self.rules_box.remove(&child);
        }
        self.rules_rows.borrow_mut().clear();
        for (idx, rule) in rules.iter().enumerate() {
            self.rule_row(idx, &rule.pattern, rule.mode);
        }
        *self.rules_loading.borrow_mut() = false;
        self.update_rule_preview();
    }

    /// One editable rule row: the pattern entry, the mode dropdown and the
    /// reorder/delete buttons. Row order is match priority — the engine
    /// takes the first rule that hits.
    fn rule_row(self: &Rc<Self>, idx: usize, pattern: &str, mode: ModeConfig) {
        let row = gtk4::Box::new(Orientation::Horizontal, 6);
        row.set_margin_start(10);
        row.set_margin_end(10);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        let entry = Entry::builder()
            .text(pattern)
            .tooltip_text(
                "Tên app: khớp một phần, không phân biệt hoa/thường (vd firefox, sublime)",
            )
            .hexpand(true)
            .build();
        let mode_dd = drop_down(MODES, mode_index(mode));
        mode_dd.set_tooltip_text(Some("Chế độ gõ khi luật khớp"));
        {
            let rows = self.rules_rows.clone();
            rows.borrow_mut().push(RuleRow {
                pattern: entry.clone(),
                mode: mode_dd.clone(),
            });
        }
        {
            let ui = self.clone();
            entry.connect_changed(move |_| ui.update_rule_preview());
        }
        {
            let ui = self.clone();
            entry.connect_activate(move |_| ui.fire_save());
        }
        {
            let ui = self.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| ui.fire_save());
            entry.add_controller(controller);
        }
        {
            let ui = self.clone();
            mode_dd.connect_selected_notify(move |_| {
                if *ui.rules_loading.borrow() {
                    return;
                }
                ui.fire_save();
                ui.update_rule_preview();
            });
        }
        let up = gtk4::Button::with_label("▲");
        up.set_tooltip_text(Some("Lên — luật trước thắng"));
        {
            let ui = self.clone();
            up.connect_clicked(move |_| {
                ui.edit_rules(|rules| {
                    if idx > 0 {
                        rules.swap(idx - 1, idx);
                    }
                });
            });
        }
        let down = gtk4::Button::with_label("▼");
        down.set_tooltip_text(Some("Xuống"));
        {
            let ui = self.clone();
            down.connect_clicked(move |_| {
                ui.edit_rules(|rules| {
                    if idx + 1 < rules.len() {
                        rules.swap(idx, idx + 1);
                    }
                });
            });
        }
        let del = gtk4::Button::with_label("✕");
        del.set_tooltip_text(Some("Xoá luật này"));
        del.add_css_class("destructive-action");
        {
            let ui = self.clone();
            del.connect_clicked(move |_| {
                ui.edit_rules(|rules| {
                    if idx < rules.len() {
                        rules.remove(idx);
                    }
                });
            });
        }
        row.append(&entry);
        row.append(&mode_dd);
        row.append(&up);
        row.append(&down);
        row.append(&del);
        let list_row = gtk4::ListBoxRow::new();
        list_row.set_activatable(false);
        list_row.set_child(Some(&row));
        self.rules_box.append(&list_row);
    }

    /// The "thử luật" verdict: which rule hits the typed name and the mode
    /// it forces. Rules only — the engine's capability fallbacks are not
    /// modelled here.
    fn update_rule_preview(&self) {
        let input = self.rule_preview_input.text().trim().to_string();
        if input.is_empty() {
            self.rule_preview.set_text("");
            return;
        }
        let rules = self.rules_from_rows();
        let target = Some(input.as_str());
        if let Some(mode) = crate::config::app_rule_mode(&rules, target) {
            let (idx, pattern) = rules
                .iter()
                .position(|r| crate::config::app_rule_matches(&r.pattern, target))
                .map(|i| (i + 1, rules[i].pattern.clone()))
                .unwrap_or((0, String::new()));
            let label = MODES[mode_index(mode) as usize];
            self.rule_preview
                .set_text(&format!("Khớp luật #{idx} \"{pattern}\" → {label}"));
        } else {
            self.rule_preview
                .set_text("Không luật nào khớp → dùng chế độ chung hiện tại");
        }
    }

    /// Run the save funnel if the window wired it (headless tests skip it).
    fn fire_save(&self) {
        let save = self.save_cb.borrow().clone();
        if let Some(save) = save {
            save();
        }
    }

    fn save_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.read();
        config.save()?;

        let mut macros = Macros::load();
        macros.set_entries(self.macros_from_rows().into_iter());
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
    let enabled = switch_row(
        "Bật tiếng Việt",
        "Tắt để gõ thẳng mọi phím (gõ tắt vẫn chạy nếu bật ở tab Gõ tắt)",
    );
    let method = combo_row("Kiểu gõ", "Telex, VNI, VIQR, Microsoft layout", METHODS);
    let mode = combo_row(
        "Chế độ gõ",
        "Gạch chân (mặc định), Không gạch chân, Surrounding, 3 mode uinput, 2 mode ForwardKeyEvent, OFF, Gõ mặc định",
        MODES,
    );
    let charset = combo_row("Bảng mã", "Unicode và 10 bảng mã cũ", CHARSETS);
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
    let modern = switch_row("Kiểu dấu mới", "hòa (chuẩn mới) thay vì hoà (chuẩn cũ)");
    let free = switch_row("Dấu tự do", "Dấu đặt anywhere trong từ: hoangf → hoàng");
    let auto_correct = switch_row(
        "Từ sai trả phím gốc",
        "Âm tiết không hợp lệ trả về đúng phím đã bấm",
    );
    let auto_restore = switch_row(
        "Khôi phục từ nước ngoài",
        "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
    );
    let abbrev = switch_row(
        "Viết tắt",
        "Giữ đ trong từ viết tắt: ddc → đc, ddk → đk, ddt → đt",
    );
    let bypass_url = switch_row(
        "Không dấu sau URL",
        "Từ gõ ngay sau / : @ . _ ~ = # & % ? giữ nguyên",
    );
    let w2u = combo_row("W thành ư", "Tắt / trừ đầu từ / mọi vị trí", ON_OFF_MODES);
    let bracket = combo_row(
        "[ ] thành ơ ư",
        "Tắt / trừ đầu từ / mọi vị trí",
        ON_OFF_MODES,
    );
    let em_dash = switch_row(
        "Gạch ngang dài",
        "Hai dấu trừ liên tiếp thành gạch dài (-- thành —)",
    );
    let auto_capitalize = switch_row("Tự viết hoa", "Sau . ! ? và Enter");
    let double_space = switch_row(
        "2 dấu cách = chấm",
        "Thử nghiệm: hai dấu cách thành dấu chấm",
    );
    let macro_enabled = switch_row("Gõ tắt", "Khóa gõ tắt mở rộng thành cụm khi hết từ");
    let capitalize_macro = switch_row("Viết hoa gõ tắt", "Khóa viết hoa → nội dung viết hoa theo");
    let macro_in_off_mode = switch_row("Gõ tắt khi OFF", "Vẫn mở rộng khi tiếng Việt đang tắt");
    let time_format = Entry::builder().width_chars(14).build();
    let date_format = Entry::builder().width_chars(14).build();
    let macros_list = gtk4::ListBox::new();
    macros_list.add_css_class("boxed-list");
    macros_list.set_selection_mode(gtk4::SelectionMode::None);
    let macros_rows = Rc::new(RefCell::new(Vec::new()));
    let dictionary_enabled = switch_row(
        "Từ điển của tôi",
        "Bật: từ trong từ điển không bị khôi phục về chuỗi phím (~/.config/ibus-mangrs/dictionary.txt)",
    );
    let dictionary_text = text_view("");
    // Fixed height so the list cannot stretch (or collapse) the page; the
    // page's own scroll takes over beyond it.
    dictionary_text.set_height_request(160);
    // App Rules editor widgets. The rows live in `rules_box`, rebuilt by
    // `fill_rules`; the rest are the fixed helpers around them.
    let rules_box = gtk4::ListBox::new();
    rules_box.add_css_class("boxed-list");
    rules_box.set_selection_mode(gtk4::SelectionMode::None);
    let rule_preview_input = Entry::builder()
        .placeholder_text("Firefox / wayland:Firefox / xim")
        .hexpand(true)
        .build();
    rule_preview_input.set_tooltip_text(Some(
        "Nhập tên app rồi Enter: cho biết luật nào sẽ khớp và chế độ gõ kết quả",
    ));
    let rule_preview = gtk4::Label::new(None);
    rule_preview.add_css_class("dimmed");
    rule_preview.set_xalign(0.0);
    rule_preview.set_wrap(true);
    let rule_focus = gtk4::Label::new(Some("—"));
    rule_focus.set_xalign(0.0);
    rule_focus.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    rule_focus.set_tooltip_text(Some(
        "Chuyển sang app đích rồi quay lại cửa sổ này — tên app sẽ hiện ở đây (cần extension MăngRS Focus)",
    ));
    let rule_focus_value = Rc::new(RefCell::new(String::new()));
    let rules_rows = Rc::new(RefCell::new(Vec::new()));
    let rules_loading = Rc::new(RefCell::new(false));
    let save_cb: SharedSave = Rc::new(RefCell::new(None));
    let hide_underline = switch_row("Ẩn gạch chân", "Chữ đang gõ không có gạch chân (tùy app)");
    let debug_log = switch_row("Ghi log gỡ lỗi", "Ghi vào ~/.cache/ibus-mangrs/debug.log");
    let x11_delete = switch_row(
        "Xoá đuôi từ ở app X11",
        "App X11 trực tiếp xoá đuôi từ dù không gửi báo cáo — như ibus-bamboo. Có thể xoá nhầm vài ký tự nếu con trỏ bị đổi chỗ.",
    );
    let preedit_xim = switch_row(
        "Preedit cho app XIM",
        "Giữ gạch chân cho app XIM (Java/JetBrains): chữ đang gõ hiện trong cửa sổ riêng rồi commit khi hết từ.",
    );
    let commit_interval = spin_row(
        "Nhịp gõ chữ",
        "Khoảng cách giữa hai lần gửi chữ (ms). Mặc định: 20",
        1.0,
        500.0,
        1.0,
    );
    let backspace_interval = spin_row(
        "Nhịp xoá lùi",
        "Khoảng cách giữa hai lần Backspace (ms). Mặc định: 5",
        1.0,
        100.0,
        1.0,
    );
    let post_delay = spin_row(
        "Độ trễ sau từ",
        "Chờ sau khi viết lại từ trước khi gõ tiếp (ms). Mặc định: 10",
        0.0,
        500.0,
        1.0,
    );
    let unicode_channel = combo_row(
        "Kênh gửi từ tiếng Việt",
        "CommitText an toàn, hoặc Clipboard (Ctrl+V, tạm đè clipboard)",
        CHANNELS,
    );

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
        macros_list,
        macros_rows,
        dictionary_enabled,
        dictionary_text,
        rules_box,
        rules_rows,
        rules_loading,
        save_cb,
        rule_preview_input,
        rule_preview,
        rule_focus,
        rule_focus_value,
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
            ui.read().app_rules,
            vec![crate::config::AppRule {
                pattern: "kate".into(),
                mode: ModeConfig::Preedit,
            }]
        );
        assert_eq!(
            ui.macros_from_rows(),
            vec![("gs".to_string(), "giờ sáng".to_string())]
        );
        // A set: the order it hands the text view back in is not the input order.
        let mut words = ui.read_dictionary();
        words.sort();
        assert_eq!(words, vec!["asl".to_string(), "tr".to_string()]);
    }
}
