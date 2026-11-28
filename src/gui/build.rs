//! `widgets()`: every row of every page, built without a window so
//! headless tests can drive `refresh`/`read` directly.

use super::*;

/// Every group description moves into a tooltip on its title: the header
/// keeps a single line, and the explanation still shows on hover.
pub(super) fn descriptions_into_tooltips(root: &impl IsA<gtk4::Widget>) {
    let mut child = root.as_ref().first_child();
    while let Some(widget) = child {
        if let Some(group) = widget.downcast_ref::<adw::PreferencesGroup>() {
            move_description_to_title(group);
        }
        descriptions_into_tooltips(&widget);
        child = widget.next_sibling();
    }
}

fn move_description_to_title(group: &adw::PreferencesGroup) {
    let Some(description) = group.description() else {
        return;
    };
    // The title label is internal to libadwaita: match it by its text. No
    // match means the text would be lost, so the line stays visible.
    let Some(title) = find_label(group.upcast_ref(), &group.title()) else {
        return;
    };
    title.set_tooltip_text(Some(&description));
    group.set_description(None);
}

pub(super) fn find_label(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        // Titles are Pango markup: `text()` is decoded, `label()` keeps the
        // entity (`Sao lưu &amp; khôi phục`), and the group carries the raw.
        if label.text() == text || label.label() == text {
            return Some(label.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_label(&widget, text) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// The widgets on their own, so a test can drive `refresh`/`read` without a
/// window; `build` composes them into pages and presents them.
pub(super) fn widgets() -> Rc<Ui> {
    let enabled = switch_row(
        &t("rows.enabled.title", "Bật tiếng Việt"),
        // {page} is the macros page's real sidebar label, so the sentence
        // cannot drift from the sidebar.
        &t("rows.enabled.subtitle", "Tắt là mọi phím đi thẳng vào app")
            .replace("{page}", &t("pages.macros", "Gõ tắt")),
    );
    let method = combo_row(
        &t("rows.method.title", "Kiểu gõ"),
        &t("rows.method.subtitle", "Cách gõ dấu tiếng Việt"),
        &labels(METHODS),
    );
    let mode_choices = ModeConfig::ALL.to_vec();
    let mode = combo_row(
        &t("rows.mode.title", "Chế độ gõ"),
        &t("rows.mode.subtitle", "Cách đưa chữ vào ứng dụng"),
        &mode_labels(&mode_choices),
    );
    let charset = combo_row(
        &t("rows.charset.title", "Bảng mã"),
        &t("rows.charset.subtitle", "Unicode và 10 bảng mã cũ"),
        &labels(CHARSETS),
    );
    // Built-in shortcut rows: the keycap display carries the row title for
    // its accessible description; capture happens in the shared dialog.
    let hotkey_display =
        ShortcutDisplay::new(&t("rows.hotkey_mode.title", "Phím tắt đổi chế độ gõ"));
    let hotkey_value = Rc::new(RefCell::new(String::new()));
    let enabled_hotkey_display = ShortcutDisplay::new(&t(
        "rows.hotkey_enabled.title",
        "Phím tắt bật/tắt tiếng Việt",
    ));
    let enabled_hotkey_value = Rc::new(RefCell::new(String::new()));
    let app_rule_hotkey_display = ShortcutDisplay::new(&t(
        "rows.hotkey_app_rule.title",
        "Phím tắt chọn luật theo app",
    ));
    let app_rule_hotkey_value = Rc::new(RefCell::new(String::new()));
    let modern = switch_row(
        &t("rows.modern.title", "Kiểu dấu mới"),
        &t(
            "rows.modern.subtitle",
            "hòa (chuẩn mới) thay vì hoà (chuẩn cũ)",
        ),
    );
    let free = switch_row(
        &t("rows.free.title", "Dấu tự do"),
        &t("rows.free.subtitle", "Dấu ở đâu trong từ cũng được"),
    );
    let auto_correct = switch_row(
        &t("rows.auto_correct.title", "Trả lại phím khi gõ sai"),
        &t("rows.auto_correct.subtitle", "Âm tiết lạ thì trả phím gốc"),
    );
    let auto_restore = switch_row(
        &t("rows.auto_restore.title", "Khôi phục từ nước ngoài"),
        &t("rows.auto_restore.subtitle", "Từ ngoại lai trả về phím gốc"),
    );
    let extended_finals = switch_row(
        &t("rows.extended_finals.title", "Âm cuối mở rộng"),
        &t(
            "rows.extended_finals.subtitle",
            "g, k, h, nk thay ng, c, nh",
        ),
    );
    let extended_onsets = switch_row(
        &t("rows.extended_onsets.title", "Âm đầu mở rộng"),
        &t("rows.extended_onsets.subtitle", "f, j, w làm âm đầu"),
    );
    let abbrev = switch_row(
        &t("rows.abbrev.title", "Giữ đ trong từ viết tắt"),
        &t("rows.abbrev.subtitle", "ddc, ddk không thành đ"),
    );
    let bypass_url = switch_row(
        &t("rows.bypass_url.title", "Không dấu sau URL"),
        &t("rows.bypass_url.subtitle", "Sau ký tự URL thì giữ nguyên"),
    );
    let w2u = combo_row(
        &t("rows.w2u.title", "W thành ư"),
        &t("rows.w2u.subtitle", "w = ư, wa = ưa"),
        &labels(ON_OFF_MODES),
    );
    let bracket = combo_row(
        &t("rows.bracket.title", "[ ] thành ơ ư"),
        &t("rows.bracket.subtitle", "[ và ] ra ơ, ư theo vị trí"),
        &labels(ON_OFF_MODES),
    );
    let backspace_tone = combo_row(
        &t("rows.backspace_tone.title", "Xóa dấu bằng Backspace"),
        &t("rows.backspace_tone.subtitle", "Cách xoá dấu khi Backspace"),
        &labels(BACKSPACE_TONE_MODES),
    );
    let nav_key_while_preedit = combo_row(
        &t(
            "rows.nav_key_while_preedit.title",
            "Phím điều hướng khi còn từ",
        ),
        &t(
            "rows.nav_key_while_preedit.subtitle",
            "Phím mũi tên đầu tiên khi còn từ gõ dở",
        ),
        &labels(NAV_KEY_MODES),
    );
    let em_dash = switch_row(
        &t("rows.em_dash.title", "Gạch ngang dài"),
        &t("rows.em_dash.subtitle", "Hai dấu trừ thành gạch dài"),
    );
    let auto_capitalize = switch_row(
        &t("rows.auto_capitalize.title", "Tự viết hoa đầu câu"),
        &t("rows.auto_capitalize.subtitle", "Sau . ! ? và Enter"),
    );
    let double_space = switch_row(
        &t("rows.double_space.title", "Hai dấu cách thành dấu chấm"),
        &t("rows.double_space.subtitle", "Thay cho gõ dấu chấm tay"),
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
        &t("rows.macro_in_off_mode.title", "Gõ tắt khi tắt tiếng Việt"),
        &t(
            "rows.macro_in_off_mode.subtitle",
            "Vẫn mở rộng khi tiếng Việt đang tắt",
        ),
    );
    let macro_valid_words = switch_row(
        &t("rows.macro_valid_words.title", "Gõ tắt thắng từ tiếng Việt"),
        &t(
            "rows.macro_valid_words.subtitle",
            "Khoá trùng từ thật vẫn mở",
        ),
    );
    let keep_composed_words = switch_row(
        &t(
            "rows.keep_composed_words.title",
            "Giữ từ khi gõ thêm phím lạ",
        ),
        &t(
            "rows.keep_composed_words.subtitle",
            "tùng + g → tùngg thay vì tungfg",
        ),
    );
    let keep_repeats_raw = switch_row(
        &t(
            "rows.keep_repeats_raw.title",
            "Giữ phím lặp nguyên bản (Flow)",
        ),
        &t(
            "rows.keep_repeats_raw.subtitle",
            "giữ o → oooo… thay vì dừng ở ô",
        ),
    );
    let suggest_words = switch_row(
        &t("rows.suggest_words.title", "Gợi ý từ khi gõ"),
        &t("rows.suggest_words.subtitle", "Dải gợi ý cạnh con trỏ"),
    );
    // One row instead of a switch plus a dropdown: off is the first choice,
    // and the row wears the feature's name rather than the mechanism's.
    let dodge_fix = combo_row(
        &t("rows.dodge_fix.title", "Sửa lỗi gõ trong ô địa chỉ"),
        &t("rows.dodge_fix.subtitle", "Trước ra oỏ, giờ ra ỏ"),
        &[
            &t("rows.dodge_fix.off", "Tắt"),
            &t("rows.dodge_fix.raw", "Xoá thẳng"),
            &t("rows.dodge_fix.marker", "Chèn dấu ·"),
        ],
    );
    let preferred_backend = combo_row(
        &t("rows.preferred_backend.title", "Cách chèn phím"),
        &t("rows.preferred_backend.subtitle", "Ưu tiên cách chèn phím"),
        // Order pinned by preferred_backend_index: auto, uinput, xtest,
        // wlr-virtual-keyboard, libei.
        &["Tự động", "Uinput", "Xtest", "Wlroots", "Libei"],
    );
    // One row for both formats: two plain dropdowns with a dot between
    // them, and the live example of each on the subtitle. A format the old
    // free-text rows saved that matches no preset is appended by
    // `select_format` instead of being rewritten.
    let time_format = gtk4::DropDown::from_strings(TIME_FORMATS);
    let date_format = gtk4::DropDown::from_strings(DATE_FORMATS);
    let formats_row = action_row(&t("rows.formats.title", "Định dạng giờ & ngày"), "");
    let dot = gtk4::Label::new(Some("·"));
    dot.add_css_class("dim-label");
    for widget in [
        time_format.upcast_ref::<gtk4::Widget>(),
        dot.upcast_ref(),
        date_format.upcast_ref(),
    ] {
        widget.set_valign(gtk4::Align::Center);
        formats_row.add_suffix(widget);
    }
    for dd in [&time_format, &date_format] {
        let row = formats_row.clone();
        let time = time_format.clone();
        let date = date_format.clone();
        dd.connect_selected_notify(move |_| {
            row.set_subtitle(&pages::macros::formats_line(&time, &date));
        });
    }
    formats_row.set_subtitle(&pages::macros::formats_line(&time_format, &date_format));
    // The item labels are proper names (LANGUAGES), never translated.
    let language = combo_row(
        &t("rows.language.title", "Ngôn ngữ"),
        &t("rows.language.subtitle", "Ngôn ngữ cửa sổ cài đặt"),
        LANGUAGES,
    );
    let macros_text = text_view("");
    let emoji_enabled = switch_row(
        &t("rows.emoji_enabled.title", "Gõ tắt emoji"),
        &t("rows.emoji_enabled.subtitle", "Gõ :tên: thành emoji"),
    );
    let emoji_hotkey_display =
        ShortcutDisplay::new(&t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"));
    let emoji_hotkey_value = Rc::new(RefCell::new(String::new()));
    let emoji_text = text_view("");

    // Custom shortcuts: the boxed list is rebuilt by `fill_custom` (rows,
    // empty state, add row); the overlay hosts the delete-undo toasts.
    let custom_box = gtk4::ListBox::new();
    custom_box.add_css_class("boxed-list");
    custom_box.set_selection_mode(gtk4::SelectionMode::None);
    let hotkey_overlay = adw::ToastOverlay::new();
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
        "Từ trong từ điển cá nhân không bị trả về chuỗi phím gốc.",
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
    // The page's toast host: built here, returned by the page as its
    // wrapper, so toasts fired from the row editors are visible.
    let rules_overlay = adw::ToastOverlay::new();
    // The add-rule form's mode picker; its model follows `mode_choices`.
    let rule_add_mode_pick = gtk4::DropDown::new(
        Some(StringList::new(&mode_labels(&mode_choices))),
        gtk4::Expression::NONE,
    );
    rule_add_mode_pick.set_selected(mode_index(&mode_choices, ModeConfig::Preedit));
    rule_add_mode_pick.set_hexpand(true);
    rule_add_mode_pick.set_height_request(40);
    rule_add_mode_pick.add_css_class("rule-fill");
    rule_add_mode_pick
        .set_tooltip_text(Some(&t("rows.rule_mode.tooltip", "Chế độ gõ của luật mới")));
    if let Some(label) = first_label(&rule_add_mode_pick) {
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    }
    // The merged add-rule form ("Thêm luật"): the pill entry whose
    // placeholder mirrors the focused app, the copy/apply pair shown only
    // while an app is detected, and the Thêm button. The page lays them
    // out; `refresh_rule_focus` feeds them.
    let rule_pill = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rule_pill.add_css_class("rule-fill");
    rule_pill.add_css_class("rule-pill");
    rule_pill.set_hexpand(true);
    let rule_name_entry = gtk4::Entry::new();
    rule_name_entry.set_hexpand(true);
    // Margins stay with the page: the pill's internal padding is a layout
    // decision of the block that hosts the pill.
    rule_name_entry.set_placeholder_text(Some(&t(
        "rows.rule_name.placeholder",
        "vd: firefox, org.gnome.Console",
    )));
    rule_name_entry.set_tooltip_text(Some(&t(
        "rows.rule_name.tooltip",
        "Tên app: khớp một phần, không phân biệt hoa/thường (vd firefox, sublime)",
    )));
    let rule_copy_btn = gtk4::Button::from_icon_name("edit-copy-symbolic");
    rule_copy_btn.add_css_class("flat");
    rule_copy_btn.add_css_class("circular");
    rule_copy_btn.set_size_request(36, 36);
    rule_copy_btn.set_valign(gtk4::Align::Center);
    rule_copy_btn.set_visible(false);
    rule_copy_btn.set_tooltip_text(Some(&t(
        "rows.rule_copy.tooltip",
        "Sao chép app đang focus",
    )));
    rule_copy_btn.update_property(&[gtk4::accessible::Property::Label(&t(
        "rows.rule_copy.tooltip",
        "Sao chép app đang focus",
    ))]);
    // The check stands for "use the detected app"; object-select is the
    // same glyph GNOME Settings uses for "apply this".
    let rule_apply_btn = gtk4::Button::from_icon_name("object-select-symbolic");
    rule_apply_btn.add_css_class("flat");
    rule_apply_btn.add_css_class("circular");
    rule_apply_btn.set_size_request(36, 36);
    rule_apply_btn.set_valign(gtk4::Align::Center);
    rule_apply_btn.set_visible(false);
    rule_apply_btn.set_tooltip_text(Some(&t("rows.rule_apply.tooltip", "Dùng app đang focus")));
    rule_apply_btn.update_property(&[gtk4::accessible::Property::Label(&t(
        "rows.rule_apply.tooltip",
        "Dùng app đang focus",
    ))]);
    let rule_add_btn = gtk4::Button::with_label(&t("rules.add_button", "Thêm luật"));
    rule_add_btn.add_css_class("suggested-action");
    rule_add_btn.set_sensitive(false);
    rule_add_btn.set_valign(gtk4::Align::Center);
    rule_add_btn.set_hexpand(true);
    let rule_focus_value = Rc::new(RefCell::new(String::new()));
    let rules_loading = Rc::new(RefCell::new(false));
    let desktop_icon = switch_row(
        &t("rows.desktop_icon.title", "Hiện trong menu ứng dụng"),
        &t(
            "rows.desktop_icon.subtitle",
            "Biểu tượng trong lưới ứng dụng",
        ),
    );
    let show_toasts = switch_row(
        &t("rows.show_toasts.title", "Thông báo nhanh"),
        &t("rows.show_toasts.subtitle", "Bong bóng báo thao tác"),
    );
    let desktop_icon_loading = Rc::new(RefCell::new(false));
    let save_cb: SharedSave = Rc::new(RefCell::new(None));
    let suppress_save = Cell::new(false);
    let hide_underline = switch_row(
        &t("rows.hide_underline.title", "Ẩn gạch chân"),
        &t(
            "rows.hide_underline.subtitle",
            "Chữ đang gõ không có gạch chân (tùy app)",
        ),
    );
    // The debug-log path reads as a path: markup is on so the subtitle can
    // wrap it in <tt> (the text carries no & — markup is safe here).
    // One switch; the two optional log tiers are picked in its confirmation
    // dialog and have no rows of their own.
    let debug_log = adw::SwitchRow::new();
    debug_log.set_use_markup(true);
    debug_log.set_title(&t("rows.debug_log.title", "Chế độ gỡ lỗi"));
    debug_log.set_subtitle(&t(
        "rows.debug_log.subtitle_markup",
        "Ghi quyết định của engine vào <tt>~/.cache/ibus-mangrs/debug.log</tt>",
    ));
    let x11_delete = switch_row(
        &t("rows.x11_delete.title", "Xoá đuôi từ ở app X11"),
        &t("rows.x11_delete.subtitle", "Cách xoá từ ở app X11"),
    );
    let preedit_xim = switch_row(
        &t("rows.preedit_xim.title", "Preedit cho app XIM"),
        &t(
            "rows.preedit_xim.subtitle",
            "Giữ gạch chân cho app XIM (Java/JetBrains): chữ đang gõ hiện trong cửa sổ riêng rồi commit khi hết từ.",
        ),
    );
    // The "Nhịp gõ" table (pages/nhip.rs) builds its grid from these spins;
    // the table itself carries the labels, the shipped default and a reset.
    let backspace_interval = spin_button(1.0, 100.0, 1.0);
    let post_delay = spin_button(0.0, 500.0, 1.0);
    let echo_stale_spin = spin_button(50.0, 1000.0, 10.0);
    echo_stale_spin.set_sensitive(false);
    let echo_stale_switch = gtk4::Switch::new();
    let idle_commit_spin = spin_button(300.0, 2000.0, 50.0);
    idle_commit_spin.set_sensitive(false);
    let idle_commit_switch = gtk4::Switch::new();
    let flow_per_key_spin = spin_button(0.0, 500.0, 5.0);
    let flow_backspace_restore_spin = spin_button(0.0, 1000.0, 10.0);
    // Both parked graces have a floor: a commit sent in the same breath as
    // its deletes overtakes them (engine MIN_PARK_MS).
    let echo_apply_grace_spin = spin_button(5.0, 500.0, 5.0);
    let key_delete_grace_spin = spin_button(5.0, 500.0, 5.0);
    let focus_bounce_spin = spin_button(0.0, 1000.0, 10.0);
    let marker_wait_spin = spin_button(0.0, 100.0, 1.0);
    let experimental_features = switch_row(
        &t("rows.experimental_features.title", "Tính năng thử nghiệm"),
        &t(
            "rows.experimental_features.subtitle",
            "Bật để dùng các tính năng bên dưới",
        ),
    );

    // Panel-menu editor: the design vector is the source of truth; the
    // boxed list is its view (rebuilt on every edit).
    let menu = Rc::new(RefCell::new(Vec::new()));
    let menu_box = gtk4::ListBox::new();
    menu_box.add_css_class("boxed-list");
    menu_box.set_selection_mode(gtk4::SelectionMode::None);
    let menu_open = Rc::new(RefCell::new(HashSet::new()));
    let menu_overlay = adw::ToastOverlay::new();
    let menu_focus_target = Rc::new(RefCell::new(None));

    // The tips are row tooltips now — no icon button at the row end;
    // hovering the row itself shows the longer explanation, whose text
    // still comes from the content file.
    macro_rules! tip {
        ($row:expr, $key:expr, $default:expr) => {
            $row.set_tooltip_text(Some(&t($key, $default)));
        };
    }
    tip!(
        enabled,
        "tips.enabled",
        "Bật hoặc tắt cả bộ gõ. Khi tắt, phím đi thẳng vào app như chưa cài."
    );
    tip!(
        method,
        "tips.method",
        "Kiểu gõ quyết định cách ra chữ có dấu, ví dụ Telex gõ ss ra á, VNI gõ a1 ra á."
    );
    tip!(
        mode,
        "tips.mode",
        "Cách đưa chữ vào app. Gạch chân hiện chữ ngay trong app, cần app hỗ trợ. Surrounding sửa đuôi từ dựa vào báo cáo của chính app. Flow chèn từng phím, cần backend chèn phím. Forward Backspace chạy được ở mọi app. OFF tắt tiếng Việt."
    );
    tip!(
        charset,
        "tips.charset",
        "Bảng mã chữ. Unicode dùng cho mọi việc thường ngày, VNI và TCVN3 chỉ để đọc file cũ."
    );
    tip!(
        modern,
        "tips.modern",
        "Đặt dấu theo chuẩn mới của Bộ GD-ĐT: hòa thay vì hoà."
    );
    tip!(
        free,
        "tips.free",
        "Cho dấu đứng tự do trong từ, không buộc phải nằm ở nguyên âm gần nhất."
    );
    tip!(
        auto_correct,
        "tips.auto_correct",
        "Âm tiết không có trong từ điển sẽ trả về đúng chuỗi phím đã bấm."
    );
    tip!(
        auto_restore,
        "tips.auto_restore",
        "Từ nước ngoài như english, hello tự trả về chuỗi phím gốc khi gõ xong."
    );
    tip!(
        extended_finals,
        "tips.extended_finals",
        "Cho g, k, h, nk thay cho ng, c, nh để dấu thanh đúng dù gõ trước hay sau âm cuối. Mặc định tắt."
    );
    tip!(
        extended_onsets,
        "tips.extended_onsets",
        "Cho f, j, w làm âm đầu và nhận dấu. Mặc định tắt để từ tiếng Anh không bị ghi dấu oan."
    );
    tip!(
        abbrev,
        "tips.abbrev",
        "Giữ đ trong từ viết tắt, ví dụ ddc, ddk không bị đổi thành đ."
    );
    tip!(
        bypass_url,
        "tips.bypass_url",
        "Liên kết thật như https:// và www. gõ nguyên cả chuỗi, dấu câu sau đó vẫn gõ tiếng Việt bình thường."
    );
    tip!(
        w2u,
        "tips.w2u",
        "Gõ w ra ư: w thành ư, wa thành ưa. Tiện cho bàn phím không có ư."
    );
    tip!(
        bracket,
        "tips.bracket",
        "Gõ [ và ] ra ơ và ư tuỳ vị trí trong từ."
    );
    tip!(show_toasts, "tips.show_toasts", "Bong bóng nhỏ khi thêm, xoá, xuất, nhập hoặc cài extension. Tắt để im lặng, lỗi vẫn hiện hộp thoại.");
    tip!(
        backspace_tone,
        "tips.backspace_tone",
        "Xoá dấu trước: hạ thành ha rồi h. Xoá cả dấu và chữ: hạ thành h, mỗi Backspace xoá đúng một ký tự trên màn hình. Unikey: lùi qua từng phím đã gõ."
    );
    tip!(
        em_dash,
        "tips.em_dash",
        "Hai gạch ngang liền nhau thành một gạch dài."
    );
    tip!(
        auto_capitalize,
        "tips.auto_capitalize",
        "Tự viết hoa chữ đầu câu sau dấu chấm, dấu chấm than, dấu hỏi hoặc Enter."
    );
    tip!(
        double_space,
        "tips.double_space",
        "Nhấn hai dấu cách liền nhau thành dấu chấm và khoảng cách."
    );
    tip!(
        macro_enabled,
        "tips.macro_enabled",
        "Bật gõ tắt: gõ xong khoá thì nội dung tắt được thay vào."
    );
    tip!(
        capitalize_macro,
        "tips.capitalize_macro",
        "Khoá viết hoa thì nội dung cũng viết hoa theo."
    );
    tip!(
        macro_in_off_mode,
        "tips.macro_in_off_mode",
        "Gõ tắt vẫn chạy khi tiếng Việt đang tắt."
    );
    tip!(
        emoji_enabled,
        "tips.emoji_enabled",
        "Gõ :smile: ra emoji. Tên không khớp thì giữ nguyên chữ đã gõ."
    );
    tip!(
        hide_underline,
        "tips.hide_underline",
        "Ẩn gạch chân ở những app vẽ gạch chân bị lỗi, ví dụ vài game và terminal."
    );
    tip!(
        debug_log,
        "tips.debug_log",
        "Engine ghi quyết định vào ~/.cache/ibus-mangrs/debug.log. Khi bật sẽ hỏi có ghi nội dung đang gõ và từng phím hay không. Log tự cắt mỗi 5 MB."
    );
    tip!(
        x11_delete,
        "tips.x11_delete",
        "Cách xoá từ ở app X11: xoá đuôi từ như ibus-bamboo, hoặc chỉ xoá theo báo cáo của app."
    );
    tip!(
        preedit_xim,
        "tips.preedit_xim",
        "Dùng cửa sổ gạch chân riêng cho app XIM như Java và JetBrains, vốn không nhận gạch chân của IBus."
    );
    tip!(
        backspace_interval,
        "tips.backspace_interval",
        "Nhịp gửi Backspace. Giảm nếu xoá chậm, tăng nếu app xử lý không kịp."
    );
    tip!(
        post_delay,
        "tips.post_delay",
        "Chờ một chút sau khi viết lại cả từ, dành cho app xử lý phím chậm."
    );
    // Every "Thử nghiệm" row: the tooltip names the feature it belongs to.
    tip!(
        experimental_features,
        "tips.experimental_features",
        "Công tắc chính của tab Thử nghiệm. Tắt thì mọi hàng bên dưới bị khoá và bộ gõ chạy theo mặc định, giá trị đã chỉnh vẫn được lưu."
    );
    tip!(
        macro_valid_words,
        "tips.macro_valid_words",
        "Khoá gõ tắt trùng một từ tiếng Việt thật, ví dụ cam, thì không mở rộng để không cản gõ. Bật thì gõ tắt thắng; muốn gõ từ thật, bấm Backspace một lần rồi gõ lại."
    );
    tip!(
        keep_composed_words,
        "tips.keep_composed_words",
        "Từ đã hoàn chỉnh thì phím mới nối vào từ chứ không kéo cả từ về chuỗi phím gốc. Mặc định tắt."
    );
    tip!(
        keep_repeats_raw,
        "tips.keep_repeats_raw",
        "Dành cho Flow: bật để giữ phím lặp lại nguyên bản như bàn phím thường. Mặc định tắt."
    );
    tip!(
        suggest_words,
        "tips.suggest_words",
        "Hiện từ đang gõ cùng dải gợi ý ngay cạnh con trỏ. Chọn bằng phím `, Alt+1 đến Alt+5 hoặc bấm vào dòng gợi ý. Mặc định tắt."
    );
    tip!(
        dodge_fix,
        "tips.dodge_inline_autocomplete",
        "Khi trình duyệt hiện gợi ý trong ô địa chỉ, từ vẫn gõ được dấu và không còn bị lẫn thành `oỏ`. Chọn `Dấu chấm` nếu cách hiện tại vẫn lỗi. Mặc định tắt."
    );
    tip!(
        preferred_backend,
        "tips.preferred_backend",
        "Chọn cách chèn phím được ưu tiên. Để tự động là tốt nhất, chỉ đổi khi cần thử nghiệm."
    );
    tip!(
        echo_stale_switch,
        "tips.echo_stale",
        "Chờ các phím Backspace ảo quay về trước khi đổ chữ đã commit, để không mất chữ trên Wayland. Số ms là thời hạn chờ, mặc định 150."
    );
    tip!(
        idle_commit_switch,
        "tips.idle_commit",
        "Dừng tay quá lâu trên một từ còn gạch chân thì từ đó tự commit như bấm dấu cách. Số ms là ngưỡng nghỉ tay."
    );
    // The two Flow tips sit on their spins now: one row carries both knobs.
    tip!(
        flow_per_key_spin,
        "tips.flow_per_key",
        "Flow hiện chữ có dấu ngay khi gõ. Số ms là độ trễ đổ chữ vào app, thấp thì dấu hiện sớm hơn."
    );
    tip!(
        flow_backspace_restore_spin,
        "tips.flow_backspace_restore",
        "Gõ thêm dấu làm mất dấu, bấm Backspace thì dấu quay lại. Số ms là thời gian chờ trước khi sửa màn hình, mặc định 150."
    );
    tip!(
        nav_key_while_preedit,
        "tips.nav_key_while_preedit",
        "Khi còn từ gạch chân, phím mũi tên vẫn chạy ngay như ibus-bamboo. Nếu chữ bị chèn sai chỗ, chọn Nuốt phím đầu để lần nhấn đầu chỉ commit từ."
    );

    Rc::new(Ui {
        enabled,
        method,
        mode,
        mode_choices: Rc::new(RefCell::new(mode_choices)),
        charset,
        hotkey_display,
        hotkey_value,
        enabled_hotkey_display,
        enabled_hotkey_value,
        app_rule_hotkey_display,
        app_rule_hotkey_value,
        modern,
        free,
        auto_correct,
        auto_restore,
        extended_finals,
        extended_onsets,
        abbrev,
        bypass_url,
        w2u,
        bracket,
        backspace_tone,
        nav_key_while_preedit,
        em_dash,
        auto_capitalize,
        double_space,
        macro_enabled,
        capitalize_macro,
        macro_in_off_mode,
        macro_valid_words,
        keep_composed_words,
        keep_repeats_raw,
        suggest_words,
        dodge_fix,
        preferred_backend,
        time_format,
        date_format,
        formats_row,
        language,
        macros_text,
        emoji_enabled,
        emoji_hotkey_display,
        emoji_hotkey_value,
        emoji_text,
        custom: Rc::new(RefCell::new(Vec::new())),
        custom_box,
        hotkey_overlay,
        dictionary_enabled,
        dictionary_text,
        rules_box,
        rule_choices: Rc::new(RefCell::new(Vec::new())),
        rule_patterns: Rc::new(RefCell::new(Vec::new())),
        rule_add_mode_pick,
        rules_overlay,
        rules_loading,
        desktop_icon,
        show_toasts,
        desktop_icon_loading,
        save_cb,
        suppress_save,
        saved_macros: RefCell::new(None),
        saved_dictionary: RefCell::new(None),
        saved_emoji: RefCell::new(None),
        self_saved_mtime: Cell::new(None),
        sidebar_select: RefCell::new(None),
        debug_tab: RefCell::new(None),
        rule_pill,
        rule_name_entry,
        rule_copy_btn,
        rule_apply_btn,
        rule_add_btn,
        rule_focus_value,
        hide_underline,
        debug_log,
        debug_tiers: Rc::new(RefCell::new((false, false))),
        debug_log_loading: Rc::new(Cell::new(false)),
        x11_delete,
        preedit_xim,
        backspace_interval,
        post_delay,
        echo_apply_grace_spin,
        key_delete_grace_spin,
        focus_bounce_spin,
        marker_wait_spin,
        nhip_gated: Rc::new(RefCell::new(Vec::new())),
        echo_stale_switch,
        echo_stale_spin,
        idle_commit_switch,
        idle_commit_spin,
        flow_per_key_spin,
        flow_backspace_restore_spin,
        experimental_features,
        menu,
        menu_box,
        menu_open,
        menu_overlay,
        menu_focus_target,
    })
}
