//! Page "Cài đặt nâng cao": everything that is neither everyday typing nor a
//! per-feature page — the app-grid icon, the focus extension installer
//! (GNOME sessions only), the JSON backup and the restore-defaults action
//! next to the debug-log switch. The page owns its action buttons, their
//! handlers and the toast host.

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
/// Wire one extension button: confirm, run, then let the row read the disk
/// and the shell again. `fixed` is the request the button always makes; the
/// primary button follows the state instead.
fn wire_extension_button(
    btn: &gtk4::Button,
    fixed: Option<ExtensionRequest>,
    ui: &Rc<Ui>,
    overlay_toasts: &adw::ToastOverlay,
    row: &adw::ActionRow,
    all: &[gtk4::Button],
) {
    let ui = ui.clone();
    let overlay_toasts = overlay_toasts.clone();
    let row = row.clone();
    let all: Vec<gtk4::Button> = all.to_vec();
    btn.connect_clicked(move |btn| {
        let request = match fixed {
            Some(request) => request,
            None => match extension_row_for(extension_state()).button {
                Some((_, request)) => request,
                None => return,
            },
        };
        let (heading, body, apply) = extension_dialog(request);
        let destructive = matches!(request, ExtensionRequest::Remove | ExtensionRequest::Logout);
        let dialog = adw::AlertDialog::builder()
            .heading(&heading)
            .body(&body)
            .build();
        dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
        dialog.add_response("apply", &apply);
        dialog.set_response_appearance(
            "apply",
            if destructive {
                adw::ResponseAppearance::Destructive
            } else {
                adw::ResponseAppearance::Suggested
            },
        );
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        {
            let ui = ui.clone();
            let overlay_toasts = overlay_toasts.clone();
            let row = row.clone();
            let all = all.clone();
            let btn = btn.clone();
            dialog.connect_response(Some("apply"), move |_, _| {
                match request {
                    ExtensionRequest::Logout => {
                        let _ = std::process::Command::new("gnome-session-quit")
                            .args(["--logout", "--no-prompt"])
                            .status();
                    }
                    ExtensionRequest::Enable => match enable_focus_extension() {
                        Ok(()) => ui.fire_toast(
                            &overlay_toasts,
                            &t("other.extension_enabled_toast", "Đã bật extension"),
                        ),
                        Err(e) => {
                            if let Some(window) = window_of(&btn) {
                                alert(&window, &e);
                            }
                        }
                    },
                    ExtensionRequest::Disable => match disable_focus_extension() {
                        Ok(()) => ui.fire_toast(
                            &overlay_toasts,
                            &t("other.extension_disabled_toast", "Đã tắt extension"),
                        ),
                        Err(e) => {
                            if let Some(window) = window_of(&btn) {
                                alert(&window, &e);
                            }
                        }
                    },
                    ExtensionRequest::Remove => match remove_focus_extension() {
                        Ok(()) => ui.fire_toast(
                            &overlay_toasts,
                            &t("other.extension_removed_toast", "Đã gỡ extension"),
                        ),
                        Err(e) => {
                            if let Some(window) = window_of(&btn) {
                                alert(&window, &e);
                            }
                        }
                    },
                    ExtensionRequest::InstallNew | ExtensionRequest::Update => {
                        match install_focus_extension() {
                            Ok(msg) => ui.fire_toast(&overlay_toasts, &msg),
                            Err(e) => {
                                if let Some(window) = window_of(&btn) {
                                    alert(&window, &e);
                                }
                            }
                        }
                    }
                }
                // The disk and the shell have moved: re-read both.
                show_extension_row(&row, &all[0], &all[1], &all[2]);
            });
        }
        dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
    });
}

