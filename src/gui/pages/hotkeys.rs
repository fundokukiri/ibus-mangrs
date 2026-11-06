//! Page "Phím tắt": the four built-in shortcuts as activatable rows, the
//! user's own shortcuts above an add row, and the mode-cycle order as an
//! expander. Capture happens in the shared dialog (`gui/hotkey.rs`); the
//! rows only open it. The page hosts its own ToastOverlay for the delete
//! undo; the window shell stays untouched.

use super::*;

/// The display label of a custom-shortcut function ("Bật/tắt tiếng Việt").
fn function_label(action: &str) -> String {
    crate::config::CUSTOM_HOTKEY_ACTIONS
        .iter()
        .position(|k| *k == action)
        .map(|idx| t(CUSTOM_ACTION_KEYS[idx].1, CUSTOM_ACTION_DEFAULTS[idx]))
        .unwrap_or_else(|| action.to_string())
}

impl Ui {
    /// The function name already holding this combo, if any — the dialog's
    /// inline conflict message. `skip` is the row's own current value when
    /// it is being re-assigned.
    pub(crate) fn conflict_for(&self, spec: &str, skip: Option<&str>) -> Option<String> {
        let mut assigned: Vec<(String, String)> = vec![
            (
                self.hotkey_value.borrow().clone(),
                t("rows.hotkey_mode.title", "Phím tắt đổi chế độ gõ"),
            ),
            (
                self.enabled_hotkey_value.borrow().clone(),
                t("rows.hotkey_enabled.title", "Phím tắt bật/tắt tiếng Việt"),
            ),
            (
                self.app_rule_hotkey_value.borrow().clone(),
                t("rows.hotkey_app_rule.title", "Phím tắt chọn luật theo app"),
            ),
            (
                self.emoji_hotkey_value.borrow().clone(),
                t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"),
            ),
        ];
        assigned.extend(
            self.custom
                .borrow()
                .iter()
                .map(|(s, a)| (s.clone(), function_label(a))),
        );
        assigned
            .iter()
            .find(|(s, _)| skip.is_none_or(|k| k != s) && same_combo(s, spec))
            .map(|(_, label)| label.clone())
    }

    /// Rebuild the custom-shortcut rows: the empty state, one row per
    /// shortcut, the add row. The design vector is the source of truth and
    /// every edit refills, so the captured indices stay valid.
    pub(crate) fn fill_custom(self: &Rc<Self>) {
        while let Some(row) = self.custom_box.row_at_index(0) {
            self.custom_box.remove(&row);
        }
        if self.custom.borrow().is_empty() {
            let empty = gtk4::Label::new(Some(&t(
                "groups.hotkeys.custom.empty",
                "Chưa có phím tắt riêng",
            )));
            empty.add_css_class("dim-label");
            empty.set_hexpand(true);
            empty.set_xalign(0.5);
            empty.set_margin_top(6);
            empty.set_margin_bottom(6);
            self.custom_box.append(&empty);
        }
        for (i, (spec, action)) in self.custom.borrow().iter().enumerate() {
            let label = function_label(action);
            let row = adw::ActionRow::builder()
                .title(&label)
                .use_markup(false)
                .activatable(false)
                .build();
            let display = ShortcutDisplay::new(&label);
            display.render(spec);
            row.add_suffix(display.widget());
            let trash = gtk4::Button::from_icon_name("user-trash-symbolic");
            trash.add_css_class("flat");
            trash.set_tooltip_text(Some(&t("misc.delete", "Xóa")));
            trash.update_property(&[gtk4::accessible::Property::Label(&t("misc.delete", "Xóa"))]);
            trash.set_valign(gtk4::Align::Center);
            // Turns red under the pointer, like the rule rows' trash.
            {
                let del_enter = trash.clone();
                let del_leave = trash.clone();
                let hover = gtk4::EventControllerMotion::new();
                hover.connect_enter(move |_, _, _| del_enter.add_css_class("error"));
                hover.connect_leave(move |_| del_leave.remove_css_class("error"));
                trash.add_controller(hover);
            }
            {
                let ui = self.clone();
                let overlay = self.hotkey_overlay.clone();
                trash.connect_clicked(move |_| {
                    let (spec, action) = ui.custom.borrow()[i].clone();
                    ui.custom.borrow_mut().remove(i);
                    ui.fill_custom();
                    ui.fire_save();
                    // Undo re-inserts at the old slot while the toast lives.
                    let toast = adw::Toast::new(&t("misc.deleted_toast", "Đã xóa"));
                    toast.set_button_label(Some(&t("misc.undo", "Hoàn tác")));
                    {
                        let ui = ui.clone();
                        toast.connect_button_clicked(move |_| {
                            let at = i.min(ui.custom.borrow().len());
                            ui.custom
                                .borrow_mut()
                                .insert(at, (spec.clone(), action.clone()));
                            ui.fill_custom();
                            ui.fire_save();
                        });
                    }
                    overlay.add_toast(toast);
                });
            }
            row.add_suffix(&trash);
            self.custom_box.append(&row);
        }
        let add_row = adw::ActionRow::builder()
            .title(t("groups.hotkeys.custom.add_row", "Thêm phím tắt riêng"))
            .use_markup(false)
            .activatable(true)
            .build();
        add_row.add_prefix(&gtk4::Image::from_icon_name("list-add-symbolic"));
        {
            let ui = self.clone();
            let parent = add_row.clone();
            add_row.connect_activated(move |_| open_add_dialog(&ui, &parent));
        }
        self.custom_box.append(&add_row);
    }
}

