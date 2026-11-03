//! End-to-end cases for the Preedit display mode and its preedit signal.

mod common;

use common::{
    commits, deletes, has_member, preedits, run_suite, underlines, Case, CAPS_BOTH, KEY_BACKSPACE,
    PUMP_MS,
};

const CASES: &[Case] = &[
    Case {
        name: "word commits on space with the PREEDIT_COMMIT flag",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: commit_on_space,
    },
    Case {
        name: "backspace pops the last raw key (unikey convention)",
        cfg: &[],
        macros: &[],
        raw: "",
        dict: "",
        f: backspace_unikey,
    },
    Case {
        name: "hide_underline sends the preedit without the underline attribute",
        cfg: &[("hide_underline", "true")],
        macros: &[],
        raw: "",
        dict: "",
        f: hide_underline,
    },
];

fn commit_on_space(s: &mut common::Sess) {
    let focus = s.focus(CAPS_BOTH, "gtk4-im:test");
    assert!(
        has_member(&focus, "RegisterProperties"),
        "no property menu on focus: {focus:?}"
    );
    assert!(
        has_member(&focus, "RequireSurroundingText"),
        "engine must request surrounding text: {focus:?}"
    );
    s.drain();

    let sigs = s.type_plain("vieejt");
    let pre = preedits(&sigs);
    assert!(!pre.is_empty(), "no preedit updates: {sigs:?}");
    assert!(
        pre.iter().all(|&(_, visible, mode)| visible && mode == 1),
        "preedit must be visible with PREEDIT_COMMIT: {pre:?}"
    );
    assert_eq!(pre.last().unwrap().0, "việt", "last preedit");
    assert!(
        underlines(&sigs).iter().all(|u| *u),
        "default preedit carries the underline attribute: {sigs:?}"
    );
    assert!(deletes(&sigs).is_empty(), "preedit never deletes: {sigs:?}");

    let sigs = s.type_plain(" ");
    assert_eq!(commits(&sigs), ["việt"], "space must commit the word");
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && !pre[0].1 && pre[0].2 == 0,
        "space must hide the preedit first: {pre:?}"
    );
}

fn backspace_unikey(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    s.type_plain("vieejt");
    assert!(
        s.key(KEY_BACKSPACE, 0),
        "backspace over a composition must be consumed"
    );
    s.pump(PUMP_MS);
    let sigs = s.drain();
    let pre = preedits(&sigs);
    assert!(
        !pre.is_empty() && pre.last().unwrap().0 == "việ",
        "backspace must shorten the word: {pre:?}"
    );
    assert!(
        commits(&sigs).is_empty(),
        "backspace must not commit: {sigs:?}"
    );
}

fn hide_underline(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("vieejt");
    assert!(
        !preedits(&sigs).is_empty(),
        "preedit must still be sent: {sigs:?}"
    );
    assert!(
        underlines(&sigs).iter().all(|u| !u),
        "hide_underline must drop the underline attribute: {sigs:?}"
    );
}

#[test]
#[ignore]
fn e2e_preedit() {
    run_suite(CASES);
}
