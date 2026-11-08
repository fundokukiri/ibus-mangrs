//! End-to-end safety invariants: a mutter-relayed context must never receive
//! a protocol delete and must never trigger key injection.

mod common;

use common::{deletes, preedits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[Case {
    name: "mutter-relayed context without reports never deletes",
    cfg: &[("mode", "\"nonpreedit\"")],
    macros: &[],
    raw: "",
    dict: "",
    f: relay_no_delete,
}];

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

// NOTE: the old "smooth resolves to preedit on a relay" case is gone — the
// 08/11 re-measurement refuted the belief it pinned, and smooth on a relay
// now injects real keys, which this harness must never fire at the real
// desktop (see tests/e2e/README.md).

#[test]
#[ignore]
fn e2e_safety() {
    run_suite(CASES);
}
