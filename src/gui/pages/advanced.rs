//! Page "Tinh chỉnh": the set-once technical settings — the uinput
//! pacing, the X11/XIM workarounds and the debug-log switch. The panel
//! menu editor has its own page (see panel.rs).

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The uinput pacing applies to Smooth / Super Smooth / Select only; the
    // spin rows save on every value change like every other control.
    let pacing = adw::PreferencesGroup::builder()
        .title("Tốc độ gõ (uinput)")
        .description("Chỉ áp dụng cho Smooth / Super Smooth / Select")
        .build();
    pacing.add(&ui.commit_interval);
    pacing.add(&ui.backspace_interval);
    pacing.add(&ui.post_delay);
    pacing.add(&ui.unicode_channel);
    page.add(&pacing);
    for row in [&ui.commit_interval, &ui.backspace_interval, &ui.post_delay] {
        let s = ui.save_cb.clone();
        row.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }

    let xim = adw::PreferencesGroup::builder()
        .title("Tương thích X11/XIM")
        .description("Cho app X11 trực tiếp và app XIM (Java/JetBrains)")
        .build();
    xim.add(&ui.x11_delete);
    xim.add(&ui.preedit_xim);
    page.add(&xim);

    let diagnostics = adw::PreferencesGroup::builder()
        .title("Chẩn đoán")
        .description("Ghi lại quyết định của engine để gỡ lỗi")
        .build();
    diagnostics.add(&ui.debug_log);
    page.add(&diagnostics);

    page
}
