//! Page "Chung": typing on/off, the two hotkeys, input style and the
//! preedit underline — the things users touch every day.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let status = adw::PreferencesGroup::builder()
        .title(t("groups.typing.status.title", "Trạng thái"))
        .description(t(
            "groups.typing.status.description",
            "Bật tiếng Việt và phím tắt đổi chế độ",
        ))
        .build();
    status.add(&ui.enabled);
    let mode_hotkey = action_row(
        &t("rows.hotkey_mode.title", "Phím tắt đổi chế độ"),
        &t("rows.hotkey_mode.subtitle", "Ví dụ: Control+Alt+v"),
    );
    mode_hotkey.add_suffix(&ui.hotkey_label);
    mode_hotkey.add_suffix(&ui.hotkey_edit);
    status.add(&mode_hotkey);
    let enabled_hotkey = action_row(
        &t("rows.hotkey_enabled.title", "Phím tắt bật/tắt tiếng Việt"),
        &t("rows.hotkey_enabled.subtitle", "Ví dụ: Control+Alt+space"),
    );
    enabled_hotkey.add_suffix(&ui.enabled_hotkey_label);
    enabled_hotkey.add_suffix(&ui.enabled_hotkey_edit);
    status.add(&enabled_hotkey);
    mode_hotkey.set_tooltip_text(Some(&t(
        "tips.hotkey_mode",
        "Gán phím chuyển tuần tự qua các chế độ gõ. Bấm vào nút bút chì, nhấn tổ hợp phím; Esc hủy, Backspace xóa.",
    )));
    enabled_hotkey.set_tooltip_text(Some(&t(
        "tips.hotkey_enabled",
        "Gán phím bật/tắt tiếng Việt nhanh. Bấm vào nút bút chì, nhấn tổ hợp phím; Esc hủy, Backspace xóa.",
    )));
    page.add(&status);
    // The recorder captures on the edit button (GNOME shortcut style) and
    // saves through the shared funnel handle.
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

    let style = adw::PreferencesGroup::builder()
        .title(t("groups.typing.style.title", "Cách gõ"))
        .description(t(
            "groups.typing.style.description",
            "Kiểu gõ, bảng mã và chế độ gõ",
        ))
        .build();
    style.add(&ui.method);
    style.add(&ui.charset);
    style.add(&ui.mode);
    page.add(&style);

    let display = adw::PreferencesGroup::builder()
        .title(t("groups.typing.display.title", "Hiển thị"))
        .description(t(
            "groups.typing.display.description",
            "Hình thức của chữ đang gõ",
        ))
        .build();
    display.add(&ui.hide_underline);
    page.add(&display);

    // The mode-cycle order is data, not decoration: a dim note row so the
    // hotkey's sequence is documented next to its button.
    let order = adw::PreferencesGroup::new();
    let note = gtk4::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .label(t(
            "groups.typing.cycle_note.text",
            "Thứ tự phím tắt đổi chế độ: Không gạch chân → Gạch chân → Surrounding → Smooth → Super Smooth → Select → ForwardKeyEvent → ForwardKeyEvent II → OFF → Gõ mặc định.",
        ))
        .build();
    note.add_css_class("dim-label");
    order.add(&note);
    page.add(&order);

    page
}
