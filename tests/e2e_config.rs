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

#[test]
#[ignore]
fn e2e_config() {
    run_suite(CASES);
}
