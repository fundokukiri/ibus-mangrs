//! End-to-end cases for the five modes' non-protocol shapes: OFF, the auto
//! alias, Forward Backspace (whose deletes travel as forwarded key events)
//! and Flow on a backend client.

mod common;

use common::{commits, deletes, preedits, run_suite, Case, Sig, CAPS_BOTH};

const BACKSPACE_KEYVAL: u32 = 0xff08;
const BACKSPACE_KEYCODE: u32 = 14;

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
        name: "auto mode picks preedit for a capable client",
        cfg: &[("mode", "\"auto\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: auto_capable,
    },
    Case {
        name: "auto mode picks a forward transport for an XIM client",
        cfg: &[("mode", "\"auto\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: auto_xim,
    },
    Case {
        name: "forwardkeyevent deletes with forwarded Backspace keys",
        cfg: &[("mode", "\"forwardkeyevent\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: forward_keyevent,
    },
    Case {
        name: "forwardbackspace serves the XIM client (ForwardSelect's profiles ride it)",
        cfg: &[("mode", "\"forwardbackspace\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: forward_backspace_xim,
    },
    Case {
        name: "flow serves the word on a relayed client, per key and at word end",
        cfg: &[("mode", "\"flow\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: flow_relayed,
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

fn forward_keyevent(s: &mut common::Sess) {
    // A mutter-relayed client: no /dev/uinput path and (here) no surrounding
    // report, so the protocol delete is unreachable — the mode must delete
    // with Backspace key events forwarded through IBus.
    s.focus(CAPS_BOTH, "gnome-shell");
    s.drain();
    let (buf, sigs) = s.type_buffered("tieengs ");
    assert_eq!(
        buf, "tiếng ",
        "forwarded backspaces must clear the raw tail"
    );
    assert!(
        deletes(&sigs).is_empty(),
        "no DeleteSurroundingText in ForwardKeyEvent mode: {sigs:?}"
    );
    assert!(preedits(&sigs).is_empty(), "no preedit: {sigs:?}");
    let presses: Vec<(u32, u32)> = sigs
        .iter()
        .filter_map(|sig| match sig {
            Sig::ForwardKeyEvent {
                keyval,
                keycode,
                state: 0,
            } => Some((*keyval, *keycode)),
            _ => None,
        })
        .collect();
    assert!(
        !presses.is_empty(),
        "the tone rewrite must forward Backspace presses: {sigs:?}"
    );
    assert!(
        presses.iter().all(|(k, _)| *k == BACKSPACE_KEYVAL),
        "only Backspace may be forwarded: {presses:x?}"
    );
    // ibus clients add 8 to this to get their own keycode, so it is the evdev
    // scancode (14), not the X keycode (22): 22 made every forwarded
    // Backspace type a `u` in the owner's app.
    assert!(
        presses.iter().all(|(_, code)| *code == BACKSPACE_KEYCODE),
        "the scancode must be 14, not the X keycode: {presses:x?}"
    );
}

fn flow_relayed(s: &mut common::Sess) {
    // Flow on a mutter-relayed client (the gnome-shell context): the raw
    // keys pass through unconsumed, and both the per-key display and the
    // word-end erase ride the injection backend — invisible to this
    // harness, which sees neither DeleteSurroundingText nor forwarded key
    // events. What the harness does see is the per-key display's commits:
    // the word grows step by step, and the mark step lands the accented
    // tail, with the terminator closing the word.
    s.focus(CAPS_BOTH, "gnome-shell");
    s.drain();
    let (_buf, sigs) = s.type_buffered("tieengs ");
    assert!(
        deletes(&sigs).is_empty(),
        "no DeleteSurroundingText in Flow mode: {sigs:?}"
    );
    assert!(preedits(&sigs).is_empty(), "no preedit: {sigs:?}");
    let committed = commits(&sigs);
    assert!(
        !committed.is_empty(),
        "the per-key display commits the word as it grows: {sigs:?}"
    );
    assert!(
        committed.contains(&"ếng"),
        "the mark step must land the accented tail: {sigs:?}"
    );
}

fn forward_backspace_xim(s: &mut common::Sess) {
    // A XIM client: the input context reports no capabilities at all, so no
    // protocol delete and no preedit is possible there. ForwardSelect's old
    // profiles ride ForwardBackspace since the merge: the tail rewrite
    // erases with forwarded Backspace keys — the Shift+Left selection
    // transport is gone — and the rewrite commits land behind them.
    s.focus(CAPS_BOTH, "xim");
    s.drain();
    let sigs = s.type_plain("tieengs");
    assert!(
        deletes(&sigs).is_empty(),
        "no DeleteSurroundingText in ForwardBackspace mode: {sigs:?}"
    );
    assert!(preedits(&sigs).is_empty(), "no preedit: {sigs:?}");
    let presses: Vec<(u32, u32)> = sigs
        .iter()
        .filter_map(|sig| match sig {
            Sig::ForwardKeyEvent {
                keyval,
                keycode,
                state: 0,
            } => Some((*keyval, *keycode)),
            _ => None,
        })
        .collect();
    assert!(
        !presses.is_empty(),
        "the tone rewrite must forward Backspace presses: {sigs:?}"
    );
    assert!(
        presses.iter().all(|(k, _)| *k == BACKSPACE_KEYVAL),
        "only Backspace may be forwarded — the Shift+Left transport is gone: {presses:x?}"
    );
    // ibus clients add 8 to this to get their own keycode, so it is the evdev
    // scancode (14), not the X keycode (22).
    assert!(
        presses.iter().all(|(_, code)| *code == BACKSPACE_KEYCODE),
        "the scancode must be the evdev one: {presses:x?}"
    );
    // The rewrite commits land within the per-key pumps — deferred by the
    // pacing window (Engine::defer_commit_timed, the engine log shows
    // "defer commit 30ms" then the timed flush), but they must land.
    let all_commits = commits(&sigs);
    assert!(
        all_commits.contains(&"ê") && all_commits.contains(&"ếng"),
        "the rewrite commits land: {sigs:?}"
    );
}

fn auto_capable(s: &mut common::Sess) {
    // A client that renders preedit and reports surrounding text: the
    // ladder's first rung — the everyday preedit path.
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let sigs = s.type_plain("tieengs ");
    assert_eq!(commits(&sigs), ["tiếng"], "{sigs:?}");
    assert!(
        !preedits(&sigs).is_empty(),
        "capable client under auto must compose as preedit: {sigs:?}"
    );
}

fn auto_xim(s: &mut common::Sess) {
    // A real XIM context reports no capabilities at all (caps 0): the
    // terminal Forward Backspace transport, not the ladder's inert fallback.
    s.focus(0, "xim");
    s.drain();
    let sigs = s.type_plain("tieengs");
    assert!(
        sigs.iter()
            .any(|s| matches!(s, Sig::ForwardKeyEvent { .. })),
        "xim under auto must resolve to Forward Backspace: {sigs:?}"
    );
}

#[test]
#[ignore]
fn e2e_modes() {
    run_suite(CASES);
}
