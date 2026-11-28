//! The "Nhịp gõ" table: every value the typing modes read, one row per
//! value, each with its shipped default and its own reset. The timings
//! were scattered over two merged rows before; a user tuning Flow against
//! Surrounding needs them side by side.

use super::*;

/// Which half of the table a row belongs to — the filter's two buckets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Time,
    Insert,
}

enum Kind {
    /// A millisecond value, with the switch that gates it when there is one.
    Spin {
        spin: gtk4::SpinButton,
        default: u64,
        switch: Option<gtk4::Switch>,
    },
    /// A row built elsewhere that carries its own title and subtitle
    /// (the switch rows and the choice rows of `build`).
    Own(gtk4::Widget),
}

struct Row {
    /// Translation key prefix; `.title` and `.subtitle` follow it.
    key: &'static str,
    /// The key this row edits in config.toml — shown in the tooltip so a
    /// user can go straight to the file (and a bug report can name it).
    config: &'static str,
    title: &'static str,
    subtitle: &'static str,
    /// One sentence on what the key does, under the key name in the tooltip.
    purpose: &'static str,
    kind: Kind,
    bucket: Bucket,
    /// Behind the experimental gate: locked while the gate is off.
    gated: bool,
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
            "Thời gian chờ và cách xoá của cả bốn chế độ.",
        ))
        .build();
    let rows = rows(ui);
    let mut placed: Vec<(gtk4::Widget, Bucket)> = Vec::new();
    for row in &rows {
        let widget: gtk4::Widget = match &row.kind {
            Kind::Own(widget) => widget.clone(),
            _ => spin_or_switch_row(row).upcast(),
        };
        // The tooltip leads with the config key, then the purpose (the
        // longer catalogue tip when the row already carries one).
        let key_line = format!("`{}`", row.config);
        let text = match widget.tooltip_text() {
            Some(existing) if !existing.is_empty() => format!("{key_line}\n{existing}"),
            _ => format!("{key_line}\n{}", row.purpose),
        };
        widget.set_tooltip_text(Some(&text));
        if row.gated {
            ui.nhip_gated.borrow_mut().push(widget.clone());
        }
        group.add(&widget);
        placed.push((widget, row.bucket));
    }
    group.add(&filter_row(placed));
    group.add(&reset_row(&rows));
    // Every spin saves on its own: the table has no apply button.
    let saves: Vec<gtk4::SpinButton> = rows
        .iter()
        .filter_map(|r| match &r.kind {
            Kind::Spin { spin, .. } => Some(spin.clone()),
            _ => None,
        })
        .collect();
    for spin in saves {
        let s = ui.save_cb.clone();
        spin.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }
    group
}

/// The filter above the table's reset: three buckets, the whole table by
/// default.
fn filter_row(rows: Vec<(gtk4::Widget, Bucket)>) -> adw::ActionRow {
    let widget = action_row(
        &t("rows.nhip.filter", "Hiện"),
        &t("rows.nhip.filter_subtitle", "Chọn nhóm thông số muốn xem"),
    );
    let drop = gtk4::DropDown::from_strings(&[
        &t("rows.nhip.filter_all", "Tất cả"),
        &t("rows.nhip.filter_time", "Thời gian"),
        &t("rows.nhip.filter_insert", "Cách xoá"),
    ]);
    drop.set_valign(gtk4::Align::Center);
    drop.set_selected(0);
    drop.connect_selected_notify(move |d| {
        let wanted = match d.selected() {
            1 => Some(Bucket::Time),
            2 => Some(Bucket::Insert),
            _ => None,
        };
        for (row, bucket) in &rows {
            row.set_visible(wanted.is_none_or(|w| w == *bucket));
        }
    });
    widget.add_suffix(&drop);
    widget
}

