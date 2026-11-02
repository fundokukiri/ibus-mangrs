//! End-to-end safety invariants: a mutter-relayed context must never receive
//! a protocol delete and must never trigger key injection.

mod common;

use common::{commits, deletes, preedits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "mutter-relayed context without reports never deletes",
        cfg: &[("mode", "\"nonpreedit\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: relay_no_delete,
    },
    Case {
        name: "smooth resolves to preedit on a relay (no injection)",
        cfg: &[("mode", "\"smooth\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: smooth_relay_fallback,
    },
];

fn relay_no_delete(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk3-im:gnome-shell");
    s.drain();
    // Plain letters commit (a normal NonPreedit append needs no delete); the
    // mark keys degrade to raw-forward. The hard invariant: no protocol
    // delete and no injection — the app still ends up with the raw keys.
    let (buf, sigs) = s.type_buffered("tieengs");
    assert!(
        deletes(&sigs).is_empty(),
        "relay without reports must never delete: {sigs:?}"
    );
    assert!(
        preedits(&sigs).is_empty(),
        "no preedit was requested: {sigs:?}"
    );
    assert_eq!(buf, "tieengs", "raw keys must reach the app");
}

fn smooth_relay_fallback(s: &mut common::Sess) {
    // Smooth is a uinput mode, but on a mutter-relayed context injected keys
    // never arrive — the mode must resolve to preedit instead, so nothing is
    // ever pressed on the real desktop during this case.
    s.focus(CAPS_BOTH, "gtk3-im:gnome-shell");
    s.drain();
    let sigs = s.type_plain("vieetj ");
    assert!(
        !preedits(&sigs).is_empty(),
        "relay must render smooth as preedit: {sigs:?}"
    );
    assert_eq!(commits(&sigs), ["việt"], "{sigs:?}");
    assert!(deletes(&sigs).is_empty(), "preedit never deletes: {sigs:?}");
}

#[test]
#[ignore]
fn e2e_safety() {
    run_suite(CASES);
}
