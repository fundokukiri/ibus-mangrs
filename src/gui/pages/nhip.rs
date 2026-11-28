//! The "Nhịp gõ" table: every timing the four marker transports read, one
//! row per value, each with its shipped default and its own reset. The
//! rows were scattered over two merged rows before; a user tuning Flow
//! against Surrounding needs them side by side.

use super::*;

/// One table row: the label pair, the spin that edits it and the shipped
/// default that the reset button writes back.
pub(crate) struct Row {
    /// Translation key prefix; `.title` and `.subtitle` follow it.
    pub key: &'static str,
    /// Vietnamese fallbacks, used when the catalogue has no entry.
    pub title: &'static str,
    pub subtitle: &'static str,
    pub spin: gtk4::SpinButton,
    pub default: u64,
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
    group.add(&reset_bar(&rows));
    group.add(&grid(&rows));
    // Every spin saves on its own: the table has no apply button, like the
    // merged rows it replaces.
    for row in &rows {
        let s = ui.save_cb.clone();
        row.spin.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }
    group
}

fn rows(ui: &Rc<Ui>) -> Vec<Row> {
    let d = crate::config::Config::default();
    vec![
        Row {
            key: "rows.backspace_interval",
            title: "Xoá lùi",
            subtitle: "Nghỉ giữa các phím Backspace bấm thật",
            spin: ui.backspace_interval.clone(),
            default: d.backspace_interval_ms,
        },
        Row {
            key: "rows.post_delay",
            title: "Chờ viết lại từ",
            subtitle: "Nghỉ sau khi sửa màn hình",
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

/// The group-level reset: every spin back to its shipped value in one click.
fn reset_bar(rows: &[Row]) -> gtk4::Box {
    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let button = gtk4::Button::with_label(&t("rows.nhip.reset", "Khôi phục mặc định"));
    button.set_halign(gtk4::Align::End);
    button.set_hexpand(true);
    button.set_tooltip_text(Some(&t(
        "rows.nhip.reset_tooltip",
        "Đưa cả bảng về giá trị gốc",
    )));
    let defaults: Vec<(gtk4::SpinButton, u64)> =
        rows.iter().map(|r| (r.spin.clone(), r.default)).collect();
    button.connect_clicked(move |_| {
        for (spin, value) in &defaults {
            spin.set_value(*value as f64);
        }
    });
    bar.append(&button);
    bar
}

fn grid(rows: &[Row]) -> gtk4::Grid {
    let grid = gtk4::Grid::builder()
        .row_spacing(10)
        .column_spacing(12)
        .margin_top(6)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    for (col, caption) in ["", "Giá trị", "Mặc định"].iter().enumerate() {
        let label = gtk4::Label::new(Some(caption));
        label.add_css_class("caption");
        label.set_halign(gtk4::Align::Start);
        label.set_xalign(0.0);
        grid.attach(&label, col as i32, 0, 1, 1);
    }
    for (i, row) in rows.iter().enumerate() {
        let line = i as i32 + 1;
        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let title = gtk4::Label::new(Some(&t(&format!("{}.title", row.key), row.title)));
        title.set_xalign(0.0);
        let subtitle = gtk4::Label::new(Some(&t(&format!("{}.subtitle", row.key), row.subtitle)));
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("caption");
        subtitle.set_wrap(true);
        text.append(&title);
        text.append(&subtitle);
        grid.attach(&text, 0, line, 1, 1);

        row.spin.set_valign(gtk4::Align::Center);
        row.spin
            .update_property(&[gtk4::accessible::Property::Label(&format!(
                "{} (ms)",
                t(&format!("{}.title", row.key), row.title)
            ))]);
        grid.attach(&row.spin, 1, line, 1, 1);

        let default = gtk4::Label::new(Some(&format!("{} ms", row.default)));
        default.add_css_class("dim-label");
        default.set_xalign(1.0);
        default.set_hexpand(true);
        grid.attach(&default, 2, line, 1, 1);

        let reset = gtk4::Button::with_label("↺");
        reset.add_css_class("flat");
        reset.set_tooltip_text(Some(&t("rows.nhip.reset_one", "Về giá trị gốc")));
        let spin = row.spin.clone();
        let value = row.default;
        reset.connect_clicked(move |_| spin.set_value(value as f64));
        grid.attach(&reset, 3, line, 1, 1);
    }
    grid
}
