//! GTK4 settings window, opened from the property menu ("Phím tắt") or with
//! `-gui`.
//!
//! Every control writes straight to `config.toml` (and `macros.toml` /
//! `dictionary.txt`); the engine watches the config file mtime, so changes
//! apply within a second without restarting ibus-daemon.
//!
//! Layout: header bar with [search] [pill tabs] [close], five pages
//! (Kiểu gõ / Dấu & Sửa lỗi / Gõ tắt / Dữ liệu / Giới thiệu), grouped cards.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::{BracketConfig, CharsetConfig, Config, MethodConfig, ModeConfig, W2uConfig};
use crate::macros::{Dictionary, Macros};
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, CheckButton, DropDown, Entry, Orientation, Revealer,
    StringList, Switch, TextView,
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

struct Ui {
    enabled: Switch,
    method: DropDown,
    mode: DropDown,
    charset: DropDown,
    hotkey: Entry,
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
    /// One checkbox per registered quick toggle (`ibus_prop::QUICK_TOGGLES`);
    /// the checked ones are the panel popup's contents (`config.quick_menu`).
    quick_menu_checks: Vec<(&'static str, CheckButton)>,
    /// One checkbox per display mode (`ibus_prop::MODES`); the checked ones
    /// are the panel popup's "Chế độ gõ" radio group (`config.menu_modes`).
    menu_mode_checks: Vec<(&'static str, CheckButton)>,
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

impl Ui {
    /// Push a config (plus the two text lists) into every widget. This is the
    /// only config → widget mapping: the initial build and the import both go
    /// through it, so an import cannot leave a stale widget that the next
    /// save would write back over the imported file.
    fn refresh(&self, config: &Config, macros: &Macros, dictionary: &Dictionary) {
        self.enabled.set_active(config.enabled);
        self.method.set_selected(method_index(config.method));
        self.mode.set_selected(mode_index(config.mode));
        self.charset.set_selected(charset_index(config.charset));
        self.hotkey.set_text(&config.cycle_mode_hotkey);
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
        for (key, check) in &self.quick_menu_checks {
            check.set_active(config.quick_menu.iter().any(|k| k == key));
        }
        for (key, check) in &self.menu_mode_checks {
            check.set_active(config.menu_modes.iter().any(|k| k == key));
        }
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
        debug_assert_eq!(&self.read(), config, "refresh missed a widget");
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
        config.cycle_mode_hotkey = self.hotkey.text().trim().to_string();
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
        // Registry order is the menu order; unchecking removes the toggle.
        config.quick_menu = self
            .quick_menu_checks
            .iter()
            .filter(|(_, check)| check.is_active())
            .map(|(key, _)| (*key).to_string())
            .collect();
        // Same for the panel's display-mode radio group.
        config.menu_modes = self
            .menu_mode_checks
            .iter()
            .filter(|(_, check)| check.is_active())
            .map(|(key, _)| (*key).to_string())
            .collect();
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
fn widgets() -> Rc<Ui> {
    let enabled = Switch::new();
    let method = drop_down(METHODS, 0);
    let mode = drop_down(MODES, 0);
    let charset = drop_down(CHARSETS, 0);
    let hotkey = Entry::builder()
        .placeholder_text("vd. Control+Alt+v")
        .width_chars(18)
        .build();
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
    let quick_menu_checks = crate::ibus_prop::QUICK_TOGGLES
        .iter()
        .map(|t| (t.key, CheckButton::new()))
        .collect();
    let menu_mode_checks = crate::ibus_prop::MODES
        .iter()
        .map(|(key, _)| (*key, CheckButton::new()))
        .collect();

    Rc::new(Ui {
        enabled,
        method,
        mode,
        charset,
        hotkey,
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
        quick_menu_checks,
        menu_mode_checks,
    })
}

fn build(app: &Application, initial_page: Option<&str>) {
    ROWS.with(|r| r.borrow_mut().clear());
    COLLAPSIBLES.with(|c| c.borrow_mut().clear());

    let config = Config::load();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let ui = widgets();
    ui.refresh(&config, &macros, &dictionary);

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
            "Khi tắt, phím gõ thẳng; gõ tắt vẫn chạy nếu bật ở tab Gõ tắt",
            &ui.enabled,
        ),
        pref_row(
            "Phím tắt đổi chế độ",
            "Ví dụ Control+Alt+v — để trống là chưa gán. Mặc định: chưa gán",
            &ui.hotkey,
        ),
    ]));
    hint(
        &typing,
        "Thứ tự phím tắt đổi chế độ: Không gạch chân → Gạch chân → Surrounding → Smooth → Super Smooth → Select → ForwardKeyEvent → ForwardKeyEvent II → OFF → Gõ mặc định.",
    );
    section(&typing, "Cách gõ", "Kiểu gõ, bảng mã và chế độ gõ");
    typing.append(&card(vec![
        pref_row("Kiểu gõ chính", "Telex, VNI, VIQR, Microsoft layout", &ui.method),
        pref_row("Bảng mã", "Unicode và 10 bảng mã cũ", &ui.charset),
        pref_row(
            "Chế độ gõ",
            "10 chế độ: Gạch chân (mặc định), Không gạch chân, Surrounding, Smooth, Super Smooth, Select, ForwardKeyEvent, ForwardKeyEvent II, OFF, Gõ mặc định",
            &ui.mode,
        ),
    ]));
    section(&typing, "Hiển thị", "Hình thức của chữ đang gõ");
    typing.append(&card(vec![pref_row(
        "Ẩn gạch chân",
        "Preedit không có gạch chân (tùy app có nhận không)",
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
            "Sửa từ sai",
            "Âm tiết sai được chuyển thành âm hợp lệ gần nhất",
            &ui.auto_correct,
        ),
        pref_row(
            "Khôi phục từ nước ngoài",
            "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
            &ui.auto_restore,
        ),
        pref_row(
            "Không gõ URL",
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
            "Bật gõ tắt",
            "Khóa gõ tắt mở rộng ở cuối từ",
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
        "Luật theo ứng dụng",
        "Mỗi dòng: tên-ứng-dụng=chế độ (nonpreedit, preedit, surrounding, smooth, supersmooth, select, off, defaulttyping). Tên đặc biệt: gnome-shell = app Wayland, none = app không báo tên",
    );
    data_box.append(&card_widget(&add_row));
    data_box.append(&card_widget(&scrolled(&ui.rules_text, 160)));
    hint(
        &data_box,
        "Chỉ app dùng GTK IM module mới báo tên riêng cho engine; app Wayland hiện là gnome-shell, app X11/khác là none. Cài extension MăngRS Focus (data/gnome-shell-extension) để engine nhận tên app Wayland-native: Chrome, Firefox, … Danh sách là các app engine đã từng thấy, không phải mọi app đang chạy.",
    );

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
    // everyday tabs stay short: the panel-menu customization lists, the
    // uinput pacing, the app-specific X11/XIM workarounds, the log switch.
    let advanced = gtk4::Box::new(Orientation::Vertical, 6);
    collapsible_section(
        &advanced,
        "Menu nhanh trên panel",
        "Tính năng hiện trong menu popup của biểu tượng MăngRS trên thanh trên",
        ui.quick_menu_checks
            .iter()
            .filter_map(|(key, check)| {
                let t = crate::ibus_prop::quick_toggle(key)?;
                Some(pref_row(t.label, t.tooltip, check))
            })
            .collect(),
    );
    collapsible_section(
        &advanced,
        "Chế độ gõ trên menu panel",
        "Chế độ gõ hiện trong submenu \"Chế độ gõ\" của menu popup — bỏ tích để ẩn khỏi menu",
        ui.menu_mode_checks
            .iter()
            .filter_map(|(key, check)| {
                let (_, label) = crate::ibus_prop::MODES.iter().find(|(k, _)| k == key)?;
                Some(pref_row(label, key, check))
            })
            .collect(),
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
            "Xoá không cần báo cáo surrounding",
            "App X11 trực tiếp (Sublime Text...) xoá đuôi từ dù không gửi báo cáo — như ibus-bamboo. Có thể xoá nhầm vài ký tự nếu con trỏ bị đổi chỗ.",
            &ui.x11_delete,
        ),
        pref_row(
            "Preedit cho app XIM",
            "Giữ gạch chân cho app dùng XIM: chữ đang gõ hiện trong cửa sổ riêng của ibus rồi commit khi hết từ. Tắt thì JetBrains chỉ gõ được ở các chế độ ForwardKeyEvent.",
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
        ("tone", "Dấu & tự động", "font-x-generic-symbolic"),
        ("macros", "Gõ tắt", "document-edit-symbolic"),
        ("data", "Dữ liệu", "view-list-symbolic"),
        ("advanced", "Tinh chỉnh", "applications-system-symbolic"),
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
         .nav-pill image { -gtk-icon-size: 18px; } \
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
        macro_rules! on_change {
            ($w:expr) => {{
                let s = save.clone();
                $w.connect_active_notify(move |_| s());
            }};
        }
        // The quick-menu checkboxes save like any switch, so a checkmark is
        // in config.toml (and on the panel) before the window loses focus.
        for (_, check) in ui
            .quick_menu_checks
            .iter()
            .chain(ui.menu_mode_checks.iter())
        {
            on_change!(check);
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
            &ui.hotkey,
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
            // Registry order: the checkboxes are presented in it, so a save
            // normalizes the list to it (a hand-edited order survives until
            // the next save).
            quick_menu: vec!["macro".into(), "bypass_url".into(), "em_dash".into()],
            menu_modes: vec!["preedit".into(), "smooth".into(), "off".into()],
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
