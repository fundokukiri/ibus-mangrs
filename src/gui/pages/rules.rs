//! Page "Luật": the per-app rule editor — focused-app chip, quick-add row,
//! the rule list (one editable row per rule) and the match preview. All
//! page widgets are created and wired here; the save funnel comes from the
//! shared `save_cb` handle.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
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
    let combo_labels: Vec<&str> = app_targets
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();

    let focus = adw::PreferencesGroup::builder()
        .title(t("groups.rules.focus.title", "Luật theo ứng dụng"))
        .description(t(
            "groups.rules.focus.description",
            "Luật đầu tiên thắng; none = app không báo tên",
        ))
        .build();
    focus.set_tooltip_text(Some(&t(
        "groups.rules.focus.tooltip",
        "Tên app khớp một phần, không phân biệt hoa/thường; đủ 10 chế độ gõ.",
    )));
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
    focus.add(&focus_row);
    page.add(&focus);

    // One add row: pick a known app, or copy the focused one, choose the
    // mode, and the rule lands in the list immediately — no intermediate
    // pattern entry to fill in.
    let add_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.add.title", "Thêm luật"))
        .description(t(
            "groups.rules.add.description",
            "Chọn app đã thấy hoặc copy app đang focus — luật vào thẳng danh sách",
        ))
        .build();
    let client_combo = drop_down(&combo_labels, 0);
    client_combo.set_tooltip_text(Some(&t(
        "rows.rule_combo.tooltip",
        "Các app engine đã từng thấy",
    )));
    let or_label = gtk4::Label::new(Some(&t("rules.or_label", "hoặc")));
    or_label.add_css_class("dim-label");
    let focus_use_btn = gtk4::Button::from_icon_name("edit-copy-symbolic");
    focus_use_btn.add_css_class("flat");
    focus_use_btn.set_tooltip_text(Some(&t(
        "rows.rule_copy.tooltip",
        "Thêm luật cho app đang focus — bấm là luật vào thẳng danh sách",
    )));
    let rule_mode = drop_down(MODES, 1);
    rule_mode.set_tooltip_text(Some(&t("rows.rule_mode.tooltip", "Chế độ gõ của luật mới")));
    let add_btn = gtk4::Button::with_label(&t("rules.add_button", "Thêm luật"));
    add_btn.add_css_class("suggested-action");
    let add_row = gtk4::Box::new(Orientation::Horizontal, 8);
    add_row.set_margin_start(10);
    add_row.set_margin_end(10);
    add_row.set_margin_top(8);
    add_row.set_margin_bottom(8);
    add_row.append(&client_combo);
    add_row.append(&or_label);
    add_row.append(&focus_use_btn);
    add_row.append(&rule_mode);
    add_row.append(&add_btn);
    add_group.add(&add_row);
    page.add(&add_group);

    let list_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.list.title", "Danh sách luật"))
        .build();
    list_group.set_tooltip_text(Some(&t(
        "groups.rules.list.tooltip",
        "Chỉ app dùng GTK IM module mới báo tên riêng; app Wayland hiện là gnome-shell, app X11/khác là none. Cài extension MăngRS Focus (data/gnome-shell-extension) để nhận tên app Wayland-native.",
    )));
    list_group.add(&ui.rules_box);
    page.add(&list_group);

    let preview_group = adw::PreferencesGroup::builder()
        .title(t("groups.rules.preview.title", "Thử luật"))
        .description(t(
            "groups.rules.preview.description",
            "Nhập tên app bất kỳ: cho biết luật nào khớp và chế độ gõ kết quả",
        ))
        .build();
    preview_group.add(&ui.rule_preview_input);
    preview_group.add(&ui.rule_preview);
    page.add(&preview_group);

    // "Dùng tên này" appends the focused app straight into the rule list,
    // with the mode picked in the add row.
    {
        let ui = ui.clone();
        let rule_mode = rule_mode.clone();
        focus_use_btn.connect_clicked(move |_| {
            let pattern = ui.rule_focus_value.borrow().clone();
            if pattern.is_empty() {
                return;
            }
            let mode = mode_at(rule_mode.selected());
            ui.edit_rules(move |rules| {
                rules.push(crate::config::AppRule { pattern, mode });
            });
        });
    }
    // The quick-add row appends a rule for the picked app, then saves
    // through the same error surface as every other control.
    {
        let ui = ui.clone();
        let client_combo = client_combo.clone();
        let rule_mode = rule_mode.clone();
        add_btn.connect_clicked(move |_| {
            let idx = client_combo.selected() as usize;
            let Some(pattern) = app_targets.get(idx).map(|(_, p)| p.clone()) else {
                return;
            };
            if pattern.is_empty() {
                return;
            }
            let mode = mode_at(rule_mode.selected());
            ui.edit_rules(move |rules| {
                rules.push(crate::config::AppRule { pattern, mode });
            });
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

    page
}