/// One built-in shortcut row: activating it opens the capture dialog with
/// the row's title; the result lands in `value` and re-renders `display`.
fn default_row(
    ui: &Rc<Ui>,
    title: &str,
    subtitle: &str,
    value: &Rc<RefCell<String>>,
    display: ShortcutDisplay,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .use_markup(false)
        .activatable(true)
        .build();
    row.add_suffix(display.widget());
    {
        let ui = ui.clone();
        let value = value.clone();
        let display = display.clone();
        let parent = row.clone();
        let title = title.to_string();
        row.connect_activated(move |_| {
            let old = value.borrow().clone();
            open_capture_dialog(
                &parent,
                &title,
                &t(
                    "misc.capture_body",
                    "Nhấn tổ hợp phím mới. Esc để hủy, Backspace để xóa.",
                ),
                None,
                None,
                {
                    let ui = ui.clone();
                    let old = old.clone();
                    move |spec| ui.conflict_for(spec, Some(&old))
                },
                {
                    let ui = ui.clone();
                    let value = value.clone();
                    let display = display.clone();
                    move |result| {
                        *value.borrow_mut() = result.unwrap_or_default();
                        let current = value.borrow().clone();
                        display.render(&current);
                        ui.fire_save();
                    }
                },
            );
        });
    }
    row
}

