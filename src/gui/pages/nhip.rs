//! The "Nhịp gõ" table: every value the typing modes read, one row per
//! value, each with its shipped default and its own reset. The timings
//! were scattered over two merged rows before; a user tuning Flow against
//! Surrounding needs them side by side.

use super::*;

/// The filter's two buckets: the two uinput pacing values, and everything
/// that parks a commit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Uinput,
    Park,
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
    /// The modes the value reaches, in the tooltip's last line.
    scope: &'static str,
    spin: gtk4::SpinButton,
    /// The switch that gates the value, for the two optional timings.
    switch: Option<gtk4::Switch>,
    bucket: Bucket,
    default: u64,
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
            "Thời gian chờ của cả bốn chế độ, tính bằng mili giây.",
        ))
        .build();
    let rows = rows(ui);
    let mut placed: Vec<(gtk4::Widget, Bucket)> = Vec::new();
    for row in &rows {
        let widget = spin_or_switch_row(row);
        // The tooltip leads with the config key, then the purpose (the
        // longer catalogue tip when the row already carries one).
        let key_line = format!("`{}`", row.config);
        let text = match widget.tooltip_text() {
            Some(existing) if !existing.is_empty() => format!("{key_line}\n{existing}"),
            _ => format!("{key_line}\n{}", row.purpose),
        };
        let text = format!("{text}\n{}: {}", t("rows.nhip.scope", "Áp dụng"), row.scope);
        widget.set_tooltip_text(Some(&text));
        if row.gated {
            ui.nhip_gated.borrow_mut().push(widget.clone().upcast());
        }
        placed.push((widget.clone().upcast(), row.bucket));
        group.add(&widget);
    }
    group.set_header_suffix(Some(&header_controls(&rows, placed)));
    // Every spin saves on its own: the table has no apply button.
    for spin in rows.iter().map(|r| r.spin.clone()) {
        let s = ui.save_cb.clone();
        spin.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }
    group
}

