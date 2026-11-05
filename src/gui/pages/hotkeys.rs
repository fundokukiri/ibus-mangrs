//! Page "Phím tắt": every engine keyboard shortcut in one place, one
//! GNOME-style recorder row each. The engine side is the registry in
//! `server/hotkey.rs` — one config field, one registry row and one row
//! here per shortcut.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let group = adw::PreferencesGroup::builder()
        .title(t("groups.hotkeys.keys.title", "Phím tắt"))
        .description(t(
            "groups.hotkeys.keys.description",
            "Bấm vào nút bút chì rồi nhấn tổ hợp phím; Esc hủy, Backspace xóa",
        ))
        .build();
    let mode_hotkey = action_row(
        &t("rows.hotkey_mode.title", "Phím tắt đổi chế độ"),
        &t("rows.hotkey_mode.subtitle", "Ví dụ: Control+Alt+v"),
    );
    mode_hotkey.add_suffix(&ui.hotkey_label);
    mode_hotkey.add_suffix(&ui.hotkey_edit);
    mode_hotkey.set_tooltip_text(Some(&t(
        "tips.hotkey_mode",
        "Gán phím chuyển tuần tự qua các chế độ gõ. Bấm vào nút bút chì, nhấn tổ hợp phím; Esc hủy, Backspace xóa.",
    )));
    group.add(&mode_hotkey);
    let enabled_hotkey = action_row(
        &t("rows.hotkey_enabled.title", "Phím tắt bật/tắt tiếng Việt"),
        &t("rows.hotkey_enabled.subtitle", "Ví dụ: Control+Alt+space"),
    );
    enabled_hotkey.add_suffix(&ui.enabled_hotkey_label);
    enabled_hotkey.add_suffix(&ui.enabled_hotkey_edit);
    enabled_hotkey.set_tooltip_text(Some(&t(
        "tips.hotkey_enabled",
        "Gán phím bật/tắt tiếng Việt nhanh. Bấm vào nút bút chì, nhấn tổ hợp phím; Esc hủy, Backspace xóa.",
    )));
    group.add(&enabled_hotkey);
    let app_rule_hotkey = action_row(
        &t("rows.hotkey_app_rule.title", "Phím tắt chọn luật theo app"),
        &t(
            "rows.hotkey_app_rule.subtitle",
            "Mở cửa sổ chọn chế độ cho app đang focus",
        ),
    );
    app_rule_hotkey.add_suffix(&ui.app_rule_hotkey_label);
    app_rule_hotkey.add_suffix(&ui.app_rule_hotkey_edit);
    app_rule_hotkey.set_tooltip_text(Some(&t(
        "tips.hotkey_app_rule",
        "Gán phím mở cửa sổ chọn chế độ gõ cho app đang focus (kiểu Shift + ` của ibus-bamboo). Bấm vào nút bút chì, nhấn tổ hợp phím; Esc hủy, Backspace xóa.",
    )));
    group.add(&app_rule_hotkey);
    let emoji_hotkey = action_row(
        &t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"),
        &t(
            "rows.hotkey_emoji.subtitle",
            "Mở menu chọn emoji tại con trỏ: 1-9/0 chọn, gõ chữ để tìm",
        ),
    );
    emoji_hotkey.add_suffix(&ui.emoji_hotkey_label);
    emoji_hotkey.add_suffix(&ui.emoji_hotkey_edit);
    emoji_hotkey.set_tooltip_text(Some(&t(
        "tips.emoji_hotkey",
        "Gán phím mở menu chọn emoji kiểu ibus-bamboo: bảng ứng viên hiện ngay tại con trỏ, gõ chữ để tìm, phím số chọn, Esc hoặc click ra chỗ khác đóng. Bấm vào nút bút chì, nhấn tổ hợp phím; Esc hủy, Backspace xóa.",
    )));
    group.add(&emoji_hotkey);
    page.add(&group);

    // Custom shortcuts: one row per assignment (spec + function), the add
    // row captures a combo through its own recorder and appends. The
    // design vector lives in Ui; rows rebuild on every edit.
    let custom_group = adw::PreferencesGroup::builder()
        .title(t("groups.hotkeys.custom.title", "Phím tắt riêng"))
        .description(t(
            "groups.hotkeys.custom.description",
            "Gán thêm phím tắt cho cùng chức năng — 4 phím ở trên là mặc định",
        ))
        .build();
    custom_group.add(&ui.custom_box);

    // The add row: plain widgets in one line — [function] [pencil+combo]
    // [Thêm]. It reads like a form, not a settings row.
    let add_line = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let entry_pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    entry_pill.add_css_class("card");
    entry_pill.set_margin_start(6);
    entry_pill.set_margin_end(6);
    entry_pill.set_margin_top(6);
    entry_pill.set_margin_bottom(6);
    entry_pill.set_valign(gtk4::Align::Center);
    ui.custom_action.set_valign(gtk4::Align::Center);
    entry_pill.append(&ui.custom_edit);
    entry_pill.append(&ui.custom_label);
    entry_pill.append(&ui.custom_action);
    add_line.append(&entry_pill);
    ui.custom_add.set_valign(gtk4::Align::Center);
    add_line.append(&ui.custom_add);
    {
        let ui = ui.clone();
        let add_button = ui.custom_add.clone();
        add_button.connect_clicked(move |_| {
            let spec = ui.custom_value.borrow().clone();
            if spec.is_empty() {
                return;
            }
            let action = crate::config::CUSTOM_HOTKEY_ACTIONS
                .get(ui.custom_action.selected() as usize)
                .unwrap_or(&"enabled")
                .to_string();
            ui.custom.borrow_mut().push((spec, action));
            *ui.custom_value.borrow_mut() = String::new();
            ui.custom_label.set_text(&t("misc.unassigned", "Chưa gán"));
            ui.fill_custom();
            ui.fire_save();
        });
    }
    // The recorder fills the dim label; capturing must not save (the Thêm
    // button does), but it enables/disables Thêm.
    {
        let value = ui.custom_value.clone();
        let add = ui.custom_add.clone();
        hotkey_wire(
            &ui.custom_edit,
            &ui.custom_label,
            &ui.custom_value,
            move || {
                add.set_sensitive(!value.borrow().is_empty());
            },
        );
    }
    custom_group.add(&add_line);
    page.add(&custom_group);

    // The recorders capture on the edit button (GNOME shortcut style) and
    // save through the shared funnel handle.
    {
        let ui = ui.clone();
        let s = ui.save_cb.clone();
        hotkey_wire(
            &ui.hotkey_edit,
            &ui.hotkey_label,
            &ui.hotkey_value,
            move || {
                if let Some(save) = s.borrow().as_ref() {
                    save();
                }
            },
        );
    }
    {
        let ui = ui.clone();
        let s = ui.save_cb.clone();
        hotkey_wire(
            &ui.enabled_hotkey_edit,
            &ui.enabled_hotkey_label,
            &ui.enabled_hotkey_value,
            move || {
                if let Some(save) = s.borrow().as_ref() {
                    save();
                }
            },
        );
    }
    {
        let ui = ui.clone();
        let s = ui.save_cb.clone();
        hotkey_wire(
            &ui.app_rule_hotkey_edit,
            &ui.app_rule_hotkey_label,
            &ui.app_rule_hotkey_value,
            move || {
                if let Some(save) = s.borrow().as_ref() {
                    save();
                }
            },
        );
    }
    {
        let ui = ui.clone();
        let s = ui.save_cb.clone();
        hotkey_wire(
            &ui.emoji_hotkey_edit,
            &ui.emoji_hotkey_label,
            &ui.emoji_hotkey_value,
            move || {
                if let Some(save) = s.borrow().as_ref() {
                    save();
                }
            },
        );
    }

    // The mode-cycle order is data, not decoration: a dim note row so the
    // hotkey's sequence is documented next to its button.
    let order = adw::PreferencesGroup::new();
    let note = gtk4::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .label(t(
            "groups.hotkeys.cycle_note.text",
            "Thứ tự phím tắt đổi chế độ: Tự động → Không gạch chân → Gạch chân → Surrounding → Smooth → Super Smooth → Select → ForwardKeyEvent → ForwardKeyEvent II → OFF.",
        ))
        .build();
    note.add_css_class("dim-label");
    order.add(&note);
    page.add(&order);

    page
}