/// The add dialog: a function picker plus the shared capture behavior.
/// Committing adds `(spec, action)` to the design vector and saves.
fn open_add_dialog(ui: &Rc<Ui>, parent: &impl IsA<gtk4::Widget>) {
    let labels: Vec<String> = CUSTOM_ACTION_KEYS
        .iter()
        .zip(CUSTOM_ACTION_DEFAULTS)
        .map(|(key, fallback)| t(key.1, fallback))
        .collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let pick = adw::ComboRow::builder()
        .title(t("groups.hotkeys.custom.function", "Chức năng"))
        .use_markup(false)
        .model(&StringList::new(&refs))
        .build();
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.append(&pick);
    open_capture_dialog(
        parent,
        &t("groups.hotkeys.custom.add_row", "Thêm phím tắt riêng"),
        &t(
            "misc.capture_add_body",
            "Chọn chức năng rồi nhấn tổ hợp phím. Esc để hủy.",
        ),
        Some(list.upcast_ref()),
        Some(pick.upcast_ref()),
        {
            let ui = ui.clone();
            move |spec| ui.conflict_for(spec, None)
        },
        {
            let ui = ui.clone();
            let pick = pick.clone();
            move |result| {
                let Some(spec) = result else {
                    return;
                };
                let action = crate::config::CUSTOM_HOTKEY_ACTIONS
                    .get(pick.selected() as usize)
                    .unwrap_or(&"enabled")
                    .to_string();
                ui.custom.borrow_mut().push((spec, action));
                ui.fill_custom();
                ui.fire_save();
            }
        },
    );
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();

    // The four built-ins: whole-row activation replaces the old pencil.
    let group = adw::PreferencesGroup::builder()
        .title(t("groups.hotkeys.keys.title", "Phím tắt mặc định"))
        .description(t(
            "groups.hotkeys.keys.description",
            "Bấm vào một dòng rồi nhấn tổ hợp phím mới.",
        ))
        .build();
    group.add(&default_row(
        ui,
        &t("rows.hotkey_mode.title", "Phím tắt đổi chế độ gõ"),
        &t("rows.hotkey_mode.subtitle", "Ví dụ: Control+Alt+v"),
        &ui.hotkey_value,
        ui.hotkey_display.clone(),
    ));
    group.add(&default_row(
        ui,
        &t("rows.hotkey_enabled.title", "Phím tắt bật/tắt tiếng Việt"),
        &t("rows.hotkey_enabled.subtitle", "Ví dụ: Control+Alt+space"),
        &ui.enabled_hotkey_value,
        ui.enabled_hotkey_display.clone(),
    ));
    group.add(&default_row(
        ui,
        &t("rows.hotkey_app_rule.title", "Phím tắt chọn luật theo app"),
        &t(
            "rows.hotkey_app_rule.subtitle",
            "Mở menu chọn chế độ ngay tại con trỏ (mũi tên + Enter)",
        ),
        &ui.app_rule_hotkey_value,
        ui.app_rule_hotkey_display.clone(),
    ));
    group.add(&default_row(
        ui,
        &t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"),
        &t(
            "rows.hotkey_emoji.subtitle",
            "Mở menu chọn emoji tại con trỏ: 1-9/0 chọn, gõ chữ để tìm",
        ),
        &ui.emoji_hotkey_value,
        ui.emoji_hotkey_display.clone(),
    ));
    page.add(&group);

    // The user's own shortcuts: rebuilt by `fill_custom` after every edit.
    let custom_group = adw::PreferencesGroup::builder()
        .title(t("groups.hotkeys.custom.title", "Phím tắt riêng"))
        .description(t(
            "groups.hotkeys.custom.description",
            "Gán thêm phím cho cùng chức năng với phím mặc định.",
        ))
        .build();
    custom_group.add(&ui.custom_box);
    page.add(&custom_group);

    // The mode-cycle order, data not decoration: derived from the shared
    // `MODES` list, whose order is the engine's cycle.
    let order_group = adw::PreferencesGroup::new();
    let expander = adw::ExpanderRow::builder()
        .title(t("groups.hotkeys.cycle.title", "Thứ tự khi đổi chế độ"))
        .subtitle(
            t("groups.hotkeys.cycle.subtitle", "{n} chế độ, xoay vòng")
                .replace("{n}", &MODES.len().to_string()),
        )
        .use_markup(false)
        .build();
    let sequence = adw::ActionRow::builder()
        .use_markup(false)
        .activatable(false)
        .subtitle(MODES.join(" → "))
        .build();
    // The ten mode names do not fit one line at the default width.
    sequence.set_subtitle_lines(3);
    expander.add_row(&sequence);
    order_group.add(&expander);
    page.add(&order_group);

    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&page));
    overlay
}
