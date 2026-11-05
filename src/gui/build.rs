//! `widgets()`: every row of every page, built without a window so
//! headless tests can drive `refresh`/`read` directly.

use super::*;

/// The widgets on their own, so a test can drive `refresh`/`read` without a
/// window; `build` composes them into pages and presents them.
pub(super) fn widgets() -> Rc<Ui> {
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
            "Tự động, Gạch chân (mặc định), Không gạch chân, Surrounding, 3 mode uinput, 2 mode ForwardKeyEvent, OFF",
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
    let app_rule_hotkey_label = gtk4::Label::new(Some(&t("misc.unassigned", "Chưa gán")));
    app_rule_hotkey_label.add_css_class("dim-label");
    let app_rule_hotkey_edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    app_rule_hotkey_edit.add_css_class("flat");
    app_rule_hotkey_edit.set_tooltip_text(Some(&capture_tooltip));
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
    let macros_text = text_view("");
    let emoji_enabled = switch_row(
        &t("rows.emoji_enabled.title", "Gõ tắt emoji"),
        &t(
            "rows.emoji_enabled.subtitle",
            "Gõ :ten: kèm dấu hai chấm sẽ thành emoji khi hết từ",
        ),
    );
    let emoji_hotkey_label = gtk4::Label::new(Some(&t("misc.unassigned", "Chưa gán")));
    emoji_hotkey_label.add_css_class("dim-label");
    let emoji_hotkey_edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    emoji_hotkey_edit.add_css_class("flat");
    emoji_hotkey_edit.set_tooltip_text(Some(&capture_tooltip));
    let emoji_hotkey_value = Rc::new(RefCell::new(String::new()));
    let emoji_text = text_view("");

    // Custom shortcuts: the list view plus the add row (function dropdown,
    // pencil recorder, add button). The design vector lives in Ui.
    let custom_box = gtk4::ListBox::new();
    custom_box.add_css_class("boxed-list");
    custom_box.set_selection_mode(gtk4::SelectionMode::None);
    let custom_action_labels: Vec<String> = CUSTOM_ACTION_KEYS
        .iter()
        .zip(CUSTOM_ACTION_DEFAULTS)
        .map(|(key, fallback)| t(key.1, fallback))
        .collect();
    let custom_action = gtk4::DropDown::from_strings(
        &custom_action_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    let custom_edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    custom_edit.add_css_class("flat");
    custom_edit.set_tooltip_text(Some(&capture_tooltip));
    let custom_label = gtk4::Label::new(Some(&t("misc.unassigned", "Chưa gán")));
    custom_label.add_css_class("dim-label");
    let custom_value = Rc::new(RefCell::new(String::new()));
    let custom_add = gtk4::Button::with_label(&t("misc.custom_add", "Thêm"));
    custom_add.add_css_class("suggested-action");
    custom_add.set_sensitive(false);
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
    let desktop_icon = switch_row(
        &t("rows.desktop_icon.title", "Hiện trong menu ứng dụng"),
        &t(
            "rows.desktop_icon.subtitle",
            "Icon MăngRS Cấu hình trong lưới ứng dụng của GNOME",
        ),
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
    let menu_open = Rc::new(RefCell::new(HashSet::new()));

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
        "Bật/tắt cả bộ gõ. Khi tắt, mọi phím đi thẳng vào app như chưa cài MăngRS."
    );
    tip!(
        method,
        "tips.method",
        "Kiểu gõ quyết định cách chữ dấu được tạo từ chuỗi phím: Telex (ss → á), VNI (a1 → á) hoặc VIQR."
    );
    tip!(
        mode,
        "tips.mode",
        "Cách hiển thị chữ đang gõ: preedit (gạch chân) dùng được ở mọi app, các mode uinput gửi phím thật nhưng có thể lệch app."
    );
    tip!(
        charset,
        "tips.charset",
        "Bảng mã chữ: Unicode cho mọi dùng đợp thường, các bảng VNI/TCVN3 chỉ cần khi đọc file văn bản cũ."
    );
    tip!(
        modern,
        "tips.modern",
        "Chuẩn đặt dấu mới của Bộ GD-ĐT: hòa thay vì hoà, thuế thay vì thuể."
    );
    tip!(
        free,
        "tips.free",
        "Cho phép dấu đứng tự do trong từ thay vì luôn ở nguyên âm gần nhất."
    );
    tip!(
        auto_correct,
        "tips.auto_correct",
        "Âm tiết không có trong từ điển tiếng Việt sẽ trả về đúng chuỗi phím đã bấm."
    );
    tip!(
        auto_restore,
        "tips.auto_restore",
        "Từ nước ngoài (english, hello) gõ xong tự trả về chuỗi phím gốc thay vì giữ nguyên."
    );
    tip!(
        abbrev,
        "tips.abbrev",
        "Giữ đ nguyên trong từ viết tắt: ddc, ddk, ddt không bị đổi thành đ."
    );
    tip!(
        bypass_url,
        "tips.bypass_url",
        "Từ ngay sau URL/email/path không được thêm dấu, tránh làm hỏng liên kết khi soạn chat."
    );
    tip!(
        w2u,
        "tips.w2u",
        "Gõ w nhận ư: w = ư, wa = ưa... tiện cho bàn phím không có sẵn ư."
    );
    tip!(
        bracket,
        "tips.bracket",
        "Gõ [ và ] nhận ơ và ư theo vị trí trong từ."
    );
    tip!(
        em_dash,
        "tips.em_dash",
        "Hai dấu gạch ngang liên tiếp (--) thành gạch ngang dài (—)."
    );
    tip!(
        auto_capitalize,
        "tips.auto_capitalize",
        "Tự viết hoa chữ cái đầu câu sau dấu chấm, !, ? hoặc Enter."
    );
    tip!(
        double_space,
        "tips.double_space",
        "Nhấn hai dấu cách liên tiếp thành dấu chấm + khoảng cách (thử nghiệm)."
    );
    tip!(
        macro_enabled,
        "tips.macro_enabled",
        "Bật bảng gõ tắt: chuỗi khóa được mở rộng thành nội dung khi gõ xong từ."
    );
    tip!(
        capitalize_macro,
        "tips.capitalize_macro",
        "Khóa viết hoa (Gs) thì nội dung cũng viết hoa theo."
    );
    tip!(
        macro_in_off_mode,
        "tips.macro_in_off_mode",
        "Gõ tắt vẫn hoạt động khi tiếng Việt đang tắt."
    );
    tip!(
        emoji_enabled,
        "tips.emoji_enabled",
        "Gõ :smile: thành emoji. Từ dán ngay sau dấu hai chấm không được thêm dấu khi tính năng này bật (như quy tắc bỏ qua URL); tắt đi nếu bạn thường viết kiểu :từ_viết."
    );
    tip!(
        emoji_hotkey_edit,
        "tips.emoji_hotkey",
        "Phím tắt mở menu chọn emoji kiểu ibus-bamboo: bảng ứng viên hiện ngay tại con trỏ, gõ chữ để tìm, phím số chọn, Esc hoặc click ra chỗ khác đóng."
    );
    tip!(
        hide_underline,
        "tips.hide_underline",
        "Ẩn gạch chân preedit cho những app vẽ gạch chân lỗi (một số game, terminal)."
    );
    tip!(
        debug_log,
        "tips.debug_log",
        "Ghi lại quyết định của engine vào ~/.cache/ibus-mangrs/debug.log để báo lỗi."
    );
    tip!(
        x11_delete,
        "tips.x11_delete",
        "Cách xoá từ ở app X11 trực tiếp: xoá đuôi từ như ibus-bamboo, hoặc chỉ xoá theo báo cáo của app."
    );
    tip!(
        preedit_xim,
        "tips.preedit_xim",
        "Dùng cửa sổ preedit riêng cho app XIM (Java/JetBrains) vốn không hỗ trợ preedit của IBus."
    );
    tip!(
        commit_interval,
        "tips.commit_interval",
        "Nhịp gửi từng chữ ở mode uinput — giảm nếu app bị mất chữ, tăng nếu bị đơ."
    );
    tip!(
        backspace_interval,
        "tips.backspace_interval",
        "Nhịp gửi Backspace ở mode uinput — giảm nếu xoá chậm, tăng nếu app xử lý không kịp."
    );
    tip!(
        post_delay,
        "tips.post_delay",
        "Chờ sau khi viết lại cả từ trước khi gõ chữ tiếp, dành cho app xử lý phím chậm."
    );
    tip!(
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
        app_rule_hotkey_label,
        app_rule_hotkey_edit,
        app_rule_hotkey_value,
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
        macros_text,
        emoji_enabled,
        emoji_hotkey_label,
        emoji_hotkey_edit,
        emoji_hotkey_value,
        emoji_text,
        custom: Rc::new(RefCell::new(Vec::new())),
        custom_box,
        custom_action,
        custom_edit,
        custom_label,
        custom_value,
        custom_add,
        dictionary_enabled,
        dictionary_text,
        rules_box,
        rules_status,
        rules_loading,
        desktop_icon,
        desktop_icon_loading,
        save_cb,
        suppress_save,
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
        menu_open,
    })
}
