//! End-to-end cases for the non-protocol modes: OFF, Default Typing and
//! ForwardKeyEvent (whose deletes travel as forwarded key events).

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
        name: "default typing is fully inert",
        cfg: &[("mode", "\"defaulttyping\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: default_typing,
    },
    Case {
        name: "forwardkeyevent deletes with forwarded Backspace keys",
        cfg: &[("mode", "\"forwardkeyevent\"")],
        macros: &[],
        raw: "",
        dict: "",
        f: forward_keyevent,
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

fn default_typing(s: &mut common::Sess) {
    s.focus(CAPS_BOTH, "gtk4-im:test");
    s.drain();
    let (buf, sigs) = s.type_buffered("x1 ");
    assert_eq!(buf, "x1 ", "everything forwarded");
    assert!(
        commits(&sigs).is_empty() && preedits(&sigs).is_empty() && deletes(&sigs).is_empty(),
        "Default Typing must emit no signals at all: {sigs:?}"
    );
}

#[test]
#[ignore]
fn e2e_modes() {
    run_suite(CASES);
}