fn spin_or_switch_row(row: &Row) -> adw::ActionRow {
    let widget = action_row(
        &row.title(),
        &t(&format!("{}.subtitle", row.key), row.subtitle),
    );
    match &row.kind {
        Kind::Spin {
            spin,
            default,
            switch,
        } => {
            // The switch first: it gates the value that follows it.
            if let Some(switch) = switch {
                switch.set_valign(gtk4::Align::Center);
                widget.add_suffix(switch);
            }
            let value = gtk4::Label::new(Some(&format!("{default} ms")));
            value.add_css_class("dim-label");
            value.add_css_class("numeric");
            value.set_valign(gtk4::Align::Center);
            value.set_tooltip_text(Some(&t(
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
                let spin = spin.clone();
                let default = *default;
                reset.connect_clicked(move |_| spin.set_value(default as f64));
            }
            spin.set_valign(gtk4::Align::Center);
            spin.update_property(&[gtk4::accessible::Property::Label(&format!(
                "{} (ms)",
                row.title()
            ))]);
            widget.add_suffix(&value);
            widget.add_suffix(&reset);
            widget.add_suffix(spin);
        }
        Kind::Own(_) => {}
    }
    widget
}

/// The group-level reset: one row at the bottom with the button on the
/// right, matching the rows above.
fn reset_row(rows: &[Row]) -> adw::ActionRow {
    let widget = action_row(
        &t("rows.nhip.reset", "Khôi phục mặc định"),
        &t("rows.nhip.reset_tooltip", "Đưa cả bảng về giá trị gốc"),
    );
    let button = gtk4::Button::with_label(&t("rows.nhip.reset_button", "Đặt lại"));
    button.add_css_class("flat");
    button.set_valign(gtk4::Align::Center);
    let defaults: Vec<(gtk4::SpinButton, u64)> = rows
        .iter()
        .filter_map(|r| match &r.kind {
            Kind::Spin { spin, default, .. } => Some((spin.clone(), *default)),
            _ => None,
        })
        .collect();
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
    let spin = |config: &'static str,
                key: &'static str,
                title: &'static str,
                subtitle: &'static str,
                purpose: &'static str,
                spin: &gtk4::SpinButton,
                default: u64,
                gated: bool| Row {
        key,
        config,
        title,
        subtitle,
        purpose,
        kind: Kind::Spin {
            spin: spin.clone(),
            default,
            switch: None,
        },
        bucket: Bucket::Time,
        gated,
    };
    vec![
        spin(
            "backspace_interval_ms",
            "rows.backspace_interval",
            "Xoá lùi",
            "Giảm nếu xoá chậm",
            "Khoảng cách giữa các phím Backspace ảo; giảm khi app xoá chậm.",
            &ui.backspace_interval,
            d.backspace_interval_ms,
            false,
        ),
        spin(
            "post_delay_ms",
            "rows.post_delay",
            "Chờ viết lại từ",
            "Cho app xử lý phím chậm",
            "Nghỉ sau mỗi lần sửa màn hình trước khi nhận phím tiếp.",
            &ui.post_delay,
            d.post_delay_ms,
            false,
        ),
        spin(
            "echo_apply_grace_ms",
            "rows.echo_apply_grace",
            "Chờ sau phím xoá",
            "Quá ngắn thì chữ hiện trước dấu",
            "Commit chờ phím xoá áp xong; quá ngắn thì chữ hiện trước dấu, quá dài thì gõ bị trễ.",
            &ui.echo_apply_grace_spin,
            d.echo_apply_grace_ms,
            false,
        ),
        spin(
            "key_delete_grace_ms",
            "rows.key_delete_grace",
            "Chờ sau phím xoá thật",
            "Cho app không báo chữ đang gõ",
            "Mức chờ riêng cho app phải xoá bằng phím thật vì không báo chữ đang gõ, ví dụ Docs trong Chrome.",
            &ui.key_delete_grace_spin,
            d.key_delete_grace_ms,
            false,
        ),
        spin(
            "flow_per_key_delay_ms",
            "rows.flow_per_key_delay",
            "Flow: hiện dấu",
            "Chờ rồi mới chèn lại từ",
            "Flow chờ chừng này rồi mới chèn lại từ sau khi đổi dấu.",
            &ui.flow_per_key_spin,
            d.flow_per_key_delay_ms,
            false,
        ),
        spin(
            "flow_backspace_restore_delay_ms",
            "rows.flow_backspace_restore_delay",
            "Flow: hoàn dấu sau Backspace",
            "Chờ Backspace đi khỏi rồi vá",
            "Chờ phím Backspace của bạn đi khỏi rồi mới vá lại dấu.",
            &ui.flow_backspace_restore_spin,
            d.flow_backspace_restore_delay_ms,
            false,
        ),
        spin(
            "focus_bounce_ms",
            "rows.focus_bounce",
            "Hoãn reset khi mất focus",
            "Nháy mất focus vẫn giữ từ",
            "Mất focus trong khoảng này thì coi là nháy, giữ nguyên từ đang gõ.",
            &ui.focus_bounce_spin,
            d.focus_bounce_ms,
            false,
        ),
        spin(
            "marker_wait_ms",
            "rows.marker_wait",
            "Chờ sau dấu ·",
            "Chỉ dùng khi chèn dấu ·",
            "Chờ sau khi chèn dấu · để browser bỏ gợi ý trước khi xoá.",
            &ui.marker_wait_spin,
            d.marker_wait_ms,
            false,
        ),
        Row {
            key: "rows.echo_stale",
            config: "echo_stale_ms",
            title: "Chờ phím ảo quay về",
            subtitle: "Quá hạn thì đổ chữ luôn",
            purpose: "Quá hạn này không thấy phím ảo quay về thì đổ chữ đã treo, để không mất chữ trên Wayland.",
            kind: Kind::Spin {
                spin: ui.echo_stale_spin.clone(),
                default: d.echo_stale_ms,
                switch: Some(ui.echo_stale_switch.clone()),
            },
            bucket: Bucket::Time,
            gated: true,
        },
        Row {
            key: "rows.idle_commit",
            config: "idle_commit_ms",
            title: "Tự chốt từ khi ngừng gõ",
            subtitle: "Như bấm dấu cách",
            purpose: "Ngừng gõ quá lâu thì tự chốt từ đang gạch chân, như vừa bấm dấu cách.",
            kind: Kind::Spin {
                spin: ui.idle_commit_spin.clone(),
                default: d.idle_commit_ms,
                switch: Some(ui.idle_commit_switch.clone()),
            },
            bucket: Bucket::Time,
            gated: true,
        },
        Row {
            key: "rows.dodge_fix",
            config: "autocomplete_fix",
            title: "Sửa lỗi gõ trong ô địa chỉ",
            subtitle: "Tắt, hoặc chọn cách sửa",
            purpose: "Tắt hẳn, hoặc chọn cách xoá khi gợi ý hiện trong ô: xoá thẳng, hay chèn dấu · để browser bỏ gợi ý trước.",
            kind: Kind::Own(ui.dodge_fix.clone().upcast()),
            bucket: Bucket::Insert,
            gated: true,
        },
        Row {
            key: "rows.nav_key_while_preedit",
            config: "nav_key_while_preedit",
            title: "Phím điều hướng khi đang gõ",
            subtitle: "Mũi tên bấm lúc còn chữ chưa chốt",
            purpose: "Bấm mũi tên khi còn chữ chưa chốt: chuyển tiếp phím cho app, hoặc chốt từ rồi mới chuyển.",
            kind: Kind::Own(ui.nav_key_while_preedit.clone().upcast()),
            bucket: Bucket::Insert,
            gated: false,
        },
        Row {
            key: "rows.x11_delete",
            config: "x11_delete_without_report",
            title: "Xoá đuôi từ ở app X11",
            subtitle: "Xoá như ibus-bamboo khi app không báo",
            purpose: "Cho app X11 xoá đuôi từ như ibus-bamboo khi app không gửi báo cáo văn bản.",
            kind: Kind::Own(ui.x11_delete.clone().upcast()),
            bucket: Bucket::Insert,
            gated: false,
        },
        Row {
            key: "rows.preedit_xim",
            config: "preedit_for_xim",
            title: "Preedit cho app XIM",
            subtitle: "Giữ gạch chân cho Java/JetBrains",
            purpose: "Dùng cửa sổ gạch chân riêng cho app XIM như Java và JetBrains, vốn không nhận gạch chân của IBus.",
            kind: Kind::Own(ui.preedit_xim.clone().upcast()),
            bucket: Bucket::Insert,
            gated: false,
        },
    ]
}
