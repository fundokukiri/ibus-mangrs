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

/// (kind id, key, display label, Vietnamese kind label) for one node.
fn menu_row(node: &MenuNode) -> (String, String, String, String) {
    match node {
        MenuNode::Toggle { key } => (
            "toggle".into(),
            key.clone(),
            menu_toggle_label(key),
            t("menu.kind_toggle", "Bật/tắt"),
        ),
        MenuNode::Radios { group, .. } => (
            "radios".into(),
            group.clone(),
            menu_radios_label(group),
            t("menu.kind_radios", "Nhóm chọn"),
        ),
        MenuNode::Action { key } => (
            "action".into(),
            key.clone(),
            menu_action_label(key),
            t("menu.kind_action", "Chức năng"),
        ),
        MenuNode::Separator => (
            "sep".into(),
            String::new(),
            "— phân cách —".into(),
            t("menu.kind_sep", "Gạch ngang"),
        ),
        MenuNode::Folder { label, .. } => (
            "folder".into(),
            String::new(),
            label.clone(),
            t("menu.kind_folder", "Thư mục"),
        ),
    }
}

fn menu_toggle_label(key: &str) -> String {
    if key == "enabled" {
        return t("menu.toggle_enabled", "Bật tiếng Việt");
    }
    crate::ibus_prop::quick_toggle(key)
        .map(|tg| tg.label.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn menu_radios_label(group: &str) -> String {
    match group {
        "charset" => t("menu.label_charset", "Bảng mã"),
        "method" => t("menu.label_method", "Kiểu gõ"),
        "display" => t("menu.label_display", "Chế độ gõ"),
        other => other.into(),
    }
}

fn menu_action_label(key: &str) -> String {
    match key {
        "macro_editor" => t("menu.action_macro_editor", "Mở bảng gõ tắt…"),
        "settings" => t("menu.action_settings", "Cấu hình"),
        "about" => t("menu.action_about", "Giới thiệu (About)"),
        other => other.into(),
    }
}

mod content;
mod hotkey;
mod pages;
mod text;
mod widgets;

use text::t;

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

struct Ui {
    enabled: adw::SwitchRow,
    method: adw::ComboRow,
    mode: adw::ComboRow,
    charset: adw::ComboRow,
    /// GNOME-style hotkey row: the dim combo text plus the small edit
    /// button; the recorder captures on the edit button (see
    /// `hotkey_wire`).
    hotkey_label: gtk4::Label,
    hotkey_edit: gtk4::Button,
    hotkey_value: Rc<RefCell<String>>,
    enabled_hotkey_label: gtk4::Label,
    enabled_hotkey_edit: gtk4::Button,
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
    /// "Thử luật" input and its verdict row.
    rule_preview_input: adw::EntryRow,
    rule_preview: adw::ActionRow,
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
    /// The panel-menu editor: the design vector is the source of truth
    /// (`read` clones it, so the round-trip holds without walking rows);
    /// the boxed list is its view, rebuilt on every edit. Folders nest one
    /// level at most, matching `config::MenuNode`.
    menu: Rc<RefCell<Vec<MenuNode>>>,
    menu_box: gtk4::ListBox,
}

/// One macro editor row's widgets.
struct MacroRow {
    key: Entry,
    value: Entry,
}

/// The save funnel handle shared between `Ui` and the rule-row closures.
type SharedSave = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Where a picked menu item lands: root end, or inside one root container
/// (a folder takes generic items, a radio group only its own entries).
#[derive(Clone)]
pub(crate) enum MenuScope {
    Root,
    Container(Vec<usize>),
}

/// "0.2.1"-style key so a drag payload carries the row's design position.
fn menu_path_key(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

/// Inverse of `menu_path_key`.
fn menu_parse_key(key: &str) -> Option<Vec<usize>> {
    key.split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<_>>>()
}

/// The swap target for one reorder step, or None at the list ends.
fn menu_step(idx: usize, delta: isize, len: usize) -> Option<usize> {
    match delta.signum() {
        -1 => idx.checked_sub(1),
        _ => Some(idx + 1).filter(|&p| p < len),
    }
}

/// Rename a root folder through a small dialog. Only root-level folders
/// are renamable: entries inside keep their built-in labels, and children
/// of folders already sit where their parent design put them.
fn menu_rename_dialog(ui: &Rc<Ui>, path: &[usize], current: &str, parent: &gtk4::Widget) {
    let entry = Entry::builder()
        .text(current)
        .activates_default(true)
        .build();
    let dialog = adw::AlertDialog::builder()
        .heading(t("panel.rename_heading", "Đổi tên thư mục"))
        .body(t("panel.rename_body", "Nhập tên mới cho thư mục đang chọn"))
        .extra_child(&entry)
        .build();
    dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
    dialog.add_response("apply", &t("panel.rename_apply", "Đổi tên"));
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    {
        let ui = ui.clone();
        let path = path.to_vec();
        let entry = entry.clone();
        dialog.connect_response(Some("apply"), move |_, _| {
            if ui.menu_rename(&path, entry.text().trim()) {
                ui.fill_menu_list();
                ui.fire_save();
            }
        });
    }
    dialog.present(parent.root().and_downcast::<gtk4::Window>().as_ref());
}

/// Mirror of the old tree-store read: folders keep one level — a nested
/// folder or radio group survives as an empty node, which the engine
/// skips anyway — and root radio groups keep their entry keys.
fn normalize_menu(nodes: &[MenuNode]) -> Vec<MenuNode> {
    nodes
        .iter()
        .map(|node| match node {
            MenuNode::Folder { label, children } => MenuNode::Folder {
                label: label.clone(),
                children: children
                    .iter()
                    .map(|child| match child {
                        MenuNode::Folder { label, .. } => MenuNode::Folder {
                            label: label.clone(),
                            children: Vec::new(),
                        },
                        MenuNode::Radios { group, .. } => MenuNode::Radios {
                            group: group.clone(),
                            children: Vec::new(),
                        },
                        other => other.clone(),
                    })
                    .collect(),
            },
            other => other.clone(),
        })
        .collect()
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
        render_hotkey(&self.hotkey_label, &config.cycle_mode_hotkey);
        *self.enabled_hotkey_value.borrow_mut() = config.enabled_hotkey.clone();
        render_hotkey(&self.enabled_hotkey_label, &config.enabled_hotkey);
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
        let menu = config.menu.clone().unwrap_or_else(|| config.default_menu());
        *self.menu.borrow_mut() = normalize_menu(&menu);
        self.fill_menu_list();
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
            .tooltip_text(t(
                "rows.macro_key.tooltip",
                "Khóa gõ tắt (gõ xong sẽ mở rộng)",
            ))
            .build();
        let value_entry = Entry::builder()
            .text(value)
            .hexpand(true)
            .tooltip_text(t(
                "rows.macro_value.tooltip",
                "Nội dung mở rộng ($TIME, $DATE)",
            ))
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
        del.set_tooltip_text(Some(&t("rows.macro_delete.tooltip", "Xoá cặp này")));
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

    // --- panel-menu editor (the design vector is the source of truth) ---

    /// The design as the user sees it: `read()` clones this, so the
    /// round-trip holds without walking the widgets.
    fn read_menu(&self) -> Vec<MenuNode> {
        self.menu.borrow().clone()
    }

    /// Rebuild the row list from the design vector. Also runs after every
    /// edit; the caller saves through the funnel.
    fn fill_menu_list(self: &Rc<Self>) {
        while let Some(child) = self.menu_box.first_child() {
            self.menu_box.remove(&child);
        }
        let nodes = self.menu.borrow().clone();
        for (idx, node) in nodes.iter().enumerate() {
            self.menu_box
                .append(&self.menu_row_widget(vec![idx], node, 0));
        }
    }

    /// One editor row: a native ActionRow (or ExpanderRow for a container
    /// with children) with the type icon, the drag handle and the per-row
    /// menu. `path` addresses the item in the design vector; folder and
    /// radios children sit one level down (depth 1, leaves by
    /// normalization).
    fn menu_row_widget(
        self: &Rc<Self>,
        path: Vec<usize>,
        node: &MenuNode,
        depth: usize,
    ) -> gtk4::Widget {
        let (kind, _key, label, kind_label) = menu_row(node);
        // A separator keeps only its dim "— phân cách —" label; containers
        // with children expand, everything else is a plain row.
        let widget: gtk4::Widget = match node {
            MenuNode::Folder { children, .. } if depth == 0 && !children.is_empty() => {
                let exp = adw::ExpanderRow::builder()
                    .title(label.as_str())
                    .subtitle(kind_label.as_str())
                    .build();
                for (i, child) in children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(i);
                    exp.add_row(&self.menu_row_widget(child_path, child, depth + 1));
                }
                exp.upcast::<gtk4::Widget>()
            }
            MenuNode::Radios { group, children } if depth == 0 && !children.is_empty() => {
                let exp = adw::ExpanderRow::builder()
                    .title(label.as_str())
                    .subtitle(kind_label.as_str())
                    .build();
                for (i, key) in children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(i);
                    let entry = adw::ActionRow::builder()
                        .title(menu_entry_label(group, key).as_str())
                        .subtitle(t("menu.kind_entry", "Mục chọn").as_str())
                        .activatable(false)
                        .build();
                    let icon = gtk4::Image::from_icon_name("radio-checked-symbolic");
                    icon.add_css_class("dim-label");
                    entry.add_prefix(&icon);
                    entry.add_prefix(&self.menu_drag_handle(&child_path));
                    entry.add_suffix(&self.menu_more_button(child_path, "entry"));
                    exp.add_row(&entry);
                }
                exp.upcast::<gtk4::Widget>()
            }
            _ => {
                let row = adw::ActionRow::builder()
                    .title(label.as_str())
                    .subtitle(kind_label.as_str())
                    .activatable(false)
                    .build();
                if kind == "sep" {
                    // A slim dim row: separators carry no title emphasis.
                    if let Some(title) = row_title(&row) {
                        title.add_css_class("dim-label");
                    }
                }
                row.upcast::<gtk4::Widget>()
            }
        };
        let icon = match kind.as_str() {
            "toggle" => Some(gtk4::Image::from_icon_name("emblem-ok-symbolic")),
            "radios" => Some(gtk4::Image::from_icon_name("radio-checked-symbolic")),
            "action" => Some(gtk4::Image::from_icon_name("system-run-symbolic")),
            "folder" => Some(gtk4::Image::from_icon_name("folder-symbolic")),
            _ => None,
        };
        if let Some(icon) = &icon {
            icon.add_css_class("dim-label");
        }
        let handle = self.menu_drag_handle(&path);
        let more = self.menu_more_button(path.clone(), &kind);
        if let Some(action) = widget.downcast_ref::<adw::ActionRow>() {
            if let Some(icon) = &icon {
                action.add_prefix(icon);
            }
            action.add_prefix(&handle);
            action.add_suffix(&more);
        } else if let Some(exp) = widget.downcast_ref::<adw::ExpanderRow>() {
            if let Some(icon) = &icon {
                exp.add_prefix(icon);
            }
            exp.add_prefix(&handle);
            exp.add_suffix(&more);
        }
        // The row itself accepts its siblings' payloads (same parent only).
        let drop =
            gtk4::DropTarget::new(gtk4::glib::types::Type::STRING, gtk4::gdk::DragAction::MOVE);
        {
            let ui = self.clone();
            let dst_path = path;
            drop.connect_drop(move |_, value, _, _| {
                let Ok(src_key) = value.get::<String>() else {
                    return false;
                };
                let Some(src_path) = menu_parse_key(&src_key) else {
                    return false;
                };
                if ui.menu_drag(&src_path, &dst_path) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                false
            });
        }
        widget.add_controller(drop);
        widget
    }

    /// The drag handle: a DragSource whose payload is the row's design
    /// path (see `menu_drag` for the receiving side).
    fn menu_drag_handle(&self, path: &[usize]) -> gtk4::Image {
        let handle = gtk4::Image::from_icon_name("list-drag-handle-symbolic");
        handle.add_css_class("dim-label");
        let payload = menu_path_key(path).to_value();
        let src = gtk4::DragSource::new();
        src.set_actions(gtk4::gdk::DragAction::MOVE);
        src.connect_prepare(move |_, _, _| Some(gtk4::gdk::ContentProvider::for_value(&payload)));
        handle.add_controller(src);
        handle
    }

    /// The per-row menu: reorder, out, rename (root folders), add-into
    /// (root containers) and delete. Every action re-renders and saves
    /// through the funnel — row state exists only in the design vector.
    fn menu_more_button(self: &Rc<Self>, path: Vec<usize>, kind: &str) -> gtk4::MenuButton {
        let more = gtk4::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.add_css_class("flat");
        more.set_tooltip_text(Some(&t("panel.row_menu_tooltip", "Thao tác với mục này")));
        let popover = gtk4::Popover::new();
        let list = gtk4::Box::new(Orientation::Vertical, 0);
        // Sensitivity: the sibling position decides up/down, the kind
        // decides rename and add-into, and only folder children can leave.
        let (pos, len, parent_kind) = {
            let menu = self.menu.borrow();
            let siblings_len = match path.len() {
                1 => menu.len(),
                2 => match menu.get(path[0]) {
                    Some(MenuNode::Folder { children, .. }) => children.len(),
                    Some(MenuNode::Radios { children, .. }) => children.len(),
                    _ => 0,
                },
                _ => 0,
            };
            let parent_kind = match path.len() {
                2 => menu.get(path[0]).map(|n| menu_row(n).0).unwrap_or_default(),
                _ => "root".to_string(),
            };
            (path.last().copied().unwrap_or(0), siblings_len, parent_kind)
        };
        let item = |list: &gtk4::Box, label: &str, css: &str| -> gtk4::Button {
            let btn = gtk4::Button::with_label(label);
            btn.add_css_class("flat");
            if !css.is_empty() {
                btn.add_css_class(css);
            }
            list.append(&btn);
            btn
        };
        let up = item(&list, &t("panel.move_up", "Lên"), "");
        up.set_sensitive(pos > 0);
        {
            let ui = self.clone();
            let p = path.clone();
            let popover = popover.clone();
            up.connect_clicked(move |_| {
                if ui.menu_move(&p, -1) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                popover.popdown();
            });
        }
        let down = item(&list, &t("panel.move_down", "Xuống"), "");
        down.set_sensitive(pos + 1 < len);
        {
            let ui = self.clone();
            let p = path.clone();
            let popover = popover.clone();
            down.connect_clicked(move |_| {
                if ui.menu_move(&p, 1) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                popover.popdown();
            });
        }
        if parent_kind == "folder" {
            let out = item(&list, &t("panel.move_out", "Đưa ra khỏi thư mục"), "");
            {
                let ui = self.clone();
                let p = path.clone();
                let popover = popover.clone();
                out.connect_clicked(move |_| {
                    if ui.menu_out(&p) {
                        ui.fill_menu_list();
                        ui.fire_save();
                    }
                    popover.popdown();
                });
            }
        }
        if kind == "folder" && path.len() == 1 {
            let rename = item(&list, &t("panel.rename_apply", "Đổi tên"), "");
            let current = self
                .menu
                .borrow()
                .get(path[0])
                .and_then(|n| match n {
                    MenuNode::Folder { label, .. } => Some(label.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            {
                let ui = self.clone();
                let p = path.clone();
                let popover = popover.clone();
                rename.connect_clicked(move |btn| {
                    menu_rename_dialog(&ui, &p, &current, btn.upcast_ref());
                    popover.popdown();
                });
            }
        }
        if (kind == "folder" || kind == "radios") && path.len() == 1 {
            let add_here = gtk4::MenuButton::new();
            add_here.set_label(&t("panel.add_here", "Thêm vào đây…"));
            add_here.add_css_class("flat");
            let add_popover = gtk4::Popover::new();
            add_popover.set_child(Some(
                &self.menu_add_list(&MenuScope::Container(vec![path[0]]), &add_popover),
            ));
            add_here.set_popover(Some(&add_popover));
            list.append(&add_here);
        }
        let del = item(&list, &t("panel.item_delete", "Xóa"), "destructive-action");
        {
            let ui = self.clone();
            let p = path.clone();
            let popover = popover.clone();
            del.connect_clicked(move |_| {
                if ui.menu_delete(&p) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                popover.popdown();
            });
        }
        popover.set_child(Some(&list));
        more.set_popover(Some(&popover));
        more
    }

    /// The addable-item list for `scope` (root, or one root container):
    /// everything generic for root/folder, a radio group's own entries for
    /// that group — the old type picker's rule. Buttons append, re-render,
    /// save and close `popover`.
    fn menu_add_list(self: &Rc<Self>, scope: &MenuScope, popover: &gtk4::Popover) -> gtk4::Widget {
        let list = gtk4::Box::new(Orientation::Vertical, 0);
        for (id, label) in self.menu_addable_items(scope) {
            let btn = gtk4::Button::with_label(&label);
            btn.add_css_class("flat");
            {
                let ui = self.clone();
                let scope = scope.clone();
                let popover = popover.clone();
                btn.connect_clicked(move |_| {
                    let Some((kind, key)) = id.split_once(':') else {
                        return;
                    };
                    if ui.menu_add(kind, key, &scope) {
                        ui.fill_menu_list();
                        ui.fire_save();
                    }
                    popover.popdown();
                });
            }
            list.append(&btn);
        }
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&list));
        scroll.upcast::<gtk4::Widget>()
    }

    /// The items addable in `scope` (id "kind:key", display label).
    fn menu_addable_items(&self, scope: &MenuScope) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let MenuScope::Container(path) = scope {
            let group = self.menu.borrow().get(path[0]).and_then(|n| match n {
                MenuNode::Radios { group, .. } => Some(group.clone()),
                _ => None,
            });
            if let Some(group) = group {
                let entries: &[(&str, &str)] = match group.as_str() {
                    "charset" => crate::ibus_prop::CHARSETS,
                    "method" => crate::ibus_prop::METHODS,
                    "display" => crate::ibus_prop::MODES,
                    _ => &[],
                };
                for (key, label) in entries {
                    out.push((format!("entry:{key}"), label.to_string()));
                }
                return out;
            }
        }
        out.push((
            "toggle:enabled".into(),
            t("menu.toggle_enabled", "Bật tiếng Việt"),
        ));
        for quick in crate::ibus_prop::QUICK_TOGGLES {
            out.push((format!("toggle:{}", quick.key), quick.label.to_string()));
        }
        out.push((
            "radios:charset".into(),
            t("menu.radios_charset", "Bảng mã (nhóm chọn)"),
        ));
        out.push((
            "radios:method".into(),
            t("menu.radios_method", "Kiểu gõ (nhóm chọn)"),
        ));
        out.push((
            "radios:display".into(),
            t("menu.radios_display", "Chế độ gõ (nhóm chọn)"),
        ));
        out.push((
            "action:macro_editor".into(),
            t("menu.action_macro_editor", "Mở bảng gõ tắt…"),
        ));
        out.push((
            "action:settings".into(),
            t("menu.action_settings", "Cấu hình"),
        ));
        out.push((
            "action:about".into(),
            t("menu.action_about", "Giới thiệu (About)"),
        ));
        out.push(("sep:".into(), t("menu.sep", "Gạch ngang phân cách")));
        // Last, so the frequently used kinds keep their default (first)
        // spot like the old picker.
        out.push(("folder:".into(), t("menu.folder", "Thư mục mới")));
        out
    }

    /// Append the picked item to its scope (root end, or the container's
    /// end — the tree editor inserted at the container's end too). A radio
    /// group takes only its own entries.
    fn menu_add(&self, kind: &str, key: &str, scope: &MenuScope) -> bool {
        let node = match kind {
            "toggle" => MenuNode::Toggle {
                key: key.to_string(),
            },
            "radios" => MenuNode::Radios {
                group: key.to_string(),
                children: Vec::new(),
            },
            "action" => MenuNode::Action {
                key: key.to_string(),
            },
            "sep" => MenuNode::Separator,
            _ => MenuNode::Folder {
                label: t("menu.folder", "Thư mục mới"),
                children: Vec::new(),
            },
        };
        let mut menu = self.menu.borrow_mut();
        match scope {
            MenuScope::Root => {
                menu.push(node);
                true
            }
            MenuScope::Container(path) => match menu.get_mut(path[0]) {
                Some(MenuNode::Radios { children, .. }) if kind == "entry" => {
                    children.push(key.to_string());
                    true
                }
                Some(MenuNode::Folder { children, .. }) => {
                    children.push(node);
                    true
                }
                _ => false,
            },
        }
    }

    /// Reorder a sibling pair. `path` points at the moved row; delta -1 is
    /// up, 1 is down (a radios entry reorders the group's key list).
    fn menu_move(&self, path: &[usize], delta: isize) -> bool {
        let mut menu = self.menu.borrow_mut();
        let Some(&idx) = path.last() else {
            return false;
        };
        match path.len() {
            1 => match menu_step(idx, delta, menu.len()) {
                Some(target) => {
                    menu.swap(idx, target);
                    true
                }
                None => false,
            },
            2 => match menu.get_mut(path[0]) {
                Some(MenuNode::Folder { children, .. }) => {
                    match menu_step(idx, delta, children.len()) {
                        Some(target) => {
                            children.swap(idx, target);
                            true
                        }
                        None => false,
                    }
                }
                Some(MenuNode::Radios { children, .. }) => {
                    match menu_step(idx, delta, children.len()) {
                        Some(target) => {
                            children.swap(idx, target);
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Drop side of the drag handle: move the source item before the
    /// target row within the same sibling list. Cross-parent drops are
    /// ignored — the popup nests one level and reparenting by drop is not
    /// worth the surprise.
    fn menu_drag(&self, src: &[usize], dst: &[usize]) -> bool {
        if src.len() != dst.len() || src == dst || src[..src.len() - 1] != dst[..dst.len() - 1] {
            return false;
        }
        let (Some(&s), Some(&d)) = (src.last(), dst.last()) else {
            return false;
        };
        let mut menu = self.menu.borrow_mut();
        match src.len() {
            1 => {
                let item = menu.remove(s);
                let d = if s < d { d - 1 } else { d };
                menu.insert(d, item);
                true
            }
            2 => match menu.get_mut(src[0]) {
                Some(MenuNode::Folder { children, .. }) => {
                    if s >= children.len() || d >= children.len() {
                        return false;
                    }
                    let item = children.remove(s);
                    let d = if s < d { d - 1 } else { d };
                    children.insert(d, item);
                    true
                }
                Some(MenuNode::Radios { children, .. }) => {
                    if s >= children.len() || d >= children.len() {
                        return false;
                    }
                    let key = children.remove(s);
                    let d = if s < d { d - 1 } else { d };
                    children.insert(d, key);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Move a folder child up to the root (appended at the end, like the
    /// tree editor appended it). Radio-group entries cannot exist at the
    /// root, so they stay put.
    fn menu_out(&self, path: &[usize]) -> bool {
        let mut menu = self.menu.borrow_mut();
        if path.len() != 2 {
            return false;
        }
        let item = match menu.get_mut(path[0]) {
            Some(MenuNode::Folder { children, .. }) if path[1] < children.len() => {
                Some(children.remove(path[1]))
            }
            _ => None,
        };
        let Some(item) = item else {
            return false;
        };
        // An emptied folder goes away with its last child.
        let emptied = matches!(
            menu.get(path[0]),
            Some(MenuNode::Folder { children, .. }) if children.is_empty()
        );
        if emptied {
            menu.remove(path[0]);
        }
        menu.push(item);
        true
    }

    /// Rename a root folder. Entries inside keep their built-in labels,
    /// and only root folders are renamable.
    fn menu_rename(&self, path: &[usize], label: &str) -> bool {
        let label = label.trim();
        if label.is_empty() || path.len() != 1 {
            return false;
        }
        let mut menu = self.menu.borrow_mut();
        match menu.get_mut(path[0]) {
            Some(MenuNode::Folder { label: l, .. }) => {
                *l = label.to_string();
                true
            }
            _ => false,
        }
    }

    /// Remove one item. The popup must never end up empty (the last
    /// top-level entry refuses), and an emptied container goes away with
    /// its last child — both as the tree editor did.
    fn menu_delete(&self, path: &[usize]) -> bool {
        let mut menu = self.menu.borrow_mut();
        let Some(&idx) = path.last() else {
            return false;
        };
        match path.len() {
            1 if menu.len() <= 1 => false,
            1 => {
                menu.remove(idx);
                true
            }
            2 => match menu.get_mut(path[0]) {
                Some(MenuNode::Folder { children, .. }) if idx < children.len() => {
                    children.remove(idx);
                    if children.is_empty() {
                        menu.remove(path[0]);
                    }
                    true
                }
                Some(MenuNode::Radios { children, .. }) if idx < children.len() => {
                    children.remove(idx);
                    if children.is_empty() {
                        menu.remove(path[0]);
                    }
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Throw the design away and go back to the shipped shape.
    fn menu_reset(&self) {
        let default = Config::load().default_menu();
        *self.menu.borrow_mut() = normalize_menu(&default);
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
        let mut out = Vec::new();
        let mut child = self.rules_box.first_child();
        while let Some(row) = child {
            child = row.next_sibling();
            if let Some(combo) = row.downcast_ref::<adw::ComboRow>() {
                out.push(crate::config::AppRule {
                    pattern: combo.title().trim().to_string(),
                    mode: mode_at(combo.selected()),
                });
            }
        }
        out
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
        let total = rules.len();
        for (idx, rule) in rules.iter().enumerate() {
            self.rule_row(idx, total, &rule.pattern, rule.mode);
        }
        self.rules_box.set_visible(total > 0);
        self.rules_status.set_visible(total == 0);
        *self.rules_loading.borrow_mut() = false;
        self.update_rule_preview();
    }

    /// One rule as a native ComboRow: the app id is the title, the forced
    /// mode the subtitle; the suffix holds the reorder pair and the trash.
    /// Row order is match priority — the engine takes the first rule that
    /// hits.
    fn rule_row(self: &Rc<Self>, idx: usize, total: usize, pattern: &str, mode: ModeConfig) {
        let row = adw::ComboRow::builder()
            .title(pattern)
            .use_subtitle(true)
            .model(&StringList::new(MODES))
            .selected(mode_index(mode))
            .build();
        // The theme ellipsize is END; a long id would hide its tail, and
        // the title label is not a public child — hence the helper.
        if let Some(title) = row_title(&row) {
            title.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        }
        row.set_tooltip_text(Some(pattern));
        let num = gtk4::Label::new(Some(&(idx + 1).to_string()));
        num.add_css_class("dim-label");
        num.add_css_class("caption");
        num.set_valign(gtk4::Align::Center);
        row.add_prefix(&num);
        {
            let ui = self.clone();
            row.connect_selected_notify(move |_| {
                if *ui.rules_loading.borrow() {
                    return;
                }
                ui.fire_save();
                ui.update_rule_preview();
            });
        }
        // Reorder as a linked pair; delete as a flat trash that only turns
        // red under the pointer ("error" is a permanent tint otherwise).
        let up = gtk4::Button::from_icon_name("go-up-symbolic");
        up.add_css_class("flat");
        up.set_sensitive(idx > 0);
        up.set_tooltip_text(Some(&t("rows.rule_up.tooltip", "Lên — luật trước thắng")));
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
        let down = gtk4::Button::from_icon_name("go-down-symbolic");
        down.add_css_class("flat");
        down.set_sensitive(idx + 1 < total);
        down.set_tooltip_text(Some(&t("rows.rule_down.tooltip", "Xuống")));
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
        let linked = gtk4::Box::new(Orientation::Horizontal, 0);
        linked.add_css_class("linked");
        linked.append(&up);
        linked.append(&down);
        let del = gtk4::Button::from_icon_name("user-trash-symbolic");
        del.add_css_class("flat");
        del.set_tooltip_text(Some(&t("rows.rule_delete.tooltip", "Xoá luật này")));
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
        {
            let del_enter = del.clone();
            let del_leave = del.clone();
            let hover = gtk4::EventControllerMotion::new();
            hover.connect_enter(move |_, _, _| del_enter.add_css_class("error"));
            hover.connect_leave(move |_| del_leave.remove_css_class("error"));
            del.add_controller(hover);
        }
        row.add_suffix(&linked);
        row.add_suffix(&del);
        self.rules_box.append(&row);
    }

    /// The "thử luật" verdict as the result row's subtitle: which rule hits
    /// the typed name and the mode it forces. Rules only — the engine's
    /// capability fallbacks are not modelled here.
    fn update_rule_preview(&self) {
        let input = self.rule_preview_input.text().trim().to_string();
        let verdict = if input.is_empty() {
            String::new()
        } else {
            let rules = self.rules_from_rows();
            let target = Some(input.as_str());
            match crate::config::app_rule_mode(&rules, target) {
                Some(mode) => {
                    let idx = rules
                        .iter()
                        .position(|r| crate::config::app_rule_matches(&r.pattern, target))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    format!("Khớp luật #{idx} · {}", MODES[mode_index(mode) as usize])
                }
                None => t("rules.no_match", "Không khớp luật nào").to_string(),
            }
        };
        self.rule_preview.set_subtitle(&verdict);
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
fn widgets() -> Rc<Ui> {
    let enabled = switch_row(
        &t("rows.enabled.title", "Bật tiếng Việt"),
        &t(
            "rows.enabled.subtitle",
            "Tắt để gõ thẳng mọi phím (gõ tắt vẫn chạy nếu bật ở tab Gõ tắt)",
        ),
    );
    let method = combo_row(
        &t("rows.method.title", "Kiểu gõ"),
        &t("rows.method.subtitle", "Telex, VNI, VIQR, Microsoft layout"),
        METHODS,
    );
    let mode = combo_row(
        &t("rows.mode.title", "Chế độ gõ"),
        &t(
            "rows.mode.subtitle",
            "Gạch chân (mặc định), Không gạch chân, Surrounding, 3 mode uinput, 2 mode ForwardKeyEvent, OFF, Gõ mặc định",
        ),
        MODES,
    );
    let charset = combo_row(
        &t("rows.charset.title", "Bảng mã"),
        &t("rows.charset.subtitle", "Unicode và 10 bảng mã cũ"),
        CHARSETS,
    );
    let capture_tooltip = t(
        "misc.capture_tooltip",
        "Gán lại: nhấn tổ hợp phím mới. Esc hủy, Backspace xóa.",
    );
    let hotkey_label = gtk4::Label::new(Some(&t("misc.unassigned", "Chưa gán")));
    hotkey_label.add_css_class("dim-label");
    let hotkey_edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    hotkey_edit.add_css_class("flat");
    hotkey_edit.set_tooltip_text(Some(&capture_tooltip));
    let hotkey_value = Rc::new(RefCell::new(String::new()));
    let enabled_hotkey_label = gtk4::Label::new(Some(&t("misc.unassigned", "Chưa gán")));
    enabled_hotkey_label.add_css_class("dim-label");
    let enabled_hotkey_edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    enabled_hotkey_edit.add_css_class("flat");
    enabled_hotkey_edit.set_tooltip_text(Some(&capture_tooltip));
    let enabled_hotkey_value = Rc::new(RefCell::new(String::new()));
    let modern = switch_row(
        &t("rows.modern.title", "Kiểu dấu mới"),
        &t(
            "rows.modern.subtitle",
            "hòa (chuẩn mới) thay vì hoà (chuẩn cũ)",
        ),
    );
    let free = switch_row(
        &t("rows.free.title", "Dấu tự do"),
        &t(
            "rows.free.subtitle",
            "Dấu đặt anywhere trong từ: hoangf → hoàng",
        ),
    );
    let auto_correct = switch_row(
        &t("rows.auto_correct.title", "Từ sai trả phím gốc"),
        &t(
            "rows.auto_correct.subtitle",
            "Âm tiết không hợp lệ trả về đúng phím đã bấm",
        ),
    );
    let auto_restore = switch_row(
        &t("rows.auto_restore.title", "Khôi phục từ nước ngoài"),
        &t(
            "rows.auto_restore.subtitle",
            "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
        ),
    );
    let abbrev = switch_row(
        &t("rows.abbrev.title", "Viết tắt"),
        &t(
            "rows.abbrev.subtitle",
            "Giữ đ trong từ viết tắt: ddc → đc, ddk → đk, ddt → đt",
        ),
    );
    let bypass_url = switch_row(
        &t("rows.bypass_url.title", "Không dấu sau URL"),
        &t(
            "rows.bypass_url.subtitle",
            "Từ gõ ngay sau / : @ . _ ~ = # & % ? giữ nguyên",
        ),
    );
    let w2u = combo_row(
        &t("rows.w2u.title", "W thành ư"),
        &t("rows.w2u.subtitle", "Tắt / trừ đầu từ / mọi vị trí"),
        ON_OFF_MODES,
    );
    let bracket = combo_row(
        &t("rows.bracket.title", "[ ] thành ơ ư"),
        &t("rows.bracket.subtitle", "Tắt / trừ đầu từ / mọi vị trí"),
        ON_OFF_MODES,
    );
    let em_dash = switch_row(
        &t("rows.em_dash.title", "Gạch ngang dài"),
        &t(
            "rows.em_dash.subtitle",
            "Hai dấu trừ liên tiếp thành gạch dài (-- thành —)",
        ),
    );
    let auto_capitalize = switch_row(
        &t("rows.auto_capitalize.title", "Tự viết hoa"),
        &t("rows.auto_capitalize.subtitle", "Sau . ! ? và Enter"),
    );
    let double_space = switch_row(
        &t("rows.double_space.title", "2 dấu cách = chấm"),
        &t(
            "rows.double_space.subtitle",
            "Thử nghiệm: hai dấu cách thành dấu chấm",
        ),
    );
    let macro_enabled = switch_row(
        &t("rows.macro_enabled.title", "Gõ tắt"),
        &t(
            "rows.macro_enabled.subtitle",
            "Khóa mở rộng thành cụm khi hết từ",
        ),
    );
    let capitalize_macro = switch_row(
        &t("rows.capitalize_macro.title", "Viết hoa gõ tắt"),
        &t(
            "rows.capitalize_macro.subtitle",
            "Khóa viết hoa → nội dung viết hoa theo",
        ),
    );
    let macro_in_off_mode = switch_row(
        &t("rows.macro_in_off_mode.title", "Gõ tắt khi OFF"),
        &t(
            "rows.macro_in_off_mode.subtitle",
            "Vẫn mở rộng khi tiếng Việt đang tắt",
        ),
    );
    // Suffix entries: sized to the format they hold and centered, so the
    // row reads as one balanced control instead of a wide box on the edge.
    let time_format = Entry::builder()
        .width_chars(8)
        .valign(gtk4::Align::Center)
        .build();
    let date_format = Entry::builder()
        .width_chars(12)
        .valign(gtk4::Align::Center)
        .build();
    let macros_list = gtk4::ListBox::new();
    macros_list.add_css_class("boxed-list");
    macros_list.set_selection_mode(gtk4::SelectionMode::None);
    let macros_rows = Rc::new(RefCell::new(Vec::new()));
    let dictionary_enabled = switch_row(
        &t("rows.dictionary_enabled.title", "Dùng từ điển cá nhân"),
        &t(
            "rows.dictionary_enabled.subtitle",
            "~/.config/ibus-mangrs/dictionary.txt",
        ),
    );
    // The subtitle is a file path: monospace caption reads as a path, not
    // prose. The label is internal, so the classes go on via the helper;
    // the old "(?)" marker's text became this row's tooltip.
    if let Some(subtitle) = row_subtitle(&dictionary_enabled) {
        subtitle.add_css_class("monospace");
        subtitle.add_css_class("caption");
        subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    }
    dictionary_enabled.set_tooltip_text(Some(&t(
        "tips.dictionary_enabled",
        "Bật từ điển cá nhân: các từ trong danh sách không bị trả về chuỗi phím gốc.",
    )));
    let dictionary_text = text_view("");
    // Fixed height so the list cannot stretch (or collapse) the page; the
    // page's own scroll takes over beyond it.
    dictionary_text.set_height_request(160);
    // App Rules editor widgets. The rows live in `rules_box`, rebuilt by
    // `fill_rules`; the rest are the fixed helpers around them.
    let rules_box = gtk4::ListBox::new();
    rules_box.add_css_class("boxed-list");
    rules_box.set_selection_mode(gtk4::SelectionMode::None);
    let rules_status = adw::StatusPage::builder()
        .title(t("rules.empty", "Chưa có luật nào"))
        .build();
    // The input is a native EntryRow; the verdict is the subtitle of a
    // read-only result row underneath it. EntryRow has no placeholder in
    // this libadwaita, so the examples live in the tooltip.
    let rule_preview_input = adw::EntryRow::builder()
        .title(t("rows.rule_name.title", "Tên app"))
        .build();
    rule_preview_input.set_tooltip_text(Some(&t(
        "rows.rule_preview_input.tooltip",
        "Nhập tên app rồi Enter: cho biết luật nào sẽ khớp. Ví dụ: Firefox / wayland:Firefox / xim",
    )));
    let rule_preview = action_row(&t("rules.result_title", "Kết quả"), "");
    let rule_focus = gtk4::Label::new(Some("—"));
    rule_focus.set_xalign(0.0);
    rule_focus.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    rule_focus.set_selectable(true);
    rule_focus.add_css_class("monospace");
    rule_focus.add_css_class("dim-label");
    rule_focus.set_tooltip_text(Some(&t(
        "rows.rule_focus.chip_tooltip",
        "Chuyển sang app đích rồi quay lại cửa sổ này — tên app sẽ hiện ở đây (cần extension MăngRS Focus)",
    )));
    let rule_focus_value = Rc::new(RefCell::new(String::new()));
    let rules_loading = Rc::new(RefCell::new(false));
    let save_cb: SharedSave = Rc::new(RefCell::new(None));
    let hide_underline = switch_row(
        &t("rows.hide_underline.title", "Ẩn gạch chân"),
        &t(
            "rows.hide_underline.subtitle",
            "Chữ đang gõ không có gạch chân (tùy app)",
        ),
    );
    let debug_log = switch_row(
        &t("rows.debug_log.title", "Ghi log gỡ lỗi"),
        &t(
            "rows.debug_log.subtitle",
            "Ghi vào ~/.cache/ibus-mangrs/debug.log",
        ),
    );
    let x11_delete = switch_row(
        &t("rows.x11_delete.title", "Xoá đuôi từ ở app X11"),
        &t(
            "rows.x11_delete.subtitle",
            "App X11 trực tiếp xoá đuôi từ dù không gửi báo cáo — như ibus-bamboo. Có thể xoá nhầm vài ký tự nếu con trỏ bị đổi chỗ.",
        ),
    );
    let preedit_xim = switch_row(
        &t("rows.preedit_xim.title", "Preedit cho app XIM"),
        &t(
            "rows.preedit_xim.subtitle",
            "Giữ gạch chân cho app XIM (Java/JetBrains): chữ đang gõ hiện trong cửa sổ riêng rồi commit khi hết từ.",
        ),
    );
    let commit_interval = spin_row(
        &t("rows.commit_interval.title", "Nhịp gõ chữ"),
        &t(
            "rows.commit_interval.subtitle",
            "Khoảng cách giữa hai lần gửi chữ (ms). Mặc định: 20",
        ),
        1.0,
        500.0,
        1.0,
    );
    let backspace_interval = spin_row(
        &t("rows.backspace_interval.title", "Nhịp xoá lùi"),
        &t(
            "rows.backspace_interval.subtitle",
            "Khoảng cách giữa hai lần Backspace (ms). Mặc định: 5",
        ),
        1.0,
        100.0,
        1.0,
    );
    let post_delay = spin_row(
        &t("rows.post_delay.title", "Độ trễ sau từ"),
        &t(
            "rows.post_delay.subtitle",
            "Chờ sau khi viết lại từ trước khi gõ tiếp (ms). Mặc định: 10",
        ),
        0.0,
        500.0,
        1.0,
    );
    let unicode_channel = combo_row(
        &t("rows.unicode_channel.title", "Kênh gửi từ tiếng Việt"),
        &t(
            "rows.unicode_channel.subtitle",
            "CommitText an toàn, hoặc Clipboard (Ctrl+V, tạm đè clipboard)",
        ),
        CHANNELS,
    );

    // Panel-menu editor: the design vector is the source of truth; the
    // boxed list is its view (rebuilt on every edit).
    let menu = Rc::new(RefCell::new(Vec::new()));
    let menu_box = gtk4::ListBox::new();
    menu_box.add_css_class("boxed-list");
    menu_box.set_selection_mode(gtk4::SelectionMode::None);

    // The "(?)" markers: hover or click for the longer explanation, text
    // from the content file. Applied here so every page gets them for
    // free; page-added suffixes (hotkey buttons, format entries) land to
    // the right of them.
    macro_rules! info {
        ($row:expr, $key:expr, $default:expr) => {
            attach_info(&$row, &t($key, $default));
        };
    }
    info!(
        enabled,
        "tips.enabled", "Bật/tắt cả bộ gõ. Khi tắt, mọi phím đi thẳng vào app như chưa cài MăngRS."
    );
    info!(
        method,
        "tips.method",
        "Kiểu gõ quyết định cách chữ dấu được tạo từ chuỗi phím: Telex (ss → á), VNI (a1 → á) hoặc VIQR."
    );
    info!(
        mode,
        "tips.mode",
        "Cách hiển thị chữ đang gõ: preedit (gạch chân) dùng được ở mọi app, các mode uinput gửi phím thật nhưng có thể lệch app."
    );
    info!(
        charset,
        "tips.charset",
        "Bảng mã chữ: Unicode cho mọi dùng đợp thường, các bảng VNI/TCVN3 chỉ cần khi đọc file văn bản cũ."
    );
    info!(
        modern,
        "tips.modern", "Chuẩn đặt dấu mới của Bộ GD-ĐT: hòa thay vì hoà, thuế thay vì thuể."
    );
    info!(
        free,
        "tips.free", "Cho phép dấu đứng tự do trong từ thay vì luôn ở nguyên âm gần nhất."
    );
    info!(
        auto_correct,
        "tips.auto_correct",
        "Âm tiết không có trong từ điển tiếng Việt sẽ trả về đúng chuỗi phím đã bấm."
    );
    info!(
        auto_restore,
        "tips.auto_restore",
        "Từ nước ngoài (english, hello) gõ xong tự trả về chuỗi phím gốc thay vì giữ nguyên."
    );
    info!(
        abbrev,
        "tips.abbrev", "Giữ đ nguyên trong từ viết tắt: ddc, ddk, ddt không bị đổi thành đ."
    );
    info!(
        bypass_url,
        "tips.bypass_url",
        "Từ ngay sau URL/email/path không được thêm dấu, tránh làm hỏng liên kết khi soạn chat."
    );
    info!(
        w2u,
        "tips.w2u", "Gõ w nhận ư: w = ư, wa = ưa... tiện cho bàn phím không có sẵn ư."
    );
    info!(
        bracket,
        "tips.bracket", "Gõ [ và ] nhận ơ và ư theo vị trí trong từ."
    );
    info!(
        em_dash,
        "tips.em_dash", "Hai dấu gạch ngang liên tiếp (--) thành gạch ngang dài (—)."
    );
    info!(
        auto_capitalize,
        "tips.auto_capitalize", "Tự viết hoa chữ cái đầu câu sau dấu chấm, !, ? hoặc Enter."
    );
    info!(
        double_space,
        "tips.double_space",
        "Nhấn hai dấu cách liên tiếp thành dấu chấm + khoảng cách (thử nghiệm)."
    );
    info!(
        macro_enabled,
        "tips.macro_enabled",
        "Bật bảng gõ tắt: chuỗi khóa được mở rộng thành nội dung khi gõ xong từ."
    );
    info!(
        capitalize_macro,
        "tips.capitalize_macro", "Khóa viết hoa (Gs) thì nội dung cũng viết hoa theo."
    );
    info!(
        macro_in_off_mode,
        "tips.macro_in_off_mode", "Gõ tắt vẫn hoạt động khi tiếng Việt đang tắt."
    );
    info!(
        hide_underline,
        "tips.hide_underline",
        "Ẩn gạch chân preedit cho những app vẽ gạch chân lỗi (một số game, terminal)."
    );
    info!(
        debug_log,
        "tips.debug_log",
        "Ghi lại quyết định của engine vào ~/.cache/ibus-mangrs/debug.log để báo lỗi."
    );
    info!(
        x11_delete,
        "tips.x11_delete",
        "Cách xoá từ ở app X11 trực tiếp: xoá đuôi từ như ibus-bamboo, hoặc chỉ xoá theo báo cáo của app."
    );
    info!(
        preedit_xim,
        "tips.preedit_xim",
        "Dùng cửa sổ preedit riêng cho app XIM (Java/JetBrains) vốn không hỗ trợ preedit của IBus."
    );
    info!(
        commit_interval,
        "tips.commit_interval",
        "Nhịp gửi từng chữ ở mode uinput — giảm nếu app bị mất chữ, tăng nếu bị đơ."
    );
    info!(
        backspace_interval,
        "tips.backspace_interval",
        "Nhịp gửi Backspace ở mode uinput — giảm nếu xoá chậm, tăng nếu app xử lý không kịp."
    );
    info!(
        post_delay,
        "tips.post_delay",
        "Chờ sau khi viết lại cả từ trước khi gõ chữ tiếp, dành cho app xử lý phím chậm."
    );
    info!(
        unicode_channel,
        "tips.unicode_channel",
        "Cách gửi từ có ký tự đặc biệt: CommitText là an toàn; Clipboard dùng Ctrl+V, tạm đè clipboard."
    );

    Rc::new(Ui {
        enabled,
        method,
        mode,
        charset,
        hotkey_label,
        hotkey_edit,
        hotkey_value,
        enabled_hotkey_label,
        enabled_hotkey_edit,
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
        rules_status,
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
        menu,
        menu_box,
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

    /// The hotkey recorder must live on the pencil button: after a click the
    /// button keeps key focus, so the key controllers there receive the
    /// pressed keys. Regression for the row-anchored recorder whose
    /// focus-leave handler cancelled the capture before any key arrived —
    /// with the controllers on the row and the button disabled on click,
    /// assigning a hotkey did nothing. Needs a display; skips where GTK
    /// cannot start.
    #[test]
    fn hotkey_recorder_lives_on_the_edit_button() {
        if gtk4::init().is_err() {
            eprintln!("gui: SKIP (no display)");
            return;
        }
        let kinds = |w: &gtk4::Widget| -> Vec<String> {
            let list = w.observe_controllers();
            (0..list.n_items())
                .filter_map(|i| list.item(i))
                .map(|o| o.type_().name().to_string())
                .collect()
        };

        let label = gtk4::Label::new(None);
        let edit = gtk4::Button::from_icon_name("document-edit-symbolic");
        let value = Rc::new(RefCell::new(String::new()));
        hotkey_wire(&edit, &label, &value, || {});

        let button = kinds(edit.upcast_ref());
        // 2 ours + GTK's internal one for the button's own key handling.
        assert!(
            button
                .iter()
                .filter(|k| *k == "GtkEventControllerKey")
                .count()
                >= 2,
            "press/release key controllers must sit on the edit button: {button:?}"
        );
        assert!(button.iter().any(|k| *k == "GtkEventControllerFocus"));

        // The capture starts on click and the button stays sensitive:
        // disabling the focus widget makes GTK move key focus off it, and
        // the focus-leave handler cancels the capture immediately.
        edit.emit_clicked();
        assert!(edit.is_sensitive(), "the recorder disabled the edit button");
        assert_eq!(label.text(), "Nhấn tổ hợp phím… (Esc hủy)");
    }
}
