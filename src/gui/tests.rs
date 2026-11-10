use super::*;

#[test]
fn restart_script_respawns_the_xim_bridge_on_wayland() {
    // The Wayland restart must bring ibus-x11 back: nothing else respawns
    // the bridge (the distro hook runs once per login) and XIM apps lose
    // input until the next logout without it.
    let script = content::restart_script(Some("wayland"));
    assert!(script.starts_with("ibus restart"));
    assert!(
        script.contains("pgrep -x ibus-x11"),
        "skip the spawn when a bridge already runs: {script}"
    );
    assert!(
        script.contains("/usr/lib/ibus/ibus-x11"),
        "spawn the bridge the way the distro hook does: {script}"
    );
    assert_eq!(
        content::restart_script(Some("x11")),
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
    let sandbox = std::env::temp_dir().join(format!("mangrs-gui-test-{}", std::process::id()));
    std::fs::create_dir_all(&sandbox).expect("sandbox dir");
    std::env::set_var("XDG_CONFIG_HOME", &sandbox);
    refresh_round_trips_every_widget();
    shortcut_helpers_render_and_capture();
    external_reload_refreshes_without_saving();
    row_texts_render_as_plain_text();
    rules_add_group_merges_the_focus_chip();
    rule_rows_and_preview_track_the_engine_match();
    macro_page_helpers_count_and_preview();
    macros_editor_follows_the_switch();
    emoji_editor_follows_the_switch();
    toggling_off_releases_the_editor_caret();
    every_switch_triggers_save();
    experimental_gate_hides_and_restores_rows();
    combo_value_stays_right_after_a_pick();
    popup_marks_the_selected_item();
    format_combos_keep_presets_and_unknowns();
    language_row_round_trips();
    nav_key_row_round_trips();
    language_switch_rebuilds_the_window();
    language_handler_rebuilds_both_ways();
    // Restore before the sandbox dir could be cleaned under a racing test.
    match original_config_dir {
        Some(dir) => std::env::set_var("XDG_CONFIG_HOME", dir),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    let _ = std::fs::remove_dir_all(&sandbox);
}

/// The combo item lists (mode/charset/method/...) render in the window's
/// language; positions stay the index contract either way.
#[test]
fn labels_follow_the_language() {
    let _g = crate::gui::text::LANG_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    set_language("vi");
    assert_eq!(labels(CHANNELS)[0], "CommitText (an toàn)");
    set_language("en");
    assert_eq!(labels(CHANNELS)[0], "CommitText (safe)");
    assert_eq!(labels(MODES)[0], "No underline");
    set_language("vi");
    assert_eq!(labels(MODES)[0], "Không gạch chân");
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
    let keys = stack
        .visible_child()
        .unwrap()
        .downcast_ref::<gtk4::ShortcutLabel>()
        .unwrap()
        .clone();
    assert_eq!(keys.accelerator().as_deref(), Some("<Control><Alt>v"));
    display.render("Shift+Control");
    let label = stack
        .visible_child()
        .unwrap()
        .downcast_ref::<gtk4::Label>()
        .unwrap()
        .clone();
    assert_eq!(label.text(), "Shift+Control");
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

/// The rebuilt rule rows: the empty list renders one dim row, a loaded rule
/// reads back through `read()` unchanged, and the "Thử luật" verdict row
/// walks the engine's own matcher — icon and subtitle per state.
fn rule_rows_and_preview_track_the_engine_match() {
    let ui = widgets();
    // Empty list: one plain row, not activatable, carrying the notice.
    ui.fill_rules(&[]);
    let empty = ui.rules_box.row_at_index(0).expect("empty-state row");
    assert!(!empty.is_activatable());
    assert!(empty.downcast_ref::<adw::ActionRow>().is_none());
    ui.rule_preview_input.set_text("");
    ui.update_rule_preview();
    assert_eq!(
        ui.rule_preview.subtitle().as_deref(),
        Some("Nhập tên app ở trên để xem kết quả")
    );
    assert!(ui.rule_preview_icon.icon_name().as_deref() == Some("system-search-symbolic"));
    assert!(!ui.rule_preview_icon.has_css_class("success"));

    // One rule: the row round-trips through read(), and the verdict row
    // reports the match with its priority and mode.
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
    assert_eq!(row.title().as_str(), "kate");
    ui.rule_preview_input.set_text("kate");
    ui.update_rule_preview();
    assert_eq!(
        ui.rule_preview.subtitle().as_deref(),
        Some("Khớp luật #1: Gạch chân (preedit)")
    );
    assert!(ui.rule_preview_icon.has_css_class("success"));
    // A non-matching name reports no match, without the success tint.
    ui.rule_preview_input.set_text("zed");
    ui.update_rule_preview();
    assert_eq!(
        ui.rule_preview.subtitle().as_deref(),
        Some("Không khớp luật nào")
    );
    assert!(!ui.rule_preview_icon.has_css_class("success"));
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
        enabled: false,
        method: MethodConfig::Vni,
        mode: ModeConfig::Select,
        charset: CharsetConfig::Tcvn3,
        modern_tone_style: false,
        free_tone_marking: true,
        auto_correct: false,
        auto_restore: false,
        bypass_url: false,
        // The switch off must NOT wipe the stored value: refresh shows it
        // (locked) and read returns it — the value reapplies when the
        // switch comes back on. Pinned by the read() round-trip below.
        extra_url_chars: "`<".into(),
        extra_url_chars_enabled: false,
        w2u: W2uConfig::NonStart,
        backspace_tone: BackspaceToneConfig::ToneFirst,
        bracket: BracketConfig::Everywhere,
        em_dash: true,
        auto_capitalize: true,
        auto_capitalize_after: ":".into(),
        auto_capitalize_after_enabled: true,
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
        commit_interval_ms: 33,
        backspace_interval_ms: 7,
        post_delay_ms: 12,
        unicode_channel: crate::config::UnicodeChannel::Clipboard,
        clipboard_restore_delay_enabled: true,
        clipboard_restore_delay_ms: 1500,
        echo_stale_enabled: true,
        echo_stale_ms: 250,
        idle_commit_enabled: true,
        idle_commit_ms: 800,
        smooth_per_key: true,
        smooth_per_key_delay_ms: 80,
        experimental_features: true,
        show_toasts: false,
        debug_log: true,
        debug_log_text: true,
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
                children: vec!["preedit".into(), "smooth".into()],
            },
        ]),
    };
    ui.refresh(&config, &macros, &dictionary);

    // The mode combo must render the config's mode under the right label:
    // the display list and the index mappers once drifted (the list kept
    // the removed "Gõ mặc định" and missed "Tự động", so Preedit showed as
    // Surrounding text) and the round-trip below could not see it because
    // it only walks the mappers, never the labels.
    assert_eq!(
        MODES[0].0, "Không gạch chân",
        "the list must lead with the first display mode"
    );
    assert!(
        !MODES
            .iter()
            .any(|(vi, en)| matches!(*vi, "Tự động" | "Gõ mặc định")
                || matches!(*en, "Tự động" | "Gõ mặc định")),
        "removed modes must not linger in the list"
    );
    for m in [
        ModeConfig::NonPreedit,
        ModeConfig::Preedit,
        ModeConfig::Surrounding,
        ModeConfig::Smooth,
        ModeConfig::SuperSmooth,
        ModeConfig::Select,
        ModeConfig::ForwardKeyEvent,
        ModeConfig::ForwardSelect,
        ModeConfig::Off,
    ] {
        let i = mode_index(m) as usize;
        assert!(i < MODES.len(), "{m:?} index {i} outside the list");
        assert_eq!(mode_at(i as u32), m, "index round-trip for {m:?}");
    }
    assert_eq!(
        ui.mode.selected(),
        mode_index(ModeConfig::Select),
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
        ui.macros_from_text(),
        vec![("gs".to_string(), "giờ sáng".to_string())]
    );
    // A set: the order it hands the text view back in is not the input order.
    let mut words = ui.read_dictionary();
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
        !ui.commit_interval.uses_markup(),
        !ui.rule_preview.uses_markup(),
    ] {
        assert!(markup_off, "row titles and subtitles must be plain text");
    }
    assert_eq!(
        ui.bypass_url.subtitle().as_deref(),
        Some("Từ gõ ngay sau / : @ . _ ~ = # & % ? giữ nguyên")
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
    let rows: [&adw::SwitchRow; 27] = [
        &ui.enabled,
        &ui.modern,
        &ui.free,
        &ui.auto_correct,
        &ui.auto_restore,
        &ui.abbrev,
        &ui.bypass_url,
        &ui.extra_url_chars_switch,
        &ui.em_dash,
        &ui.auto_capitalize,
        &ui.auto_capitalize_after_switch,
        &ui.clipboard_restore_switch,
        &ui.echo_stale_switch,
        &ui.idle_commit_switch,
        &ui.experimental_features,
        &ui.show_toasts,
        &ui.double_space,
        &ui.macro_enabled,
        &ui.capitalize_macro,
        &ui.macro_in_off_mode,
        &ui.macro_valid_words,
        &ui.emoji_enabled,
        &ui.dictionary_enabled,
        &ui.hide_underline,
        &ui.debug_log,
        &ui.x11_delete,
        &ui.preedit_xim,
    ];
    for row in rows {
        let before = saves.get();
        row.set_active(!row.is_active());
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

/// The experimental master gate: off hides every experimental row (the
/// engine forces the features off), and the saved switch states and
/// values survive the round trip — reopening the gate restores them.
fn experimental_gate_hides_and_restores_rows() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    let on = Config {
        experimental_features: true,
        macro_expand_valid_words: true,
        extra_url_chars_enabled: true,
        extra_url_chars: "`".into(),
        ..Config::default()
    };
    ui.refresh(&on, &macros, &dictionary);
    assert!(ui.macro_valid_words.is_visible());
    assert!(ui.extra_url_chars_entry.is_visible());
    assert_eq!(ui.extra_url_chars_entry.text(), "`", "the value shows");

    // Off: hidden — and the saved value is NOT wiped by the refresh.
    let mut off = on.clone();
    off.experimental_features = false;
    ui.refresh(&off, &macros, &dictionary);
    assert!(!ui.macro_valid_words.is_visible());
    assert!(!ui.extra_url_chars_entry.is_visible());
    assert_eq!(
        ui.extra_url_chars_entry.text(),
        "`",
        "the value stays saved while hidden"
    );

    // Back on: the rows and their states reappear verbatim.
    ui.refresh(&on, &macros, &dictionary);
    assert!(ui.macro_valid_words.is_active());
    assert!(ui.extra_url_chars_switch.is_active());
    assert_eq!(ui.extra_url_chars_entry.text(), "`");
}

/// The $TIME/$DATE combos: a preset selects its preset index, a format the
/// old free-text rows saved comes back verbatim as its own appended item
/// (never rewritten to a preset), and re-refreshing does not stack
/// duplicates.
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
    assert_eq!(ui.read().time_format, TIME_FORMATS[1]);
    assert_eq!(ui.read().date_format, DATE_FORMATS[2]);

    // The widened value view carries the pattern to the row's end, like
    // the typing page combos — widen_value runs in the page build, so the
    // page must exist first.
    let page = crate::gui::pages::macros::build(&ui);
    let win = gtk4::Window::new();
    win.set_child(Some(page.upcast_ref::<gtk4::Widget>()));
    win.present();
    for _ in 0..10 {
        gtk4::glib::MainContext::default().iteration(false);
    }
    assert!(
        combo_value_view(&ui.time_format).is_some_and(|v| v.hexpands()),
        "the time pattern renders at the row end"
    );
    assert!(
        combo_value_view(&ui.date_format).is_some_and(|v| v.hexpands()),
        "the date pattern renders at the row end"
    );
    // The hint lives in the subtitle — the old "Ví dụ" spot under the
    // title — not in the suffixes (that parked it mid-row).
    assert!(
        ui.time_format.uses_subtitle(),
        "the subtitle space stays reserved"
    );
    assert!(
        ui.time_format.subtitle().is_some_and(|s| !s.is_empty()),
        "the subtitle carries the live preview"
    );
    assert!(
        ui.date_format.subtitle().is_some_and(|s| !s.is_empty()),
        "the date subtitle carries the live preview"
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

/// The experimental nav-key combo: the config value selects the item and
/// the pick reads back as the config value.
fn nav_key_row_round_trips() {
    let ui = widgets();
    let macros = Macros::load();
    let dictionary = Dictionary::load();

    ui.refresh(
        &Config {
            nav_key_while_preedit: "forward".into(),
            ..Config::default()
        },
        &macros,
        &dictionary,
    );
    assert_eq!(
        ui.nav_key_while_preedit.selected(),
        1,
        "forward selects index 1"
    );
    assert_eq!(ui.read().nav_key_while_preedit, "forward");
    ui.refresh(&Config::default(), &macros, &dictionary);
    assert_eq!(
        ui.nav_key_while_preedit.selected(),
        0,
        "the default selects index 0"
    );
    assert_eq!(ui.read().nav_key_while_preedit, "swallow");
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
        "swallow",
        "an unknown value behaves as swallow"
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
    assert_eq!(nav_tip(&win1, "other"), "Cài đặt khác", "vi by default");
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
        "Other settings",
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
    assert_eq!(nav_tip(&win1, "other"), "Cài đặt khác");

    // VI → EN through the real handler.
    language_combo(&win1).set_selected(1);
    pump();
    #[cfg(test)]
    eprintln!(
        "DEBUG win1 after pick: visible={} mapped={} in_toplevels={}",
        win1.is_visible(),
        win1.is_mapped(),
        gtk4::Window::list_toplevels().iter().any(|w| {
            std::ptr::eq(
                w.as_ptr().cast::<std::ffi::c_void>(),
                win1.upcast_ref::<gtk4::Widget>()
                    .as_ptr()
                    .cast::<std::ffi::c_void>(),
            )
        })
    );
    let en = app_windows();
    assert_eq!(en.len(), 1, "exactly one window after VI→EN: {en:?}");
    let win2 = en.first().expect("the EN window");
    assert_eq!(nav_tip(win2, "other"), "Other settings", "VI→EN rebuilt");

    // EN → VI through the same handler.
    language_combo(win2).set_selected(0);
    pump();
    let vi = app_windows();
    assert_eq!(vi.len(), 1, "exactly one window after EN→VI: {vi:?}");
    let win3 = vi.first().expect("the VI window");
    assert_eq!(nav_tip(win3, "other"), "Cài đặt khác", "EN→VI rebuilt");

    win3.close();
    set_language("vi");
}
