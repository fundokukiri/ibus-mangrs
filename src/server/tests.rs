use super::hotkey::*;
use super::x11::*;
use super::*;
use crate::bamboo_wrapper::Method;
use crate::engine::Mode;
use dbus::arg::{RefArg, Variant};

#[test]
fn parse_hotkey_accepts_modifier_combinations() {
    let (mods, key) = parse_hotkey("Control+Alt+v").expect("hotkey");
    assert_eq!(key, 0x76); // 'v'
    assert_eq!(mods, keymap::CONTROL_MASK | keymap::MOD1_MASK);

    let (mods, key) = parse_hotkey("Ctrl+Shift+space").expect("hotkey");
    assert_eq!(key, 0x20);
    assert_eq!(mods, keymap::CONTROL_MASK | keymap::SHIFT_MASK);

    let (mods, key) = parse_hotkey("Super+F5").expect("hotkey");
    assert_eq!(key, 0xffbe + 4);
    assert_eq!(mods, keymap::MOD4_MASK);
}

#[test]
fn parse_hotkey_rejects_dangerous_or_bad_specs() {
    // A bare letter would swallow normal typing.
    assert_eq!(parse_hotkey("v"), None);
    // Shift-only too: capitals go through Shift.
    assert_eq!(parse_hotkey("Shift+v"), None);
    assert_eq!(parse_hotkey(""), None);
    assert_eq!(parse_hotkey("NotAModifier+q"), None);
    // A lone modifier would eat every press of that key.
    assert_eq!(parse_hotkey("Control"), None);
    assert_eq!(parse_hotkey("Shift"), None);
}

#[test]
fn parse_hotkey_accepts_modifier_only_combos() {
    use crate::keymap::{CONTROL_MASK, MOD1_MASK, SHIFT_MASK};
    let (mods, key) = parse_hotkey("Control+Shift").expect("hotkey");
    assert_eq!(mods, CONTROL_MASK | SHIFT_MASK);
    assert_eq!(
        key, 0,
        "no regular key: matching goes by the pressed modifier"
    );
    let (mods, key) = parse_hotkey("Control+Alt").expect("hotkey");
    assert_eq!(mods, CONTROL_MASK | MOD1_MASK);
    assert_eq!(key, 0);
}

#[test]
fn hotkey_matches_modifier_only_combos() {
    use crate::keymap::{CONTROL_MASK, MOD1_MASK, MOD4_MASK, SHIFT_MASK};
    // "Control+Shift": completed by pressing either Shift while Control
    // is held or Control while Shift is held — either side, either order.
    let (mods, key) = (CONTROL_MASK | SHIFT_MASK, 0);
    assert!(hotkey_matches(mods, key, 0xffe2, CONTROL_MASK)); // right Shift
    assert!(hotkey_matches(mods, key, 0xffe3, SHIFT_MASK)); // left Control
                                                            // The first modifier alone does not trigger.
    assert!(!hotkey_matches(mods, key, 0xffe1, 0));
    // A non-modifier key never matches a modifier-only combo.
    assert!(!hotkey_matches(mods, key, 0x76, CONTROL_MASK));
    // Regular combos keep matching by keyval + held modifiers.
    let (mods, key) = (CONTROL_MASK | MOD1_MASK | MOD4_MASK, 0x76);
    assert!(hotkey_matches(mods, key, 0x76, mods));
    assert!(!hotkey_matches(mods, key, 0x76, CONTROL_MASK));
}

#[test]
fn hotkey_matches_letters_case_insensitively() {
    use crate::keymap::{CONTROL_MASK, SHIFT_MASK};
    // "Shift+Control+Z": with Shift held the event carries the
    // uppercase keysym 0x5a while the config stores 'z'.
    let (mods, key) = (CONTROL_MASK | SHIFT_MASK, 0x7a);
    assert!(hotkey_matches(mods, key, 0x5a, mods));
    assert!(hotkey_matches(mods, key, 0x7a, mods));
    assert!(!hotkey_matches(mods, key, 0x78, mods));
}

/// Build an `org.freedesktop.IBus.Engine` method-call message for `member`.
fn engine_msg(member: &str) -> Message {
    // Test-only unwraps: these names are constants from this file.
    use dbus::strings::{BusName, Interface, Member, Path};
    // libdbus asserts on method_return() of a serial-less message.
    let mut msg = Message::method_call(
        &BusName::new(BUS_NAME).unwrap(),
        &Path::new(ENGINE_PATH).unwrap(),
        &Interface::new(ENGINE_IFACE).unwrap(),
        &Member::new(member).unwrap(),
    );
    msg.set_serial(1);
    msg
}

