//! GTK4 settings window, opened from the property menu ("Phím tắt") or with
//! `-gui`.
//!
//! Every control writes straight to `config.toml` (and `macros.toml` /
//! `dictionary.txt`); the engine watches the config file mtime, so changes
//! apply within a second without restarting ibus-daemon.

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, DropDown, Entry, Orientation, StringList, Switch,
    TextView,
};
use serde::{Deserialize, Serialize};

use crate::config::{
    BracketConfig, CharsetConfig, Config, MethodConfig, ModeConfig, W2uConfig,
};
use crate::macros::{Dictionary, Macros};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // The engine spawns us with `--page=<stack name>` to open a specific
    // page (menu "Mở bảng gõ tắt…"); GTK parses argv itself and rejects
    // these arguments, so read them here and hand GTK an empty argv.
    let page = std::env::args().find_map(|a| a.strip_prefix("--page=").map(str::to_string));
    let app = Application::builder().application_id("org.freedesktop.IBus.mangrs").build();
    app.connect_activate(move |app| build(app, page.as_deref()));
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
    debug_log: Switch,
    commit_interval: Entry,
    backspace_interval: Entry,
    post_delay: Entry,
    unicode_channel: DropDown,
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
        ModeConfig::Off => 6,
        ModeConfig::DefaultTyping => 7,
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
            6 => ModeConfig::Off,
            7 => ModeConfig::DefaultTyping,
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
        config.modern_tone_style = self.modern.is_active();
        config.free_tone_marking = self.free.is_active();
        config.auto_correct = self.auto_correct.is_active();
        config.auto_restore = self.auto_restore.is_active();
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
        config.backspace_interval_ms =
            self.backspace_interval.text().trim().parse().unwrap_or(5);
        config.post_delay_ms = self.post_delay.text().trim().parse().unwrap_or(10);
        config.unicode_channel = match self.unicode_channel.selected() {
            1 => crate::config::UnicodeChannel::Clipboard,
            _ => crate::config::UnicodeChannel::Commit,
        };
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
        let mut config = self.read();
        config.app_rules = self
            .read_rules()
            .into_iter()
            .map(|(pattern, mode)| crate::config::AppRule { pattern, mode })
            .collect();
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
        .text(&view.buffer().start_iter(), &view.buffer().end_iter(), false)
        .to_string()
}

fn set_text(view: &TextView, text: &str) {
    view.buffer().set_text(text);
}

