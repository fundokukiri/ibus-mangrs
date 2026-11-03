//! End-to-end cases for the transform keys: w2u (w -> ư) and the bracket
//! keys ([ ] -> ơ ư), in both nonstart and everywhere modes.

mod common;

use common::{commits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "w2u nonstart: w becomes ư except at word start",
        cfg: &[("w2u", "\"nonstart\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: w2u_nonstart,
    },
    Case {
        name: "w2u everywhere: w becomes ư anywhere",
        cfg: &[("w2u", "\"everywhere\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: w2u_everywhere,
    },
    Case {
        name: "bracket nonstart: t[ types tơ, a leading bracket stays literal",
        cfg: &[("bracket", "\"nonstart\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: bracket_nonstart,
    },
    Case {
        name: "bracket everywhere: [ types ơ even at word start",
        cfg: &[("bracket", "\"everywhere\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: bracket_everywhere,
    },
];

fn w2u_nonstart(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("w nhw ");
    assert_eq!(commits(&sigs), ["w", "như"], "{sigs:?}");
}

fn w2u_everywhere(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("w nhw ");
    assert_eq!(commits(&sigs), ["ư", "như"], "{sigs:?}");
}

fn bracket_nonstart(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("t[ ");
    assert_eq!(commits(&sigs), ["tơ"], "{sigs:?}");
    // At the start of a word the bracket stays a bracket.
    let (buf, sigs) = s.type_buffered("[ ");
    assert_eq!(buf, "[ ", "literal bracket at word start");
    assert!(commits(&sigs).is_empty(), "{sigs:?}");
}

fn bracket_everywhere(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("[ ");
    assert_eq!(commits(&sigs), ["ơ"], "{sigs:?}");
}

#[test]
#[ignore]
fn e2e_special_keys() {
    run_suite(CASES);
}
