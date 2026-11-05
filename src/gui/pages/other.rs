//! Page "Cài đặt khác": everything that is neither everyday typing nor a
//! per-feature page — the app-grid icon, the focus extension installer,
//! the JSON backup, the debug log and the restore-defaults button. The
//! uinput pacing and the X11/XIM workarounds moved to their own "Tinh
//! chỉnh" page. This host page carries a ToastOverlay: the icon switch
//! reports on the filesystem action itself.

use super::*;

pub(crate) fn build(
    ui: &Rc<Ui>,
    export_btn: &gtk4::Button,
    import_btn: &gtk4::Button,
    extension_btn: &gtk4::Button,
    reset_btn: &gtk4::Button,
) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();

    // App-grid icon: the switch writes/removes a user-level desktop-entry
    // override; `refresh` keeps the state in sync from the filesystem.
    let interface = adw::PreferencesGroup::builder()
        .title(t("groups.other.interface.title", "Giao diện"))
        .build();
    ui.desktop_icon.set_tooltip_text(Some(&t(
        "tips.desktop_icon",
        "Ẩn hay hiện dòng \u{201c}MăngRS Cấu hình\u{201d} trong lưới ứng dụng của GNOME (cần vài giây để GNOME nhận thay đổi).",
    )));
    interface.add(&ui.desktop_icon);
    page.add(&interface);

    // Focus extension: embedded, written into the user extension dir.
    let extension = adw::PreferencesGroup::builder()
        .title(t("groups.other.extension.title", "GNOME Shell extension"))
        .description(t(
            "groups.other.extension.description",
            "Cần cho luật theo app trên Wayland (tên cửa sổ đang focus)",
        ))
        .build();
    let extension_row = action_row(
        &t("rows.focus_extension.title", "Extension MăngRS Focus"),
        &t(
            "rows.focus_extension.subtitle",
            "Cài vào ~/.local/share/gnome-shell/extensions và bật",
        ),
    );
    extension_row.set_tooltip_text(Some(&t(
        "tips.focus_extension",
        "Extension báo cho engine tên cửa sổ đang focus, để luật theo app chạy được với app Wayland thuần (Chrome, Firefox…).",
    )));
    extension_row.add_suffix(extension_btn);
    extension.add(&extension_row);
    page.add(&extension);

    // Backup: moved from "Tinh chỉnh", logic unchanged. Group titles are
    // Pango markup fields with no opt-out — the & must be escaped here and
    // in the content file.
    let backup = adw::PreferencesGroup::builder()
        .title(t("groups.other.backup.title", "Sao lưu &amp; khôi phục"))
        .description(t(
            "groups.other.backup.description",
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

    // Diagnostics: moved from "Tinh chỉnh".
    let diagnostics = adw::PreferencesGroup::builder()
        .title(t("groups.other.diagnostics.title", "Chẩn đoán"))
        .description(t(
            "groups.other.diagnostics.description",
            "Ghi lại quyết định của engine để gỡ lỗi",
        ))
        .build();
    diagnostics.add(&ui.debug_log);
    page.add(&diagnostics);

    // Restore defaults: destructive, confirmed in content.rs where the
    // window and the restart banner live.
    let reset = adw::PreferencesGroup::builder()
        .title(t("groups.other.reset.title", "Khôi phục"))
        .build();
    let reset_row = action_row(
        &t("rows.reset_defaults.title", "Khôi phục mặc định"),
        &t(
            "rows.reset_defaults.subtitle",
            "Cấu hình về mặc định; gõ tắt và từ điển giữ nguyên",
        ),
    );
    reset_row.set_tooltip_text(Some(&t(
        "tips.reset_defaults",
        "Ghi đè config.toml bằng giá trị mặc định (kể cả menu panel, luật theo app, phím tắt). Gõ tắt và từ điển không bị đụng đến.",
    )));
    reset_row.add_suffix(reset_btn);
    reset.add(&reset_row);
    page.add(&reset);

    // The icon switch talks to the filesystem; report through a toast.
    let toast_overlay = adw::ToastOverlay::new();
    {
        let ui = ui.clone();
        let overlay = toast_overlay.clone();
        let switch = ui.desktop_icon.clone();
        switch.connect_active_notify(move |sw| {
            if *ui.desktop_icon_loading.borrow() {
                return;
            }
            let hide = !sw.is_active();
            match set_desktop_icon_hidden(hide) {
                Ok(()) => {
                    let msg = if hide {
                        t("misc.other_icon_toast_hidden", "Đã ẩn khỏi menu ứng dụng")
                    } else {
                        t(
                            "misc.other_icon_toast_shown",
                            "Đã hiện lại trong menu ứng dụng",
                        )
                    };
                    overlay.add_toast(adw::Toast::new(&msg));
                }
                Err(e) => {
                    overlay.add_toast(adw::Toast::new(&e));
                    // Revert the switch so it shows the real disk state.
                    *ui.desktop_icon_loading.borrow_mut() = true;
                    ui.desktop_icon.set_active(!hide);
                    *ui.desktop_icon_loading.borrow_mut() = false;
                }
            }
        });
    }
    toast_overlay.set_child(Some(&page));
    toast_overlay
}
