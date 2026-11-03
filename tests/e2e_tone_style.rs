//! End-to-end cases for tone placement: modern vs traditional style and the
//! free-marking option.

mod common;

use common::{commits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "modern placement is the default",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: modern,
    },
    Case {
        name: "traditional placement",
        cfg: &[("modern_tone_style", "false")],
        macros: &[],
        raw: "",
        dict: "",
        f: traditional,
    },
    Case {
        name: "free marking off keeps the mark near the vowel",
        cfg: &[("free_tone_marking", "false")],
        macros: &[],
        raw: "",
        dict: "",
        f: free_marking_off,
    },
];

fn modern(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("hoaf ");
    assert_eq!(commits(&sigs), ["hòa"], "modern placement: {sigs:?}");
}

fn traditional(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("hoaf ");
    assert_eq!(commits(&sigs), ["hoà"], "traditional placement: {sigs:?}");
}

fn free_marking_off(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    // Without free marking the mark key cannot reach back over "ng": the
    // word comes out exactly as typed.
    let sigs = s.type_plain("hoangf ");
    assert_eq!(commits(&sigs), ["hoangf"], "{sigs:?}");
}

#[test]
#[ignore]
fn e2e_tone_style() {
    run_suite(CASES);
}