fn build(app: &Application, initial_page: Option<&str>) {
    let config = Config::load();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let enabled = Switch::builder().active(config.enabled).build();
    let method = drop_down(METHODS, method_index(config.method));
    let mode = drop_down(MODES, mode_index(config.mode));
    let charset = drop_down(CHARSETS, charset_index(config.charset));
    let hotkey = Entry::builder()
        .text(&config.cycle_mode_hotkey)
        .placeholder_text("vd. Control+Alt+v (để trống = chưa gán)")
        .hexpand(true)
        .build();
    let modern = Switch::builder().active(config.modern_tone_style).build();
    let free = Switch::builder().active(config.free_tone_marking).build();
    let auto_correct = Switch::builder().active(config.auto_correct).build();
    let auto_restore = Switch::builder().active(config.auto_restore).build();
    let bypass_url = Switch::builder().active(config.bypass_url).build();
    let w2u = drop_down(
        ON_OFF_MODES,
        on_off_index(&config.w2u, W2uConfig::Disabled, W2uConfig::NonStart),
    );
    let bracket = drop_down(
        ON_OFF_MODES,
        on_off_index(&config.bracket, BracketConfig::Disabled, BracketConfig::NonStart),
    );
    let em_dash = Switch::builder().active(config.em_dash).build();
    let auto_capitalize = Switch::builder().active(config.auto_capitalize).build();
    let double_space = Switch::builder().active(config.double_space_period).build();
    let macro_enabled = Switch::builder().active(config.macro_enabled).build();
    let capitalize_macro = Switch::builder().active(config.capitalize_macro).build();
    let macro_in_off_mode = Switch::builder().active(config.macro_in_off_mode).build();
    let time_format = Entry::builder().text(&config.time_format).hexpand(true).build();
    let date_format = Entry::builder().text(&config.date_format).hexpand(true).build();
    let macro_text = text_view(
        &macros
            .entries()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let dictionary_enabled = Switch::builder().active(config.dictionary_enabled).build();
    let dictionary_text =
        text_view(&dictionary.words().collect::<Vec<_>>().join("\n"));
    let rules_text = text_view(
        &config
            .app_rules
            .iter()
            .map(|r| format!("{}={}", r.pattern, r.mode.key()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let debug_log = Switch::builder().active(config.debug_log).build();
    let commit_interval =
        Entry::builder().text(config.commit_interval_ms.to_string()).width_chars(6).build();
    let backspace_interval =
        Entry::builder().text(config.backspace_interval_ms.to_string()).width_chars(6).build();
    let post_delay = Entry::builder().text(config.post_delay_ms.to_string()).width_chars(6).build();
    let unicode_channel = drop_down(
        CHANNELS,
        match config.unicode_channel {
            crate::config::UnicodeChannel::Clipboard => 1,
            crate::config::UnicodeChannel::Commit => 0,
        },
    );

    let ui = Rc::new(Ui {
        enabled,
        method,
        mode,
        charset,
        hotkey,
        modern,
        free,
        auto_correct,
        auto_restore,
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
        debug_log,
        commit_interval,
        backspace_interval,
        post_delay,
        unicode_channel,
    });

    // --- tabs ---
    let general = gtk4::Box::new(Orientation::Vertical, 8);
    margin(&general);
    general.append(&row("Bật tiếng Việt", &ui.enabled));
    general.append(&row("Kiểu gõ", &ui.method));
    general.append(&row("Hiển thị", &ui.mode));
    general.append(&row("Bảng mã", &ui.charset));
    general.append(&row("Phím tắt đổi chế độ", &ui.hotkey));
    general.append(&row("Ghi log gỡ lỗi", &ui.debug_log));
    hint(
        &general,
        "Ghi log gỡ lỗi: ghi quyết định của bộ gõ vào ~/.cache/ibus-mangrs/debug.log để báo lỗi.",
    );
    hint(
        &general,
        "Đổi chế độ: Không gạch chân → Gạch chân → Surrounding → Smooth → Super Smooth → Select → OFF → Gõ mặc định.",
    );
    hint(
        &general,
        "Smooth/Super Smooth/Select cần quyền ghi /dev/uinput; ở app relay (mutter) hoặc terminal không hỗ trợ sẽ tự fallback về Gạch chân.",
    );

    let typing = gtk4::Box::new(Orientation::Vertical, 8);
    margin(&typing);
    typing.append(&row("Kiểu đặt dấu mới (hòa)", &ui.modern));
    typing.append(&row("Đặt dấu tự do (hoangf → hoàng)", &ui.free));
    typing.append(&row("Tự sửa lỗi chính tả", &ui.auto_correct));
    typing.append(&row("Tự khôi phục từ tiếng Anh", &ui.auto_restore));
    typing.append(&row("Không bỏ dấu sau : / @ . _ (URL, email, đường dẫn)", &ui.bypass_url));
    typing.append(&row("W thành ư", &ui.w2u));
    typing.append(&row("Dấu ngoặc thành ơ/ư", &ui.bracket));
    typing.append(&row("Gõ -- thành —", &ui.em_dash));
    typing.append(&row("Tự viết hoa sau . ! ?", &ui.auto_capitalize));
    typing.append(&row("Hai dấu cách thành dấu chấm", &ui.double_space));
    let pacing = gtk4::Box::new(Orientation::Horizontal, 8);
    pacing.append(&gtk4::Label::new(Some("Khoảng cách bơm phím (ms): chữ/commit")));
    pacing.append(&ui.commit_interval);
    pacing.append(&gtk4::Label::new(Some("backspace")));
    pacing.append(&ui.backspace_interval);
    pacing.append(&gtk4::Label::new(Some("sau từ")));
    pacing.append(&ui.post_delay);
    typing.append(&pacing);
    typing.append(&row("Kênh gửi từ tiếng Việt (uinput)", &ui.unicode_channel));
    hint(
        &typing,
        "Chỉ dùng cho các chế độ uinput (Smooth, Super Smooth, Select). Mặc định 20/5/10 ms; Clipboard hoạt động cả trong app đọc phím trực tiếp (game) nhưng sẽ tạm đè clipboard.",
    );

    let macro_box = gtk4::Box::new(Orientation::Vertical, 8);
    margin(&macro_box);
    macro_box.append(&row("Bật gõ tắt", &ui.macro_enabled));
    macro_box.append(&row("Viết hoa gõ tắt", &ui.capitalize_macro));
    macro_box.append(&row("Gõ tắt khi đang tắt tiếng Việt", &ui.macro_in_off_mode));
    macro_box.append(&row("Định dạng giờ ($TIME)", &ui.time_format));
    macro_box.append(&row("Định dạng ngày ($DATE)", &ui.date_format));
    macro_box.append(&label("Mỗi dòng một gõ tắt: khóa= Nội dung ($TIME, $DATE)"));
    macro_box.append(&scrolled(&ui.macro_text, 180));

    let dict_box = gtk4::Box::new(Orientation::Vertical, 8);
    margin(&dict_box);
    dict_box.append(&row("Bật từ điển của tôi", &ui.dictionary_enabled));
    dict_box.append(&label("Mỗi dòng một từ. Từ trong từ điển không bị khôi phục về chuỗi phím."));
    dict_box.append(&scrolled(&ui.dictionary_text, 220));

    let rules_box = gtk4::Box::new(Orientation::Vertical, 8);
    margin(&rules_box);
    rules_box.append(&label(
        "Mỗi dòng: tên-ứng-dụng=chế độ. Chế độ: nonpreedit, preedit, surrounding, smooth,\n\
         supersmooth, select, off, defaulttyping — khớp theo tên tiến trình, không phân biệt hoa thường.",
    ));
    // Quick add: pick a client the engine has seen, pick a mode, add.
    let client_combo = gtk4::ComboBoxText::new();
    for name in crate::config::recent_clients() {
        client_combo.append_text(&name);
    }
    client_combo.set_active(Some(0));
    let rule_mode = drop_down(
        &crate::ibus_prop::MODES.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        1, // preedit first in the menu, keep the quick-add default consistent
    );
    let add_btn = gtk4::Button::with_label("Thêm luật");
    let add_row = gtk4::Box::new(Orientation::Horizontal, 8);
    add_row.append(&client_combo);
    add_row.append(&rule_mode);
    add_row.append(&add_btn);
    rules_box.append(&add_row);
    rules_box.append(&scrolled(&ui.rules_text, 220));
    {
        let ui = ui.clone();
        add_btn.connect_clicked(move |_| {
            let Some(name) = client_combo.active_text().map(|s| s.to_string()) else { return };
            let Some((key, _)) = crate::ibus_prop::MODES.get(rule_mode.selected() as usize) else {
                return;
            };
            let mode = *key;
            let mut text = text_of(&ui.rules_text);
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("{name}={mode}\n"));
            set_text(&ui.rules_text, &text);
            if let Err(e) = ui.save_all() {
                eprintln!("cannot save config: {e}");
            }
        });
    }

    let backup_box = gtk4::Box::new(Orientation::Vertical, 8);
    margin(&backup_box);
    let export_btn = gtk4::Button::with_label("Xuất cấu hình ra JSON…");
    let import_btn = gtk4::Button::with_label("Nhập cấu hình từ JSON…");
    let btns = gtk4::Box::new(Orientation::Horizontal, 8);
    btns.append(&export_btn);
    btns.append(&import_btn);
    backup_box.append(&btns);
    backup_box.append(&label(
        "JSON chứa cấu hình, gõ tắt và từ điển — dùng để sao lưu hoặc chuyển máy.",
    ));

    let about = gtk4::Box::new(Orientation::Vertical, 4);
    about.set_halign(gtk4::Align::Center);
    about.set_valign(gtk4::Align::Start);
    about.set_margin_top(24);
    about.set_margin_bottom(24);

    // Logo is embedded at compile time: the GUI has no installed asset dir.
    const LOGO: &[u8] = include_bytes!("../data/logo-no-bg.png");
    let logo = gtk4::gdk::Texture::from_bytes(&gtk4::glib::Bytes::from_static(LOGO)).ok();
    if let Some(texture) = &logo {
        let image = gtk4::Image::from_paintable(Some(texture));
        image.set_pixel_size(192);
        about.append(&image);
    }

    let name = gtk4::Label::new(Some("MăngRS"));
    name.add_css_class("title-2");
    about.append(&name);

    let tagline = label("Bộ gõ tiếng Việt cho IBus — Rust + bamboo-core");
    tagline.add_css_class("dim-label");
    about.append(&tagline);

    let version = label(concat!("Phiên bản ", env!("CARGO_PKG_VERSION")));
    version.add_css_class("dim-label");
    about.append(&version);

    about.append(&spacer(8));
    about.append(&label("Tác giả: fundoku"));
    about.append(&link_button(
        "github.com/fundokukiri/ibus-mangrs",
        "https://github.com/fundokukiri/ibus-mangrs",
    ));
    about.append(&spacer(8));

    let license = label(
        "Giấy phép MIT — mã nguồn mở, miễn phí sử dụng, sao chép và sửa đổi.\n\
         Chi tiết: LICENSE và NOTICE.md trong kho nguồn.",
    );
    about.append(&license);

    let credits = label(
        "Lõi gõ: bamboo-core (MIT), kế thừa thuật toán ibus-bamboo.\n\
         Từ điển chính tả: Free Vietnamese Dictionary Project (dữ liệu GPL).",
    );
    credits.add_css_class("dim-label");
    about.append(&credits);

    // Modern settings layout: icon navigation on the left, one scrolled,
    // width-clamped page per section on the right, crossfading in a stack.
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_vexpand(true);
    stack.set_hexpand(true);
    // Static page table (stack name, title, themed icon); the widget boxes
    // are positionally aligned with it.
    const PAGES: [(&str, &str, &str); 7] = [
        ("general", "Chung", "preferences-system"),
        ("typing", "Kiểu gõ", "input-keyboard"),
        ("macros", "Gõ tắt", "text-editor"),
        ("dictionary", "Từ điển", "accessories-dictionary"),
        ("apps", "Ứng dụng", "view-grid"),
        ("backup", "Dự phòng", "document-save"),
        ("about", "Giới thiệu", "help-about"),
    ];
    let page_boxes: [&gtk4::Box; 7] =
        [&general, &typing, &macro_box, &dict_box, &rules_box, &backup_box, &about];
    for ((name, title, _icon), content) in PAGES.iter().zip(page_boxes) {
        // Centered content column, the settings-app look on any GTK theme.
        let clamp = gtk4::Box::new(Orientation::Vertical, 0);
        clamp.set_halign(gtk4::Align::Center);
        clamp.set_valign(gtk4::Align::Start);
        clamp.set_width_request(520);
        clamp.set_margin_top(12);
        clamp.set_margin_bottom(18);
        clamp.append(content);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_child(Some(&clamp));
        stack.add_titled(&scroll, Some(name), title);
    }

    let nav = gtk4::ListBox::new();
    nav.add_css_class("navigation-sidebar");
    nav.set_vexpand(true);
    let mut first_row: Option<gtk4::ListBoxRow> = None;
    for (_, title, icon) in PAGES {
        let row = gtk4::ListBoxRow::new();
        let h = gtk4::Box::new(Orientation::Horizontal, 10);
        let img = gtk4::Image::from_icon_name(icon);
        img.set_pixel_size(16);
        h.append(&img);
        h.append(&gtk4::Label::new(Some(title)));
        row.set_child(Some(&h));
        nav.append(&row);
        if first_row.is_none() {
            first_row = Some(row);
        }
    }
    {
        let stack = stack.clone();
        nav.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let idx = row.index() as usize;
            if let Some((name, _, _)) = PAGES.get(idx) {
                stack.set_visible_child_name(name);
            }
        });
    }

    let body = gtk4::Box::new(Orientation::Horizontal, 0);
    body.append(&nav);
    body.append(&gtk4::Separator::new(Orientation::Vertical));
    body.append(&stack);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("MăngRS")
        .default_width(760)
        .default_height(580)
        .child(&body)
        .build();

    // Row shaping (rounded, padded, floating) for the navigation list.
    let css = gtk4::CssProvider::new();
    css.load_from_data(
        ".navigation-sidebar > row { padding: 5px 10px; margin: 2px 6px; border-radius: 8px; }",
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // Open a specific page when spawned with `--page=<name>` (menu items);
    // selecting the row drives the stack through the row-selected handler.
    let wanted = initial_page.unwrap_or("general");
    stack.set_visible_child_name(wanted);
    let selected = PAGES
        .iter()
        .position(|(name, _, _)| *name == wanted)
        .and_then(|idx| nav.row_at_index(idx as i32))
        .or(first_row);
    if let Some(row) = selected {
        nav.select_row(Some(&row));
    }

    // Custom header bar with the logo; with a CSD titlebar the window keeps
    // the desktop's own controls while carrying the app identity.
    let header = gtk4::HeaderBar::new();
    let header_title = gtk4::Box::new(Orientation::Horizontal, 8);
    if let Some(texture) = &logo {
        let mark = gtk4::Image::from_paintable(Some(texture));
        mark.set_pixel_size(24);
        header_title.append(&mark);
    }
    let header_label = gtk4::Label::new(Some("MăngRS"));
    header_label.add_css_class("title-3");
    header_title.append(&header_label);
    header.set_title_widget(Some(&header_title));
    window.set_titlebar(Some(&header));

    {
        let ui_for_save = ui.clone();
        let save = move || {
            if let Err(e) = ui_for_save.save_all() {
                eprintln!("cannot save config: {e}");
            }
        };
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
        on_change!(ui.em_dash);
        on_change!(ui.auto_capitalize);
        on_change!(ui.double_space);
        on_change!(ui.macro_enabled);
        on_change!(ui.capitalize_macro);
        on_change!(ui.macro_in_off_mode);
        on_change!(ui.dictionary_enabled);
        on_change!(ui.debug_log);
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
            ui.hotkey.connect_activate(move |_| s());
        }
        {
            let s = save.clone();
            ui.time_format.connect_activate(move |_| s());
        }
        {
            let s = save.clone();
            ui.date_format.connect_activate(move |_| s());
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
        export_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileChooserNative::builder()
                .title("Xuất cấu hình")
                .action(gtk4::FileChooserAction::Save)
                .modal(true)
                .build();
            dialog.set_current_name("ibus-mangrs-backup.json");
            dialog.connect_response(move |d, resp| {
                if resp == gtk4::ResponseType::Accept {
                    if let Some(file) = d.file() {
                        let _ = export_json(file.path());
                    }
                }
                d.destroy();
            });
            dialog.show();
        });
    }
    {
        let ui = ui.clone();
        import_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileChooserNative::builder()
                .title("Nhập cấu hình")
                .action(gtk4::FileChooserAction::Open)
                .modal(true)
                .build();
            let ui = ui.clone();
            dialog.connect_response(move |d, resp| {
                if resp == gtk4::ResponseType::Accept {
                    if let Some(file) = d.file() {
                        if let Err(e) = import_json(file.path(), &ui) {
                            eprintln!("import failed: {e}");
                        }
                    }
                }
                d.destroy();
            });
            dialog.show();
        });
    }

    window.present();
}

