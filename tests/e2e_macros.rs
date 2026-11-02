//! End-to-end cases for macro expansion: visibility in the preedit, key-case
//! capitalization and the $TIME/$DATE placeholders.

mod common;

use common::{commits, preedits, run_suite, Case, CAPS_BOTH};

const CASES: &[Case] = &[
    Case {
        name: "expansion shows in the preedit and commits on space",
        cfg: &[],
        macros: &[("gs", "giờ sáng")],
        raw: "",
        dict: "",
        f: expansion,
    },
    Case {
        name: "capitalization follows the key case",
        cfg: &[],
        macros: &[("vn", "việt nam")],
        raw: "",
        dict: "",
        f: capitalization,
    },
    Case {
        name: "$TIME/$DATE expand with the configured formats",
        cfg: &[("time_format", "\"%H\""), ("date_format", "\"%Y\"")],
        macros: &[("zznow", "d=$DATE t=$TIME")],
        raw: "",
        dict: "",
        f: time_date,
    },
];

fn expansion(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("gs");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "giờ sáng",
        "expansion must be visible in the preedit: {pre:?}"
    );
    let sigs = s.type_plain(" ");
    assert_eq!(commits(&sigs), ["giờ sáng"], "{sigs:?}");
}

fn capitalization(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("VN ");
    assert_eq!(commits(&sigs), ["VIỆT NAM"], "all-caps key: {sigs:?}");
    let sigs = s.type_plain("Vn ");
    assert_eq!(commits(&sigs), ["Việt nam"], "capitalized key: {sigs:?}");
}

fn time_date(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    // "zznow" instead of "now": the macro key must not form a Vietnamese
    // word, or bamboo composes it and the expansion never matches.
    let sigs = s.type_plain("zznow ");
    let got = commits(&sigs);
    assert_eq!(got.len(), 1, "exactly one expansion: {sigs:?}");
    // The config pins %Y / %H, so the expansion must be "d=YYYY t=HH".
    let ok = got[0].split_once(' ').is_some_and(|(d, t)| {
        let year = d.strip_prefix("d=").unwrap_or("");
        let hour = t.strip_prefix("t=").unwrap_or("");
        year.len() == 4
            && hour.len() == 2
            && year.chars().all(|c| c.is_ascii_digit())
            && hour.chars().all(|c| c.is_ascii_digit())
    });
    assert!(ok, "expansion must be d=YYYY t=HH, got {got:?}");
}

#[test]
#[ignore]
fn e2e_macros() {
    run_suite(CASES);
}