#[test]
fn focus_out_keeps_the_engine_enabled_only_disable_turns_it_off() {
    // Regression: FocusOut used to call set_enabled(false), and focus
    // events get coalesced (a FocusOut whose FocusIn never arrives) —
    // every later key then came back PassThrough: raw Latin typing until
    // the engine was toggled or restarted. Only `Disable` (the panel
    // toggle) may switch the wrapper off.
    let engine: SharedEngine = Arc::new(Mutex::new(Engine::new(Method::Telex, Mode::Preedit)));
    // The daemon sends SetCapabilities before any key; without
    // CAP_PREEDIT_TEXT the engine resolves Preedit down to NonPreedit.
    lock_engine(&engine).set_capabilities(1);
    let mut handler = EngineHandler {
        engine: Arc::clone(&engine),
        // Fresh clock: maybe_reload_config stays inside its 1s gate, so
        // the test never touches the real config file.
        last_config_check: Instant::now(),
        last_config_mtime: None,
        hotkeys: Vec::new(),
        last_fire: Vec::new(),
        picker: None,
        emoji_picker: None,
        focus_ext: None,
    };
    let press = |handler: &mut EngineHandler, keyval: u32| {
        let msg = engine_msg("ProcessKeyEvent").append3(keyval, 0u32, 0u32);
        let mut out = Vec::new();
        let reply = handler.process_key_call(&msg, &mut out);
        let consumed = reply.read1::<bool>().expect("bool reply");
        (consumed, out)
    };

    let (consumed, out) = press(&mut handler, 116); // 't'
    assert!(
        consumed
            && out
                .iter()
                .any(|m| &*m.member().unwrap() == "UpdatePreeditText")
    );

    // The context loses focus and no FocusIn ever comes back.
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("FocusOut"), "FocusOut", &mut out);
    assert!(
        lock_engine(&engine).is_enabled(),
        "FocusOut must not disable the wrapper"
    );
    let (consumed, out) = press(&mut handler, 117); // 'u'
    assert!(
        consumed
            && out
                .iter()
                .any(|m| &*m.member().unwrap() == "UpdatePreeditText"),
        "keys after a FocusOut without FocusIn must still compose"
    );

    // The panel toggle still disables, and keys then pass through.
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("Disable"), "Disable", &mut out);
    assert!(!lock_engine(&engine).is_enabled());
    let (consumed, _) = press(&mut handler, 110); // 'n'
    assert!(!consumed, "a disabled engine forwards keys to the app");
}

#[test]
fn poisoned_engine_mutex_still_locks() {
    // A panic while holding the engine mutex poisons it; unwrapping
    // would then panic on every later key event (typing dead until IBus
    // restarts). lock_engine must recover instead.
    let engine: SharedEngine = Arc::new(Mutex::new(Engine::new(Method::Telex, Mode::Preedit)));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = lock_engine(&engine);
        panic!("poison the mutex");
    }));
    let mut engine = lock_engine(&engine);
    engine.set_capabilities(1); // any mutation proves the guard works
}

fn emitted(actions: &[Action]) -> Vec<Message> {
    // An engine with surrounding data + capability uses the protocol
    // delete; anything else falls back to key injection (no D-Bus signal).
    let mut e = crate::engine::Engine::new(
        crate::bamboo_wrapper::Method::Telex,
        crate::engine::Mode::NonPreedit,
    );
    e.set_capabilities(32); // CAP_SURROUNDING_TEXT
    e.set_surrounding("word".into(), 4);
    let mut out = Vec::new();
    emit(&mut e, actions, &mut out);
    out
}

#[test]
fn commit_text_is_one_ibus_text_variant() {
    // Regression: the old helper wrapped this variant in a second one, so
    // the daemon received a variant instead of an IBusText struct.
    let msgs = emitted(&[Action::Commit("tiếng".into())]);
    assert_eq!(&*msgs[0].member().unwrap(), "CommitText");
    let text = msgs[0]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("IBusText variant");
    assert_eq!(text.0.signature().to_string(), "(sa{sv}sv)");
}

#[test]
fn preedit_signal_is_vubu() {
    // Regression: the old helper produced (vvvv); ibus-daemon dropped it,
    // so preedit text only appeared when the word was committed.
    let msgs = emitted(&[Action::UpdatePreedit {
        text: "tiếng".into(),
        cursor: 5,
    }]);
    assert_eq!(&*msgs[0].member().unwrap(), "UpdatePreeditText");
    let (text, cursor, visible, mode) = msgs[0]
        .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
        .expect("(vubu) arguments");
    assert_eq!(text.0.signature().to_string(), "(sa{sv}sv)");
    // 1 = IBUS_ENGINE_PREEDIT_COMMIT: when the client resets the input
    // context (mouse click, focus change) it commits this preedit itself
    // instead of clearing it, so a pending word is never lost.
    assert_eq!((cursor, visible, mode), (5, true, 1));
}

