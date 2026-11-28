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
    page.add(&marks);

    // Every timing of the four transports in one table: the uinput pacing,
    // both parked graces, the two Flow delays and the two robustness
    // windows, each with its shipped default and a reset.
    page.add(&nhip::build(ui));

    page
}
