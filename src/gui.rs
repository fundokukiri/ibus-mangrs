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

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, DropDown, Entry, Orientation, StringList, Switch,
    TextView,
};
use crate::config::{
    BracketConfig, CharsetConfig, Config, MethodConfig, ModeConfig, W2uConfig,
};
use crate::macros::{Dictionary, Macros};

thread_local! {
    /// Every settings row (lowercase "title subtitle", widget) so the search
    /// bar can hide the ones that do not match.
    static ROWS: RefCell<Vec<(String, gtk4::Widget)>> = const { RefCell::new(Vec::new()) };
}

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
    preedit_timeout: Entry,
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
        config.preedit_commit_timeout_ms =
            self.preedit_timeout.text().trim().parse().unwrap_or(800);
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
    ROWS.with(|r| r.borrow_mut().clear());

    let config = Config::load();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let enabled = Switch::builder().active(config.enabled).build();
    let method = drop_down(METHODS, method_index(config.method));
    let mode = drop_down(MODES, mode_index(config.mode));
    let charset = drop_down(CHARSETS, charset_index(config.charset));
    let hotkey = Entry::builder()
        .text(&config.cycle_mode_hotkey)
        .placeholder_text("vd. Control+Alt+v")
        .width_chars(18)
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
    let time_format = Entry::builder().text(&config.time_format).width_chars(14).build();
    let date_format = Entry::builder().text(&config.date_format).width_chars(14).build();
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
    let preedit_timeout =
        Entry::builder().text(config.preedit_commit_timeout_ms.to_string()).width_chars(6).build();
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
        preedit_timeout,
        unicode_channel,
    });

    // --- pages: grouped-card settings style (section header + cards) ---

    // Quick add for App Rules: pick a client the engine has seen, pick a
    // mode, append one rule line.
    let client_combo = gtk4::ComboBoxText::new();
    for name in crate::config::recent_clients() {
        client_combo.append_text(&name);
    }
    client_combo.set_active(Some(0));
    let rule_mode = drop_down(
        &crate::ibus_prop::MODES.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
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

    let export_btn = gtk4::Button::with_label("Xuất…");
    let import_btn = gtk4::Button::with_label("Nhập…");

    // ===== Page 1: Kiểu gõ =====
    let typing = gtk4::Box::new(Orientation::Vertical, 6);
    section(&typing, "Điều khiển", "Bật tiếng Việt và phím tắt đổi chế độ");
    typing.append(&card(vec![
        pref_row(
            "Bật tiếng Việt",
            "Khi tắt, phím gõ thẳng; gõ tắt vẫn chạy nếu bật ở tab Gõ tắt",
            &ui.enabled,
        ),
        pref_row(
            "Phím tắt đổi chế độ",
            "Ví dụ Control+Alt+v — để trống là chưa gán",
            &ui.hotkey,
        ),
    ]));
    hint(
        &typing,
        "Thứ tự phím tắt đổi chế độ: Không gạch chân → Gạch chân → Surrounding → Smooth → Super Smooth → Select → OFF → Gõ mặc định.",
    );
    section(&typing, "Phương thức gõ", "Kiểu gõ, bảng mã và cách hiển thị");
    typing.append(&card(vec![
        pref_row("Kiểu gõ chính", "Telex, VNI, VIQR, Microsoft layout", &ui.method),
        pref_row("Bảng mã", "Unicode và 10 bảng mã cũ", &ui.charset),
        pref_row(
            "Hiển thị",
            "8 chế độ; uinput cần /dev/uinput, trên GNOME Wayland tự về Gạch chân",
            &ui.mode,
        ),
    ]));
    section(
        &typing,
        "Tự nhả từ đang gõ",
        "Chống chữ bị dời chỗ khi click chuột giữa chừng",
    );
    typing.append(&card(vec![pref_row(
        "Thời gian tự nhả (ms)",
        "Từ chưa xong tự commit sau khi nghỉ tay lâu hơn ngưỡng này (0 = tắt)",
        &ui.preedit_timeout,
    )]));
    section(
        &typing,
        "Tốc độ gõ (uinput)",
        "Chỉ áp dụng cho Smooth / Super Smooth / Select",
    );
    typing.append(&card(vec![
        pref_row(
            "Nhịp gõ chữ",
            "Khoảng cách giữa hai lần gửi chữ (ms)",
            &ui.commit_interval,
        ),
        pref_row(
            "Nhịp xoá lùi",
            "Khoảng cách giữa hai lần Backspace (ms)",
            &ui.backspace_interval,
        ),
        pref_row(
            "Độ trễ sau từ",
            "Chờ sau khi viết lại từ trước khi gõ tiếp (ms)",
            &ui.post_delay,
        ),
        pref_row(
            "Kênh gửi từ tiếng Việt",
            "CommitText an toàn, hoặc Clipboard (Ctrl+V, tạm đè clipboard)",
            &ui.unicode_channel,
        ),
    ]));
    section(&typing, "Chẩn đoán", "Ghi lại quyết định của engine để gỡ lỗi");
    typing.append(&card(vec![pref_row(
        "Ghi log gỡ lỗi",
        "Ghi vào ~/.cache/ibus-mangrs/debug.log",
        &ui.debug_log,
    )]));

    // ===== Page 2: Dấu & Sửa lỗi =====
    let tone = gtk4::Box::new(Orientation::Vertical, 6);
    section(&tone, "Dấu thanh", "Kiểu đặt dấu trên nguyên âm");
    tone.append(&card(vec![
        pref_row("Dấu thanh chuẩn", "hòa (chuẩn mới) thay vì hoà (chuẩn cũ)", &ui.modern),
        pref_row("Bỏ dấu tự do", "Dấu đặt anywhere trong từ: hoangf → hoàng", &ui.free),
    ]));
    section(&tone, "Chính tả", "Sửa lỗi và khôi phục từ");
    tone.append(&card(vec![
        pref_row(
            "Tự sửa lỗi chính tả",
            "Âm tiết sai được chuyển thành âm hợp lệ gần nhất",
            &ui.auto_correct,
        ),
        pref_row(
            "Khôi phục từ Anh",
            "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
            &ui.auto_restore,
        ),
        pref_row(
            "Không bỏ dấu trong URL",
            "Từ gõ ngay sau / : @ . _ ~ = # & % ? giữ nguyên",
            &ui.bypass_url,
        ),
    ]));
    section(&tone, "Tự động", "Viết hoa và dấu câu tự động");
    tone.append(&card(vec![
        pref_row("Tự viết hoa", "Sau . ! ? và Enter", &ui.auto_capitalize),
        pref_row("Hai cách → chấm", "Thử nghiệm: hai dấu cách thành dấu chấm", &ui.double_space),
    ]));
    section(&tone, "Ký tự đặc biệt", "Chuyển đổi phím tắt ký tự");
    tone.append(&card(vec![
        pref_row("W thành ư", "Tắt / trừ đầu từ / mọi vị trí", &ui.w2u),
        pref_row("[ ] thành ơ ư", "Tắt / trừ đầu từ / mọi vị trí", &ui.bracket),
        pref_row("Gõ -- thành —", "Hai dấu trừ liên tiếp thành gạch dài", &ui.em_dash),
    ]));

    // ===== Page 3: Gõ tắt =====
    let macro_box = gtk4::Box::new(Orientation::Vertical, 6);
    section(&macro_box, "Gõ tắt", "Mở rộng cụm tắt thành câu đầy đủ");
    macro_box.append(&card(vec![
        pref_row("Bật gõ tắt", "Khóa gõ tắt mở rộng ở cuối từ", &ui.macro_enabled),
        pref_row(
            "Viết hoa gõ tắt",
            "Khóa viết hoa → nội dung viết hoa theo",
            &ui.capitalize_macro,
        ),
        pref_row("Gõ tắt khi OFF", "Vẫn mở rộng khi tiếng Việt đang tắt", &ui.macro_in_off_mode),
        pref_row("Định dạng giờ ($TIME)", "Ví dụ %H:%M", &ui.time_format),
        pref_row("Định dạng ngày ($DATE)", "Ví dụ %d/%m/%Y", &ui.date_format),
    ]));
    section(&macro_box, "Bảng gõ tắt", "Mỗi dòng: khóa=Nội dung ($TIME, $DATE)");
    macro_box.append(&card_widget(&scrolled(&ui.macro_text, 160)));

    // ===== Page 4: Dữ liệu =====
    let data_box = gtk4::Box::new(Orientation::Vertical, 6);
    section(&data_box, "Từ điển của tôi", "Từ trong từ điển không bị khôi phục về chuỗi phím");
    data_box.append(&card(vec![pref_row(
        "Bật từ điển",
        "Đọc từ ~/.config/ibus-mangrs/dictionary.txt",
        &ui.dictionary_enabled,
    )]));
    data_box.append(&card_widget(&scrolled(&ui.dictionary_text, 160)));

    section(
        &data_box,
        "Luật theo ứng dụng",
        "Mỗi dòng: tên-ứng-dụng=chế độ (nonpreedit, preedit, surrounding, smooth, supersmooth, select, off, defaulttyping)",
    );
    data_box.append(&card_widget(&add_row));
    data_box.append(&card_widget(&scrolled(&ui.rules_text, 160)));

    section(
        &data_box,
        "Sao lưu & khôi phục",
        "Một JSON chứa cấu hình, gõ tắt và từ điển — chuyển máy hoặc dự phòng",
    );
    data_box.append(&card(vec![
        pref_row("Xuất cấu hình ra JSON", "Lưu ra file để mang đi", &export_btn),
        pref_row("Nhập cấu hình từ JSON", "Đọc lại từ file đã xuất", &import_btn),
    ]));

    // ===== Page 5: Giới thiệu =====
    let about = gtk4::Box::new(Orientation::Vertical, 6);

    // Logo is embedded at compile time: the GUI has no installed asset dir.
    const LOGO: &[u8] = include_bytes!("../data/logo-no-bg.png");
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
            "8 chế độ hiển thị",
            "Preedit, Non-preedit, Surrounding + 3 mode uinput, OFF, Gõ mặc định",
        ),
        info_row(
            "network-workgroup-symbolic",
            "URL & từ điển thông minh",
            "Từ sau / : @ . giữ nguyên; kiểm tra chính tả với 38 350 từ",
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
        info_row("avatar-default-symbolic", "Tác giả", "fundoku"),
        info_row(
            "help-about-symbolic",
            "Giấy phép mã nguồn mở",
            "MIT — mã nguồn mở, chi tiết trong LICENSE và NOTICE.md",
        ),
        info_row(
            "emblem-favorite-symbolic",
            "Credits",
            "Lõi gõ bamboo-core (MIT); từ điển Free Vietnamese Dictionary Project (dữ liệu GPL)",
        ),
    ]));

    // Content: one scrolled, width-clamped page per section, crossfading.
    // (stack name, pill label, symbolic icon from the icon theme)
    const PAGES: [(&str, &str, &str); 5] = [
        ("typing", "Kiểu gõ", "input-keyboard-symbolic"),
        ("tone", "Dấu & Sửa lỗi", "font-x-generic-symbolic"),
        ("macros", "Gõ tắt", "document-edit-symbolic"),
        ("data", "Dữ liệu", "view-list-symbolic"),
        ("about", "Giới thiệu", "help-about-symbolic"),
    ];
    let page_boxes: [&gtk4::Box; 5] = [&typing, &tone, &macro_box, &data_box, &about];
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
    });

    let content = gtk4::Box::new(Orientation::Vertical, 0);
    content.append(&search_bar);
    content.append(&stack);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("MăngRS")
        .default_width(780)
        .default_height(700)
        .child(&content)
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
    let (_, pill) = pills
        .iter()
        .find(|(n, _)| n == wanted)
        .unwrap_or(&pills[0]);
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
        on_change!(ui.bypass_url);
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
            &ui.preedit_timeout,
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

fn export_json(path: Option<PathBuf>) -> Result<(), String> {
    let Some(path) = path else { return Err("no file chosen".into()) };
    let text = crate::backup::to_json(
        Config::load(),
        Macros::load().entries().map(|(k, v)| (k.into(), v.into())).collect(),
        Dictionary::load().words().map(str::to_string).collect(),
    )?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn import_json(path: Option<PathBuf>, ui: &Ui) -> Result<(), String> {
    let Some(path) = path else { return Err("no file chosen".into()) };
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
    gtk4::LinkButton::builder().label(label_text).uri(uri).build()
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
    ROWS.with(|r| {
        r.borrow_mut()
            .push((format!("{title} {subtitle}").to_lowercase(), h.clone().upcast()))
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