#[test]
fn hide_preedit_reuses_the_preedit_signal() {
    let msgs = emitted(&[Action::HidePreedit]);
    assert_eq!(&*msgs[0].member().unwrap(), "UpdatePreeditText");
    let (_, cursor, visible, mode) = msgs[0]
        .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
        .expect("(vubu) arguments");
    // 0 = IBUS_ENGINE_PREEDIT_CLEAR: hiding must never commit the word.
    assert_eq!((cursor, visible, mode), (0, false, 0));
}

#[test]
fn hide_underline_drops_the_underline_attribute() {
    let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
    e.apply_config(&Config {
        hide_underline: true,
        ..Config::default()
    });
    let mut out = Vec::new();
    emit(
        &mut e,
        &[Action::UpdatePreedit {
            text: "tiếng".into(),
            cursor: 5,
        }],
        &mut out,
    );
    let (text, ..) = out[0]
        .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
        .expect("(vubu) arguments");
    // No IBusAttribute inside the attr list.
    let rendered = format!("{:?}", text.0);
    assert!(!rendered.contains("IBusAttribute"), "{rendered}");
}

#[test]
fn delete_surrounding_text_is_plain_int_uint() {
    let msgs = emitted(&[Action::DeleteBefore(3)]);
    assert_eq!(msgs[0].read2::<i32, u32>().unwrap(), (-3, 3));
}

