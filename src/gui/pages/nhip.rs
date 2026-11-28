//! The "Nhịp gõ" table: every timing the four marker transports read, one
//! row per value, each with its shipped default and its own reset. The
//! rows were scattered over two merged rows before; a user tuning Flow
//! against Surrounding needs them side by side.

use super::*;

/// One table row: the label pair, the spin that edits it and the shipped
/// default that the reset button writes back.
struct Row {
    /// Translation key prefix; `.title` and `.subtitle` follow it.
    key: &'static str,
    /// Vietnamese fallbacks, used when the catalogue has no entry.
    title: &'static str,
    subtitle: &'static str,
    spin: gtk4::SpinButton,
    default: u64,
}

impl Row {
    fn title(&self) -> String {
        t(&format!("{}.title", self.key), self.title)
    }
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("groups.tune.nhip.title", "Nhịp gõ"))
        .description(t(
            "groups.tune.nhip.description",
            "Thời gian chờ của cả bốn chế độ, tính bằng mili giây.",
        ))
        .build();
    let rows = rows(ui);
    for row in &rows {
        let widget = row_row(row);
        // Every spin saves on its own: the table has no apply button, like
        // the merged rows it replaces.
        let s = ui.save_cb.clone();
        row.spin.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
        group.add(&widget);
    }
    group.add(&reset_row(&rows));
    group
}

/// One row: the name and its description on the left; the shipped default,
/// the reset button and the spin as three suffixes, so every row reads the
/// same way down the group.
fn row_row(row: &Row) -> adw::ActionRow {
    let widget = action_row(
        &row.title(),
        &t(&format!("{}.subtitle", row.key), row.subtitle),
    );
    let default = gtk4::Label::new(Some(&format!("{} ms", row.default)));
    default.add_css_class("dim-label");
    default.add_css_class("numeric");
    default.set_valign(gtk4::Align::Center);
    default.set_tooltip_text(Some(&t(
        "rows.nhip.default_tooltip",
        "Giá trị gốc của dòng này",
    )));

    let reset = gtk4::Button::with_label("↺");
    reset.add_css_class("flat");
    reset.add_css_class("circular");
    reset.set_valign(gtk4::Align::Center);
    reset.set_tooltip_text(Some(&t("rows.nhip.reset_one", "Về giá trị gốc")));
    reset.update_property(&[gtk4::accessible::Property::Label(&format!(
        "{}: {}",
        t("rows.nhip.reset_one", "Về giá trị gốc"),
        row.title()
    ))]);
    {
        let spin = row.spin.clone();
        let value = row.default;
        reset.connect_clicked(move |_| spin.set_value(value as f64));
    }

    row.spin.set_valign(gtk4::Align::Center);
    row.spin
        .update_property(&[gtk4::accessible::Property::Label(&format!(
            "{} (ms)",
            row.title()
        ))]);

    widget.add_suffix(&default);
    widget.add_suffix(&reset);
    widget.add_suffix(&row.spin);
    widget
}

/// The group-level reset: one row at the bottom with the button on the
/// right, matching the rows above instead of a stray toolbar button.
fn reset_row(rows: &[Row]) -> adw::ActionRow {
    let widget = action_row(
        &t("rows.nhip.reset", "Khôi phục mặc định"),
        &t("rows.nhip.reset_tooltip", "Đưa cả bảng về giá trị gốc"),
    );
    let button = gtk4::Button::with_label(&t("rows.nhip.reset_button", "Đặt lại"));
    button.add_css_class("flat");
    button.set_valign(gtk4::Align::Center);
    let defaults: Vec<(gtk4::SpinButton, u64)> =
        rows.iter().map(|r| (r.spin.clone(), r.default)).collect();
    button.connect_clicked(move |_| {
        for (spin, value) in &defaults {
            spin.set_value(*value as f64);
        }
    });
    widget.add_suffix(&button);
    widget
}

fn rows(ui: &Rc<Ui>) -> Vec<Row> {
    let d = crate::config::Config::default();
    vec![
        Row {
            key: "rows.backspace_interval",
            title: "Xoá lùi",
            subtitle: "Giảm nếu xoá chậm",
            spin: ui.backspace_interval.clone(),
            default: d.backspace_interval_ms,
        },
        Row {
            key: "rows.post_delay",
            title: "Chờ viết lại từ",
            subtitle: "Cho app xử lý phím chậm",
            spin: ui.post_delay.clone(),
            default: d.post_delay_ms,
        },
        Row {
            key: "rows.echo_apply_grace",
            title: "Chờ sau phím xoá",
            subtitle: "Quá ngắn thì chữ hiện trước dấu",
            spin: ui.echo_apply_grace_spin.clone(),
            default: d.echo_apply_grace_ms,
        },
        Row {
            key: "rows.key_delete_grace",
            title: "Chờ sau phím xoá thật",
            subtitle: "Cho app không báo chữ đang gõ",
            spin: ui.key_delete_grace_spin.clone(),
            default: d.key_delete_grace_ms,
        },
        Row {
            key: "rows.flow_per_key_delay",
            title: "Flow: hiện dấu",
            subtitle: "Chờ rồi mới chèn lại từ",
            spin: ui.flow_per_key_spin.clone(),
            default: d.flow_per_key_delay_ms,
        },
        Row {
            key: "rows.flow_backspace_restore_delay",
            title: "Flow: hoàn dấu sau Backspace",
            subtitle: "Chờ Backspace đi khỏi rồi vá",
            spin: ui.flow_backspace_restore_spin.clone(),
            default: d.flow_backspace_restore_delay_ms,
        },
        Row {
            key: "rows.focus_bounce",
            title: "Hoãn reset khi mất focus",
            subtitle: "Nháy mất focus vẫn giữ từ",
            spin: ui.focus_bounce_spin.clone(),
            default: d.focus_bounce_ms,
        },
        Row {
            key: "rows.marker_wait",
            title: "Chờ sau dấu ·",
            subtitle: "Chỉ dùng khi chèn dấu ·",
            spin: ui.marker_wait_spin.clone(),
            default: d.marker_wait_ms,
        },
    ]
}
