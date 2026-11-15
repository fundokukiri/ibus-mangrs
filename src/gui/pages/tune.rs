//! Page "Tinh chỉnh": the technical knobs most users never touch — the
//! uinput pacing (Flow only) and the X11/XIM workarounds. The rows
//! themselves come from `build`; this page groups them and keeps subtitles
//! to two lines.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // uinput pacing plus the per-key park delay: everything only the
    // uinput modes use.
    let pacing = adw::PreferencesGroup::builder()
        .title(t("groups.tune.pacing.title", "Cấu hình uinput"))
        .description(t("groups.tune.pacing.description", "Chỉ áp dụng cho Flow."))
        .build();
    pacing.add(&ui.pacing_row);
    // The per-key display's park delay lives with the pacing knobs: it
    // paces the same uinput word rendering, outside the experimental gate.
    pacing.add(&ui.flow_per_key_delay_row);
    {
        // The spin needs its own value hook, like the pacing spins.
        let s = ui.save_cb.clone();
        ui.flow_per_key_spin.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }
    for spin in [&ui.commit_interval, &ui.backspace_interval, &ui.post_delay] {
        let s = ui.save_cb.clone();
        spin.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }
    page.add(&pacing);

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
