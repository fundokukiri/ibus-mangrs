//! Page "Tinh chỉnh": the technical knobs most users never touch — the
//! uinput pacing (Smooth / Super Smooth / Select only), the delivery
//! channel and the X11/XIM workarounds. The rows themselves come from
//! `build`; this page groups them, keeps subtitles to two lines and prints
//! the real defaults from `Config::default()` in the pacing subtitles.

use super::*;

/// AdwSpinRow has no public API for the spin width: find the inner
/// GtkSpinButton and widen its character box, like the reference mockup.
fn widen_spin(row: &adw::SpinRow, chars: i32) {
    fn walk(w: &gtk4::Widget, chars: i32) -> bool {
        if let Some(spin) = w.downcast_ref::<gtk4::SpinButton>() {
            spin.set_width_chars(chars);
            return true;
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if walk(&c, chars) {
                return true;
            }
            child = c.next_sibling();
        }
        false
    }
    walk(row.upcast_ref(), chars);
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // uinput pacing: the rewrite speed knobs only the smooth modes use.
    // The "Mặc định: N" numbers come from the config's own defaults, so
    // the text never drifts from what a fresh config actually does.
    let defaults = crate::config::Config::default();
    let pacing = adw::PreferencesGroup::builder()
        .title(t("groups.tune.pacing.title", "Tốc độ gõ (uinput)"))
        .description(t(
            "groups.tune.pacing.description",
            "Chỉ áp dụng cho Smooth, Super Smooth và Select.",
        ))
        .build();
    pacing.add(&ui.commit_interval);
    pacing.add(&ui.backspace_interval);
    pacing.add(&ui.post_delay);
    for (row, subtitle) in [
        (
            &ui.commit_interval,
            t(
                "rows.commit_interval.subtitle",
                "Khoảng cách giữa hai lần gửi chữ, tính bằng ms. Mặc định: {n}",
            )
            .replace("{n}", &defaults.commit_interval_ms.to_string()),
        ),
        (
            &ui.backspace_interval,
            t(
                "rows.backspace_interval.subtitle",
                "Khoảng cách giữa hai lần Backspace, tính bằng ms. Mặc định: {n}",
            )
            .replace("{n}", &defaults.backspace_interval_ms.to_string()),
        ),
        (
            &ui.post_delay,
            t(
                "rows.post_delay.subtitle",
                "Chờ sau khi viết lại từ, trước khi gõ tiếp, tính bằng ms. Mặc định: {n}",
            )
            .replace("{n}", &defaults.post_delay_ms.to_string()),
        ),
    ] {
        widen_spin(row, 18);
        row.set_subtitle(&subtitle);
        row.set_tooltip_text(Some(&t(
            "tips.commit_interval",
            "Nhịp gửi từng chữ ở mode uinput — giảm nếu app bị mất chữ, tăng nếu bị đơ.",
        )));
        row.update_property(&[gtk4::accessible::Property::Label(&format!(
            "{} (ms)",
            row.title()
        ))]);
    }
    for row in [&ui.commit_interval, &ui.backspace_interval, &ui.post_delay] {
        let s = ui.save_cb.clone();
        row.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }
    page.add(&pacing);

    // The delivery channel is not a speed knob: its own group keeps the
    // pacing description from crowding the selected value.
    let channel = adw::PreferencesGroup::builder()
        .title(t("groups.tune.channel.title", "Kênh gửi chữ"))
        .build();
    channel.add(&ui.unicode_channel);
    ui.unicode_channel.set_subtitle(&t(
        "rows.unicode_channel.subtitle",
        "Clipboard sẽ tạm ghi đè nội dung clipboard của bạn.",
    ));
    page.add(&channel);

    // X11/XIM workarounds.
    let xim = adw::PreferencesGroup::builder()
        .title(t("groups.tune.xim.title", "Tương thích X11/XIM"))
        .description(t(
            "groups.tune.xim.description",
            "Cho app X11 trực tiếp và app XIM (Java/JetBrains).",
        ))
        .build();
    xim.add(&ui.x11_delete);
    xim.add(&ui.preedit_xim);
    ui.x11_delete.set_subtitle(&t(
        "rows.x11_delete.subtitle",
        "App X11 xoá đuôi từ mà không báo cáo, giống ibus-bamboo. Có thể xoá nhầm ký tự nếu con trỏ bị đổi chỗ.",
    ));
    ui.preedit_xim.set_subtitle(&t(
        "rows.preedit_xim.subtitle",
        "Giữ gạch chân cho app XIM: chữ đang gõ hiện trong cửa sổ riêng, commit khi hết từ.",
    ));
    page.add(&xim);

    page
}