/// Put the current situation on the row: the subtitle names it, and the
/// buttons are either the one action that fixes it or, once it runs, the two
/// actions that undo the install.
fn show_extension_row(
    row: &adw::ActionRow,
    primary: &gtk4::Button,
    off: &gtk4::Button,
    remove: &gtk4::Button,
) {
    let text = extension_row_for(extension_state());
    row.set_subtitle(&t(text.subtitle.0, text.subtitle.1));
    match text.button {
        Some((label, _)) => {
            primary.set_label(&t(label.0, label.1));
            primary.set_visible(true);
        }
        None => primary.set_visible(false),
    }
    off.set_label(&t(RUNNING_BUTTONS[0].0 .0, RUNNING_BUTTONS[0].0 .1));
    remove.set_label(&t(RUNNING_BUTTONS[1].0 .0, RUNNING_BUTTONS[1].0 .1));
    off.set_visible(text.button.is_none());
    remove.set_visible(text.button.is_none());
}

/// The confirmation for a request: heading, body and button label, so an
/// update never says "install" and a removal warns about what it deletes.
pub(crate) fn extension_dialog(request: ExtensionRequest) -> (String, String, String) {
    match request {
        ExtensionRequest::InstallNew => (
            t("other.extension_confirm_heading", "Cài extension MăngRS Focus?"),
            t(
                "other.extension_confirm_body",
                "Vài tệp sẽ được ghi vào ~/.local/share/gnome-shell/extensions và extension được bật. Cần cho luật theo app trên Wayland.",
            ),
            t("other.extension_apply", "Cài đặt"),
        ),
        ExtensionRequest::Update => (
            t(
                "other.extension_update_heading",
                "Cập nhật extension MăngRS Focus?",
            ),
            t(
                "other.extension_update_body",
                "Bản trên máy cũ hơn bản trong app này. Ghi đè bằng bản mới rồi bật, sau đó cần đăng xuất để GNOME tải.",
            ),
            t("other.extension_update_apply", "Cập nhật"),
        ),
        ExtensionRequest::Enable => (
            t(
                "other.extension_enable_heading",
                "Bật extension MăngRS Focus?",
            ),
            t(
                "other.extension_enable_body",
                "Extension đã cài nhưng đang tắt. Bật ngay, không phải đăng xuất.",
            ),
            t("other.extension_enable_apply", "Bật"),
        ),
        ExtensionRequest::Disable => (
            t(
                "other.extension_disable_heading",
                "Tắt extension MăngRS Focus?",
            ),
            t(
                "other.extension_disable_body",
                "Luật theo app sẽ không nhận được tên app Wayland cho tới khi bật lại. Bật lại được ngay.",
            ),
            t("other.extension_disable_apply", "Tắt"),
        ),
        ExtensionRequest::Remove => (
            t(
                "other.extension_remove_heading",
                "Gỡ extension MăngRS Focus?",
            ),
            t(
                "other.extension_remove_body",
                "Các tệp extension bị xoá khỏi ~/.local/share/gnome-shell/extensions. Luật theo app sẽ không nhận được tên app Wayland.",
            ),
            t("other.extension_remove_apply", "Gỡ"),
        ),
        ExtensionRequest::Logout => (
            t(
                "other.extension_logout_heading",
                "Đăng xuất để GNOME tải extension?",
            ),
            t(
                "other.extension_logout_body",
                "Chữ đang gõ dở trong các cửa sổ sẽ mất. Đăng nhập lại thì extension bắt đầu chạy.",
            ),
            t("other.extension_logout_apply", "Đăng xuất"),
        ),
    }
}

fn window_of(btn: &gtk4::Button) -> Option<gtk4::Window> {
    btn.root().and_downcast::<gtk4::Window>()
}

/// The restart script: restart the daemon, then bring the XIM bridge back.
///
/// On a Wayland session `ibus-x11` starts once, from the distro hook
/// `/etc/xdg/Xwayland-session.d/10-ibus-x11` (`/usr/lib/ibus/ibus-x11 &`),
/// and nothing respawns it — after a plain `ibus restart` XIM clients
/// (JetBrains) cannot type until the next login while the stale
/// `XIM_SERVERS` root property still advertises the dead server. Wait for
/// the daemon, then start the bridge the way the hook does (nohup: this
/// script's shell exits right after), skipped when one already runs (an
/// X11 session keeps its own under the daemon's `--xim`). Java apps opened
/// before the restart may still need one reopen — AWT holds a stale XIC
/// (JDK-8213183).
pub(crate) fn restart_script(session_type: Option<&str>) -> &'static str {
    if session_type == Some("wayland") {
        "ibus restart; sleep 2; pgrep -x ibus-x11 >/dev/null 2>&1 \
         || nohup /usr/lib/ibus/ibus-x11 >/dev/null 2>&1 &"
    } else {
        "ibus restart"
    }
}

