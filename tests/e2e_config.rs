//! End-to-end cases for configuration plumbing: file hot-reload, the panel
//! property menu, the cycle hotkey, per-app rules and the debug log.

mod common;

use common::{commits, deletes, has_member, preedits, run_suite, Case, CAPS_BOTH, PUMP_MS};

const CASES: &[Case] = &[
    Case {
        name: "config hot-reload switches the mode without a restart",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: hot_reload,
    },
    Case {
        name: "property menu: register on focus, activate applies the mode",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: property_menu,
    },
    Case {
        name: "cycle hotkey switches the display mode",
        cfg: &[("cycle_mode_hotkey", "\"Control+Alt+v\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: cycle_hotkey,
    },
    Case {
        name: "toggle hotkey flips enabled and updates the panel",
        cfg: &[("enabled_hotkey", "\"Control+Alt+h\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: toggle_hotkey,
    },
    Case {
        name: "app rule picker: lookup table opens and writes the rule",
        cfg: &[("app_rule_hotkey", "\"Control+Alt+p\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: app_rule_picker,
    },
    Case {
        name: "emoji picker: lookup table at the caret commits the pick",
        cfg: &[("emoji_hotkey", "\"Control+Alt+e\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: emoji_picker,
    },
    Case {
        name: "app rules: a config rule overrides the default mode",
        cfg: &[],
        macros: &[],
        raw: "[[app_rules]]\npattern = \"e2eruled\"\nmode = \"nonpreedit\"\n",
        dict: "",
        f: app_rule,
    },
    Case {
        name: "app rules: the none pattern targets unnamed clients",
        cfg: &[],
        macros: &[],
        raw: "[[app_rules]]\npattern = \"none\"\nmode = \"nonpreedit\"\n",
        dict: "",
        f: none_rule,
    },
    Case {
        name: "debug log records engine decisions when enabled",
        cfg: &[("debug_log", "true")],
        macros: &[],
        raw: "",
        dict: "",
        f: debug_log,
    },
];

fn hot_reload(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("a ");
    assert_eq!(commits(&sigs), ["a"], "{sigs:?}");
    // Append the new mode; the engine re-checks the file mtime on every key,
    // gated to once a second.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(s.config_path())
        .expect("config for append");
    writeln!(f, "mode = \"nonpreedit\"").expect("append mode");
    drop(f);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    s.drain();
    let (buf, sigs) = s.type_reported("tieengs");
    assert_eq!(buf, "tiếng", "buffer after the reload");
    assert!(
        !deletes(&sigs).is_empty(),
        "after the reload the rewrite must use protocol deletes: {sigs:?}"
    );
    assert!(
        preedits(&sigs).is_empty(),
        "reload to nonpreedit must stop the preedit: {sigs:?}"
    );
}

fn property_menu(s: &mut common::Sess) {
    let focus = s.focus(CAPS_BOTH, "gtk4-im:test");
    let reg: Vec<&common::Sig> = focus
        .iter()
        .filter(|s| matches!(s, common::Sig::RegisterProperties(_)))
        .collect();
    assert!(!reg.is_empty(), "no RegisterProperties on focus: {focus:?}");
    let rendered = match reg[0] {
        common::Sig::RegisterProperties(text) => text.clone(),
        _ => unreachable!(),
    };
    assert!(
        rendered.contains("MăngRS"),
        "menu label missing: {rendered:.300}"
    );
    s.drain();

    s.property("display:nonpreedit", 1); // 1 = STATE_CHECKED
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "UpdateProperty"),
        "no UpdateProperty: {sigs:?}"
    );
    let text = std::fs::read_to_string(s.config_path()).expect("config");
    assert!(
        text.contains("mode = \"nonpreedit\""),
        "activate must persist the mode: {text:.300}"
    );
}

fn cycle_hotkey(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let control = common::CONTROL;
    let alt = common::ALT;
    assert!(
        s.key(0x76, control | alt),
        "the cycle hotkey must be consumed"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "UpdateProperty"),
        "the hotkey must push the panel update like a click does: {sigs:?}"
    );
    let text = std::fs::read_to_string(s.config_path()).expect("config");
    assert!(
        text.contains("mode = \"surrounding\""),
        "preedit must cycle to surrounding: {text:.300}"
    );
    let (buf, _) = s.type_reported("tieengs ");
    assert_eq!(buf, "tiếng ", "buffer after the cycle");
}

fn toggle_hotkey(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    assert!(
        s.key(0x68, common::CONTROL | common::ALT),
        "the toggle hotkey must be consumed"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "UpdateProperty"),
        "no panel update after the toggle: {sigs:?}"
    );
    let text = std::fs::read_to_string(s.config_path()).expect("config");
    assert!(
        text.contains("enabled = false"),
        "the toggle must persist: {text:.300}"
    );
    let (buf, _) = s.type_reported("tieengs ");
    assert_eq!(buf, "tieengs ", "Vietnamese must be off after the toggle");
}

fn app_rule_picker(s: &mut common::Sess) {
    // The picker targets the client's prgname (the e2e has no focus
    // extension): one flat page of the ten modes — the auxiliary line names
    // the app and its current mode, digit 2 applies the second entry
    // (nonpreedit), and the popup closes.
    s.focus(CAPS_BOTH, "gtk4-im:e2epicktest");
    s.drain();
    assert!(
        s.key(0x70, common::CONTROL | common::ALT),
        "hotkey consumed"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "UpdateLookupTable"),
        "the picker must show at the caret: {sigs:?}"
    );
    assert!(
        has_member(&sigs, "UpdateAuxiliaryText"),
        "the auxiliary line must name the app and its mode: {sigs:?}"
    );
    // Releasing the combo keys (the moment the menu opens) must NOT close
    // it — releases are swallowed whole while the picker is up.
    let release = 1u32 << 30;
    for kv in [0x70, 0xffe5, 0xffe9] {
        assert!(
            s.key(kv, common::CONTROL | common::ALT | release),
            "release consumed"
        );
    }
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        !has_member(&sigs, "HideLookupTable"),
        "releases must keep the menu open: {sigs:?}"
    );
    // Digit 2 applies the second entry and closes the popup.
    assert!(s.key(0x32, 0), "digit consumed by the picker");
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "HideLookupTable") && has_member(&sigs, "HideAuxiliaryText"),
        "the picker closes after the pick: {sigs:?}"
    );
    let text = std::fs::read_to_string(s.config_path()).expect("config");
    assert!(
        text.contains("pattern = \"e2epicktest\"") && text.contains("mode = \"nonpreedit\""),
        "the pick must land as the client's rule: {text:.400}"
    );
    // Mid-word pick: a composed word is still on the wire (preedit) when
    // the menu opens — the flush must commit exactly what is shown, and
    // typing after the pick must keep working (the lost-letters report).
    // A fresh client: the previous section pinned e2epicktest to
    // nonpreedit, where no preedit composes.
    s.focus(CAPS_BOTH, "gtk4-im:e2emidword");
    s.drain();
    let (_buf, preedit_sigs) = s.type_buffered("tieengs"); // composes, no terminator yet
    s.drain();
    assert!(
        !preedit_sigs.is_empty(),
        "the fresh client must compose as preedit: {preedit_sigs:?}"
    );
    assert!(
        s.key(0x70, common::CONTROL | common::ALT),
        "hotkey mid-word"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        commits(&sigs).contains(&"tiếng"),
        "opening the menu commits the composed word: {sigs:?}"
    );
    assert!(s.key(0x31, 0), "digit 1 applies auto");
    s.pump(PUMP_MS);
    s.drain();
    let (buf, _) = s.type_reported(" x tieengs ");
    assert_eq!(buf, " x tiếng ", "typing must work after a mid-word pick");

    // Mouse pick: GNOME's candidate popup forwards CandidateClicked; the
    // pick updates the rule the app already rides on.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(s.key(0x70, common::CONTROL | common::ALT), "hotkey reopens");
    s.pump(PUMP_MS);
    s.drain();
    s.call_engine(common::engine_msg("CandidateClicked").append3(3u32, 1u32, 0u32));
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "HideLookupTable"),
        "the mouse pick closes the menu: {sigs:?}"
    );
    let text = std::fs::read_to_string(s.config_path()).expect("config");
    assert!(
        text.contains("pattern = \"e2epicktest\"") && text.contains("mode = \"surrounding\""),
        "the mouse pick must update the winning rule in place: {text:.400}"
    );

    // Arrow navigation: the highlight starts on the mode in force (the
    // mouse pick above left the app on surrounding, index 3), Down moves
    // it (the table re-renders with the new cursor), Enter picks it.
    std::thread::sleep(std::time::Duration::from_millis(
        300, // past the hotkey repeat gap: a deliberate second press
    ));
    assert!(s.key(0x70, common::CONTROL | common::ALT), "hotkey reopens");
    s.pump(PUMP_MS);
    s.drain();
    assert!(s.key(65364, 0), "Down is consumed by the menu");
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        !has_member(&sigs, "HideLookupTable") && has_member(&sigs, "UpdateLookupTable"),
        "Down must move the highlight, not close the menu: {sigs:?}"
    );
    assert!(s.key(65293, 0), "Enter picks the highlighted entry");
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "HideLookupTable"),
        "the pick closes the menu: {sigs:?}"
    );
    let text = std::fs::read_to_string(s.config_path()).expect("config");
    assert!(
        text.contains("pattern = \"e2epicktest\"") && text.contains("mode = \"smooth\""),
        "Enter must apply the entry the highlight moved to (index 4 = smooth): {text:.400}"
    );
    // Second hotkey press while the menu is up toggles it closed.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        s.key(0x70, common::CONTROL | common::ALT),
        "hotkey opens again"
    );
    s.pump(PUMP_MS);
    s.drain();
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(s.key(0x70, common::CONTROL | common::ALT), "hotkey toggles");
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "HideLookupTable"),
        "the toggle press must close the menu: {sigs:?}"
    );
}

