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

    // --- pages: grouped-card settings style (section header + cards) ---

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

    // ===== Page 1: Chung — the everyday page, on its own preferences file.
    let typing_page = pages::typing::build(&ui);

    // ===== Page 2: Dấu & tự động — tone style and automatic behaviour.
    let tone_page = pages::tone::build(&ui);

    // ===== Page 3: Gõ tắt — switches, formats and the macro editor.
    let macro_page = pages::macros::build(&ui);

    // ===== Page 3b: Emoji — shortcode typing and the user's own table.
    let emoji_page = pages::emoji::build(&ui);

    // ===== Page 4: Từ điển — the personal dictionary (text area).
    let data_page = pages::data::build(&ui);

    // ===== Page 5: Luật — chip, quick-add, list editor, preview.
    let rules_page = pages::rules::build(&ui);

    // ===== Page 6: Cài đặt khác — icon, extension, backup, reset.
    let other_page = pages::other::build(&ui, &restart_btn);

    // ===== Page 7: Tinh chỉnh — uinput pacing + X11/XIM workarounds.
    let tune_page = pages::tune::build(&ui);

    // ===== Page 8: Giới thiệu — feature highlights + the about dialog.
    let about_page = pages::about::build();

    // ===== Page 6: Menu panel — the popup designer.
    let panel_page = pages::panel::build(&ui);
    let hotkeys_page = pages::hotkeys::build(&ui);

    // Content: one page per section, crossfading. Stack names are stable —
    // the engine opens pages with `--page=<name>`.
    // (stack name, sidebar label, symbolic icon from the icon theme)
    let pages: [(&str, String, &str); 11] = [
        (
            "typing",
            t("pages.typing", "Chung"),
            "input-keyboard-symbolic",
        ),
        (
            "tone",
            t("pages.tone", "Dấu và tự động"),
            "accessories-character-map-symbolic",
        ),
        (
            "macros",
            t("pages.macros", "Gõ tắt"),
            "edit-find-replace-symbolic",
        ),
        ("emoji", t("pages.emoji", "Emoji"), "face-smile-symbolic"),
        (
            "data",
            t("pages.data", "Từ điển"),
            "accessories-dictionary-symbolic",
        ),
        (
            "rules",
            t("pages.rules", "Luật theo app"),
            "view-app-grid-symbolic",
        ),
        (
            "panel",
            t("pages.panel", "Menu trên panel"),
            "open-menu-symbolic",
        ),
        (
            "hotkeys",
            t("pages.hotkeys", "Phím tắt"),
            "preferences-desktop-keyboard-shortcuts-symbolic",
        ),
        (
            "other",
            t("pages.other", "Cài đặt khác"),
            "view-more-horizontal-symbolic",
        ),
        (
            "tune",
            t("pages.tune", "Tinh chỉnh"),
            "applications-engineering-symbolic",
        ),
        (
            "about",
            t("pages.about", "Giới thiệu"),
            "help-about-symbolic",
        ),
    ];
    let pages_refs: [(&str, &str, &str); 11] = std::array::from_fn(|i| {
        let (name, title, icon) = &pages[i];
        (*name, title.as_str(), *icon)
    });
    let page_boxes: [&gtk4::Widget; 11] = [
        typing_page.upcast_ref(),
        tone_page.upcast_ref(),
        macro_page.upcast_ref(),
        emoji_page.upcast_ref(),
        data_page.upcast_ref(),
        rules_page.upcast_ref(),
        panel_page.upcast_ref(),
        hotkeys_page.upcast_ref(),
        other_page.upcast_ref(),
        tune_page.upcast_ref(),
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

    let title_label = gtk4::Label::new(Some("Chung"));
    title_label.add_css_class("title");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title_label));
    let sidebar_btn = gtk4::Button::from_icon_name("sidebar-show-symbolic");
    sidebar_btn.set_visible(false);
    header.pack_start(&sidebar_btn);

    // Sidebar rows in GNOME Settings groups: the everyday pages, the
    // feature targets, the advanced ones — one thin separator between
    // groups — and About pinned below an expanding spacer.
    let sidebar_list = gtk4::ListBox::new();
    sidebar_list.add_css_class("navigation-sidebar");
    let about_list = gtk4::ListBox::new();
    about_list.add_css_class("navigation-sidebar");
    let mut page_rows: Vec<(String, gtk4::ListBoxRow)> = Vec::new();
    /// One navigation row: icon + label, the accessible label and tooltip
    /// carry the full text, long labels ellipsize at the end.
    fn nav_row(title: &str, icon: &str, name: &str) -> gtk4::ListBoxRow {
        let row = gtk4::ListBoxRow::new();
        let h = gtk4::Box::new(Orientation::Horizontal, 12);
        // A little breathing room: navigation-sidebar's padding alone leaves
        // the icon glued to the window edge.
        h.set_margin_start(6);
        h.append(&gtk4::Image::from_icon_name(icon));
        let label = gtk4::Label::new(Some(title));
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        label.set_xalign(0.0);
        h.append(&label);
        row.set_child(Some(&h));
        // The row carries its stack name; the selection handler reads it.
        row.set_widget_name(name);
        row.set_tooltip_text(Some(title));
        row.update_property(&[gtk4::accessible::Property::Label(title)]);
        row
    }
    /// One thin group separator: not selectable, not focusable, identical
    /// margins above and below.
    fn separator_row() -> gtk4::ListBoxRow {
        let row = gtk4::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        row.set_focusable(false);
        row.set_can_target(false);
        let sep = gtk4::Box::new(Orientation::Vertical, 0);
        sep.set_margin_top(3);
        sep.set_margin_bottom(3);
        sep.append(&gtk4::Separator::new(Orientation::Horizontal));
        row.set_child(Some(&sep));
        row
    }
    let label_of = |name: &str| {
        pages_refs
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, title, _)| title.as_ref())
            .unwrap_or("")
    };
    let icon_of = |name: &str| {
        pages_refs
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, _, icon)| *icon)
            .unwrap_or("")
    };
    let groups: [&[&str]; 3] = [
        &["typing", "tone", "macros", "emoji", "data"],
        &["rules", "panel", "hotkeys"],
        &["tune", "other"],
    ];
    for (gi, group) in groups.iter().copied().enumerate() {
        if gi > 0 {
            sidebar_list.append(&separator_row());
        }
        for name in group {
            let row = nav_row(label_of(name), icon_of(name), name);
            sidebar_list.append(&row);
            page_rows.push((name.to_string(), row.clone()));
        }
    }
    let about_row = nav_row(label_of("about"), icon_of("about"), "about");
    // The pinned About group keeps the same separator style above it.
    about_list.append(&separator_row());
    about_list.append(&about_row);
    page_rows.push(("about".to_string(), about_row.clone()));
    // Owned copies for the 'static signal handlers (the const PAGES they
    // replaced lives only in the content file now).
    let app_name = t("app.name", "MăngRS");
    let page_titles: Vec<(String, String)> = pages
        .iter()
        .map(|(name, title, _)| (name.to_string(), title.clone()))
        .collect();
    // One pick handler for both lists: switch the stack, set the header
    // title, reveal the content when collapsed — and keep exactly one
    // selected row across the two lists.
    type PagePick = Rc<dyn Fn(Option<&gtk4::ListBoxRow>)>;
    let pick: PagePick = {
        let stack = stack.clone();
        let title_label = title_label.clone();
        let split_view = split_view.clone();
        let app_name = app_name.clone();
        Rc::new(move |some_row| {
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
        })
    };
    {
        let pick = pick.clone();
        let about_list = about_list.clone();
        sidebar_list.connect_row_selected(move |_, some_row| {
            pick(some_row);
            if some_row.is_some() {
                // One selection at a time across the two lists.
                about_list.select_row(None::<&gtk4::ListBoxRow>);
            }
        });
    }
    {
        let pick = pick.clone();
        let main_list = sidebar_list.clone();
        about_list.connect_row_selected(move |_, some_row| {
            pick(some_row);
            if some_row.is_some() {
                main_list.select_row(None::<&gtk4::ListBoxRow>);
            }
        });
    }

    // Search lives in the sidebar: the magnifier button stays visible and
    // TOGGLES the entry beside it — the second press hides it again (Esc
    // and jumping to a result do the same). Typing overlays the matching
    // rows over the content.
    let search_btn = gtk4::Button::from_icon_name("system-search-symbolic");
    search_btn.add_css_class("flat");
    search_btn.set_tooltip_text(Some(&t("search.tooltip", "Tìm chức năng trong mọi trang")));
    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some(&t("search.placeholder", "Tìm chức năng…")));
    search_entry.set_hexpand(true);
    search_entry.set_visible(false);
    {
        let entry = search_entry.clone();
        search_btn.connect_clicked(move |_| {
            let show = !entry.is_visible();
            entry.set_visible(show);
            if show {
                entry.grab_focus();
            } else {
                entry.set_text("");
            }
        });
    }
    let search_box = gtk4::Box::new(Orientation::Horizontal, 6);
    search_box.set_margin_top(6);
    search_box.set_margin_bottom(6);
    search_box.set_margin_start(6);
    search_box.set_margin_end(6);
    search_box.append(&search_btn);
    search_box.append(&search_entry);
    let sidebar_box = gtk4::Box::new(Orientation::Vertical, 0);
    sidebar_box.append(&search_box);
    // The main list keeps its natural height; an expanding spacer pins the
    // About group to the bottom of the sidebar.
    sidebar_box.append(&sidebar_list);
    let spacer = gtk4::Box::new(Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar_box.append(&spacer);
    sidebar_box.append(&about_list);
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
        let entry = search_entry.clone();
        search_entry.connect_stop_search(move |_| {
            // Esc: collapse the entry with the query cleared; the
            // magnifier stays and toggles it back.
            entry.set_text("");
            entry.set_visible(false);
        });
    }
    {
        let entry = search_entry.clone();
        let page_rows = page_rows.clone();
        results_list.connect_row_activated(move |_, row| {
            let name = row.widget_name().to_string();
            // Selecting the sidebar row drives stack + title, exactly like a
            // manual page pick — the row may live in the main list or the
            // pinned About list.
            let target = page_rows
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, r)| r.clone());
            if let Some(row) = target {
                if let Some(list) = row.parent().and_downcast::<gtk4::ListBox>() {
                    list.select_row(Some(&row));
                }
            }
            entry.set_text("");
            // The jump found the page: collapse the entry.
            entry.set_visible(false);
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

    // Keyboard: close and quit, Ctrl+F for the sidebar search, plus
    // Alt+0..9 to jump straight to a page (an accelerator takes one
    // character after Alt, so the 11th page has none — installing
    // "<Alt>10" only warned and did nothing).
    app.set_accels_for_action("window.close", &["<Control>w"]);
    let quit = gtk4::gio::SimpleAction::new("quit", None);
    {
        let app = app.clone();
        quit.connect_activate(move |_, _| app.quit());
    }
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    let search_action = gtk4::gio::SimpleAction::new("search", None);
    {
        let entry = search_entry.clone();
        search_action.connect_activate(move |_, _| {
            entry.set_visible(true);
            entry.grab_focus();
        });
    }
    window.add_action(&search_action);
    app.set_accels_for_action("win.search", &["<Control>f"]);
    for (i, (_, row)) in page_rows.iter().enumerate() {
        let action = gtk4::gio::SimpleAction::new(&format!("page-{}", i + 1), None);
        {
            // The row's own parent list — the main pages or the pinned
            // About list — selects it and drives the stack.
            let row = row.clone();
            action.connect_activate(move |_, _| {
                if let Some(list) = row.parent().and_downcast::<gtk4::ListBox>() {
                    list.select_row(Some(&row));
                }
            });
        }
        window.add_action(&action);
        if i < 10 {
            app.set_accels_for_action(&format!("win.page-{}", i + 1), &[&format!("<Alt>{i}")]);
        }
    }

    // The focused-app chip refreshes when the window loses focus: at that
    // moment the app the user switched to becomes focused, and that is the
    // app they came here to target. No refresh at startup — the settings
    // window itself is the focused window then, which names nothing useful.
    {
        let ui = ui.clone();
        let controller = gtk4::EventControllerFocus::new();
        controller.connect_leave(move |_| {
            ui.refresh_rule_focus();
        });
        window.add_controller(controller);
    }

    // Open a specific page when spawned with `--page=<name>` (menu items).
    // Old page names from earlier layouts are mapped onto the current ones.
    let wanted = match initial_page {
        Some("general") | None => "typing",
        // The technical page's own name, for completeness.
        Some("tune") => "tune",
        Some("hotkeys") | Some("shortcuts") => "hotkeys",
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
    // The row's own list selects it — the main pages or the pinned About.
    if let Some(list) = wanted_row.parent().and_downcast::<gtk4::ListBox>() {
        list.select_row(Some(wanted_row));
    }

    {
        let window_for_save = window.clone();
        let ui_for_save = ui.clone();
        let restart_for_save = restart_btn.clone();
        let save = move || {
            // Programmatic widget updates (external_reload) fire the same
            // handlers; saving then would write back what we just read.
            if ui_for_save.suppress_save.get() {
                return;
            }
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
        wire_switch_saves(&ui, Rc::new(save.clone()));
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
        // Text views save when focus leaves the window's text area. The
        // raw macro editor additionally rebuilds the row list from its
        // lines (the dictionary text saves straight through the funnel).
        {
            let s = save.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            ui.dictionary_text.add_controller(controller);
        }
        {
            let s = save.clone();
            let view = ui.macros_text.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            view.add_controller(controller);
        }
        {
            let s = save.clone();
            let view = ui.emoji_text.clone();
            let controller = gtk4::EventControllerFocus::new();
            controller.connect_leave(move |_| s());
            view.add_controller(controller);
        }
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

    // Live reload: the engine's hotkeys (and any other writer) change the
    // files under this window. Poll the mtime cheaply and re-read into the
    // widgets; without it the next user edit would save the stale values
    // over the change. The engine does the same from its key path.
    {
        let watch_ui = ui.clone();
        let watch_mtime = Cell::new(config_mtime());
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let m = config_mtime();
            if m != watch_mtime.get() {
                watch_mtime.set(m);
                watch_ui.external_reload();
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    window.present();
}

/// Wire every boolean row the window owns to the save funnel — one
/// `on_change!` per switch field. A switch missing here still toggles on
/// screen but never reaches `config.toml` (the "Ẩn gạch chân" bug), so
/// the gui tests flip this whole list and assert each one saves.
/// `desktop_icon` is wired on its page instead: it carries a loading
/// guard for its filesystem side effect, not a plain config save.
pub(super) fn wire_switch_saves(ui: &Ui, save: Rc<dyn Fn()>) {
    macro_rules! on_change {
        ($w:expr) => {{
            let s = Rc::clone(&save);
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
    on_change!(ui.macro_valid_words);
    on_change!(ui.emoji_enabled);
    on_change!(ui.dictionary_enabled);
    on_change!(ui.hide_underline);
    on_change!(ui.debug_log);
    on_change!(ui.debug_log_text);
    on_change!(ui.x11_delete);
    on_change!(ui.preedit_xim);
}

/// mtime of config.toml, `None` while it does not exist yet — the watch
/// baseline for the live-reload poll.
fn config_mtime() -> Option<std::time::SystemTime> {
    Config::path()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}
