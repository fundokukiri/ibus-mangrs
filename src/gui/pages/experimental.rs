//! Page "Thử nghiệm": every feature behind the experimental master gate on
//! one tab. The rows themselves come from `build`; the gate ("Cài đặt khác")
//! shows and hides the whole tab, and the "Thử nghiệm" badge sits on the
//! primary row of each feature — the switch — never on its entry/spin twin.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let typing = adw::PreferencesGroup::builder()
        .title(t("groups.experimental.typing.title", "Dấu & gõ tắt"))
        .build();
    typing.add(&ui.macro_valid_words);
    typing.add(&ui.extra_url_chars_switch);
    typing.add(&ui.extra_url_chars_entry);
    typing.add(&ui.auto_capitalize_after_switch);
    typing.add(&ui.auto_capitalize_after_entry);
    page.add(&typing);

    let tuning = adw::PreferencesGroup::builder()
        .title(t("groups.experimental.tuning.title", "Tinh chỉnh"))
        .build();
    tuning.add(&ui.clipboard_restore_switch);
    tuning.add(&ui.clipboard_restore_spin);
    tuning.add(&ui.echo_stale_switch);
    tuning.add(&ui.echo_stale_spin);
    tuning.add(&ui.idle_commit_switch);
    tuning.add(&ui.idle_commit_spin);
    page.add(&tuning);

    // The badge marks the feature's primary row — the switch that turns it
    // on. The entry/spin twins stay clean: two badges for one feature would
    // read as two features.
    decorate_experimental(&ui.macro_valid_words);
    decorate_experimental(&ui.extra_url_chars_switch);
    decorate_experimental(&ui.auto_capitalize_after_switch);
    decorate_experimental(&ui.clipboard_restore_switch);
    decorate_experimental(&ui.echo_stale_switch);
    decorate_experimental(&ui.idle_commit_switch);

    // The switch-gates-value pairs keep their lock behavior: off locks the
    // input, the typed value stays saved for when the switch returns.
    for (switch, value) in [
        (
            &ui.extra_url_chars_switch,
            ui.extra_url_chars_entry.upcast_ref::<gtk4::Widget>(),
        ),
        (
            &ui.auto_capitalize_after_switch,
            ui.auto_capitalize_after_entry.upcast_ref::<gtk4::Widget>(),
        ),
        (
            &ui.clipboard_restore_switch,
            ui.clipboard_restore_spin.upcast_ref::<gtk4::Widget>(),
        ),
        (
            &ui.echo_stale_switch,
            ui.echo_stale_spin.upcast_ref::<gtk4::Widget>(),
        ),
        (
            &ui.idle_commit_switch,
            ui.idle_commit_spin.upcast_ref::<gtk4::Widget>(),
        ),
    ] {
        let value = value.clone();
        switch.connect_active_notify(move |sw| {
            value.set_sensitive(sw.is_active());
        });
    }
    for entry in [&ui.extra_url_chars_entry, &ui.auto_capitalize_after_entry] {
        let ui = Rc::clone(ui);
        entry.connect_changed(move |_| ui.fire_save());
    }
    for spin in [
        &ui.clipboard_restore_spin,
        &ui.echo_stale_spin,
        &ui.idle_commit_spin,
    ] {
        let ui = Rc::clone(ui);
        spin.connect_value_notify(move |_| ui.fire_save());
    }

    page
}
