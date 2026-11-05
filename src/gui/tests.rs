use super::*;

/// B3 regression: the import path pushes a config into every widget through
/// `refresh`, so a widget `refresh` forgets keeps its old value and the next
/// save writes that stale value back over the imported settings. Needs a
/// display; skips where GTK cannot start.
#[test]
fn refresh_round_trips_every_widget() {
    if gtk4::init().is_err() {
        eprintln!("gui: SKIP (no display)");
        return;
    }
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
        w2u: W2uConfig::NonStart,
        bracket: BracketConfig::Everywhere,
        em_dash: true,
        auto_capitalize: true,
        double_space_period: true,
        macro_enabled: false,
        capitalize_macro: false,
        macro_in_off_mode: true,
        time_format: "%H".into(),
        date_format: "%Y".into(),
        dictionary_enabled: false,
        abbreviations: false,
        cycle_mode_hotkey: "Control+Alt+v".into(),
        enabled_hotkey: "Control+Alt+space".into(),
        hide_underline: true,
        commit_interval_ms: 33,
        backspace_interval_ms: 7,
        post_delay_ms: 12,
        unicode_channel: crate::config::UnicodeChannel::Clipboard,
        debug_log: true,
        x11_delete_without_report: true,
        preedit_for_xim: true,
        app_rules: vec![crate::config::AppRule {
            pattern: "kate".into(),
            mode: ModeConfig::Preedit,
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

/// The hotkey recorder must live on the pencil button: after a click the
/// button keeps key focus, so the key controllers there receive the
/// pressed keys. Regression for the row-anchored recorder whose
/// focus-leave handler cancelled the capture before any key arrived —
/// with the controllers on the row and the button disabled on click,
/// assigning a hotkey did nothing. Needs a display; skips where GTK
/// cannot start.
#[test]
fn hotkey_recorder_lives_on_the_edit_button() {
    if gtk4::init().is_err() {
        eprintln!("gui: SKIP (no display)");
        return;
    }
    let kinds = |w: &gtk4::Widget| -> Vec<String> {
        let list = w.observe_controllers();
        (0..list.n_items())
            .filter_map(|i| list.item(i))
            .map(|o| o.type_().name().to_string())
            .collect()
    };

    let label = gtk4::Label::new(None);
    let edit = gtk4::Button::from_icon_name("document-edit-symbolic");
    let value = Rc::new(RefCell::new(String::new()));
    hotkey_wire(&edit, &label, &value, || {});

    let button = kinds(edit.upcast_ref());
    // 2 ours + GTK's internal one for the button's own key handling.
    assert!(
        button
            .iter()
            .filter(|k| *k == "GtkEventControllerKey")
            .count()
            >= 2,
        "press/release key controllers must sit on the edit button: {button:?}"
    );
    assert!(button.iter().any(|k| *k == "GtkEventControllerFocus"));

    // The capture starts on click and the button stays sensitive:
    // disabling the focus widget makes GTK move key focus off it, and
    // the focus-leave handler cancels the capture immediately.
    edit.emit_clicked();
    assert!(edit.is_sensitive(), "the recorder disabled the edit button");
    assert_eq!(label.text(), "Nhấn tổ hợp phím… (Esc hủy)");
}
