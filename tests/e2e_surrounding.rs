//! End-to-end cases for the Surrounding display mode: report-verified typing
//! and the stale-report tolerance.

mod common;

use common::{deletes, run_suite, Buffer, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "verified typing with reports",
        cfg: &[("mode", "\"surrounding\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: verify,
    },
    Case {
        name: "a stale report never tears down the word",
        cfg: &[("mode", "\"surrounding\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: stale_report,
    },
];

fn verify(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    // "aas": â composes first, then the sac mark rewrites it — the surround
    // verification must not disturb either step.
    let (buf, _) = s.type_reported("aas ");
    assert_eq!(buf, "ấ ", "buffer");
}

fn stale_report(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let mut buf = Buffer::default();
    for ch in "tie".chars() {
        let consumed = s.key(ch as u32, 0);
        s.pump(common::PUMP_MS);
        for sig in s.drain() {
            buf.apply(&sig);
        }
        if !consumed {
            buf.insert(&ch.to_string());
        }
        s.report(&buf.text());
        s.pump(common::PUMP_MS);
        for sig in s.drain() {
            buf.apply(&sig);
        }
    }
    assert_eq!(buf.text(), "tie", "buffer");
    // The tone key deletes 1 + commits ê; the report sent afterwards is STALE
    // (it predates the commit). The engine must ignore it, not resync to it.
    let consumed = s.key('e' as u32, 0);
    s.pump(common::PUMP_MS);
    for sig in s.drain() {
        buf.apply(&sig);
    }
    if !consumed {
        buf.insert("e");
    }
    s.report("tie");
    s.pump(common::PUMP_MS);
    for sig in s.drain() {
        buf.apply(&sig);
    }
    s.drain();
    let consumed = s.key('n' as u32, 0);
    s.pump(common::PUMP_MS);
    let sigs = s.drain();
    for sig in &sigs {
        buf.apply(sig);
    }
    if !consumed {
        buf.insert("n");
    }
    s.report(&buf.text());
    s.pump(common::PUMP_MS);
    for sig in s.drain() {
        buf.apply(&sig);
    }
    assert!(
        deletes(&sigs).is_empty(),
        "stale report must not trigger a delete: {sigs:?}"
    );
    assert_eq!(buf.text(), "tiên", "composition must survive");
}

#[test]
#[ignore]
fn e2e_surrounding() {
    run_suite(CASES);
}
