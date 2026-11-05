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
    page.add(&group);

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
