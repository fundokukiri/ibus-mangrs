//! Page "Tinh chỉnh": the technical knobs most users never touch — the
//! uinput pacing (Smooth / Super Smooth / Select only) and the X11/XIM
//! workarounds. Moved here from "Cài đặt khác" so that page stays about
//! system integration (icon, extension, backup, diagnostics).

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // uinput pacing: the rewrite speed knobs only the smooth modes use.
    let pacing = adw::PreferencesGroup::builder()
        .title(t("groups.tune.pacing.title", "Tốc độ gõ (uinput)"))
        .description(t(
            "groups.tune.pacing.description",
            "Chỉ áp dụng cho Smooth / Super Smooth / Select",
        ))
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

    // X11/XIM workarounds.
    let xim = adw::PreferencesGroup::builder()
        .title(t("groups.tune.xim.title", "Tương thích X11/XIM"))
        .description(t(
            "groups.tune.xim.description",
            "Cho app X11 trực tiếp và app XIM (Java/JetBrains)",
        ))
        .build();
    xim.add(&ui.x11_delete);
    xim.add(&ui.preedit_xim);
    page.add(&xim);

    page
}
