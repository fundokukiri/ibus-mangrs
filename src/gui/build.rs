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
        &labels(METHODS),
    );
    // The visible-mode list is the single source for every mode picker in
    // the window: the main combo, the add-rule picker and the rule rows.
    let mode_choices = Config::load().visible_modes();
    let mode = combo_row(
        &t("rows.mode.title", "Chế độ gõ"),
        &t("rows.mode.subtitle", "Cách đưa chữ vào ứng dụng"),
        &mode_labels(&mode_choices),
    );
    // "Chế độ gõ hiển thị": an expander holding one switch per mode. The
    // expander keeps five rows out of the page's main flow.
    let mode_visibility = adw::ExpanderRow::new();
    mode_visibility.set_use_markup(false);
    mode_visibility.set_title(&t("rows.mode_visibility.title", "Chế độ gõ hiển thị"));
    mode_visibility.set_tooltip_text(Some(&t(
        "tips.mode_visibility",
        "Bỏ chọn một chế độ để ẩn nó khỏi menu, phím tắt, luật theo app và bộ chọn chế độ. Chế độ đang dùng sẽ chuyển sang mục còn hiện đầu tiên.",
    )));
    let mut mode_visible_switches = Vec::new();
    for (mode, ..) in MODE_LABELS {
        let switch = switch_row(mode_label(*mode), "");
        mode_visibility.add_row(&switch);
        mode_visible_switches.push(switch);
    }
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
    let extended_finals = switch_row(
        &t("rows.extended_finals.title", "Âm cuối mở rộng"),
        &t(
            "rows.extended_finals.subtitle",
            "g, k, h, nk thay ng, c, nh: nhuwgx → nhữg",
        ),
    );
    let extended_onsets = switch_row(
        &t("rows.extended_onsets.title", "Âm đầu mở rộng"),
        &t(
            "rows.extended_onsets.subtitle",
            "f, j, w làm âm đầu được ghi dấu: fos → fó",
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
    let extra_url_chars_entry = chars_entry();
    extra_url_chars_entry.set_sensitive(false);
    let (extra_url_chars_row, extra_url_chars_switch) = switch_value_row(
        &t("rows.extra_url_chars.title", "Thêm ký tự giữ nguyên dấu"),
        &t(
            "rows.extra_url_chars.subtitle",
            "Ký tự riêng liệt kê ở ô bên phải",
        ),
        &extra_url_chars_entry,
    );
    let w2u = combo_row(
        &t("rows.w2u.title", "W thành ư"),
        &t("rows.w2u.subtitle", "Chọn vị trí áp dụng"),
        &labels(ON_OFF_MODES),
    );
    let bracket = combo_row(
        &t("rows.bracket.title", "[ ] thành ơ ư"),
        &t("rows.bracket.subtitle", "Chọn vị trí áp dụng"),
        &labels(ON_OFF_MODES),
    );
    let backspace_tone = combo_row(
        &t("rows.backspace_tone.title", "Xóa dấu bằng Backspace"),
        &t(
            "rows.backspace_tone.subtitle",
            "Mặc định: xóa dấu trước, chữ giữ lại (hạ thành ha). Hoặc xóa cả dấu và chữ (hạ thành h), hoặc lùi từng phím như unikey.",
        ),
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
        &t(
            "rows.em_dash.subtitle",
            "Hai dấu trừ liên tiếp thành gạch dài (-- thành —)",
        ),
    );
    let auto_capitalize = switch_row(
        &t("rows.auto_capitalize.title", "Tự viết hoa"),
        &t("rows.auto_capitalize.subtitle", "Sau . ! ? và Enter"),
    );
    let auto_capitalize_after_entry = chars_entry();
    auto_capitalize_after_entry.set_sensitive(false);
    let (auto_capitalize_after_row, auto_capitalize_after_switch) = switch_value_row(
        &t(
            "rows.auto_capitalize_after.title",
            "Viết hoa sau ký tự thêm",
        ),
        &t(
            "rows.auto_capitalize_after.subtitle",
            "Ký tự thêm liệt kê ở ô bên phải",
        ),
        &auto_capitalize_after_entry,
    );
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
        &t(
            "rows.macro_valid_words.title",
            "Gõ tắt: nở cả trên từ hợp lệ",
        ),
        &t(
            "rows.macro_valid_words.subtitle",
            "Phím tắt trùng từ thật vẫn nở",
        ),
    );
    let keep_composed_words = switch_row(
        &t("rows.keep_composed_words.title", "Giữ từ khi gõ phím lạ"),
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
        &t("rows.suggest_words.title", "Gợi ý từ (n-gram)"),
        &t(
            "rows.suggest_words.subtitle",
            "Từ ghép khi đang gõ, từ kế tiếp sau dấu cách — cần file mô hình dự đoán",
        ),
    );
    let suggest_display = combo_row(
        &t("rows.suggest_display.title", "Hiện từ đang gõ ở đâu"),
        &t(
            "rows.suggest_display.subtitle",
            "Cả khung gợi ý lẫn ô soạn, hay chỉ trong ô soạn như các bộ gõ cũ",
        ),
        // Order pinned by suggest_display_index: mirror, inline.
        &["Cả khung gợi ý và ô soạn", "Chỉ ô soạn"],
    );
    let preferred_backend = combo_row(
        &t("rows.preferred_backend.title", "Backend ưu tiên"),
        &t(
            "rows.preferred_backend.subtitle",
            "Backend tiêm phím được ưu tiên; không dựng được thì tự rơi về tự động",
        ),
        // Order pinned by preferred_backend_index: auto, uinput, xtest,
        // wlr-virtual-keyboard, libei.
        &["Tự động", "Uinput", "Xtest", "Wlroots", "Libei"],
    );
    // The formats are preset dropdowns like every other combo; a format
    // the old free-text rows saved that matches no preset is appended by
    // `select_format` instead of being rewritten.
    // No subtitle: the row's live hint is the example — a static "Ví dụ"
    // line would duplicate it. The empty default keeps the row one-line
    // unless the owner re-adds a subtitle in gui-content.toml.
    let time_format = combo_row(
        &t("rows.time_format.title", "Định dạng giờ ($TIME)"),
        &t("rows.time_format.subtitle", ""),
        TIME_FORMATS,
    );
    let date_format = combo_row(
        &t("rows.date_format.title", "Định dạng ngày ($DATE)"),
        &t("rows.date_format.subtitle", ""),
        DATE_FORMATS,
    );
    // The item labels are proper names (LANGUAGES), never translated.
    let language = combo_row(
        &t("rows.language.title", "Ngôn ngữ"),
        &t("rows.language.subtitle", ""),
        LANGUAGES,
    );
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
    let debug_log_keys = switch_row(
        &t("rows.debug_log_keys.title", "Log từng phím nhấn"),
        &t(
            "rows.debug_log_keys.subtitle",
            "Mỗi phím một dòng keyval/state — chỉ bật khi gỡ lỗi đường phím (keymap, modifier, hotkey); mặc định tắt để log không đầy rác",
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
    // One pacing row, three columns: the three uinput timings share the
    // merged-row shape — a caption above each spin, dividers between the
    // columns — and the row subtitle carries the shipped defaults.
    let backspace_interval = spin_button(1.0, 100.0, 1.0);
    let post_delay = spin_button(0.0, 500.0, 1.0);
    let pacing_defaults = crate::config::Config::default();
    let pacing_row = action_row(
        &t("rows.pacing.title", "Nhịp uinput (ms)"),
        &t(
            "rows.pacing.subtitle",
            "Khoảng nghỉ giữa các phím ảo. Mặc định: {n}",
        )
        .replace(
            "{n}",
            &format!(
                "{} · {}",
                pacing_defaults.backspace_interval_ms, pacing_defaults.post_delay_ms
            ),
        ),
    );
    // Both merged rows share one column width (see below), so their
    // caption+input clusters start at the same x instead of each row
    // sizing itself.
    let mut merged_columns = Vec::new();
    for (key, fallback, spin) in [
        (
            "rows.backspace_interval.title",
            "Nhịp xoá lùi",
            &backspace_interval,
        ),
        ("rows.post_delay.title", "Độ trễ sau từ", &post_delay),
    ] {
        if !merged_columns.is_empty() {
            pacing_row.add_suffix(&pacing_divider());
        }
        let caption_text = t(key, fallback);
        let column = pacing_column(&caption_text, spin);
        pacing_row.add_suffix(&column);
        merged_columns.push(column);
        let spoken = format!("{} (ms)", caption_text);
        spin.update_property(&[gtk4::accessible::Property::Label(&spoken)]);
    }
    let echo_stale_spin = spin_button(50.0, 1000.0, 10.0);
    echo_stale_spin.set_sensitive(false);
    let (echo_stale_row, echo_stale_switch) = switch_value_row(
        &t("rows.echo_stale.title", "Xả commit khi echo lạc (ms)"),
        &t(
            "rows.echo_stale.subtitle",
            "Thời hạn chờ echo trước khi xả chữ",
        ),
        &echo_stale_spin,
    );
    let idle_commit_spin = spin_button(300.0, 2000.0, 50.0);
    idle_commit_spin.set_sensitive(false);
    let (idle_commit_row, idle_commit_switch) = switch_value_row(
        &t("rows.idle_commit.title", "Chấp nhận từ khi nghỉ gõ (ms)"),
        &t(
            "rows.idle_commit.subtitle",
            "Nghỉ tay lâu thì từ được chấp nhận",
        ),
        &idle_commit_spin,
    );
    let flow_per_key_spin = spin_button(0.0, 500.0, 5.0);
    let flow_backspace_restore_spin = spin_button(0.0, 1000.0, 10.0);
    // The two Flow delays share the pacing row's caption-column shape — a
    // caption above each spin, a divider between — so the group reads as
    // one style. Caption-only row: the captions carry the names, the
    // subtitle the shipped defaults; both knobs stay outside the
    // experimental gate like the pacing knobs they pace with.
    let flow_delay_row = action_row(
        "",
        &t("rows.flow_delay.subtitle", "Mặc định: {n}").replace(
            "{n}",
            &format!(
                "{} · {}",
                pacing_defaults.flow_per_key_delay_ms,
                pacing_defaults.flow_backspace_restore_delay_ms
            ),
        ),
    );
    let mut first_flow_column = true;
    for (caption, spin) in [
        (
            t("rows.flow_per_key_delay.title", "Dấu từng bước (ms)"),
            &flow_per_key_spin,
        ),
        (
            t(
                "rows.flow_backspace_restore_delay.title",
                "Hoàn dấu sau Backspace (ms)",
            ),
            &flow_backspace_restore_spin,
        ),
    ] {
        if !first_flow_column {
            flow_delay_row.add_suffix(&pacing_divider());
        }
        first_flow_column = false;
        let column = pacing_column(&caption, spin);
        // Center the column vertically: the default Fill stretches it to
        // the row's height and top-packs the caption, leaving all the
        // slack below the input.
        column.set_valign(gtk4::Align::Center);
        flow_delay_row.add_suffix(&column);
        merged_columns.push(column);
        // The caption already carries (ms): the spoken label is the caption.
        spin.update_property(&[gtk4::accessible::Property::Label(&caption)]);
    }
    equalize_columns(&merged_columns);
    // The caption column is the row's tallest child: an exact-fit row pins
    // the caption to the top edge and the input to the bottom. Growing the
    // row past its natural height floats the column vertically centered —
    // 12px of air above and below, the same balance the "Thử nghiệm" value
    // controls get from their centered valign.
    let row_height = flow_delay_row.measure(gtk4::Orientation::Vertical, -1).1;
    flow_delay_row.set_size_request(-1, row_height + 24);
    let experimental_features = switch_row(
        &t("rows.experimental_features.title", "Tính năng thử nghiệm"),
        &t(
            "rows.experimental_features.subtitle",
            "Bật để hiện và chạy các tính năng dưới",
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
        "Gạch chân (preedit): chữ gạch chân inline, commit khi hết từ — cần client hỗ trợ preedit, không hỗ trợ thì tự chuyển Surrounding. Surrounding text: mỗi phím commit ngay, đuôi từ được sửa lại qua xóa-văn-bản — cần client báo surrounding-text; client không báo thì đuôi từ được xóa bằng phím chèn (backend) hoặc phím chuyển tiếp thay thế. Flow: phím raw được chuyển thẳng, dấu hiện ngay từng bước, hết từ commit — cần backend chèn phím (uinput/Wayland), không có thì tự chạy như Forward Backspace. Forward Backspace: mỗi phím commit ngay, sửa đuôi bằng phím Backspace chuyển tiếp qua IBus — không bao giờ tự chuyển mode, chạy được ở mọi client kể cả terminal/XIM. OFF: tắt xử lý tiếng Việt, macro vẫn chạy."
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
        extended_finals,
        "tips.extended_finals",
        "Cho phép g, k, h, nk đứng thay cho âm cuối ng, c, nh nên dấu thanh đúng dù gõ trước hay sau âm cuối (nhuwgx → nhữg). Từ tiếng Anh có thể đổi (book → bôk); mặc định tắt."
    );
    tip!(
        extended_onsets,
        "tips.extended_onsets",
        "Cho phép f, j, w đứng làm âm đầu và nhận dấu thanh (fos → fó). Mặc định tắt: không từ tiếng Việt nào bắt đầu bằng các chữ này, tắt giúp từ tiếng Anh (fire, water) không bị ghi dấu oan."
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
        backspace_interval,
        "tips.backspace_interval",
        "Nhịp gửi Backspace ở mode uinput — giảm nếu xoá chậm, tăng nếu app xử lý không kịp."
    );
    tip!(
        post_delay,
        "tips.post_delay",
        "Chờ sau khi viết lại cả từ trước khi gõ chữ tiếp, dành cho app xử lý phím chậm."
    );
    // Every "Thử nghiệm" row: the tooltip names the feature it belongs to.
    tip!(
        experimental_features,
        "tips.experimental_features",
        "Công tắc chính của tab Thử nghiệm: tắt thì mọi hàng dưới bị khóa và bộ gõ chạy theo mặc định; giá trị đã chỉnh vẫn được lưu để dùng lại khi bật lại."
    );
    tip!(
        macro_valid_words,
        "tips.macro_valid_words",
        "Tính năng Gõ tắt: phím tắt trùng một từ tiếng Việt thật (vd “cam”) không nở để không cản gõ từ thường. Bật thì gõ tắt vẫn thắng — muốn gõ từ thật, Backspace một phím rồi gõ lại phím đó."
    );
    tip!(
        keep_composed_words,
        "tips.keep_composed_words",
        "Sau một từ hoàn chỉnh, phím chữ mà từ không nhận được sẽ nối vào từ thay vì đập cả từ về chuỗi phím gốc (tùng + g → tùngg). Đổi giá khi bật: từ tiếng Anh dạng soft sẽ ở lại sòt, vì từ được giữ miễn khôi phục cuối từ. Mặc định tắt."
    );
    tip!(
        keep_repeats_raw,
        "tips.keep_repeats_raw",
        "Dành riêng cho chế độ Flow: giữ một phím ở Flow chỉ lặp được một lần rồi dừng ở ô (oo → ô rồi đứng im), bật thì phím giữ chảy nguyên bản oooo… như bàn phím thường. Các chế độ gõ khác không gặp lỗi này nên không cần bật. Đổi giá khi bật: hai phím giống nhau gõ cách nhau quá ~0,4 giây sẽ ở lại nguyên bản (tooi thay vì tôi). Mặc định tắt."
    );
    tip!(
        suggest_words,
        "tips.suggest_words",
        "Từ đang gõ hiện trong cả khung gợi ý lẫn ô soạn (mặc định), hoặc chỉ ô soạn. Danh sách lấy từ mô hình dự đoán; khi mô hình không biết tiền tố, bảng âm tiết có sẵn sửa từ gõ sai một ký tự ('sinhh' thấy 'sinh'): gõ chữ cái đầu tiên là gợi ý đã chạy; các gợi ý xếp theo độ thông dụng trong mô hình, từ đơn và từ ghép chấm chung một thang ('sinh' thấy 'sinh viên' đứng đầu vì cụm đó phổ biến hơn các từ đơn hiếm), emoji luôn nằm ô 5; sau khi một từ đổ bằng dấu cách, danh sách chỉ còn những từ mô hình đã thấy đứng sau nó ('xin ' rồi gõ 'c' thấy 'cho', 'cảm', 'cấp', 'chào' — không có từ lạ nào xen vào); gõ tiếp chữ cái sẽ dựng lại danh sách theo cả chữ đó và từ đứng trước. Từ tiếng Anh vào bằng prefix ASCII thuần, emoji vào qua từ khóa. phím ` lấy ô đầu (muốn gõ ký tự ` thì nhấn hai lần: lần đầu là chọn, lần sau là ký tự; Shift+` với ~ cũng vậy), Alt+1–5 chọn ô tương ứng, hoặc click thẳng dòng gợi ý — từ được chèn kèm dấu cách; số gõ thường vẫn vào từ như bình thường; lật trang bằng nút ◀▶ trên khung hoặc Ctrl+, / Ctrl+., phím Escape vẫn kết thúc từ như thường, mũi tên giữ nguyên vai trò di chuyển con trỏ. Chạy ở chế độ Preedit và các chế độ có kênh xóa chữ (Surrounding, Forward Backspace, Flow) — một lựa chọn sẽ xóa chữ app đang giữ trước khi commit. Dùng được ở mọi preset — phím chọn là Alt+số và phím ` nên phím số làm dấu của preset không bị giành. Cần file mô hình dự đoán đã cài; không có mô hình thì chỉ từ khóa emoji mở được dải gợi ý. Chỗ hiện từ đang gõ đổi bằng hàng 'Hiện từ đang gõ ở đâu' ngay dưới. Những từ anh chọn được ghi lại để ưu tiên lần sau — nút 'Xoá dữ liệu đã học' nằm ở trang Cài đặt khác, mục Nâng cao. Mặc định tắt, thuộc nhóm tính năng thử nghiệm."
    );
    tip!(
        suggest_display,
        "tips.suggest_display",
        "Chọn chỗ hiện phần đang gõ khi dải gợi ý mở: Cả khung gợi ý và ô soạn (mặc định) — từ hiện ở cả hai, gõ tiếp hay xóa đều cập nhật đồng bộ; Chỉ ô soạn — preedit nằm trong văn bản như các bộ gõ cũ, dải gợi ý hiện bên cạnh. Chỉ có tác dụng khi 'Gợi ý từ (n-gram)' đang bật."
    );
    tip!(
        preferred_backend,
        "tips.preferred_backend",
        "Chọn backend tiêm phím được ưu tiên: uinput là bàn phím ảo của nhân, XTest chỉ có trên X11, wlroots cho Hyprland/sway, libei qua GNOME (đã đo thấy rủi ro crash — chỉ để thử nghiệm). Backend chọn được đặt lên đầu danh sách ưu tiên; không dựng được thì bộ gõ tự rơi về thứ tự theo môi trường. Tự động là hành vi mặc định."
    );
    tip!(
        extra_url_chars_row,
        "tips.extra_url_chars",
        "Tính năng Không dấu sau URL (mặc định áp dụng cho / : @ . _ ~ = # & % ?): hàng này thêm ký tự riêng vào bộ đó. Bật công tắc trái, liệt kê ký tự vào ô phải; dấu - bị loại để giữ -- thành gạch dài."
    );
    tip!(
        auto_capitalize_after_row,
        "tips.auto_capitalize_after",
        "Tính năng Tự viết hoa (sau . ! ? và Enter): hàng này thêm ký tự riêng vào bộ đó. Bật công tắc trái, liệt kê ký tự vào ô phải; không nhận chữ, số, khoảng trắng."
    );
    tip!(
        echo_stale_row,
        "tips.echo_stale",
        "Bảo vệ xả commit trên Wayland: chờ các phím Backspace ảo (echo) quay về trước khi đổ chữ đã commit vào app. Số ms bên phải là thời hạn chờ — quá hạn thì xả luôn để chữ không treo. Mặc định 150 ms."
    );
    tip!(
        idle_commit_row,
        "tips.idle_commit",
        "Tính năng Chấp nhận từ khi nghỉ gõ: dừng tay quá lâu trên một từ đang gạch chân thì từ đó được commit luôn, như bấm dấu cách. Số ms bên phải là ngưỡng nghỉ tay."
    );
    // The two Flow tips sit on their spins now: one row carries both knobs.
    tip!(
        flow_per_key_spin,
        "tips.flow_per_key",
        "Flow hiện chữ có dấu ngay trong lúc gõ (ee → ê) thay vì giữ phím raw đến hết từ — luôn bật, không có công tắc. Gõ rất nhanh các bước được gộp, chữ hiện khi nghỉ tay. Số ms trong ô là trễ đổ chữ có dấu vào văn bản — thấp là dấu hiện sớm hơn, cao là gõ mượt hơn. 0–500 ms, mặc định 20. Quá thấp có thể làm lệch thứ tự chữ trên Wayland."
    );
    tip!(
        flow_backspace_restore_spin,
        "tips.flow_backspace_restore",
        "Hoàn dấu sau Backspace, chuẩn ở Flow: gõ chào rồi gõ thêm f (dấu bị gỡ, màn hình thành chaof), bấm Backspace thì dấu quay lại (chào) — theo đúng cài đặt “Xóa dấu bằng Backspace”: Unikey và Xóa dấu trước trả lại dấu, Cả ký tự thì xoá một ký tự. Số ms trong ô là thời gian chờ sau Backspace rồi mới sửa màn hình (0–1000, mặc định 150)."
    );
    tip!(
        nav_key_while_preedit,
        "tips.nav_key_while_preedit",
        "Điều hướng khi còn từ: khi từ vẫn còn gạch chân, phím mũi tên đầu tiên mặc định đưa caret chạy ngay (kiểu ibus-bamboo) — commit vẫn nằm trong cùng lượt trả về, tới trước phím mũi tên. Nếu chữ bị chèn sai chỗ ở một app nào, chọn lại Nuốt phím đầu: lần nhấn đầu chỉ commit từ, lần hai mới di chuyển."
    );

    Rc::new(Ui {
        enabled,
        method,
        mode,
        mode_choices: Rc::new(RefCell::new(mode_choices)),
        mode_visibility,
        mode_visible_switches,
        mode_visibility_loading: Rc::new(RefCell::new(false)),
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
        extra_url_chars_row,
        extra_url_chars_switch,
        extra_url_chars_entry,
        w2u,
        bracket,
        backspace_tone,
        nav_key_while_preedit,
        em_dash,
        auto_capitalize,
        auto_capitalize_after_row,
        auto_capitalize_after_switch,
        auto_capitalize_after_entry,
        double_space,
        macro_enabled,
        capitalize_macro,
        macro_in_off_mode,
        macro_valid_words,
        keep_composed_words,
        keep_repeats_raw,
        suggest_words,
        suggest_display,
        preferred_backend,
        time_format,
        date_format,
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
        rule_add_mode_pick,
        rules_overlay,
        rules_loading,
        desktop_icon,
        show_toasts,
        desktop_icon_loading,
        save_cb,
        suppress_save,
        self_saved_mtime: Cell::new(None),
        sidebar_select: RefCell::new(None),
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
        debug_log_keys,
        x11_delete,
        preedit_xim,
        pacing_row,
        backspace_interval,
        post_delay,
        echo_stale_row,
        echo_stale_switch,
        echo_stale_spin,
        idle_commit_row,
        idle_commit_switch,
        idle_commit_spin,
        flow_delay_row,
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
