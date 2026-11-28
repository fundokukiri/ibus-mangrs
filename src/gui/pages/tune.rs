//! Page "Tinh chỉnh": the knobs that change how typing feels — the two
//! caret edits, the uinput pacing (Flow only) and the X11/XIM workarounds.
//! The rows themselves come from `build`; this page groups them and keeps
//! subtitles to two lines.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // How Backspace and the arrow keys behave while a word is open: they
    // change the typing feel, not the spelling rules of the tab above.
    let marks = adw::PreferencesGroup::builder()
        .title(t("groups.tune.marks.title", "Dấu &amp; điều hướng"))
        .build();
    marks.add(&ui.backspace_tone);
    marks.add(&ui.nav_key_while_preedit);
    page.add(&marks);

    // Every timing of the four transports in one table: the uinput pacing,
    // both parked graces, the two Flow delays and the two robustness
    // windows, each with its shipped default and a reset.
    page.add(&nhip::build(ui));

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
        "Cửa sổ preedit riêng cho XIM",
    ));
    page.add(&xim);

    page
}