fn app_rule(s: &mut common::Sess) {
    // The rule pins this client to nonpreedit although the config mode is
    // preedit (the default) — a user rule wins over the built-in heuristic.
    s.focus(CAPS_BOTH, "gtk4-im:e2eruled");
    s.drain();
    let (buf, sigs) = s.type_reported("tieengs ");
    assert_eq!(buf, "tiếng ", "buffer under the rule");
    assert!(
        preedits(&sigs).is_empty(),
        "the rule must override the default preedit: {sigs:?}"
    );
    assert!(!commits(&sigs).is_empty(), "nonpreedit commits");
}

fn none_rule(s: &mut common::Sess) {
    // Plain FocusIn: no FocusInId ever arrives — the unnamed path the
    // `none` pattern exists for.
    s.focus_anonymous(CAPS_BOTH);
    s.drain();
    let (buf, sigs) = s.type_reported("tieengs ");
    assert_eq!(buf, "tiếng ", "the none rule must apply to unnamed clients");
    assert!(
        preedits(&sigs).is_empty(),
        "the rule forces nonpreedit: {sigs:?}"
    );
    // A named client (no matching rule) falls back to the configured preedit.
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("tieeng");
    assert!(
        !preedits(&sigs).is_empty(),
        "a named client must not match the none rule: {sigs:?}"
    );
}