// --- backup / restore (Giai đoạn 11) ---

#[derive(Serialize, Deserialize)]
struct Backup {
    version: u32,
    config: Config,
    macros: Vec<(String, String)>,
    dictionary: Vec<String>,
}

fn export_json(path: Option<PathBuf>) -> Result<(), String> {
    let Some(path) = path else { return Err("no file chosen".into()) };
    let backup = Backup {
        version: 1,
        config: Config::load(),
        macros: Macros::load().entries().map(|(k, v)| (k.into(), v.into())).collect(),
        dictionary: Dictionary::load().words().map(str::to_string).collect(),
    };
    let text = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn import_json(path: Option<PathBuf>, ui: &Ui) -> Result<(), String> {
    let Some(path) = path else { return Err("no file chosen".into()) };
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let backup: Backup = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    backup.config.save().map_err(|e| e.to_string())?;
    let mut macros = Macros::load();
    macros.set_entries(backup.macros.into_iter());
    macros.save().map_err(|e| e.to_string())?;
    let mut dict = Dictionary::load();
    dict.set_words(backup.dictionary.into_iter());
    dict.save().map_err(|e| e.to_string())?;
    // Refresh the widgets to the imported values.
    let config = Config::load();
    ui.enabled.set_active(config.enabled);
    ui.method.set_selected(method_index(config.method));
    ui.mode.set_selected(mode_index(config.mode));
    ui.charset.set_selected(charset_index(config.charset));
    set_text(&ui.macro_text,
        &Macros::load().entries().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n"));
    set_text(&ui.dictionary_text,
        &Dictionary::load().words().collect::<Vec<_>>().join("\n"));
    set_text(&ui.rules_text,
        &config.app_rules.iter().map(|r| format!("{}={}", r.pattern, r.mode.key())).collect::<Vec<_>>().join("\n"));
    Ok(())
}

// --- small builders ---

fn drop_down(items: &[&str], selected: u32) -> DropDown {
    let dd = DropDown::new(Some(StringList::new(items)), gtk4::Expression::NONE);
    dd.set_selected(selected);
    dd
}

fn text_view(content: &str) -> TextView {
    let view = TextView::builder().hexpand(true).monospace(true).build();
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

fn label(text: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.set_halign(gtk4::Align::Start);
    l.set_wrap(true);
    l
}

/// Vertical breathing room inside a box.
fn spacer(height: i32) -> gtk4::Box {
    let s = gtk4::Box::new(Orientation::Vertical, 0);
    s.set_height_request(height);
    s
}

/// A link that opens in the default browser.
fn link_button(label_text: &str, uri: &str) -> gtk4::LinkButton {
    gtk4::LinkButton::builder().label(label_text).uri(uri).build()
}

fn margin(box_: &gtk4::Box) {
    box_.set_margin_top(16);
    box_.set_margin_bottom(16);
    box_.set_margin_start(16);
    box_.set_margin_end(16);
    box_.set_spacing(8);
}

fn row(label_text: &str, widget: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let row = gtk4::Box::new(Orientation::Horizontal, 12);
    let text = gtk4::Label::new(Some(label_text));
    text.set_hexpand(true);
    text.set_halign(gtk4::Align::Start);
    row.append(&text);
    row.append(widget);
    row
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
    l.set_wrap(true);
    l.add_css_class("dim-label");
    box_.append(&l);
}
