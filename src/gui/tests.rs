use super::*;

/// GTK initializes once per process and only ever on the thread that did
/// it, while libtest hands every `#[test]` its own thread — separate GTK
/// tests flake with "Attempted to initialize GTK from two different
/// threads" depending on the scheduler. One test runs every widget
/// scenario sequentially on the one thread that initialized GTK; each
/// scenario keeps its own assertions. Needs a display; skips where GTK
/// cannot start.
#[test]
fn gtk_widget_scenarios() {
    if gtk4::init().is_err() {
        eprintln!("gui: SKIP (no display)");
        return;
    }
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
        extra_url_chars: "`<".into(),
        extra_url_chars_enabled: true,
        w2u: W2uConfig::NonStart,
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
        debug_log: true,
        debug_log_text: true,
        x11_delete_without_report: true,
        preedit_for_xim: true,
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
    assert_eq!(MODES[0], "Tự động", "auto must lead the mode list");
    assert!(
        !MODES.contains(&"Gõ mặc định"),
        "the removed mode must not linger in the list"
    );
    for m in [
        ModeConfig::Auto,
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
    let rows: [&adw::SwitchRow; 24] = [
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
