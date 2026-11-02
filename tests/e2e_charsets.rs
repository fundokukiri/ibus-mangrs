//! End-to-end cases for output charsets: the commit is encoded while the
//! preedit stays Unicode.

mod common;

use common::{commits, preedits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[Case {
    name: "charset tcvn3: the commit is encoded, the preedit stays Unicode",
    cfg: &[("charset", "\"tcvn3\"")],
    macros: &[],
    raw: "",
    dict: "",
    f: tcvn3,
}];

// Pinned from a verified run of this suite (the encoding is deterministic).
const EXPECTED_TCVN3_VIET: &str = "viÖt";

fn tcvn3(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("vieejt");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "việt",
        "preedit must stay Unicode: {pre:?}"
    );
    let sigs = s.type_plain(" ");
    assert_eq!(commits(&sigs), [EXPECTED_TCVN3_VIET], "commit");
}

#[test]
#[ignore]
fn e2e_charsets() {
    run_suite(CASES);
}
