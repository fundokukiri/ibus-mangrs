//! `widgets()`: every row of every page, built without a window so
//! headless tests can drive `refresh`/`read` directly.

use super::*;

/// The widgets on their own, so a test can drive `refresh`/`read` without a
/// window; `build` composes them into pages and presents them.
pub(super) fn widgets() -> Rc<Ui> {
    let enabled = switch_row(
        &t("rows.enabled.title", "Bật tiếng Việt"),
        // {page} is the macros page's real sidebar label, so the sentence
        // cannot drift from the sidebar.
        &t(
            "rows.enabled.subtitle",
            "Tắt để gõ thẳng mọi phím. Gõ tắt vẫn chạy nếu bật ở trang Gõ tắt.",
        )
        .replace("{page}", &t("pages.macros", "Gõ tắt")),
    );
    let method = combo_row(
        &t("rows.method.title", "Kiểu gõ"),
        &t("rows.method.subtitle", "Cách gõ dấu tiếng Việt"),
        METHODS,
    );
    let mode = combo_row(
        &t("rows.mode.title", "Chế độ gõ"),
        &t("rows.mode.subtitle", "Cách đưa chữ vào ứng dụng"),
        MODES,
    );
    let charset = combo_row(
        &t("rows.charset.title", "Bảng mã"),
        &t("rows.charset.subtitle", "Unicode và 10 bảng mã cũ"),
        CHARSETS,
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
    let extra_url_chars_switch = switch_row(
        &t("rows.extra_url_chars.title", "Thêm ký tự giữ nguyên dấu"),
        &t(
            "rows.extra_url_chars.subtitle",
            "Từ đứng ngay sau ký tự bên dưới cũng giữ nguyên phím thô",
        ),
    );
    let extra_url_chars_entry = adw::EntryRow::builder()
        .use_markup(false)
        .title(t("rows.extra_url_chars_entry.title", "Ký tự riêng"))
        .show_apply_button(false)
        .build();
    extra_url_chars_entry.set_sensitive(false);
    let w2u = combo_row(
        &t("rows.w2u.title", "W thành ư"),
        &t("rows.w2u.subtitle", "Chọn vị trí áp dụng"),
        ON_OFF_MODES,
    );
    let bracket = combo_row(
        &t("rows.bracket.title", "[ ] thành ơ ư"),
        &t("rows.bracket.subtitle", "Chọn vị trí áp dụng"),
        ON_OFF_MODES,
    );
    let backspace_tone = combo_row(
        &t("rows.backspace_tone.title", "Xóa dấu bằng Backspace"),
        &t(
            "rows.backspace_tone.subtitle",
            "Mặc định: xóa dấu trước, chữ giữ lại (hạ thành ha). Hoặc xóa cả dấu và chữ (hạ thành h), hoặc lùi từng phím như unikey.",
        ),
        BACKSPACE_TONE_MODES,
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
    let auto_capitalize_after_switch = switch_row(
        &t(
            "rows.auto_capitalize_after.title",
            "Viết hoa sau ký tự thêm",
        ),
        &t(
            "rows.auto_capitalize_after.subtitle",
            "Cùng bộ . ! ? + Enter: từ ngay sau ký tự bên dưới cũng viết hoa",
        ),
    );
    let auto_capitalize_after_entry = adw::EntryRow::builder()
        .use_markup(false)
        .title(t(
            "rows.auto_capitalize_after_entry.title",
            "Ký tự kích hoạt",
        ))
        .show_apply_button(false)
        .build();
    auto_capitalize_after_entry.set_sensitive(false);
    let double_space = switch_row(
        &t(
            "rows.double_space.title",
            "Nhấn “Space” hai lần thành dấu chấm",
        ),
        &t("rows.double_space.subtitle", "Hai dấu cách thành dấu chấm"),
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
        &t("rows.macro_valid_words.title", "Gõ tắt nở cả trên từ hợp lệ"),
        &t(
            "rows.macro_valid_words.subtitle",
            "Phím tắt trùng từ thật (vd “cam”) sẽ nở thay vì giữ nguyên — Backspace một phím để gõ từ thật",
        ),
    );
    // The formats live in native EntryRows (the row carries the title, the
    // old subtitle examples moved to the page as tooltips — EntryRow has
    // no placeholder in this libadwaita). Markup off before the title, as
    // in every row helper.
    let time_format = adw::EntryRow::builder()
        .use_markup(false)
        .title(t("rows.time_format.title", "Định dạng giờ ($TIME)"))
        .show_apply_button(false)
        .build();
    let date_format = adw::EntryRow::builder()
        .use_markup(false)
        .title(t("rows.date_format.title", "Định dạng ngày ($DATE)"))
        .show_apply_button(false)
        .build();
    let macros_text = text_view("");
    let emoji_enabled = switch_row(
        &t("rows.emoji_enabled.title", "Gõ tắt emoji"),
        &t(
            "rows.emoji_enabled.subtitle",
            "Gõ :ten: kèm dấu hai chấm sẽ thành emoji khi hết từ",
        ),
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
    // The page's toast host: built here, returned by the page as its
    // wrapper, so toasts fired from the row editors are visible.
    let rules_overlay = adw::ToastOverlay::new();
    // The input is a native EntryRow; the verdict is the subtitle of a
    // read-only result row underneath it, with a status icon up front.
    // EntryRow has no placeholder in this libadwaita, so the examples live
    // in the tooltip.
    let rule_preview_input = adw::EntryRow::builder()
        .title(t("rows.rule_name.title", "Tên app"))
        .build();
    rule_preview_input.set_tooltip_text(Some(&t(
        "rows.rule_preview_input.tooltip",
        "Nhập tên app rồi Enter: cho biết luật nào sẽ khớp. Ví dụ: Firefox / wayland:Firefox / xim",
    )));
    let rule_preview = action_row(&t("rules.result_title", "Kết quả"), "");
    // The verdict icon swaps between search (nothing typed), ok (match)
    // and remove (no match); `update_rule_preview` drives it.
    let rule_preview_icon = gtk4::Image::from_icon_name("system-search-symbolic");
    rule_preview_icon.add_css_class("dim-label");
    rule_preview.add_prefix(&rule_preview_icon);
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
            "Icon MăngRS Cấu hình trong lưới ứng dụng của GNOME",
        ),
    );
    let show_toasts = switch_row(
        &t("rows.show_toasts.title", "Thông báo nhanh"),
        &t(
            "rows.show_toasts.subtitle",
            "Bong bóng nhỏ khi thêm, xóa, xuất, nhập, cài extension. Tắt để im lặng — lỗi vẫn hiện hộp thoại",
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
    // The debug-log path reads as a path: markup is on so the subtitle can
    // wrap it in <tt> (the text carries no & — markup is safe here).
    let debug_log = adw::SwitchRow::new();
    debug_log.set_use_markup(true);
    debug_log.set_title(&t("rows.debug_log.title", "Ghi log gỡ lỗi"));
    debug_log.set_subtitle(&t(
        "rows.debug_log.subtitle_markup",
        "Ghi quyết định của engine vào <tt>~/.cache/ibus-mangrs/debug.log</tt>",
    ));
    let debug_log_text = switch_row(
        &t("rows.debug_log_text.title", "Log cả nội dung đang gõ"),
        &t(
            "rows.debug_log_text.subtitle",
            "Ghi cả nội dung đang gõ vào log — chỉ bật khi tái hiện lỗi mất chữ/sai ký tự, và không đính kèm log này lên issue",
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
    let clipboard_restore_switch = switch_row(
        &t("rows.clipboard_restore.title", "Tùy chỉnh hồi clipboard"),
        &t(
            "rows.clipboard_restore.subtitle",
            "Đổi khoảng chờ trả lại clipboard cũ sau khi dán — chỉ tác dụng khi kênh là Clipboard",
        ),
    );
    let clipboard_restore_spin = spin_row(
        &t(
            "rows.clipboard_restore_spin.title",
            "Hồi clipboard sau (ms)",
        ),
        &t(
            "rows.clipboard_restore_spin.subtitle",
            "100–5000 ms. Mặc định: 600",
        ),
        100.0,
        5000.0,
        50.0,
    );
    clipboard_restore_spin.set_sensitive(false);
    let echo_stale_switch = switch_row(
        &t("rows.echo_stale.title", "Xả commit khi echo lạc"),
        &t(
            "rows.echo_stale.subtitle",
            "Chờ bao lâu thì bỏ đợi phím Backspace ảo quay về mà xả chữ — tăng khi dấu nhấp nháy rồi trả lại, giảm khi chữ hiện trễ",
        ),
    );
    let echo_stale_spin = spin_row(
        &t("rows.echo_stale_spin.title", "Xả sau (ms)"),
        &t(
            "rows.echo_stale_spin.subtitle",
            "50–1000 ms. Mặc định: 150. Quá thấp dễ mất chữ, quá cao làm chữ hiện trễ",
        ),
        50.0,
        1000.0,
        10.0,
    );
    echo_stale_spin.set_sensitive(false);
    let idle_commit_switch = switch_row(
        &t("rows.idle_commit.title", "Chấp nhận từ khi nghỉ gõ"),
        &t(
            "rows.idle_commit.subtitle",
            "Dừng tay quá lâu trên một từ có gạch chân thì từ đó được chấp nhận luôn, như bấm dấu cách",
        ),
    );
    let smooth_per_key_switch = switch_row(
        &t("rows.smooth_per_key.title", "Hiện dấu từng bước ở Flow"),
        &t(
            "rows.smooth_per_key.subtitle",
            "Flow/Turbo hiện chữ có dấu ngay trong lúc gõ (ee → ê) thay vì giữ phím raw đến hết từ — mỗi phím trễ ~60 ms; gõ rất nhanh các bước được gộp, chữ hiện khi nghỉ tay; Select không đổi",
        ),
    );
    let idle_commit_spin = spin_row(
        &t(
            "rows.idle_commit_spin.title",
            "Nghỉ bao lâu thì chấp nhận (ms)",
        ),
        &t(
            "rows.idle_commit_spin.subtitle",
            "300–2000 ms. Mặc định: 700",
        ),
        300.0,
        2000.0,
        50.0,
    );
    idle_commit_spin.set_sensitive(false);
    let experimental_features = switch_row(
        &t("rows.experimental_features.title", "Tính năng thử nghiệm"),
        &t(
            "rows.experimental_features.subtitle",
            "Hiện các tính năng đang thử nghiệm trên các trang. Tắt sẽ ẩn và tắt hết chúng; giá trị đã chỉnh vẫn được giữ để dùng lại khi bật",
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
        backspace_tone,
        "tips.backspace_tone",
        "Xóa dấu trước (mặc định): hạ thành ha, rồi h. Xóa cả dấu và chữ: hạ thành h, việt thành việc rồi vi — mỗi Backspace xóa đúng một ký tự trên màn hình. Unikey: lùi qua từng phím đã gõ."
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
        abbrev,
        bypass_url,
        extra_url_chars_switch,
        extra_url_chars_entry,
        w2u,
        bracket,
        backspace_tone,
        em_dash,
        auto_capitalize,
        auto_capitalize_after_switch,
        auto_capitalize_after_entry,
        double_space,
        macro_enabled,
        capitalize_macro,
        macro_in_off_mode,
        macro_valid_words,
        time_format,
        date_format,
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
        rules_overlay,
        rules_loading,
        desktop_icon,
        show_toasts,
        desktop_icon_loading,
        save_cb,
        suppress_save,
        rule_preview_input,
        rule_preview,
        rule_preview_icon,
        rule_pill,
        rule_name_entry,
        rule_copy_btn,
        rule_apply_btn,
        rule_add_btn,
        rule_focus_value,
        hide_underline,
        debug_log,
        debug_log_text,
        x11_delete,
        preedit_xim,
        commit_interval,
        backspace_interval,
        post_delay,
        unicode_channel,
        clipboard_restore_switch,
        clipboard_restore_spin,
        echo_stale_switch,
        echo_stale_spin,
        idle_commit_switch,
        idle_commit_spin,
        smooth_per_key_switch,
        experimental_features,
        menu,
        menu_box,
        menu_open,
        menu_overlay,
        menu_focus_target,
    })
}
