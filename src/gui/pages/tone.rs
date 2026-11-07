//! Page "Dấu & tự động": tone style, spelling recovery, automatic
//! capitalization and the special-character shortcuts. The rows come from
//! `build`; this page only groups them and decorates the experimental
//! double-space row with a caption badge before its switch.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // Row titles match the quick-menu registry labels (ibus_prop
    // ::QUICK_TOGGLES): a toggle ticked in the panel menu must be findable
    // under the same name here.
    let tones = adw::PreferencesGroup::builder()
        .title(t("groups.tone.tones.title", "Dấu thanh"))
        .description(t(
            "groups.tone.tones.description",
            "Kiểu đặt dấu trên nguyên âm",
        ))
        .build();
    tones.add(&ui.modern);
    tones.add(&ui.free);
    page.add(&tones);

    let spelling = adw::PreferencesGroup::builder()
        .title(t("groups.tone.spelling.title", "Chính tả"))
        .description(t(
            "groups.tone.spelling.description",
            "Sửa lỗi và khôi phục từ",
        ))
        .build();
    spelling.add(&ui.auto_correct);
    spelling.add(&ui.auto_restore);
    spelling.add(&ui.bypass_url);
    spelling.add(&ui.extra_url_chars_switch);
    spelling.add(&ui.extra_url_chars_entry);
    spelling.add(&ui.abbrev);
    decorate_experimental(&ui.extra_url_chars_switch);
    decorate_experimental(&ui.extra_url_chars_entry);
    // The switch gates its entry: off locks the input, and the engine
    // applies the default while the typed value stays saved for when the
    // switch returns. The save itself goes through wire_switch_saves.
    {
        let ui = Rc::clone(ui);
        let switch = ui.extra_url_chars_switch.clone();
        let entry = ui.extra_url_chars_entry.clone();
        switch.connect_active_notify(move |sw| {
            entry.set_sensitive(sw.is_active());
        });
    }
    {
        let ui = Rc::clone(ui);
        let entry = ui.extra_url_chars_entry.clone();
        entry.connect_changed(move |_| ui.fire_save());
    }
    page.add(&spelling);

    let automatic = adw::PreferencesGroup::builder()
        .title(t("groups.tone.auto.title", "Tự động"))
        .description(t(
            "groups.tone.auto.description",
            "Viết hoa và dấu câu tự động",
        ))
        .build();
    automatic.add(&ui.auto_capitalize);
    automatic.add(&ui.auto_capitalize_after_switch);
    automatic.add(&ui.auto_capitalize_after_entry);
    automatic.add(&ui.double_space);
    decorate_experimental(&ui.auto_capitalize_after_switch);
    decorate_experimental(&ui.auto_capitalize_after_entry);
    // double_space graduated: no "Thử nghiệm" badge, not gated by the
    // experimental master switch.
    // Same pair shape: off locks the input, the value stays saved.
    {
        let ui = Rc::clone(ui);
        let switch = ui.auto_capitalize_after_switch.clone();
        let entry = ui.auto_capitalize_after_entry.clone();
        switch.connect_active_notify(move |sw| {
            entry.set_sensitive(sw.is_active());
        });
    }
    {
        let ui = Rc::clone(ui);
        let entry = ui.auto_capitalize_after_entry.clone();
        entry.connect_changed(move |_| ui.fire_save());
    }
    page.add(&automatic);

    let special = adw::PreferencesGroup::builder()
        .title(t("groups.tone.special.title", "Ký tự đặc biệt"))
        .description(t(
            "groups.tone.special.description",
            "Chuyển đổi phím tắt ký tự",
        ))
        .build();
    special.add(&ui.w2u);
    special.add(&ui.bracket);
    special.add(&ui.em_dash);
    page.add(&special);

    page
}
