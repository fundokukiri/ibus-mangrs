//! Page "Luật": the per-app rule editor — a merged add-rule form whose
//! name pill mirrors the focused app (placeholder hint, copy/apply pair,
//! seen-apps chevron), one ComboRow per rule and the live match preview.
//! The widgets live in `Ui`; this file lays them out and wires them. The
//! save funnel comes from the shared `save_cb` handle. The page hosts its
//! own ToastOverlay for the copy/add confirmations; the window shell stays
//! untouched.

use super::*;

/// The page's only CSS: the app-name pill and the mode dropdown share one
/// fill so the form reads as a single control. Named colors only — the
/// look holds in dark, light and any accent.
const FORM_CSS: &str = "
/* shared fill: the app-name pill and the mode dropdown read as one control */
.rule-fill {
  background: alpha(@window_fg_color, 0.08);
  border: none;
  border-radius: 8px;
}
/* the dropdown paints through its inner button; clear it so the fill above shows */
.rule-fill > button {
  background: none;
  border: none;
}
/* the name entry sits on the pill, so it draws nothing of its own */
.rule-pill entry {
  background: none;
  border: none;
  box-shadow: none;
}
";

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();

    // The pill fill cannot be expressed with built-in classes; load the
    // page's one CSS block onto the display (a display exists whenever the
    // GUI runs).
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(FORM_CSS);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

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

    // The seen-apps chevron: a flat menu button whose popover lists the
    // targets; picking one fills the pill entry, like the old dropdown.
    let pick = gtk4::MenuButton::new();
    pick.set_icon_name("pan-down-symbolic");
    pick.add_css_class("flat");
    pick.set_valign(gtk4::Align::Center);
    pick.set_tooltip_text(Some(&t(
        "rows.rule_combo.tooltip",
        "Các app engine đã từng thấy",
    )));
    pick.update_property(&[gtk4::accessible::Property::Label(&t(
        "rows.rule_combo.label",
        "Chọn từ app đã thấy",
    ))]);
    let pick_list = gtk4::Box::new(Orientation::Vertical, 0);
    let pick_popover = gtk4::Popover::new();
    for (label, target) in &app_targets {
        let btn = gtk4::Button::with_label(label);
        btn.add_css_class("flat");
        {
            let entry = ui.rule_name_entry.clone();
            let popover = pick_popover.clone();
            let target = target.clone();
            btn.connect_clicked(move |_| {
                entry.set_text(&target);
                popover.popdown();
            });
        }
        pick_list.append(&btn);
    }
    // The seen-apps list is built once, like the old dropdown; a long list
    // scrolls inside the popover instead of stretching it.
    let pick_scroll = gtk4::ScrolledWindow::new();
    pick_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    // Tall enough that a long seen-apps list barely scrolls.
    pick_scroll.set_max_content_height(520);
    pick_scroll.set_propagate_natural_height(true);
    pick_scroll.set_child(Some(&pick_list));
    pick_popover.set_child(Some(&pick_scroll));
    pick.set_popover(Some(&pick_popover));

    // Block A — the name pill and its actions, one native card. The pill
    // gets its caption here; entry, copy and apply arrive from `Ui`.
    let card = gtk4::Box::new(Orientation::Horizontal, 6);
    card.add_css_class("card");
    card.set_margin_top(8);
    card.set_margin_end(10);
    card.set_margin_bottom(8);
    card.set_margin_start(14);
    card.set_valign(gtk4::Align::Center);
    card.append(&ui.rule_pill);
    card.append(&ui.rule_copy_btn);
    card.append(&ui.rule_apply_btn);
    card.append(&pick);
    let caption = gtk4::Label::new(Some(&t("rows.rule_name.title", "Tên app")));
    caption.add_css_class("dim-label");
    caption.add_css_class("caption");
    caption.set_xalign(0.0);
    caption.set_margin_top(5);
    caption.set_margin_start(14);
    ui.rule_pill.append(&caption);
    ui.rule_pill.append(&ui.rule_name_entry);

    // Block B — mode and Thêm on one line, both children equal width; the
    // Thêm button is the only suggested-action of the group.
    let mode_pick = gtk4::DropDown::new(Some(StringList::new(MODES)), gtk4::Expression::NONE);
    // Preedit is the default rule mode (index of "Gạch chân (preedit)").
    mode_pick.set_selected(mode_index(ModeConfig::Preedit));
    mode_pick.set_hexpand(true);
    mode_pick.set_valign(gtk4::Align::Fill);
    mode_pick.set_height_request(40);
    mode_pick.add_css_class("rule-fill");
    mode_pick.set_tooltip_text(Some(&t("rows.rule_mode.tooltip", "Chế độ gõ của luật mới")));
    ui.rule_add_btn.set_height_request(40);
    let line = gtk4::Box::new(Orientation::Horizontal, 10);
    line.set_margin_top(8);
    line.set_margin_end(12);
    line.set_margin_bottom(8);
    line.set_margin_start(12);
    line.append(&mode_pick);
    line.append(&ui.rule_add_btn);

    let add_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.add.title", "Thêm luật"))
        .description(t(
            "groups.rules.add.description",
            "Chuyển sang app đích rồi quay lại, tên app sẽ hiện mờ trong ô nhập.",
        ))
        .build();
    add_group.add(&card);
    add_group.add(&line);
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

    // The page's own toast host: the copy/add confirmations must surface
    // somewhere, and the window shell stays untouched by this page work.
    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&page));

    // Typing or a new hint both move the Thêm button; detection only ever
    // shows as the placeholder, never over typed text.
    {
        let ui = ui.clone();
        let entry = ui.rule_name_entry.clone();
        entry.connect_changed(move |_| {
            ui.update_rule_add_state();
        });
    }
    // Enter in the field adds, exactly like the Thêm button.
    let add_rule: Rc<dyn Fn()> = {
        let ui = ui.clone();
        let mode_pick = mode_pick.clone();
        let overlay = toast_overlay.clone();
        Rc::new(move || {
            let typed = ui.rule_name_entry.text().trim().to_string();
            let pattern = if typed.is_empty() {
                ui.rule_focus_value.borrow().clone()
            } else {
                typed
            };
            if pattern.is_empty() {
                return;
            }
            let mode = mode_at(mode_pick.selected());
            ui.edit_rules(|rules| {
                rules.push(crate::config::AppRule {
                    pattern: pattern.clone(),
                    mode,
                });
            });
            // Ready for the next rule; the mode choice stays.
            ui.rule_name_entry.set_text("");
            overlay.add_toast(adw::Toast::new(
                &t("rules.add_toast", "Đã thêm luật cho {app}").replace("{app}", &pattern),
            ));
        })
    };
    {
        let add_rule = add_rule.clone();
        let entry = ui.rule_name_entry.clone();
        entry.connect_activate(move |_| add_rule());
    }
    {
        let add_rule = add_rule.clone();
        let btn = ui.rule_add_btn.clone();
        btn.connect_clicked(move |_| add_rule());
    }
    // "Dùng app đang focus" fills the field and moves focus there; the rule
    // itself is added by Thêm, like every hand-typed name.
    {
        let ui = ui.clone();
        let apply = ui.rule_apply_btn.clone();
        apply.connect_clicked(move |_| {
            let value = ui.rule_focus_value.borrow().clone();
            if value.is_empty() {
                return;
            }
            ui.rule_name_entry.set_text(&value);
            ui.rule_name_entry.grab_focus();
        });
    }
    {
        let ui = ui.clone();
        let overlay = toast_overlay.clone();
        let copy = ui.rule_copy_btn.clone();
        copy.connect_clicked(move |_| {
            let value = ui.rule_focus_value.borrow().clone();
            if value.is_empty() {
                return;
            }
            ui.rule_name_entry.clipboard().set_text(&value);
            overlay.add_toast(adw::Toast::new(
                &t("rules.copy_toast", "Đã sao chép {app}").replace("{app}", &value),
            ));
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

    toast_overlay
}
