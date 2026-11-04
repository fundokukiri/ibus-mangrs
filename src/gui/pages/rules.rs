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
        "gnome-shell (app Wayland)".to_string(),
        "gnome-shell".to_string(),
    ));
    app_targets.push(("(app không báo tên)".to_string(), "none".to_string()));
    let combo_labels: Vec<&str> = app_targets
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();

    let focus = adw::PreferencesGroup::builder()
        .title("Luật theo ứng dụng")
        .description("Luật đầu tiên thắng. Tên app khớp một phần, không phân biệt hoa/thường; none = app không báo tên. Đủ 10 chế độ gõ.")
        .build();
    let focus_row = action_row(
        "App đang focus",
        "Chuyển sang app đích rồi quay lại — tên tự cập nhật qua extension MăngRS Focus",
    );
    ui.rule_focus.set_width_chars(24);
    focus_row.add_suffix(&ui.rule_focus);
    let focus_use_btn = gtk4::Button::from_icon_name("edit-paste-symbolic");
    focus_use_btn.set_tooltip_text(Some("Điền tên đang hiện vào ô thêm luật bên dưới"));
    focus_row.add_suffix(&focus_use_btn);
    focus.add(&focus_row);
    page.add(&focus);

    let add_group = adw::PreferencesGroup::builder().title("Thêm luật").build();
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
    add_btn.add_css_class("suggested-action");
    let add_row = gtk4::Box::new(Orientation::Horizontal, 8);
    add_row.set_margin_start(10);
    add_row.set_margin_end(10);
    add_row.set_margin_top(8);
    add_row.set_margin_bottom(8);
    add_row.append(&client_combo);
    add_row.append(&rule_pattern);
    add_row.append(&rule_mode);
    add_row.append(&add_btn);
    add_group.add(&add_row);
    page.add(&add_group);

    let list_group = adw::PreferencesGroup::builder()
        .title("Danh sách luật")
        .build();
    list_group.add(&ui.rules_box);
    page.add(&list_group);

    let preview_group = adw::PreferencesGroup::builder()
        .title("Thử luật")
        .description("Nhập tên app bất kỳ: cho biết luật nào khớp và chế độ gõ kết quả")
        .build();
    preview_group.add(&ui.rule_preview_input);
    preview_group.add(&ui.rule_preview);
    page.add(&preview_group);

    let limits = adw::PreferencesGroup::new();
    let note = gtk4::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .label("Chỉ app dùng GTK IM module mới báo tên riêng cho engine; app Wayland hiện là gnome-shell, app X11/khác là none. Cài extension MăngRS Focus (data/gnome-shell-extension) để nhận tên app Wayland-native: Chrome, Firefox, … Danh sách thêm nhanh là các app engine đã từng thấy, không phải mọi app đang chạy.")
        .build();
    note.add_css_class("dim-label");
    limits.add(&note);
    page.add(&limits);

    // "Dùng tên này" fills the typed-pattern entry with the chip's name.
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
    // The quick-add row appends a rule row, then saves through the same
    // error surface as every other control. A typed pattern wins over the
    // dropdown choice.
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