#[test]
fn no_surrounding_data_routes_deletes_to_injection() {
    // A client without surrounding data must never see a protocol
    // DeleteSurroundingText (mutter segfaults on contexts whose buffer it
    // never saw — the search-bar logout); uinput is the delete transport.
    let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(32); // CAP_SURROUNDING_TEXT, but no data reported
    e.set_uinput_available(true);
    let mut out = Vec::new();
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    let injected = emit_with(
        &mut e,
        &[
            Action::DeleteBefore(2),
            Action::DeleteSurrounding { offset: -3, n: 1 },
        ],
        &mut out,
        &t,
    );
    assert_eq!(injected, 5, "2 + |offset| Backspaces");
    assert!(
        out.iter().all(|m| m
            .member()
            .map(|mem| mem != "DeleteSurroundingText")
            .unwrap_or(true)),
        "{:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn unknown_client_routes_deletes_to_injection() {
    // Sublime Text / JetBrains XToolkit never send FocusInId. Their
    // injected keys do arrive, so a client-less context uses the same
    // Backspace transport instead of raw-forwarding the tone keys.
    let mut e = Engine::new(
        crate::bamboo_wrapper::Method::Telex,
        crate::engine::Mode::NonPreedit,
    );
    e.set_capabilities(1);
    e.set_uinput_available(true);
    let mut out = Vec::new();
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    let injected = emit_with(&mut e, &[Action::DeleteBefore(2)], &mut out, &t);
    assert_eq!(injected, 2);
    assert!(
        out.iter().all(|m| m
            .member()
            .map(|mem| mem != "DeleteSurroundingText")
            .unwrap_or(true)),
        "{:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn commit_after_injection_waits_for_backspace_echoes() {
    // Regression: a synchronous CommitText after injected Backspaces
    // outruns the compositor round trip — the echo then deletes the
    // freshly committed word ("accent flashes and reverts"). The commit
    // must be deferred until the last echo is forwarded.
    let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true); // no surrounding data → injection
    let mut out = Vec::new();
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    let injected = emit_with(
        &mut e,
        &[Action::DeleteBefore(1), Action::Commit("ế".into())],
        &mut out,
        &t,
    );
    e.add_injected(injected);
    assert_eq!(injected, 1);
    assert!(
        !out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "commit must be deferred, got {:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
    assert!(e.has_deferred());
    // The single Backspace echo is forwarded → the commit is ready.
    assert!(e.take_injected_echo());
    let ready = e.note_echo_forwarded();
    assert_eq!(ready, vec!["ế".to_string()]);
    assert!(!e.has_deferred());
}

#[test]
fn commit_after_selection_waits_for_nav_echoes() {
    // The Select rewrite injects Shift+Left presses and a Delete; a
    // synchronous commit would land before the app erased the selection.
    // The commit must wait until every injected nav-key echo has been
    // forwarded, then flush.
    let mut e = Engine::new(Method::Telex, Mode::Select);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    e.set_surrounding(String::new(), 0);
    let mut out = Vec::new();
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    emit_with(
        &mut e,
        &[
            Action::SelectLeft(3),
            Action::PressDelete,
            Action::Commit("việt".into()),
        ],
        &mut out,
        &t,
    );
    assert!(
        !out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "commit must be deferred, got {:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
    assert!(e.has_deferred());
    assert!(e.pending_nav_echoes());
    // The three Left echoes come back, then the Delete echo: the commit
    // is only ready after the last one.
    for _ in 0..3 {
        assert!(e.take_nav_echo(keymap::LEFT).unwrap().is_empty());
    }
    assert!(e.take_nav_echo(keymap::DELETE).unwrap() == vec!["việt".to_string()]);
    assert!(!e.has_deferred());
    assert!(!e.pending_nav_echoes());
}

#[test]
fn active_window_id_and_class_parse_xprop_output() {
    assert_eq!(
        active_window_id("_NET_ACTIVE_WINDOW(WINDOW): window id # 0x2c00007\n"),
        Some("0x2c00007".to_string())
    );
    assert_eq!(active_window_id("no id here\n"), None);
    assert_eq!(
        window_class("WM_CLASS(STRING) = \"code\", \"Code\"\n"),
        Some("Code".to_string())
    );
    // A single-string value stands in for the class.
    assert_eq!(
        window_class("WM_CLASS(STRING) = \"foo\"\n"),
        Some("foo".to_string())
    );
    assert_eq!(window_class("WM_CLASS: not found."), None);
}

#[test]
fn registry_uses_the_newest_file() {
    // Several registry files (display change, a stale one) must not send
    // the engine to a dead socket: the daemon rewrites its file on start.
    // Checked in both directions so directory order cannot make it pass.
    let dir = std::env::temp_dir().join(format!(
        "ibus-mangrs-registry-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a");
    let b = dir.join("b");
    std::fs::write(&a, "IBUS_ADDRESS=unix:path=a\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&b, "IBUS_ADDRESS=unix:path=b\n").unwrap();
    // Newest, but it carries no address: must be skipped.
    std::fs::write(dir.join("junk"), "no address here\n").unwrap();
    assert_eq!(address_from_dir(&dir).as_deref(), Some("unix:path=b"));
    std::thread::sleep(std::time::Duration::from_millis(20));
    // Rewriting a makes it the newest, the other way round.
    std::fs::write(&a, "IBUS_ADDRESS=unix:path=a\n").unwrap();
    assert_eq!(address_from_dir(&dir).as_deref(), Some("unix:path=a"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_press_delete_falls_back_to_backspace() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static PRESSES: AtomicUsize = AtomicUsize::new(0);
    PRESSES.store(0, Ordering::SeqCst);
    let mut e = Engine::new(Method::Telex, Mode::Select);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut out = Vec::new();
    let t = Transports {
        backspaces: &|n, _| {
            PRESSES.fetch_add(n as usize, Ordering::SeqCst);
            true
        },
        keys: &|_, _| false,
        select_left: &|_, _| true,
        paste: &|_| false,
    };
    // No SelectLeft: the only pending injection is the fallback key.
    emit_with(
        &mut e,
        &[Action::PressDelete, Action::Commit("việt".into())],
        &mut out,
        &t,
    );
    assert_eq!(PRESSES.load(Ordering::SeqCst), 1, "one backspace");
    assert!(
        e.has_deferred(),
        "the commit must wait for the fallback key"
    );
}

#[test]
fn paste_failure_falls_back_to_commit() {
    // Without a clipboard tool the Paste transport fails; the word must
    // still reach the application (CommitText fallback).
    let mut e = Engine::new(Method::Telex, Mode::Smooth);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut out = Vec::new();
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    emit_with(&mut e, &[Action::Paste("việt".into())], &mut out, &t);
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "{:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn paste_fallback_after_injected_backspaces_is_deferred() {
    // The smooth rewrite erases the raw keys with injected Backspaces,
    // then the paste fails. The fallback commit must not overtake the
    // echoes, or the app deletes the freshly committed word.
    let mut e = Engine::new(Method::Telex, Mode::Smooth);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut out = Vec::new();
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    emit_with(
        &mut e,
        &[Action::InjectBackspaces(6), Action::Paste("việt".into())],
        &mut out,
        &t,
    );
    assert!(
        !out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the fallback commit must wait for the Backspace echoes"
    );
    assert!(e.has_deferred());
}

#[test]
fn forward_key_event_emits_backspace_press_and_release() {
    // The wire contract every ibus client expects: clients add 8 to the
    // keycode themselves, so it must be the evdev scancode 14 — sending
    // the X keycode 22 made them use evdev KEY_U and type a `u` per
    // forwarded Backspace.
    let mut e = Engine::new(Method::Telex, Mode::ForwardKeyEvent);
    e.set_client(Some("gnome-shell".into()));
    let mut out = Vec::new();
    emit_with(
        &mut e,
        &[Action::ForwardBackspaces(2)],
        &mut out,
        &fake_transports(&|_, _| true),
    );
    let keys: Vec<(u32, u32, u32)> = out
        .iter()
        .filter(|m| m.member().map(|x| x == "ForwardKeyEvent").unwrap_or(false))
        .filter_map(|m| m.read3::<u32, u32, u32>().ok())
        .collect();
    assert_eq!(keys.len(), 4, "press and release per Backspace: {keys:?}");
    assert_eq!(
        keys.iter().filter(|(_, _, state)| *state == 0).count(),
        2,
        "two presses: {keys:?}"
    );
    assert!(
        keys.iter()
            .all(|(keyval, keycode, _)| *keyval == 0xff08 && *keycode == 14),
        "Backspace keyval with the evdev scancode: {keys:?}"
    );
}

#[test]
fn x11_delete_opt_in_reaches_the_wire_without_a_report() {
    // The emit guard follows the engine's own rule: with the opt-in on, a
    // direct client that advertises surrounding text gets the protocol
    // delete even though it never reported its buffer. Without the switch
    // the same action is dropped (raw keystrokes instead).
    for (enabled, want_delete) in [(false, false), (true, true)] {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            x11_delete_without_report: enabled,
            ..Config::default()
        });
        e.set_mode(Mode::NonPreedit);
        e.set_client(Some("gtk3-im:sublime_text".into()));
        e.set_capabilities(1 << 5); // IBUS_CAP_SURROUNDING_TEXT
        let mut out = Vec::new();
        emit_with(
            &mut e,
            &[Action::DeleteBefore(2)],
            &mut out,
            &fake_transports(&|_, _| false),
        );
        let deleted = out.iter().any(|m| {
            m.member()
                .map(|x| x == "DeleteSurroundingText")
                .unwrap_or(false)
        });
        assert_eq!(deleted, want_delete, "switch={enabled}: {out:?}");
    }
}

#[test]
fn forward_select_selects_then_erases() {
    // ForwardKeyEvent II's wire shape: Left with the shift held on the
    // press, plain release (scancode 105; clients add 8 themselves), then
    // one Backspace that erases the selection the Left keys made — the
    // commit alone does not replace an application-side selection.
    let mut e = Engine::new(Method::Telex, Mode::ForwardSelect);
    e.set_client(Some("xim".into()));
    let mut out = Vec::new();
    emit_with(
        &mut e,
        &[Action::ForwardSelectLeft(2)],
        &mut out,
        &fake_transports(&|_, _| true),
    );
    let keys: Vec<(u32, u32, u32)> = out
        .iter()
        .filter(|m| m.member().map(|x| x == "ForwardKeyEvent").unwrap_or(false))
        .filter_map(|m| m.read3::<u32, u32, u32>().ok())
        .collect();
    assert_eq!(keys.len(), 6, "two Lefts plus the erase: {keys:?}");
    let lefts: Vec<_> = keys
        .iter()
        .filter(|(keyval, _, _)| *keyval == 0xff51)
        .collect();
    assert_eq!(lefts.len(), 4, "press and release per Left: {keys:?}");
    assert!(
        lefts.iter().all(|(_, keycode, _)| *keycode == 105),
        "Left keyval with its scancode: {keys:?}"
    );
    assert_eq!(
        keys.iter()
            .filter(|(keyval, _, _)| *keyval == 0xff08)
            .count(),
        2,
        "one Backspace press and release erases the selection: {keys:?}"
    );
    assert_eq!(
        keys.iter()
            .filter(|(_, _, state)| *state == keymap::SHIFT_MASK)
            .count(),
        2,
        "only the Left presses carry the shift: {keys:?}"
    );
    assert_eq!(
        keys.iter()
            .filter(|(_, _, state)| *state == keymap::RELEASE_MASK)
            .count(),
        3,
        "three plain releases (two Lefts, one Backspace): {keys:?}"
    );
}

#[test]
fn forced_flush_delivers_lost_echo_commits() {
    let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
    e.defer_commit(2, "word".into());
    assert!(e.has_deferred());
    // One echo arrives, one is lost: a focus change forces delivery.
    e.note_echo_forwarded();
    assert!(e.has_deferred(), "threshold not reached yet");
    assert_eq!(e.flush_deferred(), vec!["word".to_string()]);
    assert!(!e.has_deferred());
}

/// An engine proven (report proof) to talk to a delete-swallowing client.
#[cfg(test)]
fn delete_dropped_engine() -> Engine {
    let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(32); // CAP_SURROUNDING_TEXT
    e.set_uinput_available(true);
    e.set_surrounding("a".into(), 1);
    let _ = e.process_char('a'); // buffer "a"
                                 // The second `a` is a rewrite: delete 1 + commit "â" — this arms
                                 // the delete-delivery proof.
    let _ = e.process_char('a');
    e.set_surrounding("aâ".into(), 2); // swallowed shape → delete_dropped
    assert!(e.delete_dropped());
    e
}

#[test]
fn dead_device_on_delete_dropping_client_keeps_raw() {
    // The uinput device died mid-session on a client already proven to
    // swallow protocol deletes: the protocol fallback lands nowhere and
    // the paired commit would duplicate the raw keys ("aê"). Both are
    // suppressed, and the engine is told the device is gone so the
    // following words resolve to preedit.
    let mut e = delete_dropped_engine();
    let fail = |_: u32, _: u64| false;
    let t = fake_transports(&fail);
    let mut out = Vec::new();
    emit_with(
        &mut e,
        &[Action::DeleteBefore(1), Action::Commit("ê".into())],
        &mut out,
        &t,
    );
    assert!(
        out.is_empty(),
        "no protocol delete, no paired commit: {:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
    assert!(!e.uinput_available(), "device death must reach the engine");
}

#[test]
fn dead_device_suppresses_rewrite_by_backspace_on_swallowers() {
    // Same as above for RewriteByBackspace: its commit rides inside the
    // action, so a failed injection must not fall back to the protocol
    // delete + commit pair the client drops anyway.
    let mut e = delete_dropped_engine();
    let fail = |_: u32, _: u64| false;
    let t = fake_transports(&fail);
    let mut out = Vec::new();
    emit_with(
        &mut e,
        &[Action::RewriteByBackspace {
            backspaces: 2,
            commit: "ê".into(),
        }],
        &mut out,
        &t,
    );
    assert!(out.is_empty(), "no protocol fallback pair: {out:?}");
}

#[test]
fn inject_failure_keeps_the_protocol_fallback_for_healthy_clients() {
    // A client that has never swallowed a delete honors the protocol:
    // when injection fails, the protocol delete is the correct fallback.
    let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(32);
    e.set_uinput_available(true);
    e.set_surrounding("abc".into(), 3);
    let fail = |_: u32, _: u64| false;
    let t = fake_transports(&fail);
    let mut out = Vec::new();
    emit_with(&mut e, &[Action::DeleteBefore(1)], &mut out, &t);
    assert!(
        out.iter().any(|m| m
            .member()
            .map(|m| m == "DeleteSurroundingText")
            .unwrap_or(false)),
        "protocol fallback must stay for healthy clients"
    );
    assert!(
        e.uinput_available(),
        "healthy clients keep injection marked"
    );
}

#[test]
fn first_navigation_key_with_pending_preedit_is_swallowed_on_relays() {
    // The preedit commit and the forwarded key travel different
    // transports on mutter-relayed contexts; a burst of arrows used to
    // move the caret before the commit landed ("em tên tùng" → "etùngm
    // tên"). The first navigation key is consumed so the commit lands
    // while the caret is still in place.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0b101001);
    for c in "tung".chars() {
        process_key(&mut e, c as u32, 0, &mut Vec::new());
    }
    let mut out = Vec::new();
    assert!(
        process_key(&mut e, 0xff51, 0, &mut out),
        "Left must be consumed"
    );
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the pending word must be committed: {:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
    // The composition is gone: the next arrow is forwarded normally.
    let mut out = Vec::new();
    assert!(!process_key(&mut e, 0xff51, 0, &mut out));
}

#[test]
fn first_navigation_key_commits_preedit_even_after_a_capability_flip() {
    // The daemon flips capabilities mid-word. The preedit is already on
    // screen (word-mode lock), so a navigation key must still commit it
    // and swallow that one press to keep it from landing at the moved
    // caret.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0b100001); // PREEDIT|SURROUNDING
    for c in "tung".chars() {
        process_key(&mut e, c as u32, 0, &mut Vec::new());
    }
    e.set_capabilities(0b100000); // preedit bit gone, preedit still visible
    let mut out = Vec::new();
    assert!(process_key(&mut e, 0xff51, keymap::SHIFT_MASK, &mut out));
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the visible preedit must be committed: {:?}",
        out.iter()
            .map(|m| m.member().map(|m| m.to_string()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn navigation_key_forwards_normally_without_pending_preedit() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0b101001);
    let mut out = Vec::new();
    assert!(!process_key(&mut e, 0xff51, 0, &mut out));
}

#[test]
fn in_process_clients_keep_navigation_keys_forwarded() {
    // Only relayed contexts need the swallow: in-process IM clients
    // apply the commit and the key in order themselves.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(0b101001);
    for c in "tung".chars() {
        process_key(&mut e, c as u32, 0, &mut Vec::new());
    }
    let mut out = Vec::new();
    assert!(!process_key(&mut e, 0xff51, 0, &mut out));
}

#[test]
fn hotkey_registry_maps_assigned_specs_in_page_order() {
    let mut config = Config {
        enabled_hotkey: "Control+Alt+h".into(),
        cycle_mode_hotkey: "Control+Alt+v".into(),
        app_rule_hotkey: "Control+Alt+p".into(),
        ..Config::default()
    };
    let reg = hotkey_registry(&config);
    assert_eq!(reg.len(), 3);
    assert_eq!(reg[0].2, HotkeyAction::ToggleEnabled);
    assert_eq!(reg[0].0, keymap::CONTROL_MASK | keymap::MOD1_MASK);
    assert_eq!(reg[0].1, 0x68); // 'h'
    assert_eq!(reg[1].2, HotkeyAction::CycleMode);
    assert_eq!(reg[2].2, HotkeyAction::AppRulePicker);
    // A same-combo collision keeps every entry; the first match wins.
    config.cycle_mode_hotkey = "Control+Alt+h".into();
    let reg = hotkey_registry(&config);
    assert_eq!(
        reg.len(),
        3,
        "the registry does not dedupe - page order rules"
    );
    // Unassigned or unusable specs register nothing.
    assert!(hotkey_registry(&Config::default()).is_empty());
    config.enabled_hotkey = "Control".into();
    config.cycle_mode_hotkey = "Shift+x".into();
    config.app_rule_hotkey = String::new();
    assert!(
        hotkey_registry(&config).is_empty(),
        "parse_hotkey rejects these"
    );
}

#[test]
fn picker_keys_map_onto_entries() {
    use super::picker::{picker_entries, PickerKey, PickerState};
    let len = picker_entries(ModeConfig::Preedit).len();
    assert_eq!(len, ibus_prop::MODES.len(), "ten modes, nothing else");
    let state = PickerState::new("e2epick".into());
    // Digits 1..9 pick entries 1..9, 0 picks the tenth; Enter picks the
    // first. There is no navigation: arrows and every other key close.
    assert_eq!(state.on_key(0x31), PickerKey::Select(0));
    assert_eq!(state.on_key(0x39), PickerKey::Select(8));
    assert_eq!(state.on_key(0x30), PickerKey::Select(9));
    assert_eq!(state.on_key(65293), PickerKey::Select(0));
    assert_eq!(state.on_key(65307), PickerKey::Cancel);
    assert_eq!(state.on_key(65364), PickerKey::Ignore); // Down
    assert_eq!(state.on_key(0x76), PickerKey::Ignore);
    // The current mode's entry carries the dot marker.
    let entries = picker_entries(ModeConfig::Preedit);
    assert!(
        entries[2].starts_with("● "),
        "preedit is the default: {}",
        entries[2]
    );
    assert!(!entries[0].starts_with("● "));
}

#[test]
fn lookup_table_variant_has_the_wire_signature() {
    // Captured from libibus 1.5.34's own serialization (GI): the candidate
    // popup refuses to render anything else.
    let table = ibus_prop::lookup_table_variant(9, 2, &["a".into(), "b".into()]);
    assert_eq!(table.0.signature(), "(sa{sv}uubbiavav)");
}

// --- emoji picker state machine ---

/// Twelve fake rows: enough for two pages, blobs fold diacritics away.
fn emoji_rows() -> Vec<crate::emoji::SearchRow> {
    const BASE: [(&str, &str, &str); 12] = [
        ("smile", "😄", "vui cuoi face"),
        ("fire", "🔥", "lua hot"),
        ("turtle", "🐢", "rua"),
        ("cat", "🐱", "meo"),
        ("dog", "🐶", "cho puppy"),
        ("rocket", "🚀", "ten lua"),
        ("heart", "❤️", "tim yeu"),
        ("star", "⭐", "ngoi sao"),
        ("pizza", "🍕", "banh"),
        ("coffee", "☕", "ca phe"),
        ("moon", "🌙", "mat trang"),
        ("sun", "☀️", "mat troi"),
    ];
    BASE.into_iter()
        .map(|(name, glyph, keywords)| crate::emoji::SearchRow {
            name: name.into(),
            glyph: glyph.into(),
            blob: crate::emoji::fold_ascii(&format!("{name} {keywords}")),
        })
        .collect()
}

#[test]
fn emoji_menu_opens_on_the_recents_page() {
    use super::emoji_picker::EmojiPickerState;
    // No recents: the table head leads (the common smileys).
    let empty = EmojiPickerState::with(emoji_rows(), Vec::new());
    let labels = empty.page_labels();
    assert_eq!(labels.len(), 10, "one flat page of ten");
    assert!(labels[0].starts_with("😄"), "{labels:?}");
    // With recents: the picks lead, newest first.
    let state = EmojiPickerState::with(
        emoji_rows(),
        vec![("fire".into(), "🔥".into()), ("smile".into(), "😄".into())],
    );
    let labels = state.page_labels();
    assert_eq!(labels[0], "🔥 fire", "{labels:?}");
    assert_eq!(labels[1], "😄 smile", "{labels:?}");
}

#[test]
fn emoji_menu_search_folds_diacritics() {
    use super::emoji_picker::EmojiPickerState;
    let mut state = EmojiPickerState::with(emoji_rows(), Vec::new());
    for c in "lua".chars() {
        state.query.push(c);
    }
    state.page = 0;
    let labels = state.page_labels();
    // fire ("lua hot") and rocket ("ten lua") both carry the word.
    assert_eq!(
        labels,
        vec!["🔥 fire".to_string(), "🚀 rocket".to_string()],
        "lua finds both lua rows"
    );
    // The query keeps what the user typed for the auxiliary line.
    assert!(state.auxiliary().contains("\"lua\""));
}

#[test]
fn emoji_menu_pages_and_picks() {
    use super::emoji_picker::EmojiPickerState;
    let mut state = EmojiPickerState::with(emoji_rows(), Vec::new());
    assert_eq!(state.page_labels().len(), 10);
    assert_eq!(
        state.pick(9).map(|(_, g)| g),
        Some("☕".into()),
        "0 picks the tenth entry"
    );
    assert_eq!(state.pick(10), None, "only ten per page");
    state.page_down();
    assert_eq!(state.page_labels().len(), 2, "twelve rows, second page two");
    assert_eq!(state.pick(0).map(|(_, g)| g), Some("🌙".into()));
    state.page_down();
    assert_eq!(state.page_labels().len(), 2, "the last page stays");
    state.page_up();
    state.page_up();
    state.page_up();
    assert!(state.page_labels().len() == 10, "back at the first page");
    // A fresh query resets the page.
    state.page_down();
    state.query.push('r');
    state.page = 0;
    assert!(state.page_labels().len() >= 2, "filtered rows");
}

#[test]
fn emoji_menu_backspace_returns_to_the_recents() {
    use super::emoji_picker::EmojiPickerState;
    let mut state = EmojiPickerState::with(emoji_rows(), vec![("fire".into(), "🔥".into())]);
    state.query.push('x');
    state.query.pop();
    // The query is empty again: the recents page is back.
    assert_eq!(state.page_labels(), vec!["🔥 fire".to_string()]);
}

#[test]
fn emoji_menu_keys_map_onto_actions() {
    use super::emoji_picker::{EmojiKey, EmojiPickerState};
    let state = EmojiPickerState::with(emoji_rows(), Vec::new());
    let map = |kv| state.on_key(kv);
    assert_eq!(map(0x31), EmojiKey::Select(0), "'1' picks the first");
    assert_eq!(map(0x39), EmojiKey::Select(8));
    assert_eq!(map(0x30), EmojiKey::Select(9), "'0' picks the tenth");
    assert_eq!(map(0xffb1), EmojiKey::Select(0), "KP_1 picks the first");
    assert_eq!(map(0xffb0), EmojiKey::Select(9), "KP_0 picks the tenth");
    assert_eq!(map(65293), EmojiKey::Select(0), "Enter picks the first");
    assert_eq!(map(65421), EmojiKey::Select(0), "KP_Enter too");
    assert_eq!(map(65307), EmojiKey::Cancel);
    assert_eq!(map(65288), EmojiKey::Backspace);
    assert_eq!(map(65365), EmojiKey::PageUp);
    assert_eq!(map(65366), EmojiKey::PageDown);
    assert_eq!(map(0x61), EmojiKey::Query('a'), "letters search");
    assert_eq!(map(0x20), EmojiKey::Query(' '), "space searches");
    assert_eq!(map(0x5f), EmojiKey::Query('_'), "underscore searches");
    assert_eq!(map(0x3b), EmojiKey::Ignore, "punctuation closes");
    assert_eq!(map(0xff51), EmojiKey::Ignore, "arrows close");
}
