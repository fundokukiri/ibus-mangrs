//! The window shell: the Adwaita navigation split view (sidebar + content),
//! the restart banner and toasts, page accelerators, and the wiring between
//! every control and the save funnel. The page contents themselves still
//! live here and move to `pages/*.rs` one commit at a time.

use super::*;

#[allow(deprecated)]
pub(super) fn build(app: &adw::Application, initial_page: Option<&str>) {
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

    // Quick add for App Rules: pick a client the engine has seen or type a
    // name, pick a mode, append one rule row.
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
    client_combo.set_tooltip_text(Some("Các app engine đã từng thấy"));
    let rule_pattern = Entry::builder()
        .placeholder_text("hoặc nhập tên app")
        .hexpand(true)
        .build();
    rule_pattern.set_tooltip_text(Some(
        "Tên pattern: khớp một phần, không phân biệt hoa/thường",
    ));
    let rule_mode = drop_down(MODES, 1);
    let add_btn = gtk4::Button::with_label("Thêm luật");
    let add_row = gtk4::Box::new(Orientation::Horizontal, 8);
    add_row.set_margin_start(10);
    add_row.set_margin_end(10);
    add_row.set_margin_top(8);
    add_row.set_margin_bottom(8);
    add_row.append(&client_combo);
    add_row.append(&rule_pattern);
    add_row.append(&rule_mode);
    add_row.append(&add_btn);
    // The clicked handler is connected after the window exists, so a failed
    // save can surface in the window (see the save block below).

    // The focused-app chip: the same extension call the engine makes. The
    // settings window itself is focused while the user edits, so the useful
    // refresh happens on focus leave — the window the user just switched to
    // is the one they came here to target.
    let focus_use_btn = gtk4::Button::with_label("Dùng tên này");
    focus_use_btn.set_tooltip_text(Some("Điền tên đang hiện vào ô thêm luật bên dưới"));
    let focus_row = gtk4::Box::new(Orientation::Horizontal, 8);
    focus_row.set_margin_start(10);
    focus_row.set_margin_end(10);
    focus_row.set_margin_top(8);
    focus_row.set_margin_bottom(4);
    let focus_title = gtk4::Label::new(Some("App đang focus:"));
    focus_row.append(&focus_title);
    focus_row.append(&ui.rule_focus);
    focus_row.append(&focus_use_btn);

    // The rule rows flow with the page (no inner scroll cap): every rule
    // stays visible while the page itself scrolls.
    let preview_row = gtk4::Box::new(Orientation::Horizontal, 8);
    preview_row.set_margin_start(10);
    preview_row.set_margin_end(10);
    preview_row.set_margin_top(8);
    let preview_title = gtk4::Label::new(Some("Thử luật:"));
    preview_row.append(&preview_title);
    preview_row.append(&ui.rule_preview_input);
    let preview_result_row = gtk4::Box::new(Orientation::Horizontal, 8);
    preview_result_row.set_margin_start(10);
    preview_result_row.set_margin_end(10);
    preview_result_row.set_margin_bottom(8);
    preview_result_row.append(&ui.rule_preview);

    let export_btn = gtk4::Button::with_label("Xuất…");
    let import_btn = gtk4::Button::with_label("Nhập…");

    // ===== Page 1: Chung — the everyday page, on its own preferences file.
    let typing_page = pages::typing::build(&ui);

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

    // ===== Page 5: Luật =====
    // A whole page for the per-app rules: the editor needs the room, the
    // rule rows flow with the page scroll instead of a cramped inner box,
    // and it keeps the "one question per tab" split.
    let rules_page = gtk4::Box::new(Orientation::Vertical, 6);
    section(
        &rules_page,
        "Luật theo ứng dụng",
        "Luật đầu tiên thắng. Tên app khớp một phần, không phân biệt hoa/thường; none = app không báo tên. Đủ 10 chế độ gõ.",
    );
    rules_page.append(&card_widget(&focus_row));
    rules_page.append(&card_widget(&add_row));
    rules_page.append(&card_widget(&ui.rules_box));
    rules_page.append(&card(vec![preview_row, preview_result_row]));
    hint(
        &rules_page,
        "Chỉ app dùng GTK IM module mới báo tên riêng cho engine; app Wayland hiện là gnome-shell, app X11/khác là none. Cài extension MăngRS Focus (data/gnome-shell-extension) để nhận tên app Wayland-native: Chrome, Firefox, … Nhãn App đang focus lấy tên từ extension — chuyển sang app đích rồi quay lại cài đặt, nó tự cập nhật. Danh sách thêm nhanh là các app engine đã từng thấy, không phải mọi app đang chạy.",
    );

    // ===== Page 6: Tinh chỉnh =====
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

    // ===== Page 7: Giới thiệu =====
    let about = gtk4::Box::new(Orientation::Vertical, 6);

    // Logo is embedded at compile time: the GUI has no installed asset dir.
    const LOGO: &[u8] = include_bytes!("../../data/logo.png");
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

    // Content: one page per section, crossfading. Stack names are stable —
    // the engine opens pages with `--page=<name>`.
    // (stack name, sidebar label, symbolic icon from the icon theme)
    const PAGES: [(&str, &str, &str); 7] = [
        ("typing", "Chung", "input-keyboard-symbolic"),
        ("tone", "Dấu & tự động", "tools-check-spelling-symbolic"),
        ("macros", "Gõ tắt", "insert-text-symbolic"),
        ("data", "Dữ liệu", "folder-documents-symbolic"),
        ("rules", "Luật", "view-list-symbolic"),
        ("advanced", "Tinh chỉnh", "emblem-system-symbolic"),
        ("about", "Giới thiệu", "help-about-symbolic"),
    ];
    let page_boxes: [&gtk4::Widget; 7] = [
        typing_page.upcast_ref(),
        tone.upcast_ref(),
        macro_box.upcast_ref(),
        data_box.upcast_ref(),
        rules_page.upcast_ref(),
        advanced.upcast_ref(),
        about.upcast_ref(),
    ];
    // Pages already on adw::PreferencesPage scroll internally; the plain gtk
    // boxes keep the width-clamped scroll wrapper until they convert.
    const ADW_PAGES: [&str; 1] = ["typing"];
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_vexpand(true);
    stack.set_hexpand(true);
    for ((name, title, _icon), content) in PAGES.iter().zip(page_boxes) {
        if ADW_PAGES.contains(name) {
            stack.add_titled(content, Some(name), title);
            continue;
        }
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

    // --- Adwaita shell: sidebar + content -----------------------------------
    // GNOME Settings layout: a navigation sidebar picks the page, the
    // content toolbar view holds the header (dynamic title), the restart
    // banner and the stack. Toasts overlay the content.
    let split_view = adw::NavigationSplitView::new();
    split_view.set_sidebar_width_fraction(0.28);

    // The restart banner replaces the floating button: it stays visible
    // under the header until the restart happens (or the user closes it) —
    // every control already writes straight to config.toml, so the only
    // thing left for the user to do is restart IBus.
    let restart_banner = adw::Banner::builder()
        .title("Đã lưu — khởi động lại IBus để áp dụng toàn bộ?")
        .button_label("Khởi động lại IBus")
        .revealed(false)
        .build();

    let title_label = gtk4::Label::new(Some("Chung"));
    title_label.add_css_class("title");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title_label));
    let sidebar_btn = gtk4::Button::from_icon_name("sidebar-show-symbolic");
    sidebar_btn.set_visible(false);
    header.pack_start(&sidebar_btn);

    // Sidebar rows in GNOME Settings groups: the everyday pages, then the
    // advanced ones, then About — separated by non-selectable rows.
    let sidebar_list = gtk4::ListBox::new();
    sidebar_list.add_css_class("navigation-sidebar");
    let mut page_rows: Vec<(String, gtk4::ListBoxRow)> = Vec::new();
    for (name, title, icon) in PAGES {
        if name == "rules" || name == "about" {
            let sep_row = gtk4::ListBoxRow::new();
            sep_row.set_selectable(false);
            sep_row.set_activatable(false);
            let sep = gtk4::Box::new(Orientation::Vertical, 0);
            sep.set_margin_top(6);
            sep.set_margin_bottom(6);
            sep.append(&gtk4::Separator::new(Orientation::Horizontal));
            sep_row.set_child(Some(&sep));
            sidebar_list.append(&sep_row);
        }
        let row = gtk4::ListBoxRow::new();
        let h = gtk4::Box::new(Orientation::Horizontal, 12);
        h.append(&gtk4::Image::from_icon_name(icon));
        h.append(&gtk4::Label::new(Some(title)));
        row.set_child(Some(&h));
        // The row carries its stack name; the selection handler reads it.
        row.set_widget_name(name);
        sidebar_list.append(&row);
        page_rows.push((name.to_string(), row.clone()));
    }
    {
        let stack = stack.clone();
        let title_label = title_label.clone();
        let split_view = split_view.clone();
        sidebar_list.connect_row_selected(move |_, some_row| {
            let Some(row) = some_row else {
                return;
            };
            let name = row.widget_name().to_string();
            let title = PAGES
                .iter()
                .find(|(n, _, _)| *n == name)
                .map(|(_, t, _)| *t)
                .unwrap_or("MăngRS");
            stack.set_visible_child_name(&name);
            title_label.set_text(title);
            // Picking a page while collapsed shows the content again, like
            // GNOME Settings on a phone-shaped window.
            if split_view.is_collapsed() {
                split_view.set_show_content(true);
            }
        });
    }

    let sidebar_header = adw::HeaderBar::new();
    let sidebar_title = gtk4::Label::new(Some("MăngRS"));
    sidebar_title.add_css_class("title");
    sidebar_header.set_title_widget(Some(&sidebar_title));
    let sidebar_box = gtk4::Box::new(Orientation::Vertical, 0);
    sidebar_box.append(&sidebar_header);
    sidebar_box.append(&sidebar_list);
    let sidebar_page = adw::NavigationPage::builder()
        .title("MăngRS")
        .child(&sidebar_box)
        .build();

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&stack));
    let content_toolbar = adw::ToolbarView::new();
    content_toolbar.add_top_bar(&header);
    content_toolbar.add_top_bar(&restart_banner);
    content_toolbar.set_content(Some(&toast_overlay));
    let content_page = adw::NavigationPage::builder()
        .title("MăngRS")
        .child(&content_toolbar)
        .build();

    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MăngRS")
        .default_width(980)
        .default_height(720)
        .content(&split_view)
        .build();
    // X11/XWayland reads the icon from the window; Wayland derives it from
    // the app id and the installed desktop file instead.
    window.set_icon_name(Some("ibus-mangrs"));

    // Collapse the sidebar into an overlay pattern on narrow widths: the
    // toggle button (visible only while collapsed) flips between the page
    // and the sidebar list. NavigationSplitView has no manual show-sidebar —
    // collapsed mode is managed through show-content.
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Px,
    ));
    bp.add_setter(&split_view, "collapsed", Some(&true.to_value()));
    bp.add_setter(&split_view, "show-content", Some(&true.to_value()));
    bp.add_setter(&sidebar_btn, "visible", Some(&true.to_value()));
    window.add_breakpoint(bp);
    {
        let split_view = split_view.clone();
        sidebar_btn.connect_clicked(move |_| {
            split_view.set_show_content(!split_view.shows_content());
        });
    }

    // Keyboard: close and quit, plus Alt+1..7 to jump straight to a page.
    app.set_accels_for_action("window.close", &["<Control>w"]);
    let quit = gtk4::gio::SimpleAction::new("quit", None);
    {
        let app = app.clone();
        quit.connect_activate(move |_, _| app.quit());
    }
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    for (i, (_, row)) in page_rows.iter().enumerate() {
        let action = gtk4::gio::SimpleAction::new(&format!("page-{}", i + 1), None);
        {
            let list = sidebar_list.clone();
            let row = row.clone();
            action.connect_activate(move |_, _| list.select_row(Some(&row)));
        }
        window.add_action(&action);
        app.set_accels_for_action(&format!("win.page-{}", i + 1), &[&format!("<Alt>{i}")]);
    }

    // The focused-app chip refreshes when the window loses focus: at that
    // moment the app the user switched to becomes focused, and that is the
    // app they came here to target. No refresh at startup — the settings
    // window itself is the focused window then, which names nothing useful.
    {
        let ui = ui.clone();
        let controller = gtk4::EventControllerFocus::new();
        controller.connect_leave(move |_| {
            refresh_focused_app(&ui.rule_focus, &ui.rule_focus_value);
        });
        window.add_controller(controller);
    }

    // Open a specific page when spawned with `--page=<name>` (menu items).
    // Old page names from earlier layouts are mapped onto the current ones.
    let wanted = match initial_page {
        Some("general") | None => "typing",
        Some("dictionary") | Some("backup") => "data",
        // The old "apps" page was the app rules page.
        Some("apps") => "rules",
        Some(other) => other,
    };
    let (_, wanted_row) = page_rows
        .iter()
        .find(|(n, _)| n == wanted)
        .unwrap_or(&page_rows[0]);
    sidebar_list.select_row(Some(wanted_row));

    // Transitional CSS for the pages that are still plain gtk boxes (the
    // group-card look). It dies with the last page conversion; the shell
    // itself uses only libadwaita styling.
    let css = gtk4::CssProvider::new();
    css.load_from_data(
        ".group-card { background: alpha(currentColor, 0.06); border-radius: 14px; padding: 2px 0; } \
         .group-card separator { background: alpha(currentColor, 0.08); min-height: 1px; } \
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
        let banner_for_save = restart_banner.clone();
        let save = move || {
            match ui_for_save.save_all() {
                // Every saved change raises the restart banner: the settings
                // are already on disk, the button is only there so a restart
                // is one click away.
                Ok(()) => banner_for_save.set_revealed(true),
                Err(e) => alert(&window_for_save, &e.to_string()),
            }
        };
        // The rule rows are created by `refresh`, so they save through this
        // handle instead of a closure captured at build time.
        *ui.rules_save.borrow_mut() = Some(Rc::new(save.clone()));
        // The quick-add row appends a rule row, then saves through the same
        // error surface as every other control. A typed pattern wins over
        // the dropdown choice.
        {
            let ui = ui.clone();
            let client_combo = client_combo.clone();
            let rule_mode = rule_mode.clone();
            let rule_pattern = rule_pattern.clone();
            add_btn.connect_clicked(move |_| {
                let idx = client_combo.selected() as usize;
                let typed = rule_pattern.text().trim().to_string();
                let pattern = if typed.is_empty() {
                    app_targets.get(idx).map(|(_, p)| p.clone())
                } else {
                    Some(typed)
                };
                let Some(pattern) = pattern else {
                    return;
                };
                if pattern.is_empty() {
                    return;
                }
                let mode = mode_at(rule_mode.selected());
                ui.edit_rules(move |rules| {
                    rules.push(crate::config::AppRule { pattern, mode });
                });
                rule_pattern.set_text("");
            });
        }
        {
            let ui = ui.clone();
            let rule_pattern = rule_pattern.clone();
            focus_use_btn.connect_clicked(move |_| {
                let value = ui.rule_focus_value.borrow().clone();
                if !value.is_empty() {
                    rule_pattern.set_text(&value);
                }
            });
        }
        {
            let ui = ui.clone();
            let input = ui.rule_preview_input.clone();
            input.connect_changed(move |_| {
                ui.update_rule_preview();
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
        for view in [&ui.macro_text, &ui.dictionary_text] {
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
        let banner_for_import = restart_banner.clone();
        import_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Nhập cấu hình")
                .modal(true)
                .build();
            let ui = ui.clone();
            let callback_window = window.clone();
            let banner_for_import = banner_for_import.clone();
            dialog.open(
                Some(&window),
                gtk4::gio::Cancellable::NONE,
                // A dismissed dialog is also an Err: nothing to import.
                move |result| {
                    if let Ok(file) = result {
                        match import_json(file.path(), &ui) {
                            // An import rewrites config.toml wholesale — the
                            // restart banner applies to it like any save.
                            Ok(()) => banner_for_import.set_revealed(true),
                            Err(e) => alert(&callback_window, &e),
                        }
                    }
                },
            );
        });
    }

    {
        let window = window.clone();
        let banner = restart_banner.clone();
        let toast_overlay = toast_overlay.clone();
        restart_banner.connect_button_clicked(move |_| {
            // User-initiated only — the engine never restarts IBus itself.
            match std::process::Command::new("ibus").arg("restart").spawn() {
                Ok(_) => {
                    banner.set_revealed(false);
                    toast_overlay.add_toast(adw::Toast::new("Đã khởi động lại IBus"));
                }
                Err(e) => alert(&window, &format!("Không khởi động lại được IBus: {e}")),
            }
        });
    }

    window.present();
}
