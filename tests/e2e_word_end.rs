//! End-to-end cases for word-end behavior: abbreviations, auto-restore, the
//! dictionary gate, digits and underscore as terminators.

mod common;

use common::{commits, preedits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "abbreviation: ddc keeps its horn (ddc -> đc)",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: abbreviation_horn,
    },
    Case {
        name: "abbreviations off: ddc keeps the raw reversion",
        cfg: &[("abbreviations", "false")],
        macros: &[],
        raw: "",
        dict: "",
        f: abbreviations_off,
    },
    Case {
        name: "auto restore: non-Vietnamese word returns the raw keys",
        cfg: &[("auto_correct", "false")],
        macros: &[],
        raw: "",
        dict: "",
        f: auto_restore_raw,
    },
    Case {
        name: "dictionary words are never auto-restored",
        cfg: &[("dictionary_enabled", "true"), ("auto_correct", "false")],
        macros: &[],
        raw: "",
        dict: "windờ\n",
        f: dictionary_gate,
    },
    Case {
        name: "digits end the word and are always forwarded",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: digits_terminate,
    },
    Case {
        name: "underscore ends the word and is kept",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: underscore_terminates,
    },
];

fn abbreviation_horn(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("ddc");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "đc",
        "preedit must show đc: {pre:?}"
    );
    let sigs = s.type_plain(" ");
    assert_eq!(commits(&sigs), ["đc"], "{sigs:?}");
}

fn abbreviations_off(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("ddc");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "ddc",
        "with abbreviations off the raw reversion stands: {pre:?}"
    );
    let sigs = s.type_plain(" ");
    assert_eq!(commits(&sigs), ["ddc"], "{sigs:?}");
}

fn auto_restore_raw(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("windowf");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "windowf",
        "preedit must show the raw keys: {pre:?}"
    );
    let sigs = s.type_plain(" ");
    assert_eq!(commits(&sigs), ["windowf"], "{sigs:?}");
}

fn dictionary_gate(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    // With auto_correct off, "windowf" composes into the bent, invalid
    // "windờ" (f = huyền on the ơ). The word-end restore would put the raw
    // keys back — unless the dictionary contains the composed form, which is
    // exactly what the user's dictionary is for.
    let sigs = s.type_plain("windowf");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "windờ",
        "the composed form must stay visible: {pre:?}"
    );
    let sigs = s.type_plain(" ");
    assert_eq!(
        commits(&sigs),
        ["windờ"],
        "dictionary hit: no restore to the raw keys: {sigs:?}"
    );
}

fn digits_terminate(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, _) = s.type_buffered("x1tieeng1 ");
    assert_eq!(buf, "x1tiêng1 ", "buffer");
}

fn underscore_terminates(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, _) = s.type_buffered("abc_bcd2 ");
    assert_eq!(buf, "abc_bcd2 ", "buffer");
}

#[test]
#[ignore]
fn e2e_word_end() {
    run_suite(CASES);
}
