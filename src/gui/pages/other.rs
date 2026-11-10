//! Page "Cài đặt khác": everything that is neither everyday typing nor a
//! per-feature page — the app-grid icon, the focus extension installer
//! (GNOME sessions only), the JSON backup and the restore-defaults action
//! next to the debug-log switch. The page owns its action buttons, their
//! handlers and the toast host; the restart banner stays with the shell
//! (the import and the reset reveal it through the shared button).

use super::*;

/// One regular (bordered) action button; `icon_name: None` renders label
/// only. The page's SizeGroup gives them all the widest button's width and
/// their shared label rows keep the heights equal.
fn action_button(icon_name: Option<&str>, label: &str) -> gtk4::Button {
    let btn = gtk4::Button::with_label(label);
    btn.set_valign(gtk4::Align::Center);
    if let Some(icon_name) = icon_name {
        let icon = gtk4::Image::from_icon_name(icon_name);
        let text = btn
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("with_label");
        // Detach the label before re-parenting it into the content box.
        btn.set_child(None::<&gtk4::Widget>);
        let content = gtk4::Box::new(Orientation::Horizontal, 6);
        content.append(&icon);
        content.append(&text);
        btn.set_child(Some(&content));
    }
    btn
}

/// A one-line JSON file filter for the open/save dialogs.
fn json_filter() -> gtk4::FileFilter {
    let filter = gtk4::FileFilter::new();
    filter.add_mime_type("application/json");
    filter.add_pattern("*.json");
    filter.set_name(Some(&t("other.json_filter", "Tệp JSON")));
    filter
}

/// The window a dialog presents on: the button's own root.
fn window_of(btn: &gtk4::Button) -> Option<gtk4::Window> {
    btn.root().and_downcast::<gtk4::Window>()
}

