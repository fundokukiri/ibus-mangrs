//! End-to-end cases for the four input methods and uppercase keystrokes.

mod common;

use common::{commits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "vni: digits carry the marks",
        cfg: &[("method", "\"vni\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: vni_marks,
    },
    Case {
        name: "viqr: punctuation keys carry the marks",
        cfg: &[("method", "\"viqr\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: viqr_marks,
    },
    Case {
        name: "microsoft layout: digit inserts and bracket vowels",
        cfg: &[("method", "\"microsoft\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: microsoft_layout,
    },
    Case {
        name: "telex 2: bracket vowels and telex marks",
        cfg: &[("method", "\"telex2\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: telex2,
    },
    Case {
        name: "telex + vni: marks from both systems, digits stay in the word",
        cfg: &[("method", "\"telexvni\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: telex_vni,
    },
    Case {
        name: "telex w: marks apply, z removes the tone",
        cfg: &[("method", "\"telexw\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: telex_w,
    },
    Case {
        name: "uppercase keystrokes compose the uppercase word",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: uppercase_word,
    },
];

fn vni_marks(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("tie6ng1 ");
    assert_eq!(commits(&sigs), ["tiếng"], "{sigs:?}");
}

fn viqr_marks(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("hoi? ");
    assert_eq!(commits(&sigs), ["hỏi"], "{sigs:?}");
    // unikey convention: a mark key that cannot apply stays literal.
    let sigs = s.type_plain("ok? ");
    assert_eq!(commits(&sigs), ["ok?"], "{sigs:?}");
}

fn microsoft_layout(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("ti3ng8 ");
    assert_eq!(commits(&sigs), ["tiếng"], "{sigs:?}");
    let sigs = s.type_plain("t[ ");
    assert_eq!(commits(&sigs), ["tư"], "{sigs:?}");
}

fn uppercase_word(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("VIEEJT ");
    assert_eq!(commits(&sigs), ["VIỆT"], "{sigs:?}");
}

fn telex2(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("tieengs ");
    assert_eq!(commits(&sigs), ["tiếng"], "{sigs:?}");
    let sigs = s.type_plain("t[ ");
    assert_eq!(commits(&sigs), ["tơ"], "{sigs:?}");
    // A digit ends the word and forwards: the preset would eat the word.
    let sigs = s.type_plain("x1 ");
    assert_eq!(commits(&sigs), ["x"], "{sigs:?}");
}

fn telex_vni(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("tieengs ");
    assert_eq!(commits(&sigs), ["tiếng"], "{sigs:?}");
    let sigs = s.type_plain("aa1 ");
    assert_eq!(commits(&sigs), ["ấ"], "{sigs:?}");
    // A digit bamboo cannot apply stays literal inside the word.
    let sigs = s.type_plain("x1 ");
    assert_eq!(commits(&sigs), ["x1"], "{sigs:?}");
}

fn telex_w(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("tieengs ");
    assert_eq!(commits(&sigs), ["tiếng"], "{sigs:?}");
    let sigs = s.type_plain("asz ");
    assert_eq!(commits(&sigs), ["a"], "{sigs:?}");
}

#[test]
#[ignore]
fn e2e_input_methods() {
    run_suite(CASES);
}