fn debug_log(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let log = s.cache_dir().join("ibus-mangrs").join("debug.log");
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        !text.is_empty(),
        "debug.log must exist and record decisions"
    );
}

fn emoji_picker(s: &mut common::Sess) {
    // The bamboo-style menu: one flat page at the caret, releases keep it
    // open, the hotkey pressed again toggles it closed (never a second
    // popup), letters search, a digit commits the glyph straight into the
    // application - no clipboard - and Escape closes.
    s.focus(CAPS_BOTH, "gtk4-im:e2emoji");
    s.drain();
    assert!(
        s.key(0x65, common::CONTROL | common::ALT),
        "hotkey consumed"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "UpdateLookupTable") && has_member(&sigs, "UpdateAuxiliaryText"),
        "the menu must show at the caret: {sigs:?}"
    );
    let release = 1u32 << 30;
    for kv in [0x65, 0xffe5, 0xffe9] {
        assert!(
            s.key(kv, common::CONTROL | common::ALT | release),
            "release consumed"
        );
    }
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        !has_member(&sigs, "HideLookupTable"),
        "releases must keep the menu open: {sigs:?}"
    );
    std::thread::sleep(std::time::Duration::from_millis(common::SETTLE_MS));
    assert!(
        s.key(0x65, common::CONTROL | common::ALT),
        "hotkey pressed again"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "HideLookupTable") && !has_member(&sigs, "UpdateLookupTable"),
        "the second press must close the menu, not open another: {sigs:?}"
    );
    // Reopen and search: letters build the query, a digit commits the glyph.
    std::thread::sleep(std::time::Duration::from_millis(common::SETTLE_MS));
    assert!(s.key(0x65, common::CONTROL | common::ALT), "hotkey reopens");
    s.pump(PUMP_MS);
    s.drain();
    for kv in [0x66, 0x69, 0x72, 0x65] {
        assert!(s.key(kv, 0), "query letter consumed by the menu");
    }
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "UpdateAuxiliaryText"),
        "the query shows in the auxiliary line: {sigs:?}"
    );
    assert!(s.key(0x31, 0), "digit 1 picks");
    s.pump(PUMP_MS);
    let sigs = s.drain();
    let commit = commits(&sigs);
    assert_eq!(
        commit.len(),
        1,
        "exactly one CommitText - the glyph: {sigs:?}"
    );
    assert!(
        !commit[0].is_ascii(),
        "the commit must be the emoji glyph itself, not a rewrite: {sigs:?}"
    );
    assert!(
        has_member(&sigs, "HideLookupTable") && has_member(&sigs, "HideAuxiliaryText"),
        "the pick closes the menu: {sigs:?}"
    );
    // The client runs the default Preedit mode: the word commits at the
    // trailing space.
    let (buf, all) = s.type_reported(" l ");
    assert_eq!(buf, " l ", "typing must work right after the pick: {all:?}");
    // Escape closes a reopened menu.
    std::thread::sleep(std::time::Duration::from_millis(common::SETTLE_MS));
    assert!(s.key(0x65, common::CONTROL | common::ALT), "hotkey reopens");
    s.pump(PUMP_MS);
    s.drain();
    assert!(s.key(0xff1b, 0), "Escape consumed by the menu");
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(
        has_member(&sigs, "HideLookupTable"),
        "Escape closes the menu: {sigs:?}"
    );
}

#[test]
#[ignore]
fn e2e_config() {
    run_suite(CASES);
}
