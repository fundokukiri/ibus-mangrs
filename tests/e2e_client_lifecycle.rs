//! End-to-end cases for the client lifecycle: Reset, FocusOut and Disable —
//! the preedit-carrying-commit contract in all its forms.

mod common;

use common::{commits, preedits, run_suite, Case, CAPS_BOTH, PUMP_MS};

const CASES: &[Case] = &[
    Case {
        name: "client reset commits nothing (the client owns the preedit commit)",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: client_reset,
    },
    Case {
        name: "focus out emits no signals at all",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: focus_out_silent,
    },
    Case {
        name: "disable forwards keys to the application",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: disable_forwards,
    },
];

fn client_reset(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    s.type_plain("chaof");
    s.reset_call();
    s.pump(PUMP_MS);
    let sigs = s.drain();
    assert!(commits(&sigs).is_empty(), "reset must not commit: {sigs:?}");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && !pre[0].1 && pre[0].2 == 0,
        "reset must hide the preedit: {pre:?}"
    );
}

fn focus_out_silent(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    s.type_plain("tung");
    s.focus_out();
    s.pump(PUMP_MS);
    assert!(
        s.drain().is_empty(),
        "focus-out must be a no-op on the wire"
    );
}

fn disable_forwards(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    s.type_plain("tun");
    s.disable();
    s.pump(PUMP_MS);
    assert!(s.drain().is_empty(), "disable must not emit signals");
    assert!(
        !s.key('t' as u32, 0),
        "after Disable the key belongs to the app"
    );
    s.pump(PUMP_MS);
    assert!(s.drain().is_empty());
}

#[test]
#[ignore]
fn e2e_client_lifecycle() {
    run_suite(CASES);
}