fn restart_ibus() -> std::io::Result<()> {
    let script = restart_script(std::env::var("XDG_SESSION_TYPE").ok().as_deref());
    std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .spawn()
        .map(|_| ())
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
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
        "Ẩn hay hiện dòng MăngRS trong lưới ứng dụng GNOME. Cần vài giây để GNOME nhận.",
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
    let extension_row = adw::ActionRow::new();
    extension_row.set_use_markup(false);
    extension_row.set_title(&t("rows.focus_extension.title", "Extension MăngRS Focus"));
    extension_row.set_tooltip_text(Some(&t(
        "tips.focus_extension",
        "Extension báo tên cửa sổ đang focus cho engine, để luật theo app chạy được với app Wayland như Chrome, Firefox.",
    )));
    let extension_btn = action_button(None, "");
    sizes.add_widget(&extension_btn);
    extension_row.add_suffix(&extension_btn);
    // A running extension has nothing to fix, so it offers what it asks of
    // the user instead: switch it off, or remove it.
    let mut running_btns = Vec::new();
    for (label, _) in RUNNING_BUTTONS {
        let btn = action_button(None, &t(label.0, label.1));
        sizes.add_widget(&btn);
        extension_row.add_suffix(&btn);
        running_btns.push(btn);
    }
    show_extension_row(
        &extension_row,
        &extension_btn,
        &running_btns[0],
        &running_btns[1],
    );
    extension.add(&extension_row);
    if session_is_gnome() {
        // Re-read the situation each time the tab is opened: the extension
        // may have been installed or switched on in the Extensions app since
        // this window was built.
        let row = extension_row.clone();
        let btns: Vec<gtk4::Button> = std::iter::once(extension_btn.clone())
            .chain(running_btns.iter().cloned())
            .collect();
        page.connect_map(move |_| show_extension_row(&row, &btns[0], &btns[1], &btns[2]));
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
        "Lưu cấu hình, gõ tắt và từ điển ra một file JSON để mang sang máy khác.",
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
        "Đọc file JSON đã xuất và ghi đè cấu hình, gõ tắt, từ điển hiện tại.",
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
    // Learned picks are behavioral data the user may want gone; the switch
    // that turns learning on lives with the strip, so the eraser sits here.
    let learn_clear_row = action_row(
        &t("rows.learn_clear.title", "Xoá dữ liệu đã học"),
        &t("rows.learn_clear.subtitle", "Bộ gõ học lại từ đầu"),
    );
    learn_clear_row.set_tooltip_text(Some(&t(
        "tips.learn_clear",
        "Chỉ ghi lại gợi ý đã chọn, không ghi phím gõ. Xoá xong bộ gõ học lại từ đầu.",
    )));
    let learn_clear_btn = action_button(None, &t("rows.learn_clear.button", "Xoá…"));
    sizes.add_widget(&learn_clear_btn);
    learn_clear_row.add_suffix(&learn_clear_btn);
    {
        let row = learn_clear_row.clone();
        learn_clear_btn.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading(t("learn_clear.heading", "Xoá dữ liệu đã học?"))
                .body(t(
                    "learn_clear.body",
                    "Hai tệp học-theo-chọn sẽ bị xoá. Bộ gõ sẽ học lại từ đầu.",
                ))
                .build();
            dialog.add_response("cancel", &t("learn_clear.cancel", "Huỷ"));
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            dialog.add_response("delete", &t("learn_clear.delete", "Xoá"));
            dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            dialog.connect_response(Some("delete"), |_, _| {
                if let Some(dir) = dirs::config_dir().map(|d| d.join("ibus-mangrs")) {
                    crate::learn::clear_dir(&dir);
                }
            });
            dialog.present(row.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }
    advanced.add(&learn_clear_row);
    let reset_row = action_row(
        &t("rows.reset_defaults.title", "Khôi phục mặc định"),
        &t("rows.reset_defaults.subtitle", "Về mặc định, giữ gõ tắt"),
    );
    reset_row.set_tooltip_text(Some(&t(
        "tips.reset_defaults",
        "Ghi đè config.toml bằng giá trị mặc định, kể cả menu panel, luật theo app và phím tắt. Gõ tắt và từ điển vẫn giữ.",
    )));
    // Label only and no red: the reference keeps the reset quiet, the
    // destructive confirmation dialog carries the warning.
    let reset_btn = action_button(None, &t("misc.reset_button", "Khôi phục…"));
    sizes.add_widget(&reset_btn);
    reset_row.add_suffix(&reset_btn);
    advanced.add(&reset_row);
    page.add(&advanced);

    // The manual IBus restart, last on the page and centered: nothing in
    // this window needs it (the run-loop hot-reload makes every settings
    // change land live), so it stays a maintenance action — a stuck daemon,
    // a hand-installed engine binary. It also brings the ibus-x11 bridge
    // back, which a terminal `ibus restart` does not: without the bridge
    // XIM apps (JetBrains) cannot type until the next logout.
    let restart_group = adw::PreferencesGroup::new();
    let restart_btn = action_button(
        Some("system-reboot-symbolic"),
        &t("misc.restart_button", "Khởi động lại IBus"),
    );
    // Amber rather than the accent blue: the restart drops every app's open
    // composition, so the button reads as a warning. Ordinary button shape,
    // like the other actions on this page.
    restart_btn.add_css_class("warning");
    restart_btn.set_halign(gtk4::Align::Center);
    restart_btn.set_margin_top(6);
    restart_btn.set_margin_bottom(6);
    restart_group.add(&restart_btn);
    page.add(&restart_group);

    restart_btn.connect_clicked(move |btn| {
        // User-initiated only — the engine never restarts IBus itself — and
        // confirmed, because unsent composition in open windows is lost.
        let window = window_of(btn);
        let dialog = adw::AlertDialog::builder()
            .heading(t("other.restart_confirm_heading", "Khởi động lại IBus?"))
            .body(t(
                "other.restart_confirm_body",
                "Chữ đang gõ dở trong các cửa sổ sẽ mất. Dùng khi IBus bị treo hoặc sau khi cài lại engine.",
            ))
            .build();
        dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
        dialog.add_response(
            "apply",
            &t("misc.restart_button", "Khởi động lại IBus"),
        );
        dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let btn = btn.clone();
        dialog.connect_response(Some("apply"), move |_, _| {
            if let Err(e) = restart_ibus() {
                if let Some(window) = window_of(&btn) {
                    alert(&window, &format!("Không khởi động lại được IBus: {e}"));
                }
            }
        });
        if let Some(window) = window {
            dialog.present(Some(&window));
        }
    });

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
    // wholesale, like any other save.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
        import_btn.connect_clicked(move |btn| {
            let dialog = gtk4::FileDialog::builder()
                .title(t("misc.import_dialog", "Nhập cấu hình"))
                .modal(true)
                .default_filter(&json_filter())
                .build();
            let ui = ui.clone();
            let overlay_toasts = overlay_toasts.clone();
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

    // Every action asks first: installing writes files and flips a GNOME
    // setting, and a logout drops whatever is open. A running extension has
    // two buttons, each with its own wording.
    {
        let mut all_btns = vec![extension_btn.clone()];
        all_btns.extend(running_btns.iter().cloned());
        wire_extension_button(
            &extension_btn,
            None,
            ui,
            &toast_overlay,
            &extension_row,
            &all_btns,
        );
        for (i, request) in [ExtensionRequest::Disable, ExtensionRequest::Remove]
            .into_iter()
            .enumerate()
        {
            wire_extension_button(
                &running_btns[i],
                Some(request),
                ui,
                &toast_overlay,
                &extension_row,
                &all_btns,
            );
        }
    }

    // Restore defaults: destructive, confirmed; macros and the dictionary
    // survive, everything else rewrites to the shipped values.
    {
        let ui = ui.clone();
        let overlay_toasts = toast_overlay.clone();
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
