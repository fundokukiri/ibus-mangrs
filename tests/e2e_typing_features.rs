//! End-to-end cases for the per-keystroke typing features: URL/email/path
//! bypass, auto-capitalization, the em-dash and the double-space period.

mod common;

use common::{run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "url/email/path bypass keeps the word raw",
        cfg: &[("mode", "\"nonpreedit\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: url_bypass,
    },
    Case {
        name: "auto capitalize after sentence end",
        cfg: &[("mode", "\"nonpreedit\""), ("auto_capitalize", "true")],
        macros: &[],
        raw: "",
        dict: "",
        f: auto_capitalize,
    },
    Case {
        name: "em dash: -- becomes —",
        cfg: &[("mode", "\"nonpreedit\""), ("em_dash", "true")],
        macros: &[],
        raw: "",
        dict: "",
        f: em_dash,
    },
    Case {
        name: "double space inserts a period",
        cfg: &[("mode", "\"nonpreedit\""), ("double_space_period", "true")],
        macros: &[],
        raw: "",
        dict: "",
        f: double_space,
    },
];

fn url_bypass(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, _) = s.type_reported("x.example.tieeng ");
    assert_eq!(buf, "x.example.tieeng ", "buffer");
}

fn auto_capitalize(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, _) = s.type_reported(". tieengs ");
    assert_eq!(buf, ". Tiếng ", "buffer");
}

fn em_dash(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, _) = s.type_reported("a--b");
    assert_eq!(buf, "a—b", "buffer");
}

fn double_space(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, _) = s.type_reported("b  c");
    assert_eq!(buf, "b. c", "buffer");
}

#[test]
#[ignore]
fn e2e_typing_features() {
    run_suite(CASES);
}
