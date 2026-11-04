//! Page "Tinh chỉnh": the set-once technical settings — the uinput
//! pacing, the X11/XIM workarounds, the JSON backup and the debug-log
//! switch. The panel menu editor has its own page (see panel.rs).

use super::*;

pub(crate) fn build(
    ui: &Rc<Ui>,
    export_btn: &gtk4::Button,
    import_btn: &gtk4::Button,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The uinput pacing applies to Smooth / Super Smooth / Select only; the
    // spin rows save on every value change like every other control.
    let pacing = adw::PreferencesGroup::builder()
        .title(t("groups.advanced.pacing.title", "Tốc độ gõ (uinput)"))
        .description(t(
            "groups.advanced.pacing.description",
            "Chỉ áp dụng cho Smooth / Super Smooth / Select",
        ))
        .build();
    pacing.add(&ui.commit_interval);
    pacing.add(&ui.backspace_interval);
    pacing.add(&ui.post_delay);
    pacing.add(&ui.unicode_channel);
    page.add(&pacing);
    for row in [&ui.commit_interval, &ui.backspace_interval, &ui.post_delay] {
        let s = ui.save_cb.clone();
        row.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }

    let xim = adw::PreferencesGroup::builder()
        .title(t("groups.advanced.xim.title", "Tương thích X11/XIM"))
        .description(t(
            "groups.advanced.xim.description",
            "Cho app X11 trực tiếp và app XIM (Java/JetBrains)",
        ))
        .build();
    xim.add(&ui.x11_delete);
    xim.add(&ui.preedit_xim);
    page.add(&xim);

    let backup = adw::PreferencesGroup::builder()
        .title(t("groups.advanced.backup.title", "Sao lưu & khôi phục"))
        .description(t(
            "groups.advanced.backup.description",
            "Một JSON chứa cấu hình, gõ tắt và từ điển — chuyển máy hoặc dự phòng",
        ))
        .build();
    let export = action_row(
        &t("rows.export.title", "Xuất cấu hình ra JSON"),
        &t("rows.export.subtitle", "Lưu ra file để mang đi"),
    );
    export.set_tooltip_text(Some(&t(
        "tips.export",
        "Lưu toàn bộ cấu hình, gõ tắt và từ điển ra một file JSON để mang sang máy khác.",
    )));
    export.add_suffix(export_btn);
    backup.add(&export);
    let import = action_row(
        &t("rows.import.title", "Nhập cấu hình từ JSON"),
        &t("rows.import.subtitle", "Đọc lại từ file đã xuất"),
    );
    import.set_tooltip_text(Some(&t(
        "tips.import",
        "Đọc lại file JSON đã xuất: ghi đè cấu hình, gõ tắt và từ điển hiện tại.",
    )));
    import.add_suffix(import_btn);
    backup.add(&import);
    page.add(&backup);

    let diagnostics = adw::PreferencesGroup::builder()
        .title(t("groups.advanced.diagnostics.title", "Chẩn đoán"))
        .description(t(
            "groups.advanced.diagnostics.description",
            "Ghi lại quyết định của engine để gỡ lỗi",
        ))
        .build();
    diagnostics.add(&ui.debug_log);
    page.add(&diagnostics);

    page
}
