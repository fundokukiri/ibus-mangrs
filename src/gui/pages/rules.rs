//! Page "Luật": the per-app rule editor — focused-app row, native add
//! fields, one ComboRow per rule and the live match preview. All page
//! widgets are created and wired here; the save funnel comes from the
//! shared `save_cb` handle. The page hosts its own ToastOverlay: the only
//! toast is the copy confirmation, and the window shell stays untouched.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();

    // Quick-add targets: real clients the engine has seen (GTK IM apps only
    // — Wayland relays show up as gnome-shell, X11/XIM apps as none) plus
    // the two pseudo-targets for those nameless groups.
    let mut app_targets: Vec<(String, String)> = crate::config::recent_clients()
        .into_iter()
        .map(|n| {
            let label = n.clone();
            (label, n)
        })
        .collect();
    app_targets.push((
        t("rules.pseudo_wayland", "gnome-shell (app Wayland)"),
        "gnome-shell".to_string(),
    ));
    app_targets.push((
        t("rules.pseudo_none", "(app không báo tên)"),
        "none".to_string(),
    ));

    // The focused app named by the extension (or xprop): a selectable
    // monospace label — select-and-copy without any button — plus a flat
    // copy button that confirms with a toast.
    let focus = adw::PreferencesGroup::builder()
        .title(t("groups.rules.focus.title", "Ứng dụng đang focus"))
        .build();
    let focus_row = action_row(
        &t("rows.rule_focus.title", "App đang focus"),
        &t(
            "rows.rule_focus.subtitle",
            "Chuyển sang app đích rồi quay lại",
        ),
    );
    focus_row.set_tooltip_text(Some(&t(
        "rows.rule_focus.tooltip",
        "Tên lấy qua extension MăngRS Focus; app không báo tên thì dùng pattern none",
    )));
    ui.rule_focus.set_width_chars(24);
    focus_row.add_suffix(&ui.rule_focus);
    let copy_btn = gtk4::Button::from_icon_name("edit-copy-symbolic");
    copy_btn.add_css_class("flat");
    copy_btn.set_tooltip_text(Some(&t(
        "rows.rule_copy.tooltip",
        "Sao chép tên app đang focus",
    )));
    focus_row.add_suffix(&copy_btn);
    focus.add(&focus_row);
    page.add(&focus);

    // Quick-add: pick a seen app (or the focused one) into a native entry,
    // choose the mode — the rule lands in the list through the shared edit
    // path, and the button stays dead until a name exists.
    let add_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.add.title", "Thêm luật"))
        .description(t(
            "groups.rules.add.description",
            "Chọn app đã thấy hoặc dùng app đang focus.",
        ))
        .build();
    let name_row = adw::EntryRow::builder()
        .title(t("rows.rule_name.title", "Tên app"))
        .build();
    name_row.set_tooltip_text(Some(&t(
        "rows.rule_name.tooltip",
        "Tên app: khớp một phần, không phân biệt hoa/thường (vd firefox, sublime)",
    )));
    let pick = gtk4::MenuButton::new();
    pick.set_icon_name("view-more-symbolic");
    pick.add_css_class("flat");
    pick.set_tooltip_text(Some(&t(
        "rows.rule_combo.tooltip",
        "Các app engine đã từng thấy",
    )));
    let pick_list = gtk4::Box::new(Orientation::Vertical, 0);
    let pick_popover = gtk4::Popover::new();
    for (label, target) in &app_targets {
        let btn = gtk4::Button::with_label(label);
        btn.add_css_class("flat");
        {
            let name_row = name_row.clone();
            let popover = pick_popover.clone();
            let target = target.clone();
            btn.connect_clicked(move |_| {
                name_row.set_text(&target);
                popover.popdown();
            });
        }
        pick_list.append(&btn);
    }
    // The seen-apps list is built once, like the old dropdown; a long list
    // scrolls inside the popover instead of stretching it.
    let pick_scroll = gtk4::ScrolledWindow::new();
    pick_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    pick_scroll.set_max_content_height(300);
    pick_scroll.set_propagate_natural_height(true);
    pick_scroll.set_child(Some(&pick_list));
    pick_popover.set_child(Some(&pick_scroll));
    pick.set_popover(Some(&pick_popover));
    let use_focus = gtk4::Button::with_label(&t("rules.use_focus_button", "Dùng app đang focus"));
    use_focus.add_css_class("flat");
    use_focus.set_tooltip_text(Some(&t(
        "rows.rule_copy.tooltip",
        "Sao chép tên app đang focus",
    )));
    name_row.add_suffix(&pick);
    name_row.add_suffix(&use_focus);
    // The mode pick leads the row without a label — the dropdown speaks
    // for itself — and the suggested-action add closes it: pick and add
    // are one decision. A plain DropDown, since a titled ComboRow would
    // put the value back on the right.
    let mode_pick = gtk4::DropDown::new(Some(StringList::new(MODES)), gtk4::Expression::NONE);
    mode_pick.set_selected(1);
    mode_pick.set_tooltip_text(Some(&t("rows.rule_mode.tooltip", "Chế độ gõ của luật mới")));
    let mode_row = action_row("", "");
    mode_row.add_prefix(&mode_pick);
    let add_btn = gtk4::Button::with_label(&t("rules.add_button", "Thêm luật"));
    add_btn.add_css_class("suggested-action");
    add_btn.set_valign(gtk4::Align::Center);
    add_btn.set_sensitive(false);
    mode_row.add_suffix(&add_btn);
    add_group.add(&name_row);
    add_group.add(&mode_row);
    page.add(&add_group);

    let list_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.list.title", "Danh sách luật"))
        .description(t(
            "groups.rules.list.description",
            "Luật đầu tiên thắng; none = app không báo tên.",
        ))
        .build();
    list_group.set_tooltip_text(Some(&t(
        "groups.rules.list.tooltip",
        "Chỉ app dùng GTK IM module mới báo tên riêng; app Wayland hiện là gnome-shell, app X11/khác là none. Cài extension MăngRS Focus (data/gnome-shell-extension) để nhận tên app Wayland-native.",
    )));
    list_group.add(&ui.rules_box);
    list_group.add(&ui.rules_status);
    page.add(&list_group);

    let preview_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.preview.title", "Thử luật"))
        .description(t(
            "groups.rules.preview.description",
            "Nhập tên app để xem luật nào khớp.",
        ))
        .build();
    preview_group.add(&ui.rule_preview_input);
    preview_group.add(&ui.rule_preview);
    page.add(&preview_group);

    // The page's own toast host: the copy confirmation must surface
    // somewhere, and the window shell stays untouched by this page work.
    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&page));

    // "Dùng app đang focus" fills the entry; the rule itself is added by
    // the button below, like every hand-typed name.
    {
        let ui = ui.clone();
        let name_row = name_row.clone();
        use_focus.connect_clicked(move |_| {
            let pattern = ui.rule_focus_value.borrow().clone();
            if !pattern.is_empty() {
                name_row.set_text(&pattern);
            }
        });
    }
    // The add button follows the entry: no name, no rule.
    {
        let name_row = name_row.clone();
        let btn = add_btn.clone();
        let watcher = name_row.clone();
        watcher.connect_changed(move |_| {
            btn.set_sensitive(!name_row.text().trim().is_empty());
        });
    }
    {
        let ui = ui.clone();
        let name_row = name_row.clone();
        let mode_pick = mode_pick.clone();
        add_btn.connect_clicked(move |_| {
            let pattern = name_row.text().trim().to_string();
            if pattern.is_empty() {
                return;
            }
            let mode = mode_at(mode_pick.selected());
            ui.edit_rules(move |rules| {
                rules.push(crate::config::AppRule { pattern, mode });
            });
            // Ready for the next rule; the mode choice stays.
            name_row.set_text("");
        });
    }
    // The preview follows typing live; mode changes and edits re-run it
    // from their own handlers.
    {
        let ui = ui.clone();
        let input = ui.rule_preview_input.clone();
        input.connect_changed(move |_| {
            ui.update_rule_preview();
        });
    }
    {
        let ui = ui.clone();
        let overlay = toast_overlay.clone();
        copy_btn.connect_clicked(move |_| {
            let value = ui.rule_focus_value.borrow().clone();
            if value.is_empty() {
                return;
            }
            ui.rule_focus.clipboard().set_text(&value);
            overlay.add_toast(adw::Toast::new(&t("rules.copy_toast", "Đã sao chép")));
        });
    }

    toast_overlay
}
