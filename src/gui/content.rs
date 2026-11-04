//! The window shell: the Adwaita navigation split view (sidebar + content),
//! the floating restart button, page accelerators, and the wiring between
//! every control and the save funnel. The page contents themselves still
//! live here and move to `pages/*.rs` one commit at a time.

use super::*;

#[allow(deprecated)]
pub(super) fn build(app: &adw::Application, initial_page: Option<&str>) {
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

    let export_btn = gtk4::Button::with_label(&t("misc.export_button", "Xuất…"));
    let import_btn = gtk4::Button::with_label(&t("misc.import_button", "Nhập…"));

    // ===== Page 1: Chung — the everyday page, on its own preferences file.
    let typing_page = pages::typing::build(&ui);

    // ===== Page 2: Dấu & tự động — tone style and automatic behaviour.
    let tone_page = pages::tone::build(&ui);

    // ===== Page 3: Gõ tắt — switches, formats and the macro editor.
    let (macro_page, macro_add_btn) = pages::macros::build(&ui);

    // ===== Page 4: Từ điển — the personal dictionary (text area).
    let data_page = pages::data::build(&ui);

    // ===== Page 5: Luật — chip, quick-add, list editor, preview.
    let rules_page = pages::rules::build(&ui);

    // ===== Page 6: Tinh chỉnh — uinput pacing, X11/XIM, backup, log.
    let advanced_page = pages::advanced::build(&ui, &export_btn, &import_btn);

    // ===== Page 7: Giới thiệu — feature highlights + the about dialog.
    let about_page = pages::about::build();

    // ===== Page 6: Menu panel — the popup designer.
    let panel_page = pages::panel::build(&ui);

    // Content: one page per section, crossfading. Stack names are stable —
    // the engine opens pages with `--page=<name>`.
    // (stack name, sidebar label, symbolic icon from the icon theme)
    let pages: [(&str, String, &str); 8] = [
        (
            "typing",
            t("pages.typing", "Chung"),
            "input-keyboard-symbolic",
        ),
        (
            "tone",
            t("pages.tone", "Dấu & tự động"),
            "tools-check-spelling-symbolic",
        ),
        (
            "macros",
            t("pages.macros", "Gõ tắt"),
            "insert-text-symbolic",
        ),
        (
            "data",
            t("pages.data", "Từ điển"),
            "accessories-dictionary-symbolic",
        ),
        ("rules", t("pages.rules", "Luật"), "view-list-symbolic"),
        (
            "panel",
            t("pages.panel", "Menu panel"),
            "document-edit-symbolic",
        ),
        (
            "advanced",
            t("pages.advanced", "Tinh chỉnh"),
            "emblem-system-symbolic",
        ),
        (
            "about",
            t("pages.about", "Giới thiệu"),
            "help-about-symbolic",
        ),
    ];
    let pages_refs: [(&str, &str, &str); 8] = std::array::from_fn(|i| {
        let (name, title, icon) = &pages[i];
        (*name, title.as_str(), *icon)
    });
    let page_boxes: [&gtk4::Widget; 8] = [
        typing_page.upcast_ref(),
        tone_page.upcast_ref(),
        macro_page.upcast_ref(),
        data_page.upcast_ref(),
        rules_page.upcast_ref(),
        panel_page.upcast_ref(),
        advanced_page.upcast_ref(),
        about_page.upcast_ref(),
    ];
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_vexpand(true);
    stack.set_hexpand(true);
    for ((name, title, _icon), content) in pages_refs.iter().zip(page_boxes) {
        stack.add_titled(content, Some(name), title);
    }

    // --- Adwaita shell: sidebar + content -----------------------------------
    // GNOME Settings layout: a navigation sidebar picks the page, the
    // content toolbar view holds the header (dynamic title) and the stack.
    // The restart button floats over the content at the bottom center.
    let split_view = adw::NavigationSplitView::new();
    split_view.set_sidebar_width_fraction(0.24);

    // The floating restart button. It stays hidden until a change is saved:
    // every control already writes straight to config.toml, so the only
    // thing left for the user to do is restart IBus (a new engine binary or
    // component XML needs it; plain settings changes apply within a second).
    let restart_btn = gtk4::Button::new();
    {
        let b = gtk4::Box::new(Orientation::Horizontal, 8);
        b.append(&gtk4::Image::from_icon_name("system-reboot-symbolic"));
        b.append(&gtk4::Label::new(Some(&t(
            "misc.restart_button",
            "Khởi động lại IBus",
        ))));
        restart_btn.set_child(Some(&b));
    }
    restart_btn.add_css_class("suggested-action");
    restart_btn.add_css_class("pill");
    restart_btn.set_halign(gtk4::Align::Center);
    restart_btn.set_valign(gtk4::Align::End);
    restart_btn.set_margin_bottom(18);
    restart_btn.set_visible(false);

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
    for (name, title, icon) in pages_refs {
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
        // A little breathing room: navigation-sidebar's padding alone leaves
        // the icon glued to the window edge.
        h.set_margin_start(6);
        h.append(&gtk4::Image::from_icon_name(icon));
        h.append(&gtk4::Label::new(Some(title)));
        row.set_child(Some(&h));
        // The row carries its stack name; the selection handler reads it.
        row.set_widget_name(name);
        sidebar_list.append(&row);
        page_rows.push((name.to_string(), row.clone()));
    }
    // Owned copies for the 'static signal handlers (the const PAGES they
    // replaced lives only in the content file now).
    let app_name = t("app.name", "MăngRS");
    let page_titles: Vec<(String, String)> = pages
        .iter()
        .map(|(name, title, _)| (name.to_string(), title.clone()))
        .collect();
    {
        let stack = stack.clone();
        let title_label = title_label.clone();
        let split_view = split_view.clone();
        let app_name = app_name.clone();
        sidebar_list.connect_row_selected(move |_, some_row| {
            let Some(row) = some_row else {
                return;
            };
            let name = row.widget_name().to_string();
            let title = page_titles
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, title)| title.as_str())
                .unwrap_or(&app_name);
            stack.set_visible_child_name(&name);
            title_label.set_text(title);
            // Picking a page while collapsed shows the content again, like
            // GNOME Settings on a phone-shaped window.
            if split_view.is_collapsed() {
                split_view.set_show_content(true);
            }
        });
    }

    // Search lives in the sidebar, hidden behind a magnifier button: the
    // click swaps the button for the entry above the page list; typing
    // overlays the matching rows over the content, Esc (or jumping to a
    // result) collapses back to the button.
    let search_btn = gtk4::Button::from_icon_name("system-search-symbolic");
    search_btn.add_css_class("flat");
    search_btn.set_tooltip_text(Some(&t("search.tooltip", "Tìm chức năng trong mọi trang")));
    search_btn.set_halign(gtk4::Align::Start);
    search_btn.set_margin_top(6);
    search_btn.set_margin_bottom(6);
    search_btn.set_margin_start(6);
    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some(&t("search.placeholder", "Tìm chức năng…")));
    search_entry.set_hexpand(true);
    search_entry.set_margin_top(6);
    search_entry.set_margin_bottom(6);
    search_entry.set_margin_start(6);
    search_entry.set_margin_end(6);
    search_entry.set_visible(false);
    let sidebar_box = gtk4::Box::new(Orientation::Vertical, 0);
    sidebar_box.append(&search_btn);
    sidebar_box.append(&search_entry);
    sidebar_list.set_vexpand(true);
    sidebar_box.append(&sidebar_list);
    let sidebar_page = adw::NavigationPage::builder()
        .title(&app_name)
        .child(&sidebar_box)
        .build();

    // The restart button floats over everything at the bottom center of
    // the page.
    let page_overlay = gtk4::Overlay::new();
    page_overlay.set_child(Some(&stack));
    page_overlay.add_overlay(&restart_btn);
    let content_toolbar = adw::ToolbarView::new();

    // --- search: the sidebar entry, results over every page -----------------
    // Typing in the sidebar entry shows a flat list of matching rows across
    // ALL pages, and activating a result jumps to that page through the
    // sidebar (title and selection stay in sync). The index is built once
    // from the finished page trees.
    {
        let btn = search_btn.clone();
        let entry = search_entry.clone();
        search_btn.connect_clicked(move |_| {
            btn.set_visible(false);
            entry.set_visible(true);
            entry.grab_focus();
        });
    }

    let results_list = gtk4::ListBox::new();
    results_list.add_css_class("boxed-list");
    results_list.set_selection_mode(gtk4::SelectionMode::None);
    let no_results = gtk4::Label::new(Some(&t("search.none", "Không có kết quả")));
    no_results.add_css_class("dim-label");
    let results_box = gtk4::Box::new(Orientation::Vertical, 12);
    // Opaque page background so the results cover the stack underneath.
    results_box.add_css_class("background");
    results_box.set_margin_start(12);
    results_box.set_margin_end(12);
    results_box.set_margin_top(12);
    results_box.set_margin_bottom(12);
    results_box.append(&results_list);
    results_box.append(&no_results);
    let results_scroll = gtk4::ScrolledWindow::new();
    results_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    results_scroll.set_child(Some(&results_box));
    results_scroll.set_visible(false);
    page_overlay.add_overlay(&results_scroll);

    // One hit per page (jump target) plus every named row inside it.
    struct SearchHit {
        page_name: &'static str,
        page_title: String,
        title: String,
        subtitle: String,
    }
    fn collect_rows(
        w: &gtk4::Widget,
        page_name: &'static str,
        page_title: &str,
        out: &mut Vec<SearchHit>,
    ) {
        if let Some(row) = w.downcast_ref::<adw::PreferencesRow>() {
            let title = row.title().to_string();
            if !title.is_empty() {
                let subtitle = w
                    .downcast_ref::<adw::ActionRow>()
                    .and_then(|r| r.subtitle())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                out.push(SearchHit {
                    page_name,
                    page_title: page_title.to_string(),
                    title,
                    subtitle,
                });
            }
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            collect_rows(&c, page_name, page_title, out);
            child = c.next_sibling();
        }
    }
    let mut index: Vec<SearchHit> = Vec::new();
    for ((name, title, _), content) in pages_refs.iter().zip(&page_boxes) {
        index.push(SearchHit {
            page_name: name,
            page_title: (*title).to_string(),
            title: (*title).to_string(),
            subtitle: t("search.page_kind", "Trang"),
        });
        collect_rows(content, name, title, &mut index);
    }

    {
        let results_list = results_list.clone();
        let no_results = no_results.clone();
        let results_scroll = results_scroll.clone();
        search_entry.connect_changed(move |entry| {
            let query = entry.text().trim().to_lowercase();
            if query.is_empty() {
                results_scroll.set_visible(false);
                return;
            }
            let hits: Vec<&SearchHit> = index
                .iter()
                .filter(|h| {
                    h.title.to_lowercase().contains(&query)
                        || h.subtitle.to_lowercase().contains(&query)
                })
                .collect();
            while let Some(row) = results_list.row_at_index(0) {
                results_list.remove(&row);
            }
            for hit in &hits {
                let row = gtk4::ListBoxRow::new();
                let b = gtk4::Box::new(Orientation::Horizontal, 12);
                b.set_margin_start(10);
                b.set_margin_end(10);
                b.set_margin_top(6);
                b.set_margin_bottom(6);
                let title = gtk4::Label::new(Some(&hit.title));
                title.set_halign(gtk4::Align::Start);
                title.set_hexpand(true);
                title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                let page = gtk4::Label::new(Some(hit.page_title.as_str()));
                page.add_css_class("dim-label");
                b.append(&title);
                b.append(&page);
                row.set_child(Some(&b));
                // The row carries the page's stack name for the jump.
                row.set_widget_name(hit.page_name);
                results_list.append(&row);
            }
            results_list.set_visible(!hits.is_empty());
            no_results.set_visible(hits.is_empty());
            results_scroll.set_visible(true);
        });
    }
    {
        let btn = search_btn.clone();
        let entry = search_entry.clone();
        search_entry.connect_stop_search(move |_| {
            // Esc: collapse back to the button with the query cleared.
            entry.set_text("");
            entry.set_visible(false);
            btn.set_visible(true);
        });
    }
    {
        let sidebar_list = sidebar_list.clone();
        let entry = search_entry.clone();
        let btn = search_btn.clone();
        results_list.connect_row_activated(move |_, row| {
            let name = row.widget_name().to_string();
            // Selecting the sidebar row drives stack + title, exactly like a
            // manual page pick.
            let mut i = 0;
            while let Some(candidate) = sidebar_list.row_at_index(i) {
                if candidate.widget_name() == name {
                    sidebar_list.select_row(Some(&candidate));
                    break;
                }
                i += 1;
            }
            entry.set_text("");
            // The jump found the page: collapse back to the magnifier.
            entry.set_visible(false);
            btn.set_visible(true);
        });
    }

    content_toolbar.add_top_bar(&header);
    content_toolbar.set_content(Some(&page_overlay));
    let content_page = adw::NavigationPage::builder()
        .title(&app_name)
        .child(&content_toolbar)
        .build();

    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(&app_name)
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
        Some("menupanel") => "panel",
        Some(other) => other,
    };
    let (_, wanted_row) = page_rows
        .iter()
        .find(|(n, _)| n == wanted)
        .unwrap_or(&page_rows[0]);
    sidebar_list.select_row(Some(wanted_row));

    {
        let window_for_save = window.clone();
        let ui_for_save = ui.clone();
        let restart_for_save = restart_btn.clone();
        let save = move || {
            match ui_for_save.save_all() {
                // Every saved change raises the restart banner: the settings
                // are already on disk, the button is only there so a restart
                // is one click away.
                Ok(()) => restart_for_save.set_visible(true),
                Err(e) => alert(&window_for_save, &e.to_string()),
            }
        };
        // The rule rows are created by `refresh`, so they save through this
        // handle instead of a closure captured at build time.
        *ui.save_cb.borrow_mut() = Some(Rc::new(save.clone()));
        // The macro add row appends an empty pair the user fills in.
        {
            let ui = ui.clone();
            macro_add_btn.connect_clicked(move |_| {
                ui.edit_macros(|entries| entries.push((String::new(), String::new())));
            });
        }
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
        for entry in [&ui.time_format, &ui.date_format] {
            let s = save.clone();
            entry.connect_activate(move |_| s());
            let s = save.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            entry.add_controller(controller);
        }
        // Text views save when focus leaves the window's text area.
        {
            let s = save.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            ui.dictionary_text.add_controller(controller);
        }
    }

    {
        let window = window.clone();
        export_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title(t("misc.export_dialog", "Xuất cấu hình"))
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
                .title(t("misc.import_dialog", "Nhập cấu hình"))
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
                            // restart banner applies to it like any save.
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
