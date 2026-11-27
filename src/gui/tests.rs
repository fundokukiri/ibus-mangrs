use super::*;

#[test]
fn restart_script_respawns_the_xim_bridge_on_wayland() {
    // The Wayland restart must bring ibus-x11 back: nothing else respawns
    // the bridge (the distro hook runs once per login) and XIM apps lose
    // input until the next logout without it.
    let script = pages::other::restart_script(Some("wayland"));
    assert!(script.starts_with("ibus restart"));
    assert!(
        script.contains("pgrep -x ibus-x11"),
        "skip the spawn when a bridge already runs: {script}"
    );
    assert!(
        script.contains("nohup /usr/lib/ibus/ibus-x11"),
        "spawn the bridge the way the distro hook does: {script}"
    );
    assert_eq!(
        pages::other::restart_script(Some("x11")),
        "ibus restart",
        "an X11 session keeps its own bridge under the daemon's --xim"
    );
}

/// GTK initializes once per process and only ever on the thread that did
/// it, while libtest hands every `#[test]` its own thread — separate GTK
/// tests flake with "Attempted to initialize GTK from two different
/// threads" depending on the scheduler. One test runs every widget
/// scenario sequentially on the one thread that initialized GTK; each
/// scenario keeps its own assertions. Needs a display; skips where GTK
/// cannot start.
#[test]
fn gtk_widget_scenarios() {
    // Serialized against the text.rs language tests: both sides read the
    // process-global language, and libtest runs tests on separate threads.
    let _g = crate::gui::text::LANG_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if gtk4::init().is_err() {
        eprintln!("gui: SKIP (no display)");
        return;
    }
    // Sandbox the config dir for the whole scenario run: the rebuild
    // scenario drives the REAL row handler, whose save funnel writes the
    // config files. Concurrent unit tests keep reading the sandboxed dir —
    // they are self-contained writers, so that stays consistent.
    let original_config_dir = std::env::var("XDG_CONFIG_HOME").ok();
    let sandbox = crate::test_support::TempDir::new("mangrs-gui-test");
    std::env::set_var("XDG_CONFIG_HOME", sandbox.path());
    refresh_round_trips_every_widget();
    shortcut_helpers_render_and_capture();
    external_reload_refreshes_without_saving();
    row_texts_render_as_plain_text();
    flow_delay_row_shows_captions();
    rules_add_group_merges_the_focus_chip();
    rule_rows_round_trip_through_read();
    macro_page_helpers_count_and_preview();
    macros_editor_follows_the_switch();
    emoji_editor_follows_the_switch();
    toggling_off_releases_the_editor_caret();
    close_test_windows();
    the_debug_tab_follows_the_switch();
    each_column_carries_its_own_header_bar();
    the_window_can_shrink_to_fit_a_small_screen();
    close_test_windows();
    close_test_windows();
    the_search_button_opens_the_entry();
    close_test_windows();
    every_switch_on_a_page_lands_on_the_same_edge();
    the_two_debug_checkboxes_share_a_row();
    close_test_windows();
    the_debug_tab_row_appears_only_with_debug_mode();
    the_open_log_pane_picks_up_what_is_written();
    the_debug_page_opens_with_both_columns();
    close_test_windows();
    the_debug_page_shows_the_status_the_rows_and_the_footer();
    close_test_windows();
    close_test_windows();
    every_switch_triggers_save();
    debug_mode_switch_keeps_the_dialog_choice();
    group_descriptions_become_title_tooltips();
    experimental_badge_sits_beside_the_title();
    experimental_css_parses();
    hotkey_action_choices_follow_the_experimental_switches();
    experimental_gate_toggle_keeps_the_saved_values();
    popover_list_buttons_read_from_the_left();
    row_popover_items_read_from_the_left();
    mode_dropdowns_list_every_mode();
    cycle_subtitle_lists_the_five_modes();
    close_test_windows();
    pacing_columns_are_symmetric();
    close_test_windows();
    experimental_gate_locks_rows_and_preserves_values();
    preferred_backend_combo_saves_loads_and_survives_the_gate();
    combo_value_stays_right_after_a_pick();
    close_test_windows();
    popup_marks_the_selected_item();
    close_test_windows();
    combo_popups_do_not_select_on_hover();
    format_combos_keep_presets_and_unknowns();
    close_test_windows();
    language_row_round_trips();
    nav_key_row_round_trips();
    language_switch_rebuilds_the_window();
    language_handler_rebuilds_both_ways();
    save_round_skips_unchanged_files_and_stamps_self_writes();
    poll_reloads_skips_self_writes();
    close_test_windows();
    // Restore before the sandbox dir could be cleaned under a racing test.
    match original_config_dir {
        Some(dir) => std::env::set_var("XDG_CONFIG_HOME", dir),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    close_test_windows();
    the_window_controls_survive_a_narrow_window();
}

/// The combo item lists (mode/charset/method/...) render in the window's
/// language; positions stay the index contract either way. `labels()` keeps a
/// direct assert on one surviving list, `mode_label` on the mode helper.
#[test]
fn labels_follow_the_language() {
    let _g = crate::gui::text::LANG_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    set_language("vi");
    assert_eq!(labels(BACKSPACE_TONE_MODES)[0], "Xóa dấu trước");
    assert_eq!(mode_label(ModeConfig::Preedit), "Gạch chân (preedit)");
    set_language("en");
    assert_eq!(labels(BACKSPACE_TONE_MODES)[0], "Tone first");
    assert_eq!(mode_label(ModeConfig::Preedit), "Underline (preedit)");
    // Restore for other tests; LANG is process-global and the GTK scenario
    // runner asserts Vietnamese text in a parallel thread.
    set_language("vi");
}

/// Close every mapped toplevel. The scenarios present real windows only to
/// run map-time checks; left alive, they sit frozen on the desktop for the
/// rest of the run and the session offers a force-close when the test
/// process exits.
fn close_test_windows() {
    for w in gtk4::Window::list_toplevels() {
        if let Some(win) = w.downcast_ref::<gtk4::Window>() {
            win.destroy();
        }
    }
    for _ in 0..3 {
        gtk4::glib::MainContext::default().iteration(false);
    }
}

/// The merged pacing row is laid out symmetrically: the two caption
/// columns share one width and the divider between them sits at that
/// pair's centre. Packed at natural widths ("Nhịp xoá lùi" is much shorter
/// than "Chờ sau khi viết lại từ") the divider drifted and the row read
/// lopsided.
fn pacing_columns_are_symmetric() {
    let ui = widgets();
    let page = crate::gui::pages::tune::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let column_of = |row: &gtk4::Widget, caption: &str| {
        let label = descendants::<gtk4::Label>(row)
            .into_iter()
            .find(|l| l.text() == caption)
            .expect("the caption label");
        label
            .parent()
            .and_downcast::<gtk4::Box>()
            .expect("the caption column")
    };
    let pacing = ui.pacing_row.clone().upcast::<gtk4::Widget>();
    let flow = ui.flow_delay_row.clone().upcast::<gtk4::Widget>();
    let left = column_of(&pacing, "Xoá lùi");
    let right = column_of(&pacing, "Chờ viết lại từ");
    let bounds = |w: &gtk4::Widget| {
        w.compute_bounds(ui.pacing_row.upcast_ref::<gtk4::Widget>())
            .expect("an allocated column")
    };
    let (l, r) = (bounds(left.upcast_ref()), bounds(right.upcast_ref()));
    assert_eq!(
        l.width(),
        r.width(),
        "both caption columns take the same width"
    );
    let divider = descendants::<gtk4::Separator>(&pacing)
        .into_iter()
        .next()
        .expect("the divider between the columns");
    let d = bounds(divider.upcast_ref());
    let pair_centre = (l.x() + r.x() + r.width()) / 2.0;
    assert!(
        (d.x() + d.width() / 2.0 - pair_centre).abs() <= 1.0,
        "the divider sits at the pair's centre"
    );
    // The Flow row shares the width, so its first column starts at the same
    // x — the two rows read as one grid instead of two clusters.
    let flow_left = bounds(column_of(&flow, "Hiện dấu").upcast_ref::<gtk4::Widget>());
    assert_eq!(
        flow_left.x(),
        l.x(),
        "both merged rows start their columns at the same x"
    );
}

/// Every widget below `w` matching `T`, in tree order. Test-only — the
/// combo internals (popover, value view) have no public accessors.
fn descendants<T: IsA<gtk4::Widget>>(w: &gtk4::Widget) -> Vec<T> {
    fn walk<T: IsA<gtk4::Widget>>(w: &gtk4::Widget, out: &mut Vec<T>) {
        if let Some(matched) = w.downcast_ref::<T>() {
            out.push(matched.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c, out);
            child = c.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(w, &mut out);
    out
}

/// The widened combo value stays right-aligned after a pick: the value
/// view rebuilds the item widget for the new selection, so the align poke
/// must run on the selection change too — a one-time poke left the picked
/// value at the default alignment (the text jumped toward the center
/// until the window reopened).
fn combo_value_stays_right_after_a_pick() {
    let ui = widgets();
    let page = crate::gui::pages::typing::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let view = combo_value_view(&ui.method).expect("the combo's value view");
    let label = first_label(&view).expect("the value label after map");
    assert_eq!(label.xalign(), 1.0, "map aligns the selected value");
    // Pick another item: the displayed label is rebuilt for it.
    let model = ui.method.model().expect("the combo model");
    let next = (ui.method.selected() + 1) % model.n_items();
    ui.method.set_selected(next);
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let view = combo_value_view(&ui.method).expect("the value view after pick");
    let label = first_label(&view).expect("the value label after pick");
    assert_eq!(label.xalign(), 1.0, "a pick must not reset the alignment");
}

/// The popup list marks the combo's selected item with a checkmark: the
/// checkmark exists only in libadwaita's default factory, which the
/// widened combos replace, so the popup factory rebuilds it. With the
/// popup open, exactly one checkmark must be lit — the selected item's.
fn popup_marks_the_selected_item() {
    let ui = widgets();
    let page = crate::gui::pages::typing::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let popover = descendants::<gtk4::Popover>(ui.method.upcast_ref())
        .into_iter()
        .next()
        .expect("the combo popover");
    popover.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    // Template order: the inline value view first, the popup list second.
    let list = descendants::<gtk4::ListView>(ui.method.upcast_ref())
        .into_iter()
        .nth(1)
        .expect("the popup list view");
    let lit = descendants::<gtk4::Image>(list.upcast_ref())
        .into_iter()
        .filter(|icon| icon.opacity() == 1.0)
        .count();
    assert_eq!(lit, 1, "exactly the selected item carries a lit checkmark");
}

/// Combo popups must not select whatever row the pointer merely crosses:
/// libadwaita's combo template ships its popup ListView with single-click-
/// activate on, which GTK reads as "activate on click and SELECT on hover" —
/// the selected look (accent row, checkmark) swept along with the mouse
/// instead of waiting for a press. The builder turns the flag off below
/// every combo row; picking happens on click instead (see widgets.rs).
fn combo_popups_do_not_select_on_hover() {
    let ui = widgets();
    let combos = [
        &ui.method,
        &ui.mode,
        &ui.charset,
        &ui.w2u,
        &ui.backspace_tone,
        &ui.nav_key_while_preedit,
        &ui.bracket,
        &ui.preferred_backend,
        &ui.language,
    ];
    for row in combos {
        let sweeps_with_hover = descendants::<gtk4::ListView>(row.upcast_ref())
            .into_iter()
            .any(|view| view.is_single_click_activate());
        assert!(
            !sweeps_with_hover,
            "{}'s popup must not select rows on hover",
            row.title()
        );
    }
}

/// While typing in a raw table, a switch turned off (by hand or by a live
/// config reload) must not leave the caret blinking in the dead editor —
/// focus is released. The text itself stays.
fn toggling_off_releases_the_editor_caret() {
    let ui = widgets();
    let page = crate::gui::pages::data::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    ui.dictionary_text.grab_focus();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let scroll = find_card_scroll(page.upcast_ref()).expect("the editor card");
    // The editor must be live to take focus at all (switch on first).
    ui.dictionary_enabled.set_active(true);
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    // grab_focus is unreliable in a test window the display never
    // activates; the focus-widget property is the same state the gate
    // releases.
    gtk4::prelude::GtkWindowExt::set_focus(&win, Some(&ui.dictionary_text));
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let focused = gtk4::prelude::GtkWindowExt::focus(&win);
    println!(
        "DEBUG focus after set_focus: {:?} inside={}",
        focused.as_ref().map(|f| f.type_().name().to_string()),
        focused.as_ref().is_some_and(|f| f.is_ancestor(&scroll))
    );
    assert!(
        focused.is_some_and(|f| f.is_ancestor(&scroll)),
        "the caret starts inside the editor"
    );
    ui.dictionary_enabled.set_active(false);
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let focused = gtk4::prelude::GtkWindowExt::focus(&win);
    assert!(
        !focused.is_some_and(|f| f.is_ancestor(&scroll)),
        "no caret may stay in the disabled editor"
    );
    // The text survives the round trip.
    ui.dictionary_enabled.set_active(true);
    for _ in 0..5 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(scroll.is_sensitive());
}

/// The editor card of a raw-table page: the page's only ScrolledWindow
/// with the card class.
fn find_card_scroll(w: &gtk4::Widget) -> Option<gtk4::ScrolledWindow> {
    if let Some(s) = w.downcast_ref::<gtk4::ScrolledWindow>() {
        if s.has_css_class("card") {
            return Some(s.clone());
        }
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        if let Some(s) = find_card_scroll(&c) {
            return Some(s);
        }
        child = c.next_sibling();
    }
    None
}

/// With "Bật gõ tắt" off, the macro editor visibly disables with the
/// switch (the dictionary editor's behavior) — but never stops saving.
fn macros_editor_follows_the_switch() {
    let ui = widgets();
    let page = crate::gui::pages::macros::build(&ui);
    let scroll = find_card_scroll(page.upcast_ref()).expect("the editor card");
    ui.macro_enabled.set_active(true);
    for _ in 0..5 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(scroll.is_sensitive(), "editor editable with Gõ tắt on");
    ui.macro_enabled.set_active(false);
    for _ in 0..5 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(!scroll.is_sensitive(), "editor dead with Gõ tắt off");
}

/// Same gating on the Emoji page: with "Gõ tắt emoji" off the shortcode
/// table is uneditable.
fn emoji_editor_follows_the_switch() {
    let ui = widgets();
    let page = crate::gui::pages::emoji::build(&ui);
    let scroll = find_card_scroll(page.upcast_ref()).expect("the editor card");
    ui.emoji_enabled.set_active(true);
    for _ in 0..5 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(scroll.is_sensitive(), "editor editable with the switch on");
    ui.emoji_enabled.set_active(false);
    for _ in 0..5 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(!scroll.is_sensitive(), "editor dead with the switch off");
}

/// The raw-table helpers: the preview rejects invalid strftime patterns
/// (chrono's own verdict, the same call the engine's expand makes), and
/// the line stats follow the parsers' drop rule — only a non-blank line
/// without '=' counts as an error line.
fn macro_page_helpers_count_and_preview() {
    use crate::gui::pages::macros::preview;
    use crate::gui::widgets::line_stats;
    let shown = preview("%H:%M").expect("a valid pattern previews");
    assert_eq!(shown.len(), 5, "HH:MM shape");
    assert!(preview("%").is_none(), "a lone % is an invalid pattern");
    assert_eq!(
        preview("").as_deref(),
        Some(""),
        "empty pattern, empty preview"
    );
    let (valid, err) = line_stats(
        "vn=Việt Nam\ntphcm=Thành phố Hồ Chí Minh\ndongloi",
        "groups.macros.raw",
    );
    assert_eq!(valid, 2);
    assert_eq!(err.as_deref(), Some("Dòng 3 thiếu dấu ="));
    let (valid, err) = line_stats("a=1\nbad1\nb=2\nbad2\nbad3", "groups.macros.raw");
    assert_eq!(valid, 2);
    assert_eq!(
        err.as_deref(),
        Some("3 dòng thiếu dấu =, bắt đầu từ dòng 2")
    );
    // Blank lines and empty key/value sides are dropped silently, as they
    // always were — they must not be reported as missing '='.
    let (valid, err) = line_stats("a=1\n\nb=\n=x\n", "groups.macros.raw");
    assert_eq!(valid, 1);
    assert_eq!(err, None);
    // The emoji page shares the helper with its own message keys.
    let (valid, err) = line_stats("cuoi=😄\nbad", "groups.emoji.raw");
    assert_eq!(valid, 1);
    assert_eq!(err.as_deref(), Some("Dòng 2 thiếu dấu ="));
}

/// The reworked "Phím tắt" helpers: the stored spec renders as keycaps with
/// the dim fallback for unassigned or unconvertible values, conflicts
/// compare on the engine parse, and the capture dialog offers only "Hủy".
fn shortcut_helpers_render_and_capture() {
    // Accelerator rendering: modifiers first; modifier-only specs have no
    // keycap shape.
    assert_eq!(
        accelerator_label("Control+Alt+v").as_deref(),
        Some("<Control><Alt>v")
    );
    assert_eq!(
        accelerator_label("Control+space").as_deref(),
        Some("<Control>space")
    );
    assert_eq!(
        accelerator_label("Control+f5").as_deref(),
        Some("<Control>F5")
    );
    // Punctuation keys render as keycaps too: the accelerator string uses
    // the canonical keyval name GTK's parser accepts, and ShortcutLabel
    // shows the plain character again.
    assert_eq!(
        accelerator_label("Shift+~").as_deref(),
        Some("<Shift>asciitilde")
    );
    assert_eq!(accelerator_label("Alt+,").as_deref(), Some("<Alt>comma"));
    assert_eq!(
        accelerator_label("Control+`").as_deref(),
        Some("<Control>grave")
    );
    // A modifier-only combo has no key to draw a keycap for.
    assert_eq!(accelerator_label("Shift+Control"), None);
    assert!(same_combo("Alt+Control+v", "Control+Alt+v"));
    assert!(!same_combo("Control+v", "Control+Alt+v"));
    // The display swaps keycaps and the dim fallback.
    let display = ShortcutDisplay::new("Phím tắt đổi chế độ gõ");
    let stack = display.widget();
    display.render("");
    let label = stack
        .visible_child()
        .unwrap()
        .downcast_ref::<gtk4::Label>()
        .unwrap()
        .clone();
    assert_eq!(label.text(), "Chưa gán");
    display.render("Control+Alt+v");
    // The keycaps sit in their own right-aligned box inside the fixed slot.
    let keys = descendants::<gtk4::ShortcutLabel>(stack.visible_child().unwrap().upcast_ref())
        .into_iter()
        .next()
        .expect("a shortcut label");
    assert_eq!(keys.accelerator().as_deref(), Some("<Control><Alt>v"));
    // A modifier-only combo draws keycaps, not the raw spec text.
    display.render("Shift+Control");
    let caps: Vec<(String, bool)> =
        descendants::<gtk4::Label>(stack.visible_child().unwrap().upcast_ref())
            .into_iter()
            .map(|l| (l.text().to_string(), l.has_css_class("modifier-keycap")))
            .collect();
    assert_eq!(
        caps,
        vec![
            ("Ctrl".to_string(), true),
            ("+".to_string(), false),
            ("Shift".to_string(), true)
        ]
    );
    // An unparseable spec still falls back to the plain text.
    display.render("not-a-combo");
    let label = stack
        .visible_child()
        .unwrap()
        .downcast_ref::<gtk4::Label>()
        .unwrap()
        .clone();
    assert_eq!(label.text(), "not-a-combo");
    // The capture dialog: only the Hủy response, no rejection note yet.
    let (dialog, _preview, note) = build_capture_dialog(
        "Phím tắt đổi chế độ gõ",
        "Nhấn tổ hợp phím mới. Esc để hủy, Backspace để xóa.",
        None,
    );
    assert_eq!(dialog.heading().as_deref(), Some("Phím tắt đổi chế độ gõ"));
    assert_eq!(dialog.close_response(), "cancel");
    assert!(!note.is_visible());
}

/// The merged add-rule form: the focused app surfaces only as the entry's
/// placeholder hint plus the copy/apply pair — never over typed text — and
/// the hint alone is enough to enable Thêm.
fn rules_add_group_merges_the_focus_chip() {
    let ui = widgets();
    // Build time has no detection yet: copy/apply stay hidden, Thêm is
    // dead, and the placeholder is the fallback example.
    assert!(!ui.rule_copy_btn.is_visible());
    assert!(!ui.rule_apply_btn.is_visible());
    assert!(!ui.rule_add_btn.is_sensitive());
    assert_eq!(
        ui.rule_name_entry.placeholder_text().as_deref(),
        Some("vd: firefox, org.gnome.Console")
    );
    // A detected app lights the form up even with the field empty: the
    // hint alone can be added, and detection never writes into the field.
    *ui.rule_focus_value.borrow_mut() = "sublime_text".to_string();
    ui.update_rule_add_state();
    assert!(ui.rule_copy_btn.is_visible());
    assert!(ui.rule_apply_btn.is_visible());
    assert!(ui.rule_add_btn.is_sensitive());
    assert!(ui.rule_name_entry.text().is_empty());
    // A typed name rules over the hint and keeps Thêm enabled.
    ui.rule_name_entry.set_text("fire");
    assert!(ui.rule_add_btn.is_sensitive());
    // Block A's icon buttons share one 36px box; measure works even while
    // the pair is hidden (no detected app). The seen-apps chevron is built
    // with the same request on the page.
    for btn in [&ui.rule_copy_btn, &ui.rule_apply_btn] {
        let (min_w, _, _, _) = btn.measure(gtk4::Orientation::Horizontal, -1);
        let (min_h, _, _, _) = btn.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!((min_w, min_h), (36, 36), "icon buttons must share 36px");
    }
}

/// The rebuilt rule rows: the empty list renders one dim row, and a loaded
/// rule reads back through `read()` unchanged.
fn rule_rows_round_trip_through_read() {
    let ui = widgets();
    // Empty list: one plain row, not activatable, carrying the notice.
    ui.fill_rules(&[]);
    let empty = ui.rules_box.row_at_index(0).expect("empty-state row");
    assert!(!empty.is_activatable());
    assert!(empty.downcast_ref::<adw::ActionRow>().is_none());

    // One rule: the row round-trips through read().
    ui.fill_rules(&[crate::config::AppRule {
        pattern: "kate".into(),
        mode: ModeConfig::Preedit,
    }]);
    assert_eq!(
        ui.read().app_rules,
        vec![crate::config::AppRule {
            pattern: "kate".into(),
            mode: ModeConfig::Preedit,
        }]
    );
    let row = ui
        .rules_box
        .row_at_index(0)
        .and_downcast::<adw::ActionRow>()
        .expect("rule row");
    // The row reads the app's own name; the stored pattern stays the class
    // the rule matches, which the assertion above already covers.
    assert_eq!(row.title().as_str(), "Kate");
    assert_eq!(row.tooltip_text().as_deref(), Some("kate"));
}

/// The extension row must name the real situation and offer only the action
/// that fixes it: a working extension has nothing left to press.
#[test]
fn the_extension_row_offers_one_action_per_state() {
    use crate::gui::{extension_row_for, ExtensionRequest, ExtensionState};
    let cases = [
        (ExtensionState::Missing, Some(ExtensionRequest::InstallNew)),
        (ExtensionState::Stale, Some(ExtensionRequest::Update)),
        (ExtensionState::Disabled, Some(ExtensionRequest::Enable)),
        (ExtensionState::NeedsLogout, Some(ExtensionRequest::Logout)),
        (ExtensionState::Running, None),
    ];
    let mut subtitles = Vec::new();
    for (state, action) in cases {
        let text = extension_row_for(state);
        assert_eq!(text.button.map(|(_, action)| action), action, "{state:?}");
        assert!(!text.subtitle.1.is_empty());
        assert!(
            !subtitles.contains(&text.subtitle.1),
            "{state:?} reuses one"
        );
        subtitles.push(text.subtitle.1);
    }
    // A missing extension is installed, never "turned on" or "logged out of".
    let label = extension_row_for(ExtensionState::Missing).button;
    assert_eq!(label.map(|((_, text), _)| text), Some("Cài đặt"));

    // Every request has its own confirmation, so an update never says
    // "install" and a removal says what it deletes.
    let mut headings = Vec::new();
    for request in [
        ExtensionRequest::InstallNew,
        ExtensionRequest::Update,
        ExtensionRequest::Enable,
        ExtensionRequest::Disable,
        ExtensionRequest::Remove,
        ExtensionRequest::Logout,
    ] {
        let (heading, body, apply) = crate::gui::pages::other::extension_dialog(request);
        assert!(!heading.is_empty() && !body.is_empty() && !apply.is_empty());
        assert!(!headings.contains(&heading), "{request:?} reuses a heading");
        headings.push(heading);
    }
    // A running extension asks nothing to fix; it offers its two undo
    // buttons instead.
    let running = extension_row_for(ExtensionState::Running);
    assert!(running.button.is_none());
    assert_eq!(
        crate::gui::RUNNING_BUTTONS.map(|(_, request)| request),
        [ExtensionRequest::Disable, ExtensionRequest::Remove]
    );
}

/// "Chế độ gỡ lỗi" is one switch: the two log tiers come from its dialog,
/// stay saved while it is off, and the row says what it is recording.
fn debug_mode_switch_keeps_the_dialog_choice() {
    use crate::gui::content::{debug_choice, wire_debug_mode};
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    ui.refresh(&Config::default(), &macros, &dictionary);
    assert!(!ui.debug_log.is_active(), "off by default");
    assert_eq!(*ui.debug_tiers.borrow(), (false, false));
    *ui.save_cb.borrow_mut() = Some(Rc::new({
        let ui = ui.clone();
        move || ui.save_all().expect("debug save")
    }));
    wire_debug_mode(&ui);

    // The dialog's verdict lands in the three config keys and the row says
    // what it records.
    ui.debug_log.set_active(true); // no parent window: the saved choice applies
    debug_choice(&ui, true, true);
    assert_eq!(*ui.debug_tiers.borrow(), (true, true));
    assert!(
        ui.debug_log
            .subtitle()
            .is_some_and(|s| s.contains("nội dung đang gõ + từng phím")),
        "the row reports the tiers: {:?}",
        ui.debug_log.subtitle()
    );
    let saved = Config::load();
    assert!(saved.debug_log && saved.debug_log_text && saved.debug_log_keys);

    // Off is immediate and keeps the tiers for next time.
    ui.debug_log.set_active(false);
    let off = Config::load();
    assert!(!off.debug_log, "the switch is a kill switch");
    assert!(off.debug_log_text && off.debug_log_keys, "tiers remembered");
    assert_eq!(*ui.debug_tiers.borrow(), (true, true));
    assert!(ui
        .debug_log
        .subtitle()
        .is_some_and(|s| !s.contains("Đang bật")));

    // Reopening the window shows the switch off with the tiers still known;
    // turning it on re-applies them without asking again in this harness.
    ui.refresh(&off, &macros, &dictionary);
    assert_eq!(*ui.debug_tiers.borrow(), (true, true));
    ui.debug_log.set_active(true);
    let back_on = Config::load();
    assert!(back_on.debug_log && back_on.debug_log_text && back_on.debug_log_keys);
}

/// Group headers stay one line: the description moves into the title's
/// tooltip instead of sitting under it.
fn group_descriptions_become_title_tooltips() {
    let group = adw::PreferencesGroup::builder()
        .title("Dấu thanh")
        .description("Kiểu đặt dấu trên nguyên âm")
        .build();
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.append(&group);
    crate::gui::build::descriptions_into_tooltips(&page);
    assert!(
        group.description().is_none_or(|d| d.is_empty()),
        "the line under the title is gone: {:?}",
        group.description()
    );
    let title = crate::gui::build::find_label(group.upcast_ref(), "Dấu thanh")
        .expect("the title label stays");
    assert_eq!(
        title.tooltip_text().as_deref(),
        Some("Kiểu đặt dấu trên nguyên âm")
    );

    // Titles are Pango markup: the escaped "&amp;" must still match the label
    // showing "&", or that one group keeps its line (the "Sao lưu" bug).
    let marked = adw::PreferencesGroup::builder()
        .title("Sao lưu &amp; khôi phục")
        .description("Một tệp JSON chứa cấu hình, gõ tắt và từ điển.")
        .build();
    page.append(&marked);
    crate::gui::build::descriptions_into_tooltips(&page);
    assert!(
        marked.description().is_none_or(|d| d.is_empty()),
        "an escaped title must not keep its description"
    );
    let marked_title =
        crate::gui::build::find_label(marked.upcast_ref(), "Sao lưu &amp; khôi phục")
            .expect("the escaped title label is found");
    assert_eq!(
        marked_title.tooltip_text().as_deref(),
        Some("Một tệp JSON chứa cấu hình, gõ tắt và từ điển.")
    );
}

/// The experimental badge is an amber "[Thử nghiệm]" tag in front of the title.
fn experimental_badge_sits_beside_the_title() {
    let ui = widgets();
    widgets::decorate_experimental(&ui.suggest_words);
    let title = widgets::row_title(&ui.suggest_words).expect("the title label");
    let badge = title
        .prev_sibling()
        .and_downcast::<gtk4::Label>()
        .expect("the badge leads the title");
    assert_eq!(badge.text(), "[Thử nghiệm]");
    assert!(badge.has_css_class("warning"));
    assert_eq!(badge.parent(), title.parent(), "both share one line");
}

/// The seen-apps and add-item popover lists read from the left edge: the
/// stock button centers its label.
fn popover_list_buttons_read_from_the_left() {
    let btn = widgets::list_button("firefox");
    let label = btn.child().and_downcast::<gtk4::Label>().expect("a label");
    assert_eq!(label.xalign(), 0.0);
    assert_eq!(label.halign(), gtk4::Align::Start);
    assert!(btn.has_css_class("flat"));
}

/// Both row popovers (the panel menu's ⋮ and the rules' ⋮) build their
/// items through `list_button`: stock buttons centre the label, which made
/// the popover read as a column of loose words.
fn row_popover_items_read_from_the_left() {
    let ui = widgets();
    *ui.menu.borrow_mut() = Config::default().default_menu();
    let menu_more = ui.menu_more_button(vec![0], "root");
    let rule_more = widgets::list_button("Kiểm tra");
    assert_eq!(
        rule_more
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("a label")
            .xalign(),
        0.0
    );
    let labels = descendants::<gtk4::Label>(menu_more.upcast_ref());
    assert!(!labels.is_empty(), "the row menu has items");
    for label in labels {
        assert_eq!(label.xalign(), 0.0, "{} centres its text", label.text());
    }
}

/// B3 regression: the import path pushes a config into every widget through
/// `refresh`, so a widget `refresh` forgets keeps its old value and the next
/// save writes that stale value back over the imported settings.
fn refresh_round_trips_every_widget() {
    let ui = widgets();
    let mut macros = Macros::load();
    macros.set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    let mut dictionary = Dictionary::load();
    dictionary.set_words(["asl".to_string(), "tr".to_string()].into_iter());

    // `read()` starts from the file, and two fields pass through it
    // untouched now: `quick_menu` (its checkbox list was replaced by the
    // menu editor; the list is only the design default) and anything
    // else without a widget.
    let baseline = ui.read();
    let config = Config {
        debug_log_paused: false,
        debug_hide_text: true,
        enabled: false,
        method: MethodConfig::Vni,
        mode: ModeConfig::ForwardBackspace,
        charset: CharsetConfig::Tcvn3,
        modern_tone_style: false,
        free_tone_marking: true,
        auto_correct: false,
        extended_finals: true,
        extended_onsets: true,
        keep_composed_words: true,
        keep_repeats_raw: true,
        suggest_words: true,
        dodge_inline_autocomplete: true,
        autocomplete_fix: crate::config::AutocompleteFix::Raw,
        preferred_backend: crate::config::PreferredBackendConfig::WlrVirtualKeyboard,
        auto_restore: false,
        bypass_url: false,
        w2u: W2uConfig::NonStart,
        backspace_tone: BackspaceToneConfig::ToneFirst,
        bracket: BracketConfig::Everywhere,
        em_dash: true,
        auto_capitalize: true,
        double_space_period: true,
        macro_enabled: false,
        capitalize_macro: false,
        macro_in_off_mode: true,
        macro_expand_valid_words: true,
        time_format: "%H".into(),
        date_format: "%Y".into(),
        language: "vi".into(),
        dictionary_enabled: false,
        abbreviations: false,
        cycle_mode_hotkey: "Control+Alt+v".into(),
        enabled_hotkey: "Control+Alt+space".into(),
        app_rule_hotkey: String::new(),
        emoji_hotkey: "Control+Alt+e".into(),
        emoji_enabled: true,
        hide_underline: true,
        backspace_interval_ms: 7,
        post_delay_ms: 12,
        unicode_channel: crate::config::UnicodeChannel::Commit,
        echo_stale_enabled: true,
        echo_stale_ms: 250,
        idle_commit_enabled: true,
        idle_commit_ms: 800,
        flow_per_key_delay_ms: 80,
        flow_backspace_restore_delay_ms: 40,
        experimental_features: true,
        show_toasts: false,
        debug_log: true,
        debug_log_text: true,
        debug_log_keys: true,
        x11_delete_without_report: true,
        preedit_for_xim: true,
        nav_key_while_preedit: "forward".into(),
        app_rules: vec![crate::config::AppRule {
            pattern: "kate".into(),
            mode: ModeConfig::Preedit,
        }],
        // One custom shortcut: the round-trip must carry it through
        // refresh/read like the menu design does.
        custom_hotkeys: vec![crate::config::CustomHotkey {
            spec: "Control+Shift".into(),
            action: "emoji".into(),
        }],
        // Passes through read() untouched (see baseline above).
        quick_menu: baseline.quick_menu,
        menu_modes: baseline.menu_modes,
        // The editor always produces a design, and refresh normalizes an
        // absent/empty one to the shipped shape; here a small tree that
        // must survive the store round-trip unchanged.
        menu: Some(vec![
            MenuNode::Toggle {
                key: "enabled".into(),
            },
            MenuNode::Folder {
                label: "Cá nhân".into(),
                children: vec![
                    MenuNode::Toggle {
                        key: "em_dash".into(),
                    },
                    MenuNode::Separator,
                    MenuNode::Action {
                        key: "settings".into(),
                    },
                ],
            },
            MenuNode::Radios {
                group: "display".into(),
                children: vec!["preedit".into(), "flow".into()],
            },
        ]),
    };
    ui.refresh(&config, &macros, &dictionary);

    // The mode combo must render the config's mode under the right label:
    // labels are looked up by mode value now, so a hidden mode leaves no
    // positional hole to drift into.
    assert_eq!(
        MODE_LABELS[0].0,
        ModeConfig::Preedit,
        "the table must lead with the first display mode"
    );
    assert!(
        !MODE_LABELS
            .iter()
            .any(|(_, vi, en)| matches!(*vi, "Tự động" | "Gõ mặc định")
                || matches!(*en, "Tự động" | "Gõ mặc định")),
        "removed modes must not linger in the table"
    );
    let all_modes = vec![
        ModeConfig::Preedit,
        ModeConfig::Surrounding,
        ModeConfig::Flow,
        ModeConfig::ForwardBackspace,
        ModeConfig::Off,
    ];
    for m in &all_modes {
        let i = mode_index(&all_modes, *m) as usize;
        assert!(i < all_modes.len(), "{m:?} index {i} outside the list");
        assert_eq!(
            mode_at(&all_modes, i as u32),
            *m,
            "index round-trip for {m:?}"
        );
    }
    // Flow sits between Surrounding and Forward Backspace in the fixed
    // table — by neighbors, so removing a row does not shift the claim.
    let flow = MODE_LABELS
        .iter()
        .position(|(m, ..)| *m == ModeConfig::Flow)
        .expect("Flow row");
    assert_eq!(MODE_LABELS[flow - 1].0, ModeConfig::Surrounding);
    assert_eq!(MODE_LABELS[flow + 1].0, ModeConfig::ForwardBackspace);
    assert_eq!(
        ui.mode.selected(),
        mode_index(&all_modes, ModeConfig::ForwardBackspace),
        "refresh must place the config's mode at the right list position"
    );

    assert_eq!(ui.read(), config);
    assert_eq!(
        ui.read().app_rules,
        vec![crate::config::AppRule {
            pattern: "kate".into(),
            mode: ModeConfig::Preedit,
        }]
    );
    assert_eq!(
        super::parse_macros(&super::text_of(&ui.macros_text)),
        vec![("gs".to_string(), "giờ sáng".to_string())]
    );
    // A set: the order it hands the text view back in is not the input order.
    let mut words = super::parse_dictionary(&super::text_of(&ui.dictionary_text));
    words.sort();
    assert_eq!(words, vec!["asl".to_string(), "tr".to_string()]);
}

/// The live-reload path pushes fresh file contents through the same change
/// handlers a user edit fires; none of them may reach the save funnel.
fn external_reload_refreshes_without_saving() {
    let ui = widgets();
    let calls = Rc::new(Cell::new(0u32));
    {
        let calls = calls.clone();
        *ui.save_cb.borrow_mut() = Some(Rc::new(move || calls.set(calls.get() + 1)));
    }
    ui.fire_save();
    assert_eq!(calls.get(), 1, "the funnel saves when not suppressed");
    ui.external_reload();
    assert_eq!(calls.get(), 1, "external_reload must not save");
    assert!(!ui.suppress_save.get(), "the flag must be cleared after");
}

/// Adw rows parse Pango markup in title/subtitle by default; the URL
/// rule's subtitle carries a bare `&`, which failed to parse and rendered
/// as nothing. The row builders switch markup off — assert one row per
/// builder, plus the subtitle data of the reported row.
fn row_texts_render_as_plain_text() {
    let ui = widgets();
    for markup_off in [
        !ui.bypass_url.uses_markup(),
        !ui.method.uses_markup(),
        !ui.echo_stale_row.uses_markup(),
    ] {
        assert!(markup_off, "row titles and subtitles must be plain text");
    }
    assert_eq!(
        ui.bypass_url.subtitle().as_deref(),
        Some("URL giữ nguyên tới dấu cách")
    );
}

/// The per-key display switch died (the per-key look is the only Flow
/// path) and both Backspace switches died (the retrack and the mark
/// restore are standard Flow behavior): both delay spins survive as one
/// always-active caption row, still wired to their config keys.
fn flow_delay_row_shows_captions() {
    let ui = widgets();
    assert!(
        !ui.flow_delay_row.uses_markup(),
        "row titles and subtitles must be plain text"
    );
    assert!(
        ui.flow_delay_row.is_sensitive()
            && ui.flow_per_key_spin.is_sensitive()
            && ui.flow_backspace_restore_spin.is_sensitive(),
        "the delay row has no switch left to lock it"
    );
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    {
        let ui_for_save = Rc::clone(&ui);
        *ui.save_cb.borrow_mut() = Some(Rc::new(move || {
            ui_for_save.save_all().expect("delay row save");
        }));
    }
    ui.refresh(
        &Config {
            flow_per_key_delay_ms: 80,
            flow_backspace_restore_delay_ms: 40,
            ..Config::default()
        },
        &macros,
        &dictionary,
    );
    assert_eq!(
        ui.flow_per_key_spin.value(),
        80.0,
        "refresh shows the file value"
    );
    assert_eq!(
        ui.flow_backspace_restore_spin.value(),
        40.0,
        "refresh shows the file value"
    );
    ui.flow_per_key_spin.set_value(200.0);
    ui.flow_backspace_restore_spin.set_value(250.0);
    ui.fire_save();
    let saved = Config::load();
    assert_eq!(
        saved.flow_per_key_delay_ms, 200,
        "the detached spin still saves the delay key"
    );
    assert_eq!(
        saved.flow_backspace_restore_delay_ms, 250,
        "the detached spin still saves the restore key"
    );
}

/// Every mode dropdown renders the five modes in `ModeConfig::ALL` order —
/// the same list the panel submenu, the rule pickers and the cycle walk.
fn mode_dropdowns_list_every_mode() {
    let ui = widgets();
    let config = Config::default();
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    ui.refresh(&config, &macros, &dictionary);
    assert_eq!(*ui.mode_choices.borrow(), ModeConfig::ALL.to_vec());
    assert_eq!(ui.mode_choices.borrow().len(), 5);
    assert_eq!(
        mode_at(&ui.mode_choices.borrow()[..], ui.mode.selected()),
        config.mode,
        "the combo shows the configured mode"
    );
}

/// The cycle hotkey's expander derives from the same visible list the
/// engine cycles: the count reads five and the sequence row joins the five
/// labels in shipped order — a mode added or removed shows up here too.
fn cycle_subtitle_lists_the_five_modes() {
    let ui = widgets();
    let page = crate::gui::pages::hotkeys::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let expander = descendants::<adw::ExpanderRow>(page.upcast_ref())
        .into_iter()
        .find(|row| row.title() == "Thứ tự khi đổi chế độ")
        .expect("the cycle-order expander");
    assert_eq!(
        expander.subtitle().as_str(),
        "5 chế độ, xoay vòng",
        "the count follows the five visible modes"
    );
    let sequence = descendants::<adw::ActionRow>(expander.upcast_ref())
        .into_iter()
        .find(|row| row.subtitle().is_some_and(|s| s.contains(" → ")))
        .expect("the sequence row");
    assert_eq!(
        sequence.subtitle().as_ref().map(|s| s.as_str()),
        Some("Gạch chân (preedit) → Surrounding text → Flow → Forward Backspace → OFF"),
        "the sequence joins the five labels in shipped order"
    );
}

/// The "Gỡ lỗi" tab is the one conditional page: its row is in the sidebar
/// only while debug mode is on, and the switch that turns it on lives on
/// another page, so the removal has to happen without a rebuild.
fn the_debug_tab_follows_the_switch() {
    let ui = widgets();
    let seen = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        *ui.debug_tab.borrow_mut() = Some(Rc::new(move |visible: bool| {
            seen.borrow_mut().push(visible)
        }));
    }
    crate::gui::content::wire_debug_tab(&ui);
    ui.debug_log.set_active(true);
    ui.debug_log.set_active(false);
    assert_eq!(
        *seen.borrow(),
        vec![true, false],
        "the row follows the switch"
    );
}

/// A window built while debug mode is on carries the row, one built while it
/// is off does not.
fn the_debug_tab_row_appears_only_with_debug_mode() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-debug-tab")
        .build();
    let nav = |w: &adw::ApplicationWindow, name: &str| {
        descendants::<gtk4::ListBoxRow>(w.upcast_ref())
            .into_iter()
            .find(|r| r.widget_name() == name)
    };
    Config::default().save().expect("config off");
    let win = crate::gui::content::build(&app, Some("typing"));
    assert!(
        nav(&win, "debugging").is_none(),
        "no debug tab while debug mode is off"
    );
    win.close();
    Config {
        debug_log: true,
        ..Config::default()
    }
    .save()
    .expect("config on");
    let win = crate::gui::content::build(&app, Some("debugging"));
    assert!(
        nav(&win, "debugging").is_some(),
        "the tab is there with debug mode on"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// Both split-view pages carry a header bar. A collapsed split view shows the
/// sidebar as the root page, so a sidebar without one loses the window controls
/// — the close button disappeared as soon as the window got narrow.
fn each_column_carries_its_own_header_bar() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-sidebar-header")
        .build();
    Config::default().save().expect("config on");
    let win = crate::gui::content::build(&app, Some("typing"));
    let split = descendants::<adw::NavigationSplitView>(win.upcast_ref())
        .into_iter()
        .next()
        .expect("the shell is a split view");
    // Both columns start at the top of the window: a bar on each, so neither
    // leaves an empty strip above its first row.
    for page in [split.sidebar(), split.content()] {
        let page = page.expect("both pages exist");
        assert_eq!(
            descendants::<adw::HeaderBar>(page.upcast_ref()).len(),
            1,
            "each column carries exactly one header bar"
        );
    }
    // The search button lives in the sidebar's bar, not in a row of its own.
    let sidebar_bar = descendants::<adw::HeaderBar>(split.sidebar().unwrap().upcast_ref())
        .into_iter()
        .next()
        .expect("the sidebar bar");
    assert!(
        descendants::<gtk4::Button>(sidebar_bar.upcast_ref())
            .into_iter()
            .any(|b| b.icon_name().is_some_and(|i| i.contains("search"))),
        "the search button sits in the sidebar's bar"
    );
    // The window controls live in the content column's bar.
    let content_bar = descendants::<adw::HeaderBar>(split.content().unwrap().upcast_ref())
        .into_iter()
        .next()
        .expect("the content bar");
    assert!(
        descendants::<gtk4::WindowControls>(content_bar.upcast_ref())
            .into_iter()
            .any(|c| c.side() == gtk4::PackType::End),
        "the window controls live in the content bar"
    );
    // The window holds the split view directly: nothing above either column.
    assert!(
        win.content()
            .and_downcast::<adw::NavigationSplitView>()
            .is_some(),
        "the split view is the window's own content"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// The window's own minimum width has to stay under what a small screen
/// allows: a content wider than its window also makes the header bar wider
/// than the window, and the close button at its right end falls outside.
/// Measured at 1030 px while the footer kept its four labels on one row.
fn the_window_can_shrink_to_fit_a_small_screen() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-window-min")
        .build();
    Config {
        debug_log: true,
        ..Config::default()
    }
    .save()
    .expect("config on");
    let win = crate::gui::content::build(&app, Some("debugging"));
    let split = descendants::<adw::NavigationSplitView>(win.upcast_ref())
        .into_iter()
        .next()
        .expect("the shell is a split view");
    let (min, _, _, _) = split.measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        min <= 850,
        "the window cannot shrink below {min} px, so a narrower screen clips its close button"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// The close button must survive a resized window: at 700 px the header bar
/// (window level, outside the split view) still holds the window controls and
/// the sidebar has collapsed. This is the case that used to push the controls
/// past the window edge, because each page carried its own header bar at the
/// page's minimum width.
fn the_window_controls_survive_a_narrow_window() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-narrow-window")
        .build();
    Config {
        debug_log: true,
        ..Config::default()
    }
    .save()
    .expect("config on");
    let win = crate::gui::content::build(&app, Some("debugging"));
    win.set_default_size(700, 700);
    win.present();
    let ctx = gtk4::glib::MainContext::default();
    for _ in 0..40 {
        while ctx.pending() {
            ctx.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Whichever pane is shown, one bar inside the window carries the controls.
    let end = descendants::<gtk4::WindowControls>(win.upcast_ref())
        .into_iter()
        .filter(|c| c.side() == gtk4::PackType::End)
        .find(|c| c.is_mapped() && c.allocation().x() >= 0 && c.width() > 0)
        .expect("a visible set of window controls");
    assert!(
        end.allocation().x() + end.width() <= win.width(),
        "the window controls end at {} px, past the window's {} px",
        end.allocation().x() + end.width(),
        win.width()
    );
    let split = descendants::<adw::NavigationSplitView>(win.upcast_ref())
        .into_iter()
        .next()
        .expect("the shell is a split view");
    assert!(
        split.is_collapsed(),
        "900 px is the collapse width, so 700 px is collapsed"
    );
    // The pages' own minimum (623 px) must not reach the window: a horizontal
    // scroller under the header bar keeps the bar at the window's width even
    // when the window is narrower than every page.
    let content_min = win
        .content()
        .map(|c| c.measure(gtk4::Orientation::Horizontal, -1).0)
        .unwrap_or(-1);
    assert!(
        content_min <= 420,
        "the window content asks for {content_min} px, so the window cannot be narrowed to its own minimum"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// The search button sits in the sidebar's header bar now, and pressing it has
/// to open the entry: the row it lives in starts hidden, and an entry inside a
/// hidden row is neither mapped nor focusable.
fn the_search_button_opens_the_entry() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-search-button")
        .build();
    Config::default().save().expect("config on");
    let win = crate::gui::content::build(&app, Some("typing"));
    win.set_default_size(1000, 700);
    win.present();
    let ctx = gtk4::glib::MainContext::default();
    let pump = |ticks: usize| {
        for _ in 0..ticks {
            while ctx.pending() {
                ctx.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    pump(60);
    let split = descendants::<adw::NavigationSplitView>(win.upcast_ref())
        .into_iter()
        .next()
        .expect("the shell is a split view");
    let sidebar = split.sidebar().expect("the sidebar page");
    let btn = descendants::<gtk4::Button>(sidebar.upcast_ref())
        .into_iter()
        .find(|b| {
            b.tooltip_text()
                .is_some_and(|t| t.contains("Tìm chức năng"))
        })
        .expect("the search button");
    let entry = descendants::<gtk4::SearchEntry>(sidebar.upcast_ref())
        .into_iter()
        .next()
        .expect("the search entry");
    assert!(!entry.is_visible(), "the entry starts hidden");
    btn.emit_clicked();
    pump(30);
    assert!(entry.is_visible(), "pressing the button opens the entry");
    assert!(
        entry.is_mapped(),
        "the entry is on screen, not just visible"
    );
    entry.set_text("luật");
    pump(40);
    let content = split.content().expect("the content page");
    assert!(
        descendants::<gtk4::Label>(content.upcast_ref())
            .into_iter()
            .any(|l| l.text().contains("Luật")),
        "typing in the entry lists matching rows over the page"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// Every switch on a page ends on the same vertical edge, including the one
/// whose row used to carry a second control in its suffix area (an expanded
/// suffix box packed its children from the left, leaving that switch 46 px
/// short of the others).
fn every_switch_on_a_page_lands_on_the_same_edge() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-switch-line")
        .build();
    Config::default().save().expect("config on");
    let win = crate::gui::content::build(&app, Some("experimental"));
    win.set_default_size(1180, 740);
    win.present();
    let ctx = gtk4::glib::MainContext::default();
    for _ in 0..60 {
        while ctx.pending() {
            ctx.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let right_edge = |row: &adw::ActionRow| -> Option<i32> {
        let sw = descendants::<gtk4::Switch>(row.upcast_ref())
            .into_iter()
            .next()?;
        let (mut x, _) = (0, 0);
        let mut cur = Some(sw.upcast_ref::<gtk4::Widget>().clone());
        while let Some(c) = cur {
            x += c.allocation().x();
            cur = c.parent();
        }
        Some(x + sw.width())
    };
    let rows: Vec<adw::ActionRow> = descendants::<adw::ActionRow>(win.upcast_ref());
    let pick = |needle: &str| {
        rows.iter()
            .find(|r| r.title().contains(needle))
            .and_then(right_edge)
    };
    let two_controls = pick("địa chỉ").expect("the address-bar row");
    let plain = pick("Giữ từ khi gõ thêm").expect("a plain switch row");
    assert_eq!(
        two_controls, plain,
        "the switch of a two-control row must end where a plain row's does"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// "Tạm dừng ghi log" sits beside "Ẩn nội dung gõ": both are about what the
/// copy produces, and they share one row.
fn the_two_debug_checkboxes_share_a_row() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-debug-footer")
        .build();
    Config {
        debug_log: true,
        ..Config::default()
    }
    .save()
    .expect("config on");
    let win = crate::gui::content::build(&app, Some("debugging"));
    // The language is process-global and an earlier scenario may have switched
    // it, so both spellings are matched.
    let checks: Vec<gtk4::CheckButton> = descendants::<gtk4::CheckButton>(win.upcast_ref())
        .into_iter()
        .filter(|c| {
            c.label().is_some_and(|l| {
                ["Ẩn", "Tạm", "Hide", "Pause"]
                    .iter()
                    .any(|p| l.starts_with(p))
            })
        })
        .collect();
    assert_eq!(checks.len(), 2, "both switches exist");
    let row = |c: &gtk4::CheckButton| c.parent().map(|p| p.as_ptr());
    assert_eq!(
        row(&checks[0]),
        row(&checks[1]),
        "the pause switch must sit on the same row as the masking switch"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// The window opens wide enough for the two columns: the stacked layout is
/// for a window the user has narrowed, not for the first look.
fn the_debug_page_opens_with_both_columns() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-open-width")
        .build();
    let saved = Config::load();
    Config {
        debug_log: true,
        ..saved
    }
    .save()
    .expect("config on");
    let win = crate::gui::content::build(&app, Some("debugging"));
    win.present();
    for _ in 0..300 {
        while gtk4::glib::MainContext::default().iteration(false) {}
        if win.width() > 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    while gtk4::glib::MainContext::default().iteration(false) {}

    let split_view = descendants::<adw::NavigationSplitView>(win.upcast_ref());
    assert!(
        split_view.iter().all(|view| !view.is_collapsed()),
        "the sidebar stays open at the default size"
    );
    // The page's own split is the box holding a separator between two halves.
    let split = descendants::<gtk4::Box>(win.upcast_ref())
        .into_iter()
        .find(|b| {
            let mut child = b.first_child();
            let mut separator = false;
            while let Some(w) = child {
                separator |= w.downcast_ref::<gtk4::Separator>().is_some();
                child = w.next_sibling();
            }
            separator && b.orientation() == Orientation::Horizontal
        })
        .expect("the two-column split");
    assert_eq!(
        split.orientation(),
        Orientation::Horizontal,
        "the log and the tools sit side by side on the first look"
    );
    win.close();
    Config::default().save().expect("config back off");
}

/// A window the user is looking at picks up what the engine writes: the pane
/// used to need a tab switch, because the writer rewrote the newline of the
/// line above and the reader never saw the bytes that moved behind it.
fn the_open_log_pane_picks_up_what_is_written() {
    let dir = crate::debug_log::TempLogDir::new("live-log");
    let path = dir.dir().join("debug.log");
    std::fs::write(&path, "1795619563000 ctx start uinput=1\n").expect("a log");
    let ui = widgets();
    // The page captures the log path when it is built; nothing else in this
    // binary shares the file afterwards.
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
    let overlay = pages::debug::build(&ui);
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = None;

    let win = gtk4::Window::builder()
        .default_width(1180)
        .default_height(740)
        .build();
    win.set_child(Some(&overlay));
    win.present();
    let pump = |ms: u64| {
        let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        while std::time::Instant::now() < end {
            while gtk4::glib::MainContext::default().iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    for _ in 0..300 {
        while gtk4::glib::MainContext::default().iteration(false) {}
        if win.width() > 1 && overlay.is_mapped() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    pump(700);

    // Append-only, the way the engine writes: two lines in one write, then a
    // repeat run and the line that ends it.
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("the log file");
        file.write_all(b"1795619563100 ctx step=7\n1795619563200 ctx step=7 rep=3\n")
            .expect("a write");
        // Enough to overflow the box, so following has to keep the newest
        // line in view.
        for i in 0..40 {
            file.write_all(format!("1795619563300 ctx fill={i}\n").as_bytes())
                .expect("a write");
        }
    }
    pump(900);
    // The log's own scroller keeps the newest line in view: no pause unless
    // the button says so.
    let log_scroller = descendants::<gtk4::ScrolledWindow>(overlay.upcast_ref())
        .into_iter()
        .find(|sc| {
            sc.child()
                .is_some_and(|child| child.downcast_ref::<gtk4::TextView>().is_some())
        })
        .expect("the log's scroller");
    let adj = log_scroller.vadjustment();
    assert!(
        adj.value() >= adj.upper() - adj.page_size() - 2.0,
        "the newest line is the one in view: value={} upper={} page={}",
        adj.value(),
        adj.upper(),
        adj.page_size()
    );
    let pause = descendants::<gtk4::ToggleButton>(overlay.upcast_ref())
        .into_iter()
        .find(|b| {
            b.icon_name()
                .is_some_and(|i| i.contains("pause") || i.contains("play"))
        })
        .expect("the pause button");
    assert!(
        pause.is_active(),
        "scrolling never pauses following on its own"
    );
    let view = descendants::<gtk4::TextView>(overlay.upcast_ref());
    let text = view
        .first()
        .map(|v| {
            let buffer = v.buffer();
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string()
        })
        .unwrap_or_default();
    assert!(text.contains("ctx step=7"), "the line arrived: {text}");
    assert!(text.contains("ctx step=7 rep=3"), "its count too: {text}");
    assert!(
        text.lines().all(|line| line.contains("ctx ")),
        "no fragment lines: {text}"
    );
    win.close();
}

fn the_debug_page_shows_the_status_the_rows_and_the_footer() {
    let ui = widgets();
    let overlay = pages::debug::build(&ui);
    // Two system rows hide (Engine and the MăngRS version travel only in the
    // report), leaving six there, three for the insert path and two files.
    let rows = descendants::<adw::ActionRow>(overlay.upcast_ref());
    assert_eq!(rows.len(), 11, "the diagnostics rows");
    assert_eq!(
        descendants::<adw::PreferencesGroup>(overlay.upcast_ref()).len(),
        4,
        "a group per section, plus the typing test"
    );
    let dots = descendants::<gtk4::Label>(overlay.upcast_ref())
        .iter()
        .filter(|l| l.text() == "●")
        .count();
    assert_eq!(dots, 12, "a dot per row, plus the status bar's");
    let picks = descendants::<gtk4::DropDown>(overlay.upcast_ref()).len();
    assert_eq!(picks, 1, "the type list");
    let labels: Vec<String> = descendants::<gtk4::Label>(overlay.upcast_ref())
        .iter()
        .map(|l| l.text().to_string())
        .collect();
    for wanted in ["Hệ thống", "Cách chèn", "Tệp", "Thử gõ"] {
        assert!(
            labels.iter().any(|l| l == wanted),
            "the page has a `{wanted}` group: {labels:?}"
        );
    }
    assert!(
        labels
            .iter()
            .any(|l| l.starts_with("Engine đang chạy") || l.starts_with("Engine không chạy")),
        "the status bar names the engine: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.starts_with("Trễ TB")),
        "the status bar carries the latency: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "Khởi động lại IBus"),
        "the footer carries the restart button"
    );
    assert!(
        labels.iter().any(|l| l.contains("Sao chép")),
        "the footer carries the report button: {labels:?}"
    );
}

/// Every switch wired by `wire_switch_saves` must fire the save funnel
/// when toggled — a row missing from that list edits on screen only and
/// never reaches `config.toml` (the "Ẩn gạch chân" switch). Each row
/// flips away from its current state, so the notify always fires.
fn every_switch_triggers_save() {
    let ui = widgets();
    let saves = Rc::new(Cell::new(0u32));
    let counter = saves.clone();
    wire_switch_saves(&ui, Rc::new(move || counter.set(counter.get() + 1)));
    let rows: [&adw::SwitchRow; 22] = [
        &ui.enabled,
        &ui.modern,
        &ui.free,
        &ui.auto_correct,
        &ui.auto_restore,
        &ui.abbrev,
        &ui.bypass_url,
        &ui.em_dash,
        &ui.auto_capitalize,
        &ui.experimental_features,
        &ui.show_toasts,
        &ui.double_space,
        &ui.macro_enabled,
        &ui.capitalize_macro,
        &ui.macro_in_off_mode,
        &ui.macro_valid_words,
        &ui.dodge_switch,
        &ui.emoji_enabled,
        &ui.dictionary_enabled,
        &ui.hide_underline,
        // `debug_log` has its own confirmation dialog and is covered by
        // `debug_mode_switch_keeps_the_dialog_choice` instead.
        &ui.x11_delete,
        &ui.preedit_xim,
    ];
    for row in rows {
        let before = saves.get();
        row.set_active(!row.is_active());
        assert!(saves.get() > before, "a switch toggled without saving");
    }
    // The merged "Thử nghiệm" rows carry bare GtkSwitch suffixes — same
    // wiring, different widget type.
    let switches: [&gtk4::Switch; 2] = [&ui.echo_stale_switch, &ui.idle_commit_switch];
    for switch in switches {
        let before = saves.get();
        switch.set_active(!switch.is_active());
        assert!(saves.get() > before, "a switch toggled without saving");
    }
    // The reported bug end to end: the toggled state survives read(), so
    // the next save writes it and refresh restores it after a reopen.
    ui.hide_underline.set_active(true);
    assert!(
        ui.read().hide_underline,
        "read() must see the toggled switch"
    );
}

/// The tab's one CSS rule must parse: an unknown colour name drops the whole
/// rule and every switch silently stays the theme's blue.
fn experimental_css_parses() {
    let provider = gtk4::CssProvider::new();
    let errors = Rc::new(std::cell::RefCell::new(Vec::new()));
    {
        let errors = Rc::clone(&errors);
        provider.connect_parsing_error(move |_, _, err| errors.borrow_mut().push(err.to_string()));
    }
    provider.load_from_data(pages::experimental::EXPERIMENTAL_CSS);
    let errors = errors.borrow();
    assert!(errors.is_empty(), "{errors:?}");
}

/// The experimental master gate lives inside the "Thử nghiệm" tab: off
/// locks every experimental row and displays the shipped defaults, while
/// the save funnel keeps the file's values and reopening restores them.
/// widgets() has no page wiring, so the gate's presentation runs through
/// the same methods its handler calls, and the file round trip runs
/// through a real save_cb.
fn experimental_gate_locks_rows_and_preserves_values() {
    let ui = widgets();
    // The tab's amber switch trough is scoped by a page class; a class on
    // the row never reached the switch the merged rows wrap.
    let page = pages::experimental::build(&ui);
    assert!(page.has_css_class("experimental-page"));
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    {
        let ui_for_save = Rc::clone(&ui);
        *ui.save_cb.borrow_mut() = Some(Rc::new(move || {
            ui_for_save.save_all().expect("scenario save");
        }));
    }

    // Seed the sandbox file: the gate on, distinctive saved values.
    let saved = Config {
        experimental_features: true,
        echo_stale_enabled: true,
        echo_stale_ms: 250,
        flow_per_key_delay_ms: 200,
        ..Config::default()
    };
    saved.save().expect("seed config");
    let on = Config::load();
    ui.refresh(&on, &macros, &dictionary);
    assert!(ui.macro_valid_words.is_sensitive());
    assert!(ui.suggest_words.is_sensitive());
    // The merged row locks as one unit with the gate.
    assert!(ui.echo_stale_row.is_sensitive());
    assert_eq!(ui.echo_stale_spin.value(), 250.0);

    // Off (driven through the file, as the real funnel produces it): rows
    // locked, shipped defaults on display.
    let mut off = on.clone();
    off.experimental_features = false;
    off.save().expect("seed off");
    let off = Config::load();
    ui.refresh(&off, &macros, &dictionary);
    assert!(!ui.macro_valid_words.is_sensitive());
    assert!(!ui.suggest_words.is_sensitive());
    assert!(!ui.echo_stale_row.is_sensitive());
    assert_eq!(ui.echo_stale_spin.value(), 150.0);

    // A save with the gate off — the funnel reads the default-displaying
    // widgets — must not wipe the file's values.
    ui.fire_save();
    let closed = Config::load();
    assert!(!closed.experimental_features);
    assert_eq!(closed.echo_stale_ms, 250, "no wipe on save");
    assert_eq!(closed.flow_per_key_delay_ms, 200);

    // Reopen (what the real toggle does: flip the switch, then the
    // presentation runs in the handler's order — restore first, sensitivity
    // second, so the twin corrections see the restored switch states).
    ui.experimental_features.set_active(true);
    ui.restore_experimental_saved();
    ui.set_experimental_sensitive(true);
    assert!(ui.echo_stale_row.is_sensitive());
    assert_eq!(
        ui.echo_stale_spin.value(),
        250.0,
        "reopening restores the saved value"
    );

    // Close again: locked and defaults on display.
    ui.experimental_features.set_active(false);
    ui.set_experimental_sensitive(false);
    ui.show_experimental_defaults();
    assert!(!ui.macro_valid_words.is_sensitive());
    // Hand the sandbox file to the later scenarios the way they expect it:
    // with the gate off, read() reports the file's experimental fields, so
    // a leftover saved value would trip their refresh asserts.
    Config::default().save().expect("reset sandbox config");
}

/// The add-shortcut picker offers an experimental switch only while the
/// gate is open and that switch is on: the engine ignores the others, so a
/// shortcut for them would do nothing.
fn hotkey_action_choices_follow_the_experimental_switches() {
    let ui = widgets();
    let actions = |ui: &Ui| -> Vec<&'static str> {
        pages::hotkeys::action_choices(ui)
            .into_iter()
            .map(|(action, _)| action)
            .collect()
    };
    let base = Config::default();
    let (macros, dictionary) = (Macros::load(), Dictionary::load());

    // Gate closed: the fixed list only.
    ui.refresh(&base, &macros, &dictionary);
    let closed = actions(&ui);
    assert_eq!(closed.len(), crate::config::CUSTOM_HOTKEY_ACTIONS.len());
    assert!(!closed.contains(&"suggest_words"));

    // Gate open, every experimental switch off: still nothing to offer.
    let mut gate_only = base.clone();
    gate_only.experimental_features = true;
    ui.refresh(&gate_only, &macros, &dictionary);
    assert_eq!(
        actions(&ui).len(),
        crate::config::CUSTOM_HOTKEY_ACTIONS.len()
    );

    // Two switches on: exactly those two join the list, in table order.
    let mut two = gate_only.clone();
    two.suggest_words = true;
    two.keep_repeats_raw = true;
    ui.refresh(&two, &macros, &dictionary);
    let listed = actions(&ui);
    assert_eq!(listed.len(), crate::config::CUSTOM_HOTKEY_ACTIONS.len() + 2);
    assert_eq!(
        &listed[crate::config::CUSTOM_HOTKEY_ACTIONS.len()..],
        &["suggest_words", "keep_repeats_raw"]
    );
    // A switch turned off after a shortcut exists drops back out.
    ui.suggest_words.set_active(false);
    assert!(!actions(&ui).contains(&"suggest_words"));
}

/// Closing the experimental gate keeps every experimental value the user
/// set: `read()` restores the file's values for the fields the gate locks,
/// and the suggestion and word-keeping switches were missing from that
/// list, so a single save with the gate off overwrote them with defaults.
fn experimental_gate_toggle_keeps_the_saved_values() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    {
        let ui_for_save = Rc::clone(&ui);
        *ui.save_cb.borrow_mut() = Some(Rc::new(move || {
            ui_for_save.save_all().expect("scenario save");
        }));
    }
    let saved = Config {
        experimental_features: true,
        suggest_words: true,
        keep_composed_words: true,
        keep_repeats_raw: true,
        echo_stale_enabled: true,
        echo_stale_ms: 250,
        ..Config::default()
    };
    saved.save().expect("seed config");
    let on = Config::load();
    ui.refresh(&on, &macros, &dictionary);
    assert!(ui.suggest_words.is_active());

    // Gate off: the rows display the shipped defaults and save on every
    // swap, so one save here used to wipe the file.
    ui.experimental_features.set_active(false);
    ui.show_experimental_defaults();
    ui.fire_save();
    let closed = Config::load();
    assert!(!closed.experimental_features);
    assert!(closed.suggest_words, "off must not wipe suggest_words");
    assert!(
        closed.keep_composed_words,
        "off must not wipe keep_composed"
    );
    assert!(closed.keep_repeats_raw, "off must not wipe keep_repeats");
    assert_eq!(closed.echo_stale_ms, 250, "off must not wipe the ms");

    // Gate on again: the saved values come back on the rows and in the file.
    ui.restore_experimental_saved();
    ui.set_experimental_sensitive(true);
    ui.fire_save();
    assert!(ui.suggest_words.is_active(), "reopen restores the switch");
    assert_eq!(ui.echo_stale_spin.value(), 250.0, "reopen restores the ms");
    let reopened = Config::load();
    assert!(reopened.suggest_words && reopened.keep_composed_words && reopened.keep_repeats_raw);
    assert_eq!(reopened.echo_stale_ms, 250);
    Config::default().save().expect("reset sandbox config");
}

/// The preferred-backend combo round-trips through read()/refresh and joins
/// the gate's preserve/restore machinery: off locks the row and shows the
/// auto default, and a save with the gate off must not wipe the file's
/// choice. Item order: auto, uinput, xtest, wlr-virtual-keyboard, libei.
fn preferred_backend_combo_saves_loads_and_survives_the_gate() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    {
        let ui_for_save = Rc::clone(&ui);
        *ui.save_cb.borrow_mut() = Some(Rc::new(move || {
            ui_for_save.save_all().expect("scenario save");
        }));
    }

    // Seed the sandbox file: the gate on, libei chosen.
    let saved = Config {
        experimental_features: true,
        preferred_backend: crate::config::PreferredBackendConfig::Libei,
        ..Config::default()
    };
    saved.save().expect("seed config");
    let on = Config::load();
    ui.refresh(&on, &macros, &dictionary);
    assert!(ui.preferred_backend.is_sensitive());
    assert_eq!(ui.preferred_backend.selected(), 4, "libei loads at index 4");

    // Widget to config: the selected item maps back to the same variant.
    ui.preferred_backend.set_selected(1);
    assert_eq!(
        ui.read().preferred_backend,
        crate::config::PreferredBackendConfig::Uinput
    );
    ui.preferred_backend.set_selected(2);
    assert_eq!(
        ui.read().preferred_backend,
        crate::config::PreferredBackendConfig::XTest
    );
    ui.preferred_backend.set_selected(4);

    // Gate off: locked, auto on display, save preserves the file's value.
    let mut off = on.clone();
    off.experimental_features = false;
    off.save().expect("seed off");
    let off = Config::load();
    ui.refresh(&off, &macros, &dictionary);
    assert!(!ui.preferred_backend.is_sensitive());
    assert_eq!(ui.preferred_backend.selected(), 0, "auto on display");
    ui.fire_save();
    let closed = Config::load();
    assert_eq!(
        closed.preferred_backend,
        crate::config::PreferredBackendConfig::Libei,
        "no wipe on save with the gate off"
    );

    // Reopen restores the saved choice; closing shows the default again.
    ui.experimental_features.set_active(true);
    ui.restore_experimental_saved();
    ui.set_experimental_sensitive(true);
    assert!(ui.preferred_backend.is_sensitive());
    assert_eq!(
        ui.preferred_backend.selected(),
        4,
        "reopening restores libei"
    );
    ui.experimental_features.set_active(false);
    ui.set_experimental_sensitive(false);
    ui.show_experimental_defaults();
    assert_eq!(ui.preferred_backend.selected(), 0, "auto on display");
    Config::default().save().expect("reset sandbox config");
}

/// The merged $TIME/$DATE row: a preset selects its preset index, a format
/// the old free-text rows saved comes back verbatim as its own appended
/// item (never rewritten to a preset), re-refreshing does not stack
/// duplicates, and the subtitle shows both formats rendered live.
fn format_combos_keep_presets_and_unknowns() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let config = Config {
        time_format: TIME_FORMATS[1].into(),
        date_format: DATE_FORMATS[2].into(),
        ..Config::default()
    };
    ui.refresh(&config, &macros, &dictionary);
    assert_eq!(ui.time_format.selected(), 1, "preset selects its index");
    assert_eq!(ui.date_format.selected(), 2);
    assert_eq!(ui.read().time_format, TIME_FORMATS[1]);
    assert_eq!(ui.read().date_format, DATE_FORMATS[2]);

    // One row holds both dropdowns and one live line: time then date.
    let page = crate::gui::pages::macros::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    let subtitle = ui.formats_row.subtitle().unwrap_or_default();
    let parts: Vec<&str> = subtitle.split(" · ").collect();
    assert_eq!(parts.len(), 2, "both formats render: {subtitle}");
    // Read the shape, not the clock: the exact time can tick over between
    // the refresh and this line.
    assert!(parts.iter().all(|part| !part.is_empty()), "{subtitle}");
    assert!(parts[0].contains(':'), "the time sits first: {subtitle}");
    assert_eq!(
        parts[1].split('.').count(),
        3,
        "the date follows in the picked %d.%m.%Y shape: {subtitle}"
    );

    let mut unknown = config.clone();
    unknown.time_format = "%Hh%M-%S".into();
    unknown.date_format = "%Y-%m".into();
    ui.refresh(&unknown, &macros, &dictionary);
    assert_eq!(ui.read().time_format, "%Hh%M-%S", "unknown stays saved");
    assert_eq!(ui.read().date_format, "%Y-%m");
    assert!(
        ui.time_format.selected() >= TIME_FORMATS.len() as u32,
        "the unknown sits beyond the presets"
    );

    // The appended items are temporary: a second refresh rebuilds the list
    // from the presets, so the unknown never stacks twice.
    let items = ui.time_format.model().map(|m| m.n_items());
    ui.refresh(&unknown, &macros, &dictionary);
    assert_eq!(
        ui.time_format.model().map(|m| m.n_items()),
        items,
        "no duplicate items after a re-refresh"
    );
}

/// The Language combo: the config's language selects the right item and
/// the pick reads back as "vi"/"en".
fn language_row_round_trips() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let config = Config {
        language: "en".into(),
        ..Config::default()
    };
    ui.refresh(&config, &macros, &dictionary);
    assert_eq!(ui.language.selected(), 1, "English selects index 1");
    assert_eq!(ui.read().language, "en");
    ui.refresh(&Config::default(), &macros, &dictionary);
    assert_eq!(
        ui.language.selected(),
        0,
        "Vietnamese (default) selects index 0"
    );
    assert_eq!(ui.read().language, "vi");
    // A hand-edited value behaves as Vietnamese, and the refresh round
    // trip must not trip the debug assert (read() normalizes to "vi").
    let config = Config {
        language: "fr".into(),
        ..Config::default()
    };
    ui.refresh(&config, &macros, &dictionary);
    assert_eq!(ui.language.selected(), 0, "an unknown value selects vi");
    assert_eq!(ui.read().language, "vi", "an unknown value behaves as vi");
}

/// The nav-key combo: the config value selects the item and the pick reads
/// back as the config value — a normal row now, no gate needed.
fn nav_key_row_round_trips() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    ui.refresh(
        &Config {
            nav_key_while_preedit: "swallow".into(),
            ..Config::default()
        },
        &macros,
        &dictionary,
    );
    assert_eq!(
        ui.nav_key_while_preedit.selected(),
        1,
        "swallow selects index 1"
    );
    assert_eq!(ui.read().nav_key_while_preedit, "swallow");
    ui.refresh(&Config::default(), &macros, &dictionary);
    assert_eq!(
        ui.nav_key_while_preedit.selected(),
        0,
        "the default selects index 0"
    );
    assert_eq!(ui.read().nav_key_while_preedit, "forward");
    ui.refresh(
        &Config {
            nav_key_while_preedit: "replay".into(),
            ..Config::default()
        },
        &macros,
        &dictionary,
    );
    assert_eq!(
        ui.nav_key_while_preedit.selected(),
        2,
        "replay selects index 2"
    );
    assert_eq!(ui.read().nav_key_while_preedit, "replay");
    // A hand-edited value behaves as the default.
    ui.refresh(
        &Config {
            nav_key_while_preedit: "weird".into(),
            ..Config::default()
        },
        &macros,
        &dictionary,
    );
    assert_eq!(
        ui.nav_key_while_preedit.selected(),
        0,
        "an unknown value selects index 0"
    );
    assert_eq!(
        ui.read().nav_key_while_preedit,
        "forward",
        "an unknown value behaves as forward"
    );
}

/// The rebuild path behind the language pick: building the window again
/// after set_language re-resolves every text in the new language and lands
/// on the requested page. Runs LAST (global language + windows cleaned up
/// here). The row handler itself saves the user's config, so it is never
/// driven in tests — the owner verifies the live switch. The two
/// Gtk-CRITICALs about windows created outside GApplication::startup are
/// expected noise from building windows without the app having run.
fn language_switch_rebuilds_the_window() {
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-lang")
        .build();
    let nav_tip = |w: &adw::ApplicationWindow, name: &str| {
        descendants::<gtk4::ListBoxRow>(w.upcast_ref())
            .into_iter()
            .find(|r| r.widget_name() == name)
            .expect("nav row")
            .tooltip_text()
            .map(|s| s.to_string())
            .expect("tooltip")
    };
    let win1 = crate::gui::content::build(&app, Some("typing"));
    assert_eq!(nav_tip(&win1, "other"), "Cài đặt nâng cao", "vi by default");
    let picked = |w: &adw::ApplicationWindow, name: &str| {
        descendants::<gtk4::ListBoxRow>(w.upcast_ref())
            .into_iter()
            .find(|r| r.widget_name() == name)
            .expect("nav row")
            .is_selected()
    };
    assert!(picked(&win1, "typing"), "the requested page is selected");
    // Rebuild in English: texts re-resolve, the requested page is honored.
    set_language("en");
    let win2 = crate::gui::content::build(&app, Some("other"));
    assert_eq!(
        nav_tip(&win2, "other"),
        "Advanced settings",
        "rebuild re-resolves texts"
    );
    assert!(
        picked(&win2, "other"),
        "the rebuild lands on the viewed page"
    );
    // Cleanup: the global language is process state, restore it.
    win1.close();
    win2.close();
    set_language("vi");
}

/// The live switch through the REAL row handler, both directions — the
/// owner's flow (config sandboxed, so the funnel's writes are safe). After
/// each pick the old window must be closed AND exactly one new window must
/// exist, in the picked language: the EN→VI pick once closed the window
/// without rebuilding (owner report).
fn language_handler_rebuilds_both_ways() {
    let pump = || {
        for _ in 0..60 {
            gtk4::glib::MainContext::default().iteration(false);
        }
    };
    let app_windows = || {
        gtk4::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<adw::ApplicationWindow>().ok())
            .collect::<Vec<_>>()
    };
    let nav_tip = |w: &adw::ApplicationWindow, name: &str| {
        descendants::<gtk4::ListBoxRow>(w.upcast_ref())
            .into_iter()
            .find(|r| r.widget_name() == name)
            .expect("nav row")
            .tooltip_text()
            .map(|s| s.to_string())
            .expect("tooltip")
    };
    let language_combo = |w: &adw::ApplicationWindow| {
        descendants::<adw::ComboRow>(w.upcast_ref())
            .into_iter()
            .find(|r| {
                r.model()
                    .and_downcast::<gtk4::StringList>()
                    .and_then(|m| m.string(0))
                    .map(|first| first == "Tiếng Việt")
                    .unwrap_or(false)
            })
            .expect("the language combo")
    };

    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.test-lang")
        .build();
    let win1 = crate::gui::content::build(&app, Some("typing"));
    assert_eq!(nav_tip(&win1, "other"), "Cài đặt nâng cao");

    // VI → EN through the real handler.
    language_combo(&win1).set_selected(1);
    pump();
    let en = app_windows();
    assert_eq!(en.len(), 1, "exactly one window after VI→EN: {en:?}");
    let win2 = en.first().expect("the EN window");
    assert_eq!(nav_tip(win2, "other"), "Advanced settings", "VI→EN rebuilt");

    // EN → VI through the same handler.
    language_combo(win2).set_selected(0);
    pump();
    let vi = app_windows();
    assert_eq!(vi.len(), 1, "exactly one window after EN→VI: {vi:?}");
    let win3 = vi.first().expect("the VI window");
    assert_eq!(nav_tip(win3, "other"), "Cài đặt nâng cao", "EN→VI rebuilt");

    win3.close();
    close_test_windows();
    set_language("vi");
}

/// The settings save round runs on every toggle: files the round did not
/// change must not be rewritten (the window's own mtime watch would reload
/// for nothing), and config.toml's self-write stamp must be recorded for
/// that skip. A real change still writes and moves the stamp.
fn save_round_skips_unchanged_files_and_stamps_self_writes() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();
    ui.refresh(&Config::default(), &macros, &dictionary);
    {
        let ui_for_save = Rc::clone(&ui);
        *ui.save_cb.borrow_mut() = Some(Rc::new(move || {
            ui_for_save.save_all().expect("save");
        }));
    }
    ui.fire_save();
    assert!(
        ui.self_saved_mtime.get().is_some(),
        "a config write must record the self-save stamp"
    );
    let stamps = file_stamps();
    std::thread::sleep(std::time::Duration::from_millis(30));
    ui.fire_save();
    assert_eq!(
        file_stamps(),
        stamps,
        "an identical save round must not rewrite any file"
    );
    // A real change still writes, and the stamp follows the new write.
    // widgets() has no page wiring: the spin's own save hook lives in the
    // tune page build, so the funnel fires explicitly here.
    ui.flow_per_key_spin.set_value(123.0);
    ui.fire_save();
    let after_change = file_stamps();
    assert_ne!(
        after_change[0], stamps[0],
        "a changed config must be written"
    );
    assert!(ui.self_saved_mtime.get().is_some());
}

/// The mtimes of the four settings files, in config/macros/dictionary/emoji
/// order; a missing file stamps as None.
fn file_stamps() -> Vec<Option<std::time::SystemTime>> {
    [
        crate::config::Config::path(),
        crate::macros::Macros::path(),
        crate::macros::Dictionary::path(),
        crate::emoji::UserEmoji::path(),
    ]
    .into_iter()
    .map(|p| {
        p.and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
    })
    .collect()
}

/// The live-reload poll: a self-written mtime must not reload (the widgets
/// already show it), an external write and a deleted file must.
fn poll_reloads_skips_self_writes() {
    use std::time::{Duration, SystemTime};
    let base = SystemTime::now();
    let later = base + Duration::from_secs(1);
    assert!(
        !crate::gui::content::poll_reloads(Some(base), Some(base)),
        "our own save must not reload"
    );
    assert!(
        crate::gui::content::poll_reloads(Some(later), Some(base)),
        "an external write must reload"
    );
    assert!(
        crate::gui::content::poll_reloads(None, Some(base)),
        "a deleted config must reload"
    );
    assert!(
        crate::gui::content::poll_reloads(Some(base), None),
        "no stamp (a failed save) must reload"
    );
}

#[test]
fn a_save_only_rewrites_the_list_that_changed() {
    // The window compares the raw editor text with what it last wrote, so a
    // knob change never re-parses (let alone rewrites) the personal dictionary
    // (~7 ms to parse at 20k words).
    let last = "học\nsinh\n";
    assert_eq!(parse_dictionary(last).len(), 2);
    assert_eq!(
        parse_dictionary("học\n# cũ\n\nsinh\n"),
        parse_dictionary(last)
    );
    assert_ne!(
        parse_dictionary("học\nsinh\nviên\n"),
        parse_dictionary(last)
    );
    assert_eq!(
        parse_macros("gs = giờ sáng\n= hỏng\nxx =\n"),
        vec![("gs".to_string(), "giờ sáng".to_string())]
    );
}