pub(crate) fn build(ui: &Rc<Ui>, restart_btn: &gtk4::Button) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();
    let toast_overlay = adw::ToastOverlay::new();

    // One shared width for every action button, the longest ("Khôi phục…")
    // deciding it.
    let sizes = gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal);

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
    interface.add(&ui.show_toasts);
    interface.add(&ui.language);
    page.add(&interface);

    // Focus extension: embedded, written into the user extension dir.
    // The group hides on non-GNOME sessions — the extension cannot run
    // anywhere else.
    let extension = adw::PreferencesGroup::builder()
        .title(t("groups.other.extension.title", "GNOME Shell extension"))
        .description(t(
            "groups.other.extension.description",
            "Cần cho luật theo app trên Wayland.",
        ))
        .build();
    let installed = focus_extension_installed();
    // The subtitle carries a monospace path (markup, no CSS): the text has
    // no & in it.
    let extension_row = adw::ActionRow::new();
    extension_row.set_use_markup(true);
    extension_row.set_title(&t("rows.focus_extension.title", "Extension MăngRS Focus"));
    extension_row.set_subtitle(&if installed {
        t(
            "rows.focus_extension.subtitle_installed",
            "Đã cài qua gnome-extensions",
        )
    } else {
        t(
            "rows.focus_extension.subtitle",
            "Cài qua gnome-extensions và bật",
        )
    });
    extension_row.set_tooltip_text(Some(&t(
        "tips.focus_extension",
        "Extension báo cho engine tên cửa sổ đang focus, để luật theo app chạy được với app Wayland thuần (Chrome, Firefox…).",
    )));
    let extension_btn = action_button(
        None,
        &if installed {
            t("misc.reinstall_button", "Cài lại")
        } else {
            t("misc.extension_button", "Cài đặt")
        },
    );
    sizes.add_widget(&extension_btn);
    extension_row.add_suffix(&extension_btn);
    // First-install logout: GNOME 45+ caches extension ES modules, so the
    // fresh install only loads after a logout. Session-scoped on purpose —
    // the button exists from the first successful install until the app
    // restarts, which a real logout always sits between.
    let logout_btn = action_button(None, &t("misc.logout_button", "Đăng xuất"));
    logout_btn.set_tooltip_text(Some(&t(
        "tips.extension_logout",
        "Đăng xuất để extension mới được tải (GNOME giữ module cũ trong bộ nhớ).",
    )));
    logout_btn.set_visible(false);
    sizes.add_widget(&logout_btn);
    extension_row.add_suffix(&logout_btn);
    extension.add(&extension_row);
    if session_is_gnome() {
        page.add(&extension);
    }

    // Backup: one JSON per machine transfer; the two rows share the size
    // group with the other buttons. Group titles are Pango markup fields
    // with no opt-out — the & must be escaped here and in the content file.
    let backup = adw::PreferencesGroup::builder()
        .title(t("groups.other.backup.title", "Sao lưu &amp; khôi phục"))
        .description(t(
            "groups.other.backup.description",
            "Một tệp JSON chứa cấu hình, gõ tắt và từ điển.",
        ))
        .build();
    let export_row = action_row(
        &t("rows.export.title", "Xuất cấu hình ra JSON"),
        &t(
            "rows.export.subtitle",
            "Lưu ra tệp để mang đi hoặc dự phòng",
        ),
    );
    export_row.set_tooltip_text(Some(&t(
        "tips.export",
        "Lưu toàn bộ cấu hình, gõ tắt và từ điển ra một file JSON để mang sang máy khác.",
    )));
    let export_btn = action_button(
        Some("document-save-symbolic"),
        &t("misc.export_button", "Xuất…"),
    );
    sizes.add_widget(&export_btn);
    export_row.add_suffix(&export_btn);
    backup.add(&export_row);
    let import_row = action_row(
        &t("rows.import.title", "Nhập cấu hình từ JSON"),
        &t("rows.import.subtitle", "Đọc lại từ tệp đã xuất"),
    );
    import_row.set_tooltip_text(Some(&t(
        "tips.import",
        "Đọc lại file JSON đã xuất: ghi đè cấu hình, gõ tắt và từ điển hiện tại.",
    )));
    let import_btn = action_button(
        Some("document-revert-symbolic"),
        &t("misc.import_button", "Nhập…"),
    );
    sizes.add_widget(&import_btn);
    import_row.add_suffix(&import_btn);
    backup.add(&import_row);
    page.add(&backup);

    // Advanced: the debug-log switches and the destructive reset share one
    // group, so the page holds four groups instead of five fragments. The
    // experimental gate moved into the "Thử nghiệm" tab it controls.
    let advanced = adw::PreferencesGroup::builder()
        .title(t("groups.other.advanced.title", "Nâng cao"))
        .build();
    advanced.add(&ui.debug_log);
    advanced.add(&ui.debug_log_text);
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
    // Label only and no red: the reference keeps the reset quiet, the
    // destructive confirmation dialog carries the warning.
    let reset_btn = action_button(None, &t("misc.reset_button", "Khôi phục…"));
    sizes.add_widget(&reset_btn);
    reset_row.add_suffix(&reset_btn);
    advanced.add(&reset_row);
    page.add(&advanced);

    // The icon switch talks to the filesystem; report through a toast.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
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
                    ui.fire_toast(&overlay_toasts, &msg);
                }
                Err(e) => {
                    ui.fire_toast(&overlay_toasts, &e);
                    // Revert the switch so it shows the real disk state.
                    *ui.desktop_icon_loading.borrow_mut() = true;
                    ui.desktop_icon.set_active(!hide);
                    *ui.desktop_icon_loading.borrow_mut() = false;
                }
            }
        });
    }

    // Export: save the whole state as one JSON file.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
        export_btn.connect_clicked(move |btn| {
            let dialog = gtk4::FileDialog::builder()
                .title(t("misc.export_dialog", "Xuất cấu hình"))
                .modal(true)
                .initial_name("mangrs-config.json")
                .default_filter(&json_filter())
                .build();
            let overlay_toasts = overlay_toasts.clone();
            let ui = ui.clone();
            let window = window_of(btn);
            let dialog_parent = window.clone();
            dialog.save(
                dialog_parent.as_ref(),
                gtk4::gio::Cancellable::NONE,
                // A dismissed dialog is also an Err: nothing to save, no error.
                move |result| {
                    if let Ok(file) = result {
                        match export_json(file.path()) {
                            Ok(()) => ui.fire_toast(
                                &overlay_toasts,
                                &t("misc.exported_toast", "Đã xuất cấu hình"),
                            ),
                            Err(e) => {
                                if let Some(window) = window {
                                    alert(&window, &e);
                                }
                            }
                        }
                    }
                },
            );
        });
    }

    // Import: read a JSON backup back in; a success rewrites config.toml
    // wholesale, so the restart banner applies like any other save.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
        let restart_btn = restart_btn.clone();
        import_btn.connect_clicked(move |btn| {
            let dialog = gtk4::FileDialog::builder()
                .title(t("misc.import_dialog", "Nhập cấu hình"))
                .modal(true)
                .default_filter(&json_filter())
                .build();
            let ui = ui.clone();
            let overlay_toasts = overlay_toasts.clone();
            let restart_btn = restart_btn.clone();
            let window = window_of(btn);
            let dialog_parent = window.clone();
            dialog.open(
                dialog_parent.as_ref(),
                gtk4::gio::Cancellable::NONE,
                // A dismissed dialog is also an Err: nothing to import.
                move |result| {
                    if let Ok(file) = result {
                        match import_json(file.path(), &ui) {
                            Ok(()) => {
                                restart_btn.set_visible(true);
                                ui.fire_toast(
                                    &overlay_toasts,
                                    &t("misc.imported_toast", "Đã nhập cấu hình"),
                                );
                            }
                            Err(e) => {
                                if let Some(window) = window {
                                    alert(&window, &e);
                                }
                            }
                        }
                    }
                },
            );
        });
    }

    // Install: confirmed like the destructive actions — the install goes
    // through the official gnome-extensions channel and flips a GNOME
    // setting, so a stray click must not do it. Same dialog shape as the
    // reset. No manager-GUI gate: the install needs only the
    // gnome-extensions CLI that ships with GNOME Shell, and the
    // post-install note points at the Extensions apps when enable fails.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
        let row = extension_row.clone();
        let logout_btn = logout_btn.clone();
        extension_btn.connect_clicked(move |btn| {
            let dialog = adw::AlertDialog::builder()
                .heading(t("other.extension_confirm_heading", "Cài extension MăngRS Focus?"))
                .body(t(
                    "other.extension_confirm_body",
                    "Extension được cài qua gnome-extensions (kênh chính thức của GNOME) và được bật — cần cho luật theo app trên Wayland. Lần cài đầu sẽ hiện nút Đăng xuất.",
                ))
                .build();
            dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
            dialog.add_response("apply", &t("other.extension_apply", "Cài đặt"));
            dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            {
                let ui = ui.clone();
                let overlay_toasts = overlay_toasts.clone();
                let row = row.clone();
                let btn = btn.clone();
                let logout_btn = logout_btn.clone();
                dialog.connect_response(Some("apply"), move |_, _| {
                    match install_focus_extension() {
                        Ok(msg) => {
                            ui.fire_toast(&overlay_toasts, &msg);
                            row.set_subtitle(&t(
                                "rows.focus_extension.subtitle_installed",
                                "Đã cài qua gnome-extensions",
                            ));
                            btn.set_label(&t("misc.reinstall_button", "Cài lại"));
                            logout_btn.set_visible(true);
                        }
                        Err(e) => {
                            if let Some(window) = window_of(&btn) {
                                alert(&window, &e);
                            }
                        }
                    }
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }

    // The logout itself: GNOME asks nothing with --no-prompt, and the
    // button only exists because an install is waiting for a reload.
    {
        logout_btn.connect_clicked(|_| {
            let _ = std::process::Command::new("gnome-session-quit")
                .args(["--logout", "--no-prompt"])
                .status();
        });
    }

    // Restore defaults: destructive, confirmed; macros and the dictionary
    // survive, everything else rewrites to the shipped values.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
        let restart_btn = restart_btn.clone();
        reset_btn.connect_clicked(move |btn| {
            let dialog = adw::AlertDialog::builder()
                .heading(t("other.reset_heading", "Khôi phục mặc định?"))
                .body(t(
                    "other.reset_body",
                    "Cấu hình sẽ về mặc định. Gõ tắt và từ điển được giữ nguyên.",
                ))
                .build();
            dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
            dialog.add_response("apply", &t("other.reset_apply", "Khôi phục"));
            dialog.set_response_appearance("apply", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            {
                let ui = ui.clone();
                let overlay_toasts = overlay_toasts.clone();
                let restart_btn = restart_btn.clone();
                let window = window_of(btn);
                dialog.connect_response(Some("apply"), move |_, _| {
                    if let Err(e) = Config::default().save_fresh() {
                        if let Some(window) = &window {
                            alert(window, &format!("Không ghi được cấu hình mặc định: {e}"));
                        }
                        return;
                    }
                    let config = Config::load();
                    let macros = Macros::load();
                    let dictionary = Dictionary::load();
                    // Suppressed like `external_reload`: the refresh fires
                    // the row handlers, and a mid-refresh save would write
                    // back the pre-reset values still held by the widgets.
                    ui.suppress_save.set(true);
                    ui.refresh(&config, &macros, &dictionary);
                    ui.suppress_save.set(false);
                    restart_btn.set_visible(true);
                    ui.fire_toast(
                        &overlay_toasts,
                        &t("misc.reset_toast", "Đã khôi phục mặc định"),
                    );
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }

    toast_overlay.set_child(Some(&page));
    toast_overlay
}