fn spin_or_switch_row(row: &Row) -> adw::ActionRow {
    let widget = action_row(
        &row.title(),
        &t(&format!("{}.subtitle", row.key), row.subtitle),
    );
    {
        let (spin, default, switch) = (&row.spin, row.default, &row.switch);
        {
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
                let default = row.default;
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
    }
    widget
}

/// The group header carries both controls, right of the title: the filter
/// that narrows the table and the button that puts every value back.
fn header_controls(rows: &[Row], placed: Vec<(gtk4::Widget, Bucket)>) -> gtk4::Box {
    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    bar.set_valign(gtk4::Align::Center);

    let filter = gtk4::DropDown::from_strings(&[
        &t("rows.nhip.filter_all", "Tất cả"),
        &t("rows.nhip.filter_uinput", "Phím ảo"),
        &t("rows.nhip.filter_park", "Chờ chữ hiện"),
    ]);
    filter.set_tooltip_text(Some(&t(
        "rows.nhip.filter_subtitle",
        "Chọn nhóm thông số muốn xem",
    )));
    {
        filter.connect_selected_notify(move |d| {
            let wanted = match d.selected() {
                1 => Some(Bucket::Uinput),
                2 => Some(Bucket::Park),
                _ => None,
            };
            for (row, bucket) in &placed {
                row.set_visible(wanted.is_none_or(|w| w == *bucket));
            }
        });
    }

    let reset = gtk4::Button::with_label(&t("rows.nhip.reset", "Khôi phục mặc định"));
    reset.set_tooltip_text(Some(&t(
        "rows.nhip.reset_tooltip",
        "Đưa cả bảng về giá trị gốc",
    )));
    {
        let defaults: Vec<(gtk4::SpinButton, u64)> =
            rows.iter().map(|r| (r.spin.clone(), r.default)).collect();
        reset.connect_clicked(move |_| {
            for (spin, value) in &defaults {
                spin.set_value(*value as f64);
            }
        });
    }

    bar.append(&filter);
    bar.append(&reset);
    bar
}

fn rows(ui: &Rc<Ui>) -> Vec<Row> {
    let d = crate::config::Config::default();
    let spin = |config: &'static str,
                key: &'static str,
                title: &'static str,
                subtitle: &'static str,
                purpose: &'static str,
                scope: &'static str,
                spin: &gtk4::SpinButton,
                bucket: Bucket,
                default: u64,
                gated: bool| Row {
        key,
        config,
        title,
        subtitle,
        purpose,
        scope,
        spin: spin.clone(),
        switch: None,
        bucket,
        default,
        gated,
    };
    vec![
        spin(
            "backspace_interval_ms",
            "rows.backspace_interval",
            "Xoá lùi",
            "Giảm nếu xoá chậm",
            "các chế độ bấm phím ảo: Flow, Surrounding, ForwardBackspace",
            "Khoảng cách giữa các phím Backspace ảo; giảm khi app xoá chậm.",
            &ui.backspace_interval,
            Bucket::Uinput,
            d.backspace_interval_ms,
            false,
        ),
        spin(
            "post_delay_ms",
            "rows.post_delay",
            "Chờ viết lại từ",
            "Cho app xử lý phím chậm",
            "các chế độ bấm phím ảo: Flow, Surrounding, ForwardBackspace",
            "Nghỉ sau mỗi lần sửa màn hình trước khi nhận phím tiếp.",
            &ui.post_delay,
            Bucket::Uinput,
            d.post_delay_ms,
            false,
        ),
        spin(
            "echo_apply_grace_ms",
            "rows.echo_apply_grace",
            "Chờ sau phím xoá",
            "Quá ngắn thì chữ hiện trước dấu",
            "Surrounding, Flow, ForwardBackspace",
            "Commit chờ phím xoá áp xong; quá ngắn thì chữ hiện trước dấu, quá dài thì gõ bị trễ.",
            &ui.echo_apply_grace_spin,
            Bucket::Park,
            d.echo_apply_grace_ms,
            false,
        ),
        spin(
            "key_delete_grace_ms",
            "rows.key_delete_grace",
            "Chờ sau phím xoá thật",
            "Cho app không báo chữ đang gõ",
            "Surrounding, khi app không báo chữ đang gõ",
            "Mức chờ riêng cho app phải xoá bằng phím thật vì không báo chữ đang gõ, ví dụ Docs trong Chrome.",
            &ui.key_delete_grace_spin,
            Bucket::Park,
            d.key_delete_grace_ms,
            false,
        ),
        spin(
            "flow_per_key_delay_ms",
            "rows.flow_per_key_delay",
            "Flow: hiện dấu",
            "Chờ rồi mới chèn lại từ",
            "chỉ Flow",
            "Flow chờ chừng này rồi mới chèn lại từ sau khi đổi dấu.",
            &ui.flow_per_key_spin,
            Bucket::Park,
            d.flow_per_key_delay_ms,
            false,
        ),
        spin(
            "flow_backspace_restore_delay_ms",
            "rows.flow_backspace_restore_delay",
            "Flow: hoàn dấu sau Backspace",
            "Chờ Backspace đi khỏi rồi vá",
            "chỉ Flow",
            "Chờ phím Backspace của bạn đi khỏi rồi mới vá lại dấu.",
            &ui.flow_backspace_restore_spin,
            Bucket::Park,
            d.flow_backspace_restore_delay_ms,
            false,
        ),
        spin(
            "focus_bounce_ms",
            "rows.focus_bounce",
            "Hoãn reset khi mất focus",
            "Nháy mất focus vẫn giữ từ",
            "tất cả các chế độ",
            "Mất focus trong khoảng này thì coi là nháy, giữ nguyên từ đang gõ.",
            &ui.focus_bounce_spin,
            Bucket::Park,
            d.focus_bounce_ms,
            false,
        ),
        spin(
            "marker_wait_ms",
            "rows.marker_wait",
            "Chờ sau dấu ·",
            "Chỉ dùng khi chèn dấu ·",
            "tất cả, nhưng chỉ khi chọn cách Chèn dấu ·",
            "Chờ sau khi chèn dấu · để browser bỏ gợi ý trước khi xoá.",
            &ui.marker_wait_spin,
            Bucket::Park,
            d.marker_wait_ms,
            false,
        ),
        Row {
            key: "rows.echo_stale",
            config: "echo_stale_ms",
            title: "Chờ phím ảo quay về",
            subtitle: "Quá hạn thì đổ chữ luôn",
            purpose: "Quá hạn này không thấy phím ảo quay về thì đổ chữ đã treo, để không mất chữ trên Wayland.",
            scope: "các chế độ bấm phím ảo: Flow, Surrounding, ForwardBackspace",
            spin: ui.echo_stale_spin.clone(),
            switch: Some(ui.echo_stale_switch.clone()),
            bucket: Bucket::Park,
            default: d.echo_stale_ms,
            gated: true,
        },
        Row {
            key: "rows.idle_commit",
            config: "idle_commit_ms",
            title: "Tự chốt từ khi ngừng gõ",
            subtitle: "Như bấm dấu cách",
            purpose: "Ngừng gõ quá lâu thì tự chốt từ đang gạch chân, như vừa bấm dấu cách.",
            scope: "chế độ còn gạch chân từ đang gõ (Preedit)",
            spin: ui.idle_commit_spin.clone(),
            switch: Some(ui.idle_commit_switch.clone()),
            bucket: Bucket::Park,
            default: d.idle_commit_ms,
            gated: true,
        },
    ]
}
