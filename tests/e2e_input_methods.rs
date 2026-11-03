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

#[test]
#[ignore]
fn e2e_input_methods() {
    run_suite(CASES);
}
