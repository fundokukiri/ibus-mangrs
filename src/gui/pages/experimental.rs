//! Page "Thử nghiệm": every feature behind the experimental master gate on
//! one tab. The gate switch itself leads this tab: off locks every row and
//! displays the shipped defaults (config.toml keeps the user's values, the
//! engine bakes the defaults in) — the tab never hides. Each feature is one
//! row: the "Thử nghiệm" badge sits left of its switch, the value control
//! follows on the same row; a switch and its value are never two rows.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The gate leads its own tab: turning it off locks the rows below and
    // shows their defaults instead of hiding anything (config save goes
    // through wire_switch_saves).
    let gate = adw::PreferencesGroup::builder()
        .title(t("groups.experimental.gate.title", "Bật thử nghiệm"))
        .build();
    gate.add(&ui.experimental_features);
    {
        let ui = Rc::clone(ui);
        let switch = ui.experimental_features.clone();
        switch.connect_active_notify(move |sw| {
            // Instant presentation: the watcher round-trips the config
            // anyway, but the user must not stare at a stale tab for a
            // second. No page jump — the tab never hides. Restore runs
            // before the sensitivity pass so the twin corrections see the
            // restored switch states, not the displayed defaults.
            if sw.is_active() {
                ui.restore_experimental_saved();
                ui.set_experimental_sensitive(true);
            } else {
                ui.set_experimental_sensitive(false);
                ui.show_experimental_defaults();
            }
        });
    }
    page.add(&gate);

    let typing = adw::PreferencesGroup::builder()
        .title(t("groups.experimental.typing.title", "Dấu &amp; gõ tắt"))
        .build();
    typing.add(&ui.macro_valid_words);
    typing.add(&ui.extra_url_chars_row);
    typing.add(&ui.auto_capitalize_after_row);
    page.add(&typing);

    let tuning = adw::PreferencesGroup::builder()
        .title(t("groups.experimental.tuning.title", "Tinh chỉnh"))
        .build();
    tuning.add(&ui.clipboard_restore_row);
    tuning.add(&ui.echo_stale_row);
    tuning.add(&ui.idle_commit_row);
    tuning.add(&ui.flow_per_key_spin);
    tuning.add(&ui.flow_backspace_retrack);
    tuning.add(&ui.nav_key_while_preedit);
    page.add(&tuning);

    // The badge marks each feature's one row — the merged rows carry it
    // ahead of their own switch, the standalone rows on themselves.
    decorate_experimental(&ui.macro_valid_words);
    decorate_experimental(&ui.extra_url_chars_row);
    decorate_experimental(&ui.auto_capitalize_after_row);
    decorate_experimental(&ui.clipboard_restore_row);
    decorate_experimental(&ui.echo_stale_row);
    decorate_experimental(&ui.idle_commit_row);
    // A combo-only feature: the combo is its own primary row.
    decorate_experimental(&ui.nav_key_while_preedit);
    // The per-key delay knob is experimental on its own: its switch lives on
    // the Tuning page and is not gated, so the spin is this feature's only
    // experimental row and carries the badge itself.
    decorate_experimental(&ui.flow_per_key_spin);
    decorate_experimental(&ui.flow_backspace_retrack);
    // The switches themselves glow the same warning amber as the badge and
    // the tab label: one look, one meaning.
    for switch in [
        ui.macro_valid_words.upcast_ref::<gtk4::Widget>(),
        ui.extra_url_chars_switch.upcast_ref::<gtk4::Widget>(),
        ui.auto_capitalize_after_switch.upcast_ref::<gtk4::Widget>(),
        ui.clipboard_restore_switch.upcast_ref::<gtk4::Widget>(),
        ui.echo_stale_switch.upcast_ref::<gtk4::Widget>(),
        ui.idle_commit_switch.upcast_ref::<gtk4::Widget>(),
    ] {
        switch.add_css_class("experimental");
    }

    // The amber trough cannot be expressed with built-in classes; load the
    // tab's one CSS block onto the display (a display exists whenever the
    // GUI runs).
    const EXPERIMENTAL_CSS: &str =
        "switch.experimental:checked { background-color: @warning_color; }";
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(EXPERIMENTAL_CSS);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // The switch-gates-value pairs keep their lock behavior: off locks the
    // input, the typed value stays saved for when the switch returns. The
    // flow pair keeps its own row types — its switch lives on the Tune tab.
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
        let ui = Rc::clone(ui);
        let value = value.clone();
        switch.connect_active_notify(move |sw| {
            // The master gate locks the whole tab: a switch flip must not
            // unlock its twin while the gate is off.
            value.set_sensitive(sw.is_active() && ui.experimental_features.is_active());
        });
    }
    {
        let ui = Rc::clone(ui);
        let switch = ui.flow_per_key_switch.clone();
        let value = ui.flow_per_key_spin.upcast_ref::<gtk4::Widget>().clone();
        switch.connect_active_notify(move |sw| {
            value.set_sensitive(sw.is_active() && ui.experimental_features.is_active());
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
    {
        let ui = Rc::clone(ui);
        let spin = ui.flow_per_key_spin.clone();
        spin.connect_value_notify(move |_| ui.fire_save());
    }

    page
}
