//! End-to-end cases for the NonPreedit display mode: per-key commits,
//! protocol-deleted rewrites and the protocol backspace.

mod common;

use common::{
    commits, deletes, preedits, run_suite, Buffer, Case, CAPS_BOTH, CAP_SURROUNDING, KEY_BACKSPACE,
    PUMP_MS,
};

const CASES: &[Case] = &[
    Case {
        name: "per-key commits and protocol deletes (reported client)",
        cfg: &[("mode", "\"nonpreedit\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: rewrite,
    },
    Case {
        name: "backspace deletes via the protocol",
        cfg: &[("mode", "\"nonpreedit\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: backspace_protocol,
    },
    Case {
        name: "client without the preedit capability falls back to nonpreedit",
        cfg: &[("mode", "\"preedit\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: no_preedit_cap,
    },
];

fn rewrite(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, sigs) = s.type_reported("tieengs ");
    assert_eq!(buf, "tiếng ", "buffer");
    assert!(
        !deletes(&sigs).is_empty(),
        "tone keys must rewrite via protocol delete"
    );
    assert!(preedits(&sigs).is_empty(), "no preedit in nonpreedit mode");
}

fn backspace_protocol(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    // "việt" minus the huyền key: one character shorter, same base letters —
    // a pure single-character delete through the protocol.
    let (buf, _) = s.type_reported("vieejt");
    assert_eq!(buf, "việt", "buffer before the backspace");
    assert!(
        s.key(KEY_BACKSPACE, 0),
        "backspace over committed text is consumed"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert_eq!(
        deletes(&sigs),
        [(-1, 1)],
        "the delete must ride the protocol: {sigs:?}"
    );
    let mut replay = Buffer::default();
    replay.insert("việt");
    for sig in &sigs {
        replay.apply(sig);
    }
    assert_eq!(replay.text(), "việ", "buffer after the delete");
}

fn no_preedit_cap(s: &mut common::Sess) {
    s.focus(CAP_SURROUNDING, "gtk4-im:test");
    s.drain();
    let (buf, sigs) = s.type_reported("tieengs ");
    assert_eq!(buf, "tiếng ", "buffer");
    assert!(
        preedits(&sigs).is_empty(),
        "no preedit without the capability: {sigs:?}"
    );
    assert!(!commits(&sigs).is_empty(), "the word must still type");
}

#[test]
#[ignore]
fn e2e_nonpreedit() {
    run_suite(CASES);
}
