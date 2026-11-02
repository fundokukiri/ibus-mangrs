//! End-to-end cases for the two non-protocol modes: OFF and Default Typing.

mod common;

use common::{commits, deletes, preedits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "off mode forwards raw keys",
        cfg: &[("mode", "\"off\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: off_forwards,
    },
    Case {
        name: "off mode still expands macros when configured",
        cfg: &[("mode", "\"off\""), ("macro_in_off_mode", "true")],
        macros: &[("gs", "giờ sáng")],
        raw: "",
        dict: "",
        f: off_macro,
    },
    Case {
        name: "default typing is fully inert",
        cfg: &[("mode", "\"defaulttyping\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: default_typing,
    },
];

fn off_forwards(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, sigs) = s.type_buffered("vieetj ");
    assert_eq!(buf, "vieetj ", "raw keys in OFF mode");
    assert!(commits(&sigs).is_empty(), "OFF commits nothing: {sigs:?}");
    assert!(preedits(&sigs).is_empty(), "OFF shows no preedit: {sigs:?}");
}

fn off_macro(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, sigs) = s.type_reported("gs ");
    assert_eq!(buf, "giờ sáng ", "the expansion must replace the raw keys");
    assert_eq!(commits(&sigs), ["giờ sáng"], "{sigs:?}");
}

fn default_typing(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, sigs) = s.type_buffered("x1 ");
    assert_eq!(buf, "x1 ", "everything forwarded");
    assert!(
        commits(&sigs).is_empty() && preedits(&sigs).is_empty() && deletes(&sigs).is_empty(),
        "Default Typing must emit no signals at all: {sigs:?}"
    );
}

#[test]
#[ignore]
fn e2e_modes() {
    run_suite(CASES);
}
