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
    /// The modes the value reaches.
    scope: &'static str,
    /// What raising or lowering it does, in the tooltip's last line.
    effect: &'static str,
    /// The modes the value acts on, as the badge under the subtitle:
    /// P = Preedit, S = Surrounding, F = Flow, FB = ForwardBackspace.
    modes: &'static str,
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
            "Thời gian chờ của cả bốn chế độ, tính bằng mili giây. P: gạch chân · S: Surrounding · F: Flow · FB: ForwardBackspace.",
        ))
        .build();
    let rows = rows(ui);
    let mut placed: Vec<(gtk4::Widget, Bucket)> = Vec::new();
    for row in &rows {
        let widget = spin_or_switch_row(row);
        // The tooltip leads with the config key, then three catalogue lines:
        // what the key does, the modes it reaches, what raising it does. All
        // four travel through `t` so an English window reads English.
        let line =
            |field: &str, fallback: &str| t(&format!("nhip.{}.{}", row.config, field), fallback);
        let text = format!(
            "`{}`\n{}\n{}: {}\n{}: {}",
            row.config,
            line("purpose", row.purpose),
            t("rows.nhip.scope", "Áp dụng"),
            line("scope", row.scope),
            t("rows.nhip.effect", "Chỉnh"),
            line("effect", row.effect)
        );
        widget.set_tooltip_text(Some(&text));
        if row.gated {
            ui.nhip_gated.borrow_mut().push(widget.clone().upcast());
        }
        placed.push((widget.clone().upcast(), row.bucket));
        group.add(&widget);
    }
    group.set_header_suffix(Some(&header_controls(&rows, placed)));
    // The marker pause only exists for one address-bar style: the dot that
    // makes a browser drop its inline completion. Locked for the other two.
    gate_marker_pause(ui);
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
    // The modes lead the description line: a user compares values by what
    // they reach, so the badge comes before the prose.
    let widget = action_row(
        &row.title(),
        &format!(
            "{} {}",
            row.modes,
            t(&format!("{}.subtitle", row.key), row.subtitle)
        ),
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

/// Write the shipped values back through the spins, so the normal save
/// funnel persists them.
pub(crate) fn reset_spins(defaults: &[(gtk4::SpinButton, u64)]) {
    for (spin, value) in defaults {
        spin.set_value(*value as f64);
    }
}

/// The marker pause follows the address-bar style: editable only while
/// "Chèn dấu ·" is the pick.
fn gate_marker_pause(ui: &Rc<Ui>) {
    ui.marker_wait_spin
        .set_sensitive(ui.dodge_fix.selected() == 2);
    let spin = ui.marker_wait_spin.clone();
    ui.dodge_fix
        .connect_selected_notify(move |d| spin.set_sensitive(d.selected() == 2));
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
        reset.connect_clicked(move |button| {
            // A reset rewrites ten values at once, and the button sits in a
            // header a scroll can brush past: ask first.
            let Some(window) = button.root().and_downcast::<gtk4::Window>() else {
                reset_spins(&defaults);
                return;
            };
            let dialog = adw::AlertDialog::builder()
                .heading(t("rows.nhip.reset_confirm_title", "Khôi phục mặc định?"))
                .body(t(
                    "rows.nhip.reset_confirm_body",
                    "Mọi giá trị trong bảng trở về như lúc mới cài.",
                ))
                .build();
            dialog.add_response("cancel", &t("misc.cancel", "Hủy"));
            dialog.add_response("reset", &t("rows.nhip.reset", "Khôi phục mặc định"));
            dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            let defaults = defaults.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "reset" {
                    reset_spins(&defaults);
                }
            });
            dialog.present(Some(&window));
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
                effect: &'static str,
                modes: &'static str,
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
        effect,
        modes,
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
            "Tăng: xoá chậm mà chắc ở app nặng; giảm: xoá nhanh, dễ mất ký tự nếu app xử lý chậm.",
            "[S,F,FB]",
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
            "Tăng: app kịp xử lý nhưng gõ thấy trễ; giảm: mượt hơn, dễ dính chữ khi app chậm.",
            "[S,F,FB]",
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
            "Tăng: chữ chắc chắn hiện sau khi xoá xong nhưng gõ trễ; giảm: mượt hơn, có thể hiện chữ trước dấu.",
            "[S,F,FB]",
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
            "Tăng: an toàn cho app chậm nhưng gõ trễ; giảm: nhanh hơn, dễ hiện chữ khi chưa xoá xong.",
            "[S]",
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
            "Tăng: dấu hiện chậm mà chắc; giảm: dấu hiện ngay, dễ nhấp nháy hoặc lẫn dấu.",
            "[F]",
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
            "Tăng: chắc phím Backspace đã xong nên hoàn dấu đúng hơn; giảm: sớm quá thì phím Backspace xoá mất.",
            "[F]",
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
            "Tăng: giữ từ qua lần mất focus lâu hơn; giảm: reset sớm, dễ mất từ khi cửa sổ nháy.",
            "[P,S,F,FB]",
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
            "Tăng: browser kịp bỏ gợi ý rồi mới xoá; giảm: xoá sớm, dễ xoá nhầm gợi ý.",
            "[S,F,FB]",
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
            effect: "Tăng: chờ lâu hơn nên ít mất chữ nhưng chữ hiện trễ; giảm: đổ chữ sớm, dễ mất chữ khi phím ảo về chậm.",
            modes: "[S,F,FB]",
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
            effect: "Tăng: từ giữ gạch chân lâu hơn; giảm: tự chốt từ sớm, gõ tiếp thành từ mới.",
            modes: "[P]",
            spin: ui.idle_commit_spin.clone(),
            switch: Some(ui.idle_commit_switch.clone()),
            bucket: Bucket::Park,
            default: d.idle_commit_ms,
            gated: true,
        },
    ]
}
