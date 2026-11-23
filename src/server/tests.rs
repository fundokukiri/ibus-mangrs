use super::hotkey::*;
use super::x11::*;
use super::*;
use crate::bamboo_wrapper::Method;
use crate::engine::Mode;
use crate::transport::{BackendOps, Injection, Kind};
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
fn hotkey_registry_maps_the_new_custom_actions() {
    let config = Config {
        custom_hotkeys: vec![
            crate::config::CustomHotkey {
                spec: "Control+Alt+c".into(),
                action: "charset".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+t".into(),
                action: "method".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+m".into(),
                action: "macro".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+g".into(),
                action: "macro_editor".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+s".into(),
                action: "settings".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+a".into(),
                action: "about".into(),
            },
            // Unknown actions register nothing.
            crate::config::CustomHotkey {
                spec: "Control+Alt+q".into(),
                action: "nonsense".into(),
            },
        ],
        ..Config::default()
    };
    let registry = hotkey_registry(&config);
    assert!(registry
        .iter()
        .any(|(_, _, a)| matches!(a, HotkeyAction::CycleCharset)));
    assert!(registry
        .iter()
        .any(|(_, _, a)| matches!(a, HotkeyAction::CycleMethod)));
    assert!(registry
        .iter()
        .any(|(_, _, a)| matches!(a, HotkeyAction::QuickToggle("macro"))));
    assert!(registry
        .iter()
        .any(|(_, _, a)| matches!(a, HotkeyAction::OpenSettings("macros"))));
    assert!(registry
        .iter()
        .any(|(_, _, a)| matches!(a, HotkeyAction::OpenSettings("general"))));
    assert!(registry
        .iter()
        .any(|(_, _, a)| matches!(a, HotkeyAction::OpenSettings("about"))));
    // Six custom entries, the unknown one skipped.
    assert_eq!(
        registry
            .iter()
            .filter(|(_, _, a)| !matches!(
                a,
                HotkeyAction::ToggleEnabled
                    | HotkeyAction::CycleMode
                    | HotkeyAction::AppRulePicker
                    | HotkeyAction::OpenEmoji
            ))
            .count(),
        6
    );
}

#[test]
fn shift_punctuation_hotkeys_pass_the_key_through() {
    use crate::keymap::{CONTROL_MASK, MOD1_MASK, SHIFT_MASK};
    // The SECOND tap of "Shift+~" types ~ instead of re-firing.
    assert!(hotkey_passes_key(SHIFT_MASK, 0x7e));
    assert!(hotkey_passes_key(SHIFT_MASK, 0x2c));
    // Control and Alt block the character: regular combos consume.
    assert!(!hotkey_passes_key(CONTROL_MASK, 0x7e));
    assert!(!hotkey_passes_key(CONTROL_MASK | MOD1_MASK, 0x76));
    // Modifier-only combos have no key at all.
    assert!(!hotkey_passes_key(SHIFT_MASK | CONTROL_MASK, 0));
}

#[test]
fn cycle_next_wraps_around_the_list() {
    let list = &[("a", "A", "a'"), ("b", "B", "b'"), ("c", "C", "c'")];
    assert_eq!(cycle_next(list, "a"), Some("b"));
    assert_eq!(cycle_next(list, "c"), Some("a"));
    assert_eq!(cycle_next(list, "z"), None);
}

#[test]
fn visible_cycle_skips_hidden_modes_and_wraps() {
    use super::hotkey::next_visible_mode;
    let visible = vec![
        ModeConfig::Preedit,
        ModeConfig::Surrounding,
        ModeConfig::Off,
    ];
    assert_eq!(
        next_visible_mode(&visible, ModeConfig::Preedit),
        ModeConfig::Surrounding
    );
    assert_eq!(
        next_visible_mode(&visible, ModeConfig::Off),
        ModeConfig::Preedit,
        "wraps to the first"
    );
    assert_eq!(
        next_visible_mode(&visible, ModeConfig::Flow),
        ModeConfig::Preedit,
        "a mode outside the visible list lands on the first visible entry"
    );
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
    // CAP_PREEDIT_TEXT the engine resolves Preedit down to the
    // commit-as-you-type walk.
    lock_engine(&engine).set_capabilities(1);
    let mut handler = EngineHandler {
        engine: Arc::clone(&engine),
        injection: Injection::new(),
        // Fresh clock: maybe_reload_config stays inside its 1s gate, so
        // the test never touches the real config file.
        last_config_check: Instant::now(),
        last_config_mtime: None,
        hotkeys: Vec::new(),
        last_fire: Vec::new(),
        picker: None,
        emoji_picker: None,
        burst: BurstWatch::default(),
        burst_buffer: BurstBuffer::default(),
        bounce_deadline: None,
        stash: None,
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
        crate::engine::Mode::Surrounding,
    );
    e.set_capabilities(32); // CAP_SURROUNDING_TEXT
    e.set_surrounding("word".into(), 4);
    let mut out = Vec::new();
    emit(&mut e, &Injection::new(), actions, &mut out);
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
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        hide_underline: true,
        ..Config::default()
    });
    let mut out = Vec::new();
    emit(
        &mut e,
        &Injection::new(),
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
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
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
        crate::engine::Mode::Surrounding,
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
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
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
fn parse_hotkey_accepts_the_owner_requested_combos() {
    // Modifier-only combos used to die on a trailing '+' in the recorded
    // spec; punctuation keys died on their GDK names ("asciitilde"); the
    // Shift-only gate and the two-character F-key check rejected the rest.
    assert!(super::parse_hotkey("Control+Shift").is_some());
    assert!(super::parse_hotkey("Control+Alt").is_some());
    assert!(super::parse_hotkey("Control+Shift+Alt").is_some());
    assert!(super::parse_hotkey("Shift+~").is_some());
    assert!(super::parse_hotkey("Control+~").is_some());
    // A bare key and Shift+letter still eat normal typing.
    assert_eq!(super::parse_hotkey("~"), None);
    assert_eq!(super::parse_hotkey("Shift+a"), None);
    // f10..f12 used to fail the two-character length check (with a
    // modifier: a bare key is still rejected by design).
    assert!(super::parse_hotkey("Control+f1").is_some());
    assert!(super::parse_hotkey("Control+F10").is_some());
    assert!(super::parse_hotkey("Control+f12").is_some());
}

#[test]
fn registry_includes_custom_shortcuts() {
    let config = Config {
        custom_hotkeys: vec![
            crate::config::CustomHotkey {
                spec: "Control+Shift".into(),
                action: "mode".into(),
            },
            // A spec the parser cannot read and an action key it does not
            // know: both register nothing.
            crate::config::CustomHotkey {
                spec: "nonsense".into(),
                action: "emoji".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+j".into(),
                action: "volume".into(),
            },
            crate::config::CustomHotkey {
                spec: "Control+Alt+k".into(),
                action: "rule".into(),
            },
        ],
        ..Config::default()
    };
    let reg = hotkey_registry(&config);
    assert_eq!(reg.len(), 2, "bad spec and unknown action are skipped");
    assert_eq!(reg[0].2, HotkeyAction::CycleMode);
    assert_eq!(reg[1].2, HotkeyAction::AppRulePicker);
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

// --- emit over the selected injection backend ---

/// The injected ops, recorded by [`RecordingBackend`]. One entry per call,
/// so `Backspaces(2)` is one paced batch, not two presses.
#[derive(Debug, PartialEq)]
enum RecordedOp {
    Backspaces(u32),
}

/// Test double for the backend the server's `Injection` selects: records
/// every op into a log shared with the test, which cannot read the backend
/// back out of the holder.
struct RecordingBackend {
    kind: Kind,
    ops: Arc<Mutex<Vec<RecordedOp>>>,
}

impl BackendOps for RecordingBackend {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn available(&self) -> bool {
        true
    }

    fn backspaces(&self, n: u32, _gap_ms: u64) -> bool {
        self.ops.lock().unwrap().push(RecordedOp::Backspaces(n));
        true
    }

    // Whitelist surface: Delete stays a backend op, but no engine action
    // reaches it anymore, so there is nothing to record.
    fn delete(&self, _n: u32) -> bool {
        true
    }
}

/// An `Injection` holding a [`RecordingBackend`], built through the real
/// `select_with` seam (the holder itself is the test setter), plus the
/// shared op log.
///
/// The routing tests drive production `emit` through this holder: if routing
/// ever regressed to the uinput statics, the failing test would inject real
/// Backspaces into whatever window is focused — that failure mode is the
/// point of the pin, but be aware of it when running these alone.
fn recorded_injection() -> (Injection, Arc<Mutex<Vec<RecordedOp>>>) {
    let ops = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&ops);
    let mut injection = Injection::new();
    let stub = |kind: Kind| -> Option<Box<dyn BackendOps>> {
        Some(Box::new(RecordingBackend {
            kind,
            ops: Arc::clone(&ops),
        }))
    };
    assert_eq!(
        injection.select_with(&[Kind::XTest], &stub),
        Some(Kind::XTest)
    );
    (injection, log)
}

#[test]
fn emit_uses_the_selected_backend_not_uinput_statics() {
    // emit builds its transports over the server's Injection: a rewrite
    // whose delete succeeds on the selected backend must reach the
    // application as exactly that Backspace batch and never ALSO as a
    // protocol delete of the same region (double deletion erased words).
    let (injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut out = Vec::new();
    let injected = emit(
        &mut e,
        &injection,
        &[Action::RewriteByBackspace {
            backspaces: 2,
            commit: "ê".into(),
        }],
        &mut out,
    );
    assert_eq!(injected, 2, "the batch counts toward the echo deferral");
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(2)],
        "exactly one Backspace batch through the selected backend"
    );
    assert!(
        !out.iter().any(|m| m
            .member()
            .map(|m| m == "DeleteSurroundingText")
            .unwrap_or(false)),
        "the same region must not also be protocol-deleted: {:?}",
        member_names(&out)
    );
}

#[test]
fn relay_context_never_attempts_injection() {
    // Relayed Wayland clients lose injected keys; the engine bans
    // injection there (injection_forbidden). Even with a backend selected
    // and the availability flag fed, emit must not find a way around the
    // ban: the record stays empty while the commit still lands.
    let (injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_uinput_available(true);
    let mut out = Vec::new();
    emit(
        &mut e,
        &injection,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
    );
    assert!(
        ops.lock().unwrap().is_empty(),
        "no injection may be attempted on a relayed client"
    );
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the commit still lands: {:?}",
        member_names(&out)
    );
}

#[test]
fn compositor_routed_backend_injects_on_a_relayed_context() {
    // The ban's reason is device-injected (uinput/XTest) keys getting
    // lost between the compositor and a relayed client. A compositor-
    // routed backend delivers through the compositor itself, so the
    // exemption must hold on the inject path too: the DeleteBefore the
    // engine emits on the no-report path is served by real Backspace
    // presses through the selected backend, with the commit parked
    // behind the echoes like on any injected path. Without the
    // exemption the same batch stays banned (relay ban test above).
    let (injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_uinput_available(true);
    e.set_injection_relay_safe(true);
    let mut out = Vec::new();
    let injected = emit(
        &mut e,
        &injection,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
    );
    assert_eq!(injected, 2, "the exempt backend serves the delete");
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(2)],
        "real Backspace presses through the compositor-routed backend"
    );
    assert!(
        !out.iter().any(|m| m
            .member()
            .map(|m| m == "DeleteSurroundingText")
            .unwrap_or(false)),
        "no protocol delete on the no-report path: {:?}",
        member_names(&out)
    );
    assert!(e.has_deferred(), "the commit waits for the echoes");

    // Without the exemption the relay ban holds: the same batch is not
    // injected and the commit lands ungated.
    let (injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_uinput_available(true);
    let mut out = Vec::new();
    emit(
        &mut e,
        &injection,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
    );
    assert!(
        ops.lock().unwrap().is_empty(),
        "no injection without the compositor-routed exemption"
    );
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the commit still lands: {:?}",
        member_names(&out)
    );
}

#[test]
fn stale_echo_flushes_parked_commit() {
    // A rewrite's echo never comes back (injection lost) and no key
    // arrives: the main-loop tick must still deliver the parked commit
    // through emit, without attempting another injection on the way out.
    let (injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        experimental_features: true,
        echo_stale_enabled: true,
        echo_stale_ms: 1,
        ..Config::default()
    });
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    e.defer_commit(2, "tiếng".into());
    e.add_injected(2);
    assert!(e.take_injected_echo(), "the first echo arrives");
    assert!(
        e.note_echo_forwarded().is_empty(),
        "the second echo never comes: the commit stays parked"
    );
    assert!(e.has_deferred());
    std::thread::sleep(Duration::from_millis(5)); // past the 1ms stale window
    let actions = e.tick();
    assert_eq!(
        actions,
        vec![Action::Commit("tiếng".into())],
        "the stale tick hands the parked commit to emit: {actions:?}"
    );
    let mut out = Vec::new();
    emit(&mut e, &injection, &actions, &mut out);
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the parked commit lands: {:?}",
        member_names(&out)
    );
    assert!(
        ops.lock().unwrap().is_empty(),
        "the flush must not inject anything"
    );
    assert!(!e.has_deferred() && !e.pending_echoes());
}

#[test]
fn watchdog_flushes_a_lost_burst_after_the_deadline() {
    // A rewrite burst on a client without surrounding data: the deletes
    // are injected and the commit parks behind their echoes. When the
    // echoes never come back (the relay ate the batch), the burst
    // watchdog declares the burst lost on its deadline — the parked
    // commit flushes and the counters clear — while a burst whose echoes
    // keep arriving is never tripped: each echo re-arms the deadline.
    let _guard = crate::debug_log::test_guard();
    // Clear the log globals first: LOG_FILE caches the open handle for
    // the process lifetime, and a handle left over from a parallel test
    // (opened against the real cache file) would swallow every line this
    // test's machinery writes — the override is only consulted on open.
    crate::debug_log::test_reset();
    let dir = std::env::temp_dir().join(format!("ibus-mangrs-watchdog-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("debug.log");
    let _ = std::fs::remove_file(&path);
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
    crate::debug_log::set_enabled(true);

    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true); // no surrounding data → injection
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    let mut burst = BurstWatch::default();

    // Lost burst: the batch is injected, no echo ever arrives.
    let mut out = Vec::new();
    let injected = emit_with(
        &mut e,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
        &t,
    );
    add_injected_batch(&mut e, &mut burst, injected);
    assert_eq!(injected, 2);
    assert!(burst.in_flight(), "the burst is armed");
    assert!(
        !out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the commit parks behind the echoes: {:?}",
        member_names(&out)
    );
    assert!(e.has_deferred());
    // Well past the deadline (one pacing gap + the grace).
    std::thread::sleep(Duration::from_millis(190));
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the parked commit lands: {:?}",
        member_names(&out)
    );
    assert!(!burst.in_flight(), "the burst is settled");
    assert!(!e.pending_echoes(), "counters cleared");
    assert!(!e.has_deferred());
    // A pass after settling emits nothing and logs nothing: the disarm
    // is the log-once gate.
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(out.is_empty());

    // Healthy burst: the echoes arrive, each re-arms the deadline.
    let mut out = Vec::new();
    let injected = emit_with(
        &mut e,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
        &t,
    );
    add_injected_batch(&mut e, &mut burst, injected);
    assert_eq!(injected, 2);
    // The first echo arrives near the end of the original window: the
    // deadline moves out and the burst survives its old deadline.
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        note_burst_echo(&mut e, &mut burst),
        "the first echo arrives"
    );
    std::thread::sleep(Duration::from_millis(60)); // past the old deadline
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(
        out.is_empty(),
        "the re-armed deadline holds: {:?}",
        member_names(&out)
    );
    assert!(burst.in_flight(), "one echo is still expected");
    // The second echo settles the burst; nothing flushes behind it.
    assert!(note_burst_echo(&mut e, &mut burst));
    assert!(!burst.in_flight());
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(out.is_empty());

    crate::debug_log::test_reset();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text.lines().filter(|l| l.contains("echo watchdog")).count(),
        1,
        "one watchdog line for one lost burst: {text}"
    );
}

#[test]
fn watchdog_rearms_the_deadline_on_each_arriving_echo() {
    // The deadline is echo-anchored, not arming-anchored: an echo
    // arriving near the end of the window moves it out and the burst
    // survives its old deadline; only a window that passes with echoes
    // still expected aborts the burst.
    // The expiry logs: hold the debug-log guard so parallel tests that
    // assert log lines never see this test's lines (process-wide state),
    // and start from cleared globals so this test's own lines can never
    // land in a handle a previous test left open.
    let _guard = crate::debug_log::test_guard();
    crate::debug_log::test_reset();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    let mut burst = BurstWatch::default();
    let mut out = Vec::new();
    let injected = emit_with(
        &mut e,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
        &t,
    );
    add_injected_batch(&mut e, &mut burst, injected);
    assert!(burst.in_flight());
    std::thread::sleep(Duration::from_millis(100));
    assert!(note_burst_echo(&mut e, &mut burst), "the echo arrives");
    std::thread::sleep(Duration::from_millis(80)); // past the old deadline
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(
        out.is_empty(),
        "the re-armed deadline holds: {:?}",
        member_names(&out)
    );
    assert!(burst.in_flight(), "one echo is still expected");
    // No further echo: the re-armed deadline passes and the burst is
    // declared lost after all.
    std::thread::sleep(Duration::from_millis(100));
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(
        out.iter()
            .any(|m| m.member().map(|m| m == "CommitText").unwrap_or(false)),
        "the parked commit still lands once the re-armed window passes: {:?}",
        member_names(&out)
    );
    assert!(!burst.in_flight());
    assert!(!e.pending_echoes() && !e.has_deferred());
}

#[test]
fn watchdog_window_follows_the_custom_stale_window() {
    // Built-in: one pacing gap plus the grace constant. A custom
    // lost-echo window (experimental gate) replaces the grace with the
    // user's value — the same fold `Engine::echo_stale` applies.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(
        burst_watch_window(&e),
        Duration::from_millis(e.backspace_interval_ms() + ECHO_WATCHDOG_GRACE_MS)
    );
    e.apply_config(&Config {
        experimental_features: true,
        echo_stale_enabled: true,
        echo_stale_ms: 900,
        ..Config::default()
    });
    assert_eq!(
        burst_watch_window(&e),
        Duration::from_millis(e.backspace_interval_ms() + 900)
    );
}

#[test]
fn watchdog_pass_settles_a_burst_the_stale_flush_already_ended() {
    // The engine's arm-anchored stale pass (inside tick, and on the key
    // paths) can drop the pending echoes before the watchdog deadline;
    // the watchdog pass must reconcile instead of flushing a second time.
    // The machinery logs: hold the debug-log guard (see the re-arm test).
    let _guard = crate::debug_log::test_guard();
    crate::debug_log::test_reset();
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let bs = |_: u32, _: u64| true;
    let t = fake_transports(&bs);
    let mut burst = BurstWatch::default();
    let mut out = Vec::new();
    let injected = emit_with(
        &mut e,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
        &t,
    );
    add_injected_batch(&mut e, &mut burst, injected);
    assert!(burst.in_flight());
    // The stale window elapsed: tick's stale block drops the pending
    // echoes and flushes the parked commits (replicated here).
    std::thread::sleep(Duration::from_millis(190));
    assert!(e.take_stale_injection(e.echo_stale()));
    assert!(e.flush_deferred().len() == 1, "tick's flush delivered it");
    // The watchdog pass sees the burst already ended: disarm, no output.
    let mut out = Vec::new();
    burst_watch_pass(&mut e, &mut burst, &mut out);
    assert!(out.is_empty(), "no double flush: {:?}", member_names(&out));
    assert!(!burst.in_flight());
    assert!(!e.pending_echoes());
}

// --- keys held during a burst, replayed on completion ---

/// One run-loop pass over the burst state, mirroring `run`: the watchdog
/// pass, the tick, the emit and the replay. The tests drive the same
/// sequence the live loop executes (the loop itself needs a bus).
fn burst_pass(
    e: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    buffer: &mut BurstBuffer,
    out: &mut Vec<Message>,
) {
    bounce_pass(e, injection, burst, buffer, &mut None, &mut None, out);
}

/// The full run-loop pass, bounce state included: bounce resolution (an
/// expired window aborts before anything can deliver), the freeze while the
/// window is open (watchdog, tick and the held-key replay all skipped — the
/// parked commits belong to the bounce and a mid-window burst completion
/// must not send the held keys to the wire), the burst sequence, the replay
/// and the stash expiry. No bounce pending behaves exactly like
/// `burst_pass`.
fn bounce_pass(
    e: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    buffer: &mut BurstBuffer,
    bounce_deadline: &mut Option<Instant>,
    stash: &mut Option<StashedRewrite>,
    out: &mut Vec<Message>,
) {
    if bounce_deadline.is_some_and(|d| Instant::now() >= d) {
        *bounce_deadline = None;
        abort_deferred_focus_out(e, burst, buffer);
    }
    if bounce_deadline.is_none() {
        burst_watch_pass(e, burst, out);
        let actions = e.tick();
        let injected = emit(e, injection, &actions, out);
        add_injected_batch(e, burst, injected);
        replay_buffered_keys(e, injection, burst, buffer, out);
    }
    if let Some(s) = stash {
        if Instant::now() >= s.deadline {
            debug_log::log("stashed rewrite dropped: no report within the reconcile window");
            *stash = None;
        }
    }
}

/// One keystroke through the real `process_key` path.
fn press_key(
    e: &mut Engine,
    injection: &mut Injection,
    burst: &mut BurstWatch,
    buffer: &mut BurstBuffer,
    keyval: u32,
    keycode: u32,
    out: &mut Vec<Message>,
) -> bool {
    process_key(
        e,
        injection,
        burst,
        buffer,
        KeyEvent {
            keyval,
            keycode,
            state: 0,
        },
        out,
    )
}

#[test]
fn keys_during_a_burst_replay_in_order_after_it() {
    // Keys typed while a rewrite burst is in flight must not process
    // against the mid-rewrite screen: they hold in the burst buffer and
    // replay through process_key in arrival order once the burst fully
    // completes (echo settled, parked insert delivered). The replayed
    // keys join the locked Flow word — the word-mode lock outlives the
    // burst, so u, o, c compose onto đ instead of starting new words.
    let (mut injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut out = Vec::new();

    // Start the Flow word: 'd' parks its per-key insert, no batch yet.
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'd' as u32,
        30,
        &mut out
    ));
    assert!(
        buffer.keys.is_empty(),
        "idle typing never holds: {:?}",
        buffer.keys
    );
    // The pass lands the park: the arming 'd' can only inject once the
    // screen holds the landed 'd' (a pending park is swapped, not deleted).
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    burst_pass(&mut e, &mut injection, &mut burst, &mut buffer, &mut out);
    assert!(!e.has_deferred(), "the park landed");

    // Arm the rewrite batch: the second 'd' composes đ — one injected
    // Backspace erases the landed 'd', the insert parks behind the echo.
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'd' as u32,
        30,
        &mut out
    ));
    assert!(burst.in_flight(), "the rewrite batch is armed");
    assert_eq!(burst.expected_echoes(), 1);

    // Three keys typed mid-burst: held in order, nothing processed.
    for c in ['u', 'o', 'c'] {
        assert!(
            press_key(
                &mut e,
                &mut injection,
                &mut burst,
                &mut buffer,
                c as u32,
                30,
                &mut out
            ),
            "a held key is swallowed"
        );
    }
    assert_eq!(
        buffer.keys,
        vec![
            ('u' as u32, 30, 0),
            ('o' as u32, 30, 0),
            ('c' as u32, 30, 0)
        ],
        "the three keys hold in arrival order"
    );
    assert_eq!(e.tail(), "đ", "nothing may process mid-burst: {}", e.tail());

    // Settle: the injected Backspace's echo comes back through
    // ProcessKeyEvent and is forwarded untouched — never held.
    assert!(
        !press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            keymap::BACKSPACE,
            14,
            &mut out
        ),
        "the echo is forwarded to the application"
    );
    assert!(!burst.in_flight(), "the echo settled the burst");
    assert_eq!(buffer.keys.len(), 3, "the echo is not a keystroke");

    // The burst completes when the parked insert lands: the pass delivers
    // "đ" and the buffer replays in one sweep, in order.
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    burst_pass(&mut e, &mut injection, &mut burst, &mut buffer, &mut out);
    assert!(buffer.keys.is_empty(), "the buffer cleared");
    assert_eq!(
        e.tail(),
        "đuoc",
        "the three keys composed onto the word in order: {}",
        e.tail()
    );

    // The final insert lands on the next pass: the word is complete and
    // the replay armed no new batch (u, o, c were plain appends).
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    burst_pass(&mut e, &mut injection, &mut burst, &mut buffer, &mut out);
    assert!(
        !e.has_deferred(),
        "every insert landed: {:?}",
        member_names(&out)
    );
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(1)],
        "exactly the arming batch was injected"
    );
}

#[test]
fn keys_buffered_during_a_burst_replay_after_the_watchdog_expiry() {
    // The second completion route: the echoes never come back, the
    // watchdog declares the burst lost and flushes the parked commit —
    // the held keys then replay in the same pass. Keys the engine passes
    // through are forwarded explicitly: the replay rides the tick path,
    // so there is no ProcessKeyEvent reply to hand them over. The burst
    // state is constructed directly here (the machinery is mode
    // independent); Off mode makes the replayed letters pass through, so
    // their arrival order is visible on the wire.
    let _guard = crate::debug_log::test_guard();
    crate::debug_log::test_reset();
    let (mut injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Off);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut out = Vec::new();

    // Arm a rewrite batch: deletes injected, commit parked behind echoes.
    let injected = emit(
        &mut e,
        &injection,
        &[Action::DeleteBefore(2), Action::Commit("tiếng".into())],
        &mut out,
    );
    add_injected_batch(&mut e, &mut burst, injected);
    assert_eq!(injected, 2);
    assert!(burst.in_flight());

    // Three keys mid-burst: held in order, no processing.
    for (keyval, keycode) in [('u' as u32, 30), ('o' as u32, 32), ('c' as u32, 33)] {
        assert!(
            press_key(
                &mut e,
                &mut injection,
                &mut burst,
                &mut buffer,
                keyval,
                keycode,
                &mut out
            ),
            "a held key is swallowed"
        );
    }
    assert_eq!(buffer.keys.len(), 3);

    // The watchdog window passes with no echo: the expiry pass completes
    // the burst (parked commit flushed) and the replay follows in it.
    std::thread::sleep(Duration::from_millis(190));
    burst_pass(&mut e, &mut injection, &mut burst, &mut buffer, &mut out);
    assert!(buffer.keys.is_empty(), "the buffer cleared");
    assert!(!burst.in_flight() && !e.pending_echoes() && !e.has_deferred());
    let members = member_names(&out);
    assert!(
        members.iter().any(|m| m == "CommitText"),
        "the parked commit landed on the expiry: {members:?}"
    );
    let forwards: Vec<(u32, u32, u32)> = out
        .iter()
        .filter(|m| m.member().map(|m| m == "ForwardKeyEvent").unwrap_or(false))
        .filter_map(|m| m.read3::<u32, u32, u32>().ok())
        .filter(|(_, _, state)| state & keymap::RELEASE_MASK == 0)
        .collect();
    assert_eq!(
        forwards,
        vec![
            ('u' as u32, 30, 0),
            ('o' as u32, 32, 0),
            ('c' as u32, 33, 0)
        ],
        "the held keys land in original order: {forwards:?}"
    );
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(2)],
        "the replay armed no new batch"
    );
}

#[test]
fn burst_buffer_cap_drops_the_newest_and_logs_once() {
    // A stuck burst must not accumulate without limit: past the cap the
    // newest key drops and the overflow logs ONCE per episode — the gate
    // resets when the buffer drains, so a fresh episode logs again.
    let _guard = crate::debug_log::test_guard();
    crate::debug_log::test_reset();
    let dir = std::env::temp_dir().join(format!(
        "ibus-mangrs-burst-buffer-log-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("debug.log");
    let _ = std::fs::remove_file(&path);
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
    crate::debug_log::set_enabled(true);

    let mut buffer = BurstBuffer::default();
    for i in 0..BURST_BUFFER_CAP as u32 {
        buffer.push(0x61 + i, 30, 0);
    }
    assert_eq!(buffer.keys.len(), BURST_BUFFER_CAP);
    buffer.push(0x99, 31, 0);
    assert_eq!(
        buffer.keys.len(),
        BURST_BUFFER_CAP,
        "the newest key is the drop"
    );
    assert_eq!(
        buffer.keys.last().map(|k| k.0),
        Some(0x60 + BURST_BUFFER_CAP as u32),
        "the earliest keys survive"
    );
    for _ in 0..10 {
        buffer.push(0x99, 31, 0);
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text.lines()
            .filter(|l| l.contains("burst buffer full"))
            .count(),
        1,
        "one line per overflow episode: {text}"
    );

    // The drain resets the gate: a fresh overflow logs again.
    buffer.take();
    for i in 0..=BURST_BUFFER_CAP as u32 {
        buffer.push(0x61 + i, 30, 0);
    }
    crate::debug_log::test_reset();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text.lines()
            .filter(|l| l.contains("burst buffer full"))
            .count(),
        2,
        "a fresh episode logs again: {text}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn idle_typing_never_buffers() {
    // No burst armed, nothing parked: every key processes immediately —
    // the hold check is the hot path's only cost when nothing is armed.
    let mut injection = Injection::new();
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(1);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut out = Vec::new();
    for c in ['t', 'i', 'e'] {
        assert!(
            press_key(
                &mut e,
                &mut injection,
                &mut burst,
                &mut buffer,
                c as u32,
                30,
                &mut out
            ),
            "the key composes the preedit"
        );
    }
    assert!(buffer.keys.is_empty(), "nothing held");
    assert!(!burst.in_flight() && !e.has_deferred());
    assert_eq!(
        member_names(&out)
            .iter()
            .filter(|m| *m == "UpdatePreeditText")
            .count(),
        3,
        "each key processed at once: {:?}",
        member_names(&out)
    );
}

// --- focus-bounce tolerance during a burst ---

#[test]
fn focus_bounce_within_the_window_continues_the_burst() {
    // FocusOut mid-burst, FocusIn inside the bounce window: the reset is
    // deferred, not run — the composition, the parked commits and the held
    // keys all survive, nothing reaches the wire while the focus is in
    // flight, and the word still completes as if nothing happened.
    let (mut injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();

    // Start the Flow word: two keys land a two-character prefix — the stash
    // anchor must span MIN_ANCHOR_CHARS (a one-character anchor captures
    // nothing; see stash_in_flight_rewrite).
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        't' as u32,
        30,
        &mut out
    ));
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'a' as u32,
        30,
        &mut out
    ));
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(!e.has_deferred(), "both parks landed");

    // Arm the rewrite batch: 's' composes tá — one injected Backspace
    // erases the landed 'a' (the common prefix is "t"), the insert "á"
    // parks behind the echo.
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        's' as u32,
        30,
        &mut out
    ));
    assert!(burst.in_flight(), "the rewrite batch is armed");
    assert_eq!(burst.expected_echoes(), 1);
    let out_len = out.len();

    // FocusOut mid-burst: the reset defers and the rewrite is stashed.
    assert!(
        defer_focus_out_during_burst(&mut e, &mut burst, &mut bounce_deadline, &mut stash,),
        "a live burst defers the focus-out reset"
    );
    assert!(
        bounce_deadline.is_some(),
        "the reset defers for the bounce window"
    );
    let stashed = stash.as_ref().expect("the rewrite is stashed");
    assert_eq!(stashed.text, "á", "the parked insert is the rewrite text");
    assert_eq!(
        stashed.expected_tail, "ta",
        "the anchor is what the app certainly holds"
    );
    assert_eq!(
        stashed.tail, "tá",
        "the re-anchor value is the believed tail"
    );
    assert_eq!(
        stashed.client.as_deref(),
        Some("gtk4-im:test"),
        "the departed client is recorded for the foreign guard"
    );

    // A key typed during the bounce: held, order preserved.
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'u' as u32,
        30,
        &mut out
    ));
    assert_eq!(buffer.keys.len(), 1, "the held key survives the bounce");

    // A pass inside the window delivers nothing: the freeze holds the park
    // (past its own deadline) and the replay — nothing may reach the wire
    // mid-bounce.
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(10));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(
        out.len() == out_len,
        "no signal may fly during the bounce window: {:?}",
        member_names(&out[out_len..])
    );
    assert!(e.has_deferred(), "the park stays parked through the freeze");
    assert_eq!(
        e.tail(),
        "tá",
        "the composition survives the deferred reset"
    );
    assert!(burst.in_flight(), "the burst itself is untouched");

    // FocusIn inside the window, same client: the burst resumes untouched.
    assert!(
        resolve_focus_bounce_on_focus_in(
            &mut e,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            Some("gtk4-im:test"),
        ),
        "the same context back inside the window resumes"
    );
    assert!(
        bounce_deadline.is_none() && stash.is_none(),
        "resumed: the deadline clears and the fallback stash drops"
    );
    assert_eq!(buffer.keys.len(), 1, "the held key still waits");
    assert_eq!(e.tail(), "tá");
    assert_eq!(
        e.client(),
        Some("gtk4-im:test"),
        "the resume keeps the departed client held through the window"
    );

    // The burst continues as if nothing happened: the echo settles it, the
    // pass lands the parked insert and replays the held key in order.
    assert!(
        !press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            keymap::BACKSPACE,
            14,
            &mut out
        ),
        "the echo is forwarded to the application"
    );
    assert!(!burst.in_flight(), "the echo settled the burst");
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(buffer.keys.is_empty(), "the held key replayed");
    assert_eq!(e.tail(), "táu", "the word still completes: {}", e.tail());
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(!e.has_deferred(), "every insert landed");
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(1)],
        "exactly the arming batch was injected"
    );
}

#[test]
fn stashed_rewrite_reconciles_within_the_window_or_drops() {
    // FocusOut mid-burst, the window expires: the burst aborts and the
    // stash survives RECONCILE_WINDOW. FocusIn with a matching report
    // (the screen still shows the stashed state) repairs by rewrite; a
    // mismatching report drops it; no report drops it at expiry — logged
    // once per drop. Every arming word lands a two-character prefix first:
    // the stash anchor must span MIN_ANCHOR_CHARS to carry evidence.
    let _guard = crate::debug_log::test_guard();
    crate::debug_log::test_reset();
    let dir = std::env::temp_dir().join(format!("ibus-mangrs-bounce-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("debug.log");
    let _ = std::fs::remove_file(&path);
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
    crate::debug_log::set_enabled(true);

    let (mut injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();

    // Arm "ta" -> tá exactly like the bounce test: land "ta", then 's'
    // composes tá — one injected Backspace erases the landed 'a', the
    // insert "á" parks behind the echo.
    for c in ['t', 'a'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
        std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
        bounce_pass(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            &mut out,
        );
    }
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        's' as u32,
        30,
        &mut out
    ));
    assert!(burst.in_flight());
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    let stashed = stash.as_ref().expect("stash armed");
    assert_eq!(
        stashed.client.as_deref(),
        Some("gtk4-im:test"),
        "the departed client is recorded"
    );
    let out_len = out.len();

    // The window expires without a refocus: a real switch. The abort drops
    // the composition, disarms the burst and holds every parked commit —
    // the stash owns the rewrite now, the wire sees nothing.
    std::thread::sleep(Duration::from_millis(FOCUS_BOUNCE_MS + 20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(
        bounce_deadline.is_none(),
        "the expired window aborts the deferral"
    );
    assert!(stash.is_some(), "the stash survives the abort");
    assert!(
        !burst.in_flight() && !e.pending_echoes(),
        "the burst aborted with the context"
    );
    assert!(!e.has_deferred(), "the parks dropped without emitting");
    assert_eq!(
        e.tail(),
        "",
        "the deferred reset ran: the composition is gone"
    );
    assert!(
        out.len() == out_len,
        "the parked rewrite must not flush to the wire on abort: {:?}",
        member_names(&out[out_len..])
    );

    // FocusIn within the window, matching report ("ta" — the landed prefix
    // is all the screen ever held): repair by rewrite.
    reconcile_stashed_rewrite(&mut e, &injection, &mut burst, &mut stash, "ta", &mut out);
    assert!(stash.is_none(), "the stash is consumed by the reconcile");
    assert_eq!(
        e.tail(),
        "tá",
        "the believed tail re-anchored to the stashed target"
    );
    assert!(burst.in_flight(), "the repair arms its own echo burst");
    assert!(
        e.has_deferred(),
        "the repair's commit parks behind its echo"
    );
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(1), RecordedOp::Backspaces(1)],
        "the repair re-emits the erase batch"
    );
    // The repair's echo comes back; the pass delivers the rewritten word.
    assert!(!press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        keymap::BACKSPACE,
        14,
        &mut out
    ));
    std::thread::sleep(
        Duration::from_millis(Engine::ECHO_APPLY_GRACE_MS) + Duration::from_millis(20),
    );
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(!e.has_deferred(), "the repaired word landed");
    assert!(
        member_names(&out).iter().any(|m| m == "CommitText"),
        "the repair reaches the wire as a commit: {:?}",
        member_names(&out)
    );

    // Second cycle, matching report with a commit-only repair: land "ng",
    // then 'h' parks its append — the repair is a plain commit, no erase
    // needed.
    for c in ['n', 'g'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
        std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
        bounce_pass(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            &mut out,
        );
    }
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'h' as u32,
        30,
        &mut out
    ));
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    let stashed = stash.as_ref().expect("stash armed");
    assert_eq!(
        (stashed.text.as_str(), stashed.expected_tail.as_str()),
        ("h", "ng")
    );
    std::thread::sleep(Duration::from_millis(FOCUS_BOUNCE_MS + 20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    let out_len = out.len();
    reconcile_stashed_rewrite(&mut e, &injection, &mut burst, &mut stash, "ng", &mut out);
    assert!(stash.is_none());
    assert_eq!(e.tail(), "ngh", "the re-anchor carries the full word");
    assert!(
        !burst.in_flight(),
        "a commit-only repair arms no echo burst"
    );
    assert!(
        out.len() == out_len + 1 && member_names(&out[out_len..]) == vec!["CommitText".to_string()],
        "the repair is exactly one commit: {:?}",
        member_names(&out[out_len..])
    );
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(1), RecordedOp::Backspaces(1)],
        "no new erase batch"
    );

    // Third cycle: a mismatching report drops the stash. The abort wiped
    // the composition, so the word needs a landed two-character prefix
    // before the parked key for the anchor to carry evidence.
    for c in ['x', 'y'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
        std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
        bounce_pass(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            &mut out,
        );
    }
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'z' as u32,
        30,
        &mut out
    ));
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    let stashed = stash.as_ref().expect("stash armed");
    assert_eq!(stashed.expected_tail, "xy");
    std::thread::sleep(Duration::from_millis(FOCUS_BOUNCE_MS + 20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    let out_len = out.len();
    reconcile_stashed_rewrite(&mut e, &injection, &mut burst, &mut stash, "abc", &mut out);
    assert!(stash.is_none(), "a foreign report drops the stash");
    assert!(
        out.len() == out_len,
        "nothing is rewritten on a mismatch: {:?}",
        member_names(&out[out_len..])
    );

    // Fourth cycle: no report at all — the stash expires on the pass.
    for c in ['b', 'o'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
        std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
        bounce_pass(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            &mut out,
        );
    }
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'n' as u32,
        30,
        &mut out
    ));
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    std::thread::sleep(Duration::from_millis(FOCUS_BOUNCE_MS + 20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    // Expire the stash without sleeping out the 2s window.
    let stashed = stash.as_mut().expect("stash armed");
    stashed.deadline = Instant::now() - Duration::from_secs(1);
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(stash.is_none(), "the expired stash drops");

    crate::debug_log::test_reset();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text.lines()
            .filter(|l| l.contains("stashed rewrite dropped"))
            .count(),
        2,
        "one drop line per drop, mismatch and expiry: {text}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn real_switch_drops_the_held_keys_a_bounce_keeps_them() {
    // Task 12's context-death drop, moved into the bounce machinery: a
    // bounced focus keeps the held keys (the burst resumes and replays
    // them); a real switch drops them with the context — and the departed
    // client's identity dies with the abort (a plain-FocusIn next client
    // must not inherit the stale string: it feeds the relay ban and
    // app-rule resolution).
    let (mut injection, _ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();

    // Land a two-character prefix, then arm the batch ('s' composes tá).
    for c in ['t', 'a'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
        std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
        bounce_pass(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            &mut out,
        );
    }
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        's' as u32,
        30,
        &mut out
    ));
    // Two keys mid-burst, then the focus leaves: the buffer keeps them.
    for c in ['u', 'o'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
    }
    assert_eq!(buffer.keys.len(), 2);
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    assert_eq!(buffer.keys.len(), 2, "a bounced focus keeps the held keys");
    assert_eq!(
        e.client(),
        Some("gtk4-im:test"),
        "the deferred reset holds the client through the window"
    );
    // The window expires: the real-switch abort drops them.
    std::thread::sleep(Duration::from_millis(FOCUS_BOUNCE_MS + 20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(
        buffer.keys.is_empty(),
        "a real switch drops the held keys with the context"
    );
    assert!(stash.is_some(), "the rewrite stays stashed for reconcile");
    assert_eq!(
        e.client(),
        None,
        "the abort ends the departed client: a plain-FocusIn next client \
         starts with no identity, not the stale string"
    );
}

#[test]
fn idle_focus_out_never_defers_or_stashes() {
    // Focus changes without a burst behave exactly as today: the branch
    // declines, no deadline arms, no stash is captured.
    let mut injection = Injection::new();
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(1);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        't' as u32,
        30,
        &mut out
    ));
    assert!(!e.has_deferred() && !burst.in_flight(), "idle typing");
    assert!(
        !defer_focus_out_during_burst(&mut e, &mut burst, &mut bounce_deadline, &mut stash,),
        "an idle focus-out takes today's path"
    );
    assert!(
        bounce_deadline.is_none() && stash.is_none(),
        "no deferral, no stash"
    );
    // The engine-level FocusOut pin (enabled stays on, keys still compose)
    // lives in focus_out_keeps_the_engine_enabled_only_disable_turns_it_off,
    // which drives the real arm with an idle engine.
}

#[test]
fn focus_out_arm_defers_the_reset_for_a_live_burst() {
    // The arm wiring: an engine_call FocusOut with a burst live takes the
    // bounce branch — no flush reaches the wire, the parks and composition
    // stay, the deadline and the stash arm on the handler.
    let engine: SharedEngine = Arc::new(Mutex::new(Engine::new(Method::Telex, Mode::Flow)));
    let mut handler = EngineHandler {
        engine: Arc::clone(&engine),
        injection: Injection::new(),
        last_config_check: Instant::now(),
        last_config_mtime: None,
        hotkeys: Vec::new(),
        last_fire: Vec::new(),
        picker: None,
        emoji_picker: None,
        burst: BurstWatch::default(),
        burst_buffer: BurstBuffer::default(),
        bounce_deadline: None,
        stash: None,
        focus_ext: None,
    };
    // A live burst, constructed directly: one echo expected, one commit
    // parked behind it, the tail believing "chaò" (the parked "ò" is its
    // suffix — the anchor falls back to the deletes-landed state "cha").
    {
        let mut e = lock_engine(&engine);
        e.reanchor_tail("chaò".into());
        e.defer_commit(1, "ò".into());
        handler.burst.expected = 1;
        handler.burst.deadline = Some(Instant::now() + Duration::from_secs(10));
    }
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("FocusOut"), "FocusOut", &mut out);
    assert!(
        handler.bounce_deadline.is_some(),
        "the arm defers the reset for the bounce window"
    );
    let stashed = handler.stash.as_ref().expect("the rewrite is stashed");
    assert_eq!(stashed.text, "ò");
    assert_eq!(stashed.expected_tail, "cha");
    assert!(
        out.is_empty(),
        "the arm must not flush the parked commit: {:?}",
        member_names(&out)
    );
    let e = lock_engine(&engine);
    assert!(e.has_deferred(), "the park stays with the bounce");
    assert_eq!(e.tail(), "chaò", "the composition stays with the bounce");
}

#[test]
fn the_freeze_holds_the_replay_until_the_window_resolves() {
    // The replay sits INSIDE the freeze gate: a burst that completes
    // mid-window — echo debt settles while no parked commit remains (the
    // per-key retract shape: the empty insert cancelled the park while the
    // InjectBackspaces were still out) — must not send the held keys to the
    // wire while the focus is still in flight. A genuine bounce is exactly
    // when the old app's echoes arrive, so the settle-inside-the-window
    // order is the realistic one; the resume replays them.
    let (mut injection, _ops) = recorded_injection();
    // Off mode makes the replayed letters pass through, so the delivery is
    // observable on the wire (the machinery is mode independent).
    let mut e = Engine::new(Method::Telex, Mode::Off);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();

    // Echo debt, no parked commit: the retract batch's shape.
    let injected = emit(&mut e, &injection, &[Action::DeleteBefore(1)], &mut out);
    add_injected_batch(&mut e, &mut burst, injected);
    assert_eq!(injected, 1);
    assert!(burst.in_flight() && !e.has_deferred());

    // A key mid-burst: held.
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'u' as u32,
        30,
        &mut out
    ));
    assert_eq!(buffer.keys.len(), 1);

    // FocusOut mid-burst: the reset defers.
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    // The echo arrives — a genuine bounce is exactly when echoes come back —
    // and settles the burst with no parked commit behind it.
    assert!(!press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        keymap::BACKSPACE,
        14,
        &mut out
    ));
    assert!(!burst.in_flight() && !e.has_deferred());

    // The next pass, window still open: the replay must NOT run.
    let out_len = out.len();
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(
        out.len() == out_len,
        "the freeze holds the replay: {:?}",
        member_names(&out[out_len..])
    );
    assert_eq!(buffer.keys.len(), 1, "the held key waits for the window");

    // FocusIn resumes: the deadline clears and the replay goes.
    assert!(resolve_focus_bounce_on_focus_in(
        &mut e,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        Some("gtk4-im:test"),
    ));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(
        buffer.keys.is_empty(),
        "the held key replayed after the resume"
    );
    let forwards: Vec<(u32, u32, u32)> = out[out_len..]
        .iter()
        .filter(|m| m.member().map(|m| m == "ForwardKeyEvent").unwrap_or(false))
        .filter_map(|m| m.read3::<u32, u32, u32>().ok())
        .filter(|(_, _, state)| state & keymap::RELEASE_MASK == 0)
        .collect();
    assert_eq!(
        forwards,
        vec![('u' as u32, 30, 0)],
        "the held key lands exactly once, after the resume: {forwards:?}"
    );
}

#[test]
fn foreign_client_refuses_the_stashed_rewrite() {
    // The stash records the departed client: a live client naming a
    // DIFFERENT context refuses the reconcile — the report describes a
    // foreign buffer, and the repair would fire its deletes and commit
    // into the wrong document. Drop, logged once.
    let _guard = crate::debug_log::test_guard();
    crate::debug_log::test_reset();
    let dir = std::env::temp_dir().join(format!("ibus-mangrs-foreign-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("debug.log");
    let _ = std::fs::remove_file(&path);
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
    crate::debug_log::set_enabled(true);

    let (mut injection, ops) = recorded_injection();
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();

    // Land "ta", arm "tá", FocusOut, window expires: the stash is armed.
    for c in ['t', 'a'] {
        assert!(press_key(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            c as u32,
            30,
            &mut out
        ));
        std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
        bounce_pass(
            &mut e,
            &mut injection,
            &mut burst,
            &mut buffer,
            &mut bounce_deadline,
            &mut stash,
            &mut out,
        );
    }
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        's' as u32,
        30,
        &mut out
    ));
    assert!(defer_focus_out_during_burst(
        &mut e,
        &mut burst,
        &mut bounce_deadline,
        &mut stash,
    ));
    std::thread::sleep(Duration::from_millis(FOCUS_BOUNCE_MS + 20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(stash.is_some(), "the stash survives the abort");

    // The next focus names a different app (a confirmed-different client):
    // even a matching report must not drive the repair.
    e.set_client(Some("gtk4-im:other".into()));
    let out_len = out.len();
    reconcile_stashed_rewrite(&mut e, &injection, &mut burst, &mut stash, "ta", &mut out);
    assert!(stash.is_none(), "the foreign context drops the stash");
    assert!(
        out.len() == out_len,
        "no repair may reach the wire for a foreign context: {:?}",
        member_names(&out[out_len..])
    );
    assert_eq!(
        *ops.lock().unwrap(),
        vec![RecordedOp::Backspaces(1)],
        "no repair batch through the backend"
    );

    crate::debug_log::test_reset();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text.lines()
            .filter(|l| l.contains("stashed rewrite dropped"))
            .count(),
        1,
        "the foreign refusal logs once: {text}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn weak_or_garbage_anchors_arm_no_stash() {
    // A one-character anchor cannot evidence the departed screen: a single
    // stray keystroke in ANY context satisfies the suffix match, so the
    // capture refuses it (MIN_ANCHOR_CHARS). And a parked text that is not
    // the tail's suffix (a macro/emoji timed park) must not manufacture a
    // garbage anchor by stripping. Both deferrals still arm — only the
    // reconcile fallback is declined.
    let (mut injection, _ops) = recorded_injection();
    let mut burst = BurstWatch::default();
    let mut buffer = BurstBuffer::default();
    let mut bounce_deadline = None;
    let mut stash = None;
    let mut out = Vec::new();

    // Weak anchor: the landed prefix is one character.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        't' as u32,
        30,
        &mut out
    ));
    std::thread::sleep(e.per_key_delay() + Duration::from_millis(20));
    bounce_pass(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        &mut bounce_deadline,
        &mut stash,
        &mut out,
    );
    assert!(press_key(
        &mut e,
        &mut injection,
        &mut burst,
        &mut buffer,
        'a' as u32,
        30,
        &mut out
    ));
    assert!(
        defer_focus_out_during_burst(&mut e, &mut burst, &mut bounce_deadline, &mut stash,),
        "the deferral itself still arms"
    );
    assert!(stash.is_none(), "a one-character anchor captures nothing");
    assert!(
        bounce_deadline.is_some(),
        "the deferral stands without the stash"
    );

    // Garbage anchor: the parked text is not the tail's suffix — stripping
    // it would leave an unrelated prefix as the anchor.
    let mut e2 = Engine::new(Method::Telex, Mode::Flow);
    e2.set_client(Some("gtk4-im:test".into()));
    e2.set_uinput_available(true);
    e2.reanchor_tail("abcdef".into());
    e2.defer_commit(1, "xyz".into());
    let mut burst2 = BurstWatch::default();
    burst2.arm(&e2, 1);
    let mut bounce_deadline2 = None;
    let mut stash2 = None;
    assert!(
        defer_focus_out_during_burst(&mut e2, &mut burst2, &mut bounce_deadline2, &mut stash2,),
        "the echo debt still defers the reset"
    );
    assert!(
        stash2.is_none(),
        "a non-suffix parked text manufactures no anchor"
    );
}

#[test]
fn forward_key_event_emits_backspace_press_and_release() {
    // The wire contract every ibus client expects: clients add 8 to the
    // keycode themselves, so it must be the evdev scancode 14 — sending
    // the X keycode 22 made them use evdev KEY_U and type a `u` per
    // forwarded Backspace.
    let mut e = Engine::new(Method::Telex, Mode::ForwardBackspace);
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
        let mut e = Engine::new(Method::Telex, Mode::Surrounding);
        e.apply_config(&Config {
            x11_delete_without_report: enabled,
            ..Config::default()
        });
        e.set_mode(Mode::Surrounding);
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
fn forced_flush_delivers_lost_echo_commits() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
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
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
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
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
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
    // tên"). The `swallow` value consumes the first navigation key so
    // the commit lands while the caret is still in place — the safe
    // opt-in next to the shipped `forward` default.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        nav_key_while_preedit: "swallow".into(),
        ..Config::default()
    });
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0b101001);
    for c in "tung".chars() {
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: c as u32,
                keycode: 0,
                state: 0,
            },
            &mut Vec::new(),
        );
    }
    let mut out = Vec::new();
    assert!(
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: 0xff51,
                keycode: 0,
                state: 0,
            },
            &mut out,
        ),
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
    assert!(!process_key(
        &mut e,
        &mut Injection::new(),
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0xff51,
            keycode: 0,
            state: 0,
        },
        &mut out
    ));
}

#[test]
fn first_navigation_key_commits_preedit_even_after_a_capability_flip() {
    // The daemon flips capabilities mid-word. The preedit is already on
    // screen (word-mode lock), so a navigation key must still commit it;
    // with `swallow` that one press is consumed to keep the text from
    // landing at the moved caret.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        nav_key_while_preedit: "swallow".into(),
        ..Config::default()
    });
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0b100001); // PREEDIT|SURROUNDING
    for c in "tung".chars() {
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: c as u32,
                keycode: 0,
                state: 0,
            },
            &mut Vec::new(),
        );
    }
    e.set_capabilities(0b100000); // preedit bit gone, preedit still visible
    let mut out = Vec::new();
    assert!(process_key(
        &mut e,
        &mut Injection::new(),
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0xff51,
            keycode: 0,
            state: keymap::SHIFT_MASK,
        },
        &mut out
    ));
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
    assert!(!process_key(
        &mut e,
        &mut Injection::new(),
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0xff51,
            keycode: 0,
            state: 0,
        },
        &mut out
    ));
}

#[test]
fn in_process_clients_keep_navigation_keys_forwarded() {
    // Only relayed contexts need the swallow: in-process IM clients
    // apply the commit and the key in order themselves.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(0b101001);
    for c in "tung".chars() {
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: c as u32,
                keycode: 0,
                state: 0,
            },
            &mut Vec::new(),
        );
    }
    let mut out = Vec::new();
    assert!(!process_key(
        &mut e,
        &mut Injection::new(),
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0xff51,
            keycode: 0,
            state: 0,
        },
        &mut out
    ));
}

// --- nav_key_while_preedit values ---

/// Type "tung" into a relayed Preedit engine configured with `value`.
fn pending_preedit_engine(value: &str) -> Engine {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        nav_key_while_preedit: value.into(),
        ..Config::default()
    });
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0b101001);
    for c in "tung".chars() {
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: c as u32,
                keycode: 0,
                state: 0,
            },
            &mut Vec::new(),
        );
    }
    e
}

fn member_names(out: &[Message]) -> Vec<String> {
    out.iter()
        .map(|m| m.member().map(|m| m.to_string()).unwrap_or_default())
        .collect()
}

#[test]
fn forward_mode_commits_and_forwards_the_first_navigation_key() {
    // The forward value: commit and key ride one reply — ibus-bamboo's
    // sequence. The caret moves on the first press.
    let mut e = pending_preedit_engine("forward");
    let mut out = Vec::new();
    assert!(
        !process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: 0xff51,
                keycode: 0,
                state: 0,
            },
            &mut out,
        ),
        "Left must be forwarded"
    );
    let members = member_names(&out);
    assert!(
        members.iter().any(|m| m == "CommitText"),
        "the pending word must be committed: {members:?}"
    );
    // The second press is still a plain forward (nothing pending).
    let mut out = Vec::new();
    assert!(!process_key(
        &mut e,
        &mut Injection::new(),
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0xff51,
            keycode: 0,
            state: 0,
        },
        &mut out
    ));
}

#[test]
fn replay_mode_re_emits_the_navigation_key_after_the_commit() {
    // The replay value: the key is consumed and re-emitted as a
    // ForwardKeyEvent press+release pair AFTER the commit signals, so the
    // caret still moves on the first physical press.
    let mut e = pending_preedit_engine("replay");
    let mut out = Vec::new();
    assert!(
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: 0xff51,
                keycode: 0,
                state: 0,
            },
            &mut out,
        ),
        "Left must be consumed"
    );
    let members = member_names(&out);
    let commit = members
        .iter()
        .position(|m| m == "CommitText")
        .expect("the pending word must be committed");
    let press = members
        .iter()
        .position(|m| m == "ForwardKeyEvent")
        .expect("the key must be replayed");
    assert!(commit < press, "commit must land first: {members:?}");
    let keys: Vec<(u32, u32, u32)> = out
        .iter()
        .filter(|m| m.member().map(|x| x == "ForwardKeyEvent").unwrap_or(false))
        .filter_map(|m| m.read3::<u32, u32, u32>().ok())
        .collect();
    assert_eq!(
        keys,
        vec![(0xff51, 105, 0), (0xff51, 105, keymap::RELEASE_MASK)],
        "one press and one release at the evdev scancode: {keys:?}"
    );
}

#[test]
fn replay_falls_back_to_swallow_for_a_modified_navigation_key() {
    // The relay drops modifier state on forwarded keys: a replayed
    // Shift+Left would move instead of select, so the swallow stands.
    let mut e = pending_preedit_engine("replay");
    let mut out = Vec::new();
    assert!(process_key(
        &mut e,
        &mut Injection::new(),
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0xff51,
            keycode: 0,
            state: keymap::SHIFT_MASK,
        },
        &mut out
    ));
    let members = member_names(&out);
    assert!(
        members.iter().all(|m| m != "ForwardKeyEvent"),
        "no key may be replayed: {members:?}"
    );
    assert!(
        members.iter().any(|m| m == "CommitText"),
        "the pending word must still be committed: {members:?}"
    );
}

#[test]
fn the_nav_key_choice_applies_without_the_experimental_gate() {
    // Graduated: the choice applies like every config value, no master
    // gate needed. The default forward sends the first key in the reply;
    // a saved swallow still consumes it.
    let mut e = pending_preedit_engine("forward");
    let mut out = Vec::new();
    assert!(
        !process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: 0xff51,
                keycode: 0,
                state: 0,
            },
            &mut out,
        ),
        "Left must be forwarded without the gate"
    );
    let members = member_names(&out);
    assert!(
        members.iter().any(|m| m == "CommitText"),
        "the pending word must still be committed: {members:?}"
    );
    let mut e = pending_preedit_engine("swallow");
    let mut out = Vec::new();
    assert!(
        process_key(
            &mut e,
            &mut Injection::new(),
            &mut BurstWatch::default(),
            &mut BurstBuffer::default(),
            KeyEvent {
                keyval: 0xff51,
                keycode: 0,
                state: 0,
            },
            &mut out,
        ),
        "Left must be consumed with a saved swallow"
    );
    let members = member_names(&out);
    assert!(
        members.iter().any(|m| m == "CommitText"),
        "the pending word must still be committed: {members:?}"
    );
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
    let visible = Config::default().visible_modes();
    let len = picker_entries(&visible, ModeConfig::Preedit, false).len();
    assert_eq!(len, visible.len(), "only the visible modes, nothing else");
    let state = PickerState::new("e2epick".into(), 1);
    // Digits 1..9 pick entries 1..9, 0 picks the last; Enter picks the
    // highlighted entry; Up/Down move the highlight.
    assert_eq!(state.on_key(0x31), PickerKey::Select(0));
    assert_eq!(state.on_key(0x39), PickerKey::Select(8));
    assert_eq!(state.on_key(0x30), PickerKey::SelectLast);
    assert_eq!(
        state.on_key(65293),
        PickerKey::Select(1),
        "Enter picks the highlight"
    );
    assert_eq!(state.on_key(65307), PickerKey::Cancel);
    assert_eq!(state.on_key(65362), PickerKey::Move(-1)); // Up
    assert_eq!(state.on_key(65364), PickerKey::Move(1)); // Down
    assert_eq!(state.on_key(0x76), PickerKey::Ignore);
    // The current mode's entry carries the dot marker (Preedit is first in
    // the default visible order).
    let entries = picker_entries(&visible, ModeConfig::Preedit, false);
    assert!(
        entries[0].starts_with("● "),
        "preedit is first in the default visible order: {}",
        entries[0]
    );
    assert!(!entries[1].starts_with("● "));
    // A shortened visible list renders exactly its entries; a hidden mode
    // never appears in the picker.
    let short = vec![ModeConfig::Preedit, ModeConfig::Off];
    let entries = picker_entries(&short, ModeConfig::Off, true);
    assert_eq!(entries, vec!["Underline (preedit)", "● OFF"]);
}

#[test]
fn picker_cursor_moves_with_wrap() {
    // The same arithmetic the Move arm runs: wrapping over the visible list.
    let len = Config::default().visible_modes().len() as i32;
    let mut cursor = 2usize;
    for delta in [1, -1, -1] {
        cursor = (cursor as i32 + delta).rem_euclid(len) as usize;
    }
    assert_eq!(cursor, 1, "down then up twice: 2 -> 3 -> 2 -> 1");
    cursor = 0;
    cursor = (cursor as i32 - 1).rem_euclid(len) as usize;
    assert_eq!(
        cursor as i32,
        len - 1,
        "Up from the first wraps to the last"
    );
    cursor = (len - 1) as usize;
    cursor = (cursor as i32 + 1).rem_euclid(len) as usize;
    assert_eq!(cursor, 0, "Down from the last wraps to the first");
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
fn emoji_menu_arrows_wrap_within_the_page() {
    use super::emoji_picker::EmojiPickerState;
    let mut state = EmojiPickerState::with(emoji_rows(), Vec::new());
    assert_eq!(state.cursor, 0, "the first row starts highlighted");
    state.cursor_down();
    state.cursor_down();
    assert_eq!(state.cursor, 2);
    assert_eq!(
        state.pick(state.cursor).map(|(_, g)| g),
        Some("🐢".into()),
        "Enter will commit exactly the highlighted row"
    );
    // Up past the page's top wraps to the page's last row, same page.
    state.cursor_up();
    state.cursor_up();
    state.cursor_up();
    assert_eq!(state.page, 0);
    assert_eq!(state.cursor, 9);
    assert_eq!(state.pick(9).map(|(_, g)| g), Some("☕".into()));
    // Down past the page's end wraps back to the first row — the arrows
    // never turn a page (< > do).
    state.cursor_down();
    assert_eq!(state.page, 0);
    assert_eq!(state.cursor, 0);
    // Page turns keep the slot but clamp it to a short page.
    state.cursor = 9;
    state.page_down();
    assert_eq!(state.page, 1);
    assert_eq!(state.cursor, 1, "the last page holds two rows");
    state.page_up();
    assert_eq!(state.page, 0);
    assert_eq!(state.cursor, 1);
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
    assert_eq!(
        map(65293),
        EmojiKey::Select(state.cursor),
        "Enter picks the highlighted row"
    );
    assert_eq!(map(65421), EmojiKey::Select(state.cursor), "KP_Enter too");
    assert_eq!(map(65307), EmojiKey::Cancel);
    assert_eq!(map(65288), EmojiKey::Backspace);
    assert_eq!(map(65361), EmojiKey::PageLeft, "Left turns the page back");
    assert_eq!(map(65363), EmojiKey::PageRight, "Right turns the page");
    assert_eq!(map(65362), EmojiKey::CursorUp, "Up moves the highlight");
    assert_eq!(map(65364), EmojiKey::CursorDown, "Down moves the highlight");
    assert_eq!(map(65365), EmojiKey::PageUp);
    assert_eq!(map(65366), EmojiKey::PageDown);
    assert_eq!(map(0x61), EmojiKey::Query('a'), "letters search");
    assert_eq!(map(0x20), EmojiKey::Query(' '), "space searches");
    assert_eq!(map(0x5f), EmojiKey::Query('_'), "underscore searches");
    assert_eq!(map(0x3b), EmojiKey::Ignore, "punctuation closes");
    assert_eq!(map(0xff51), EmojiKey::PageLeft, "arrows navigate now");
}

/// The auxiliary hint line under the emoji popup follows the engine
/// language — the owner found it Vietnamese after switching to English.
#[test]
fn emoji_menu_auxiliary_follows_the_language() {
    use super::emoji_picker::EmojiPickerState;
    let mut en = EmojiPickerState::with_lang(emoji_rows(), Vec::new(), true);
    assert!(
        en.auxiliary().contains("type to search"),
        "{}",
        en.auxiliary()
    );
    for c in "lua".chars() {
        en.query.push(c);
    }
    assert!(en.auxiliary().contains("results"), "{}", en.auxiliary());
    let vi = EmojiPickerState::with(emoji_rows(), Vec::new());
    assert!(vi.auxiliary().contains("gõ chữ"), "{}", vi.auxiliary());
}

/// The panel refreshes when the config file changes, on the engine's own
/// loop pass — no key event or IBus restart needed (a GUI toggle reaches
/// the panel within a second). GNOME applies only per-item UpdateProperty
/// mid-session; a full RegisterProperties re-register is ignored there.
#[test]
fn config_reload_refreshes_the_panel_without_a_key_event() {
    let config_path = Config::path().expect("a config path");
    if !config_path.exists() {
        eprintln!("server: SKIP (no config.toml to watch)");
        return;
    }
    // maybe_reload_config flips the process-wide debug-log state (it
    // mirrors the config): hold the same guard the log-asserting tests
    // hold, or a parallel test's log capture gets clobbered mid-flight.
    let _guard = crate::debug_log::test_guard();
    let engine: SharedEngine = Arc::new(Mutex::new(Engine::new(Method::Telex, Mode::Preedit)));
    lock_engine(&engine).set_capabilities(1);
    let mut handler = EngineHandler {
        engine: Arc::clone(&engine),
        injection: Injection::new(),
        // A stale clock: maybe_reload_config gets past its 1s gate.
        last_config_check: Instant::now() - Duration::from_secs(2),
        // Older than the real file's mtime: the first pass sees a change.
        last_config_mtime: Some(SystemTime::UNIX_EPOCH),
        hotkeys: Vec::new(),
        last_fire: Vec::new(),
        picker: None,
        emoji_picker: None,
        burst: BurstWatch::default(),
        burst_buffer: BurstBuffer::default(),
        bounce_deadline: None,
        stash: None,
        focus_ext: None,
    };
    let mut out = Vec::new();
    handler.maybe_reload_config(&mut out);
    let members: Vec<String> = out
        .iter()
        .map(|m| m.member().map(|m| m.to_string()).unwrap_or_default())
        .collect();
    // String::as_str via UFCS: `m.as_str()` with m: &&String resolves to
    // dbus's RefArg::as_str (impl for &T, returns Option<&str>) before the
    // autoderef chain reaches the inherent method — RefArg is in scope here.
    let updates = members
        .iter()
        .filter(|m| String::as_str(m) == "UpdateProperty")
        .count();
    assert!(
        updates >= 5,
        "the panel gets per-item UpdateProperty on the loop pass: {members:?}"
    );
    assert!(
        !members
            .iter()
            .any(|m| String::as_str(m) == "RegisterProperties"),
        "a mid-session re-register is ignored by GNOME — per-item updates only: {members:?}"
    );
}

#[test]
fn self_heal_picks_uinput_up_mid_session() {
    // The Task 3 ruling's binding deliverable at the wiring level: a
    // keystroke must retry the Uinput denial and re-select through the
    // holder — the old static probe's mid-session ACL pickup, restored.
    // The real constructor runs, so skip on uinput-less machines like the
    // device probe test does.
    if !crate::uinput::Uinput::available() {
        return;
    }
    let mut engine = Engine::new(Method::Telex, Mode::Preedit);
    engine.set_uinput_available(false);
    let mut injection = Injection::new();
    // Everything denied, nothing held: the shape left by an environment
    // where every constructor failed once.
    injection.select_with(&[Kind::Uinput], &|_: Kind| None);
    let mut out = Vec::new();
    assert!(super::process_key(
        &mut engine,
        &mut injection,
        &mut BurstWatch::default(),
        &mut BurstBuffer::default(),
        KeyEvent {
            keyval: 0x61,
            keycode: 0,
            state: 0,
        },
        &mut out
    ));
    assert_eq!(injection.current(), Some(Kind::Uinput));
    assert!(engine.uinput_available());
}

#[test]
fn flow_fallback_re_requests_the_backend_on_later_words() {
    // The paced re-request walks the real constructor table, so the story
    // only completes where a constructor can answer — the same skip the
    // self-heal test uses. The flag is raised at the engine level and the
    // arm is driven directly: inside process_key the top self-heal would
    // heal the real device before any word could resolve downgraded.
    if !crate::uinput::Uinput::available() {
        return;
    }
    let _guard = crate::debug_log::test_guard();
    let mut engine = Engine::new(Method::Telex, Mode::Flow);
    engine.set_client(Some("gtk4-im:test".into()));
    engine.set_capabilities(1); // CAP_PREEDIT_TEXT
    engine.set_uinput_available(false);
    let mut injection = Injection::new();
    // Word 1 starts with no backend: the walk downgrades Flow to the
    // forwarded transport and raises the flag the re-request reads.
    let _ = engine.process_char('v');
    assert!(engine.flow_needs_backend(), "word 1 resolved downgraded");
    // A later Flow word re-requests the backend; the flags follow the
    // holder's answer whichever kind the environment offers.
    flow_fallback_re_request(&mut engine, &mut injection);
    assert!(
        injection.current().is_some(),
        "the re-request built a backend"
    );
    assert!(
        engine.uinput_available(),
        "flags refreshed from the holder's answer"
    );
    assert_eq!(
        engine.injection_relay_safe(),
        matches!(
            injection.current(),
            Some(Kind::Libei | Kind::WlrVirtualKeyboard)
        ),
        "relay-safe must track the built kind"
    );
    // The arm must not clear the flag itself: the word in flight rides its
    // locked downgrade until the next resolution.
    assert!(engine.flow_needs_backend());
    let _ = engine.process_char(' ');
    let _ = engine.process_char('n');
    assert!(
        !engine.flow_needs_backend(),
        "the next word resolves through the walk with the backend"
    );
}

#[test]
fn preferred_kind_maps_the_config_variants() {
    let mut c = crate::config::Config::default();
    assert_eq!(preferred_kind(&c), None, "auto is the environment matrix");
    for (value, want) in [
        (
            crate::config::PreferredBackendConfig::Uinput,
            Some(Kind::Uinput),
        ),
        (
            crate::config::PreferredBackendConfig::XTest,
            Some(Kind::XTest),
        ),
        (
            crate::config::PreferredBackendConfig::WlrVirtualKeyboard,
            Some(Kind::WlrVirtualKeyboard),
        ),
        (
            crate::config::PreferredBackendConfig::Libei,
            Some(Kind::Libei),
        ),
    ] {
        c.preferred_backend = value;
        assert_eq!(preferred_kind(&c), want);
    }
}

#[test]
fn backend_selection_log_names_the_role() {
    assert_eq!(
        backend_selection_log(Some(Kind::Uinput), None),
        "injection backend: uinput (matrix)"
    );
    assert_eq!(
        backend_selection_log(Some(Kind::XTest), Some(Kind::XTest)),
        "injection backend: xtest (preferred)"
    );
    assert_eq!(
        backend_selection_log(Some(Kind::Uinput), Some(Kind::XTest)),
        "injection backend: uinput (fallback — xtest denied)"
    );
    assert_eq!(
        backend_selection_log(None, Some(Kind::XTest)),
        "injection backend: none (fallback — xtest denied)"
    );
}

// --- The suggestion strip on the IBus lookup table (server plumbing) ---

/// The seven-item strip list the paging tests share: two pages, the second
/// holding two entries — the partial last page the labels must survive.
fn strip_items() -> Vec<String> {
    ["một", "hai", "ba", "bốn", "năm", "sáu", "bảy"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The decoded `IBusLookupTable` payload a strip table carries — page size,
/// cursor, cursor visible, round, orientation, candidate texts, label
/// texts. A table read back off the wire arrives as a `VecDeque` of boxed
/// fields: [type tag, property map, page size, cursor, cursor visible,
/// round, orientation, candidates, labels]; the first two lead and are
/// dropped.
fn table_parts(
    table: &Variant<Box<dyn RefArg>>,
) -> (u64, u64, bool, bool, i64, Vec<String>, Vec<String>) {
    /// The IBusText payloads of one av field of a wire-decoded table.
    fn texts(field: &(dyn RefArg + 'static)) -> Vec<String> {
        let list = field
            .as_any()
            .downcast_ref::<Vec<Variant<Box<dyn RefArg>>>>()
            .expect("av field");
        list.iter()
            .map(|v| text_from_variant(v).expect("IBusText"))
            .collect()
    }
    let fields = table
        .0
        .as_any()
        .downcast_ref::<std::collections::VecDeque<Box<dyn RefArg>>>()
        .unwrap_or_else(|| panic!("IBusLookupTable fields: {:?}", table.0));
    let flag = |i: usize| {
        fields[i]
            .as_any()
            .downcast_ref::<bool>()
            .copied()
            .expect("bool field")
    };
    (
        fields[2].as_u64().expect("page size"),
        fields[3].as_u64().expect("cursor"),
        flag(4),
        flag(5),
        fields[6].as_i64().expect("orientation"),
        texts(&fields[7]),
        texts(&fields[8]),
    )
}

/// The members of one outgoing batch, for shape assertions.
fn members(out: &[Message]) -> Vec<String> {
    out.iter()
        .filter_map(|m| m.member().map(|m| m.to_string()))
        .collect()
}

/// The strip state is per-thread; reset it so a test that needs a closed
/// strip starts from one regardless of test order.
fn strip_reset_for_test() {
    STRIP.with(|cell| *cell.borrow_mut() = None);
}

/// A handler over a plain Telex/Preedit engine with the preedit capability.
/// The config stays at its defaults unless the test applies its own: the
/// strip's `suggest_words` switch rides the experimental master gate (the
/// engine bakes `experimental && suggest_words`), so a test that turns the
/// strip on turns the gate on with it.
fn strip_handler(suggest_words: bool) -> (SharedEngine, EngineHandler) {
    let engine: SharedEngine = Arc::new(Mutex::new(Engine::new(Method::Telex, Mode::Preedit)));
    {
        let mut e = lock_engine(&engine);
        e.apply_config(&Config {
            mode: Mode::Preedit.into(),
            suggest_words,
            experimental_features: suggest_words,
            ..Config::default()
        });
        e.set_method(Method::Telex);
        e.set_capabilities(1);
        if suggest_words {
            // The strip opens only with a model now (the dictionary left it):
            // pin the committed fixture instead of apply_config's auto-load —
            // that one picks up whatever file the host carries and would
            // make these shapes machine-dependent.
            e.model = Some(crate::engine::tests::sample_model());
        }
    }
    let handler = EngineHandler {
        engine: Arc::clone(&engine),
        injection: Injection::new(),
        // The hot-reload check stays past the horizon: the gui sandbox test
        // swaps XDG_CONFIG_HOME process-wide while running, and a reload
        // mid-test would swap this engine's switches under its own asserts.
        last_config_check: Instant::now() + std::time::Duration::from_secs(3600),
        last_config_mtime: None,
        hotkeys: Vec::new(),
        last_fire: Vec::new(),
        picker: None,
        emoji_picker: None,
        burst: BurstWatch::default(),
        burst_buffer: BurstBuffer::default(),
        bounce_deadline: None,
        stash: None,
        focus_ext: None,
    };
    (engine, handler)
}

/// One ProcessKeyEvent press through the handler: (consumed, signals).
fn press(handler: &mut EngineHandler, keyval: u32) -> (bool, Vec<Message>) {
    press_with_state(handler, keyval, 0)
}

/// One ProcessKeyEvent press with a modifier state (Ctrl combos).
fn press_with_state(handler: &mut EngineHandler, keyval: u32, state: u32) -> (bool, Vec<Message>) {
    let msg = engine_msg("ProcessKeyEvent").append3(keyval, 0u32, state);
    let mut out = Vec::new();
    let reply = handler.process_key_call(&msg, &mut out);
    (reply.read1::<bool>().expect("bool reply"), out)
}

#[test]
fn update_candidates_renders_the_full_list() {
    strip_reset_for_test();
    let msgs = emitted(&[Action::UpdateCandidates {
        word: "th".into(),
        items: strip_items(),
    }]);
    assert_eq!(
        members(&msgs),
        vec!["UpdateAuxiliaryText", "UpdateLookupTable"],
        "the box's input line leads, the table follows"
    );
    let (table, visible) = msgs[1]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table + visible");
    assert!(visible);
    let (page_size, cursor, cursor_visible, round, orientation, cands, labels) =
        table_parts(&table);
    assert_eq!(page_size, 5);
    assert_eq!(cursor, 0, "page 0 at open");
    assert!(cursor_visible);
    // round=false: the server clamps the pager at both ends, so the panel
    // hides the prev/next buttons there instead of wrapping.
    assert!(!round);
    assert_eq!(orientation, 0, "horizontal: one row of labeled entries");
    assert_eq!(
        cands,
        ["một", "hai", "ba", "bốn", "năm", "sáu", "bảy"],
        "the full list travels at once — the panel's page count then renders the pager buttons"
    );
    assert_eq!(labels, ["1.", "2.", "3.", "4.", "5."]);
}

#[test]
fn hide_candidates_hides_and_clears_the_strip() {
    strip_reset_for_test();
    let msgs = emitted(&[
        Action::UpdateCandidates {
            word: "th".into(),
            items: strip_items(),
        },
        Action::HideCandidates,
    ]);
    assert_eq!(
        members(&msgs),
        vec![
            "UpdateAuxiliaryText",
            "UpdateLookupTable",
            "HideAuxiliaryText",
            "HideLookupTable"
        ]
    );
    // The state cleared with the hide: a second close is silent instead of
    // pushing a stray signal (the engine's close racing the server's own
    // teardown must not double).
    let msgs = emitted(&[Action::HideCandidates]);
    assert!(msgs.is_empty(), "{:?}", members(&msgs));
}

#[test]
fn the_strip_keeps_the_word_in_both_places() {
    strip_reset_for_test();
    // The shipped display is both places: the batch that opens the strip
    // sends the word as the client's inline preedit AND as the box's input
    // line beside the table. The first letter opens the strip (1-char
    // contract).
    let (_engine, mut handler) = strip_handler(true);
    let (_, out) = press(&mut handler, 0x73); // 's'
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "the first key opens the strip with the word in both places"
    );
    let preedit = out[0]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("preedit variant");
    assert_eq!(
        text_from_variant(&preedit).as_deref(),
        Some("s"),
        "the client's field shows the word-in-flight"
    );
    let aux = out[1]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("auxiliary variant");
    assert_eq!(
        text_from_variant(&aux).as_deref(),
        Some("s"),
        "the word-in-flight also lives in the box's input line"
    );
    // Both surfaces track every word change.
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "the second letter updates both surfaces"
    );
    let aux = out[1]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("auxiliary variant");
    assert_eq!(
        text_from_variant(&aux).as_deref(),
        Some("si"),
        "the box's input line follows the word"
    );
}

#[test]
fn backspace_tracks_the_box_line_and_the_list() {
    strip_reset_for_test();
    // The box is the word's only display: a Backspace shrinks the box's
    // input line and the list follows the shorter word — the client's
    // field stays clean the whole time. The strip stays open at one letter
    // now, so the deletion lands on the box line with the list re-served.
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, out) = press(&mut handler, 0x69); // 'i' — the strip is up
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "the strip is up: {:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, keymap::BACKSPACE);
    assert!(consumed, "backspace edits the word inside the box");
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "both surfaces track the deletion"
    );
    let aux = out[1]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("auxiliary variant");
    assert_eq!(
        text_from_variant(&aux).as_deref(),
        Some("s"),
        "the box's input line follows the word down"
    );
}

#[test]
fn the_mirror_display_shows_the_word_in_both_places() {
    strip_reset_for_test();
    // The mirror choice: the word-in-flight lives in both places at once —
    // the client's inline preedit AND the box's input line — one source,
    // both surfaces, synced per keystroke.
    let (engine, mut handler) = strip_handler(true);
    {
        let mut e = lock_engine(&engine);
        e.apply_config(&Config {
            suggest_display: SuggestDisplayConfig::Mirror,
            suggest_words: true,
            experimental_features: true,
            ..Config::default()
        });
        // apply_config re-loads the model from disk — a None here (no file
        // installed, or the gui sandbox holding XDG_CONFIG_HOME right now)
        // would wipe the fixture injection and the strip would live on the
        // emoji band alone. Pin the fixture back.
        e.model = Some(crate::engine::tests::sample_model());
    }
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "both surfaces ride the same batch"
    );
    let inline = out[0]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("preedit variant");
    let aux = out[1]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("auxiliary variant");
    assert_eq!(
        text_from_variant(&inline).as_deref(),
        Some("si"),
        "the inline preedit carries the word"
    );
    assert_eq!(
        text_from_variant(&aux).as_deref(),
        Some("si"),
        "the box's input line carries the same word"
    );
    // Backspace down to the single letter: the strip STAYS open there (the
    // 1-char contract serves the model's rows for one letter too) — both
    // surfaces keep tracking the shorter word.
    let (consumed, out) = press(&mut handler, keymap::BACKSPACE);
    assert!(consumed, "backspace edits the word inside the box");
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "both surfaces follow the word down to one letter"
    );
    let inline = out[0]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("preedit variant");
    assert_eq!(
        text_from_variant(&inline).as_deref(),
        Some("s"),
        "the remaining key shows inline"
    );
}

#[test]
fn the_classic_display_keeps_the_inline_preedit() {
    strip_reset_for_test();
    // The inline-only choice: the word stays inline in the client and the
    // table rides beside it — no floating input line anywhere.
    let (engine, mut handler) = strip_handler(true);
    {
        let mut e = lock_engine(&engine);
        e.apply_config(&Config {
            suggest_display: SuggestDisplayConfig::Inline,
            suggest_words: true,
            experimental_features: true,
            ..Config::default()
        });
    }
    let (_, _) = press(&mut handler, 0x74); // 't'
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert_eq!(
        members(&out),
        vec!["UpdatePreeditText", "UpdateLookupTable"],
        "classic: the preedit stays inline, no auxiliary line"
    );
}

#[test]
fn minus_equals_end_the_word_while_the_strip_is_up() {
    strip_reset_for_test();
    // `-`/`=` are word terminators again: the strip owns no paging keys —
    // pressing one while the strip is up ends the word exactly like a
    // non-label digit (commit, box closes) and no paging signal exists
    // anywhere.
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "the strip is up: {:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, 0x2d); // '-' — a terminator again
    assert!(!consumed, "the terminator belongs to the app");
    let members_seen = members(&out);
    assert!(
        members_seen.contains(&"CommitText".to_string())
            && members_seen.contains(&"HideAuxiliaryText".to_string())
            && members_seen.contains(&"HideLookupTable".to_string())
            && !members_seen.contains(&"UpdateLookupTable".to_string()),
        "the word ends, the box closes, no paging: {:?}",
        members_seen
    );
}

#[test]
fn page_keys_end_the_word_while_the_strip_is_up() {
    strip_reset_for_test();
    // PageUp/PageDown keep their app role: while the strip is up they end
    // the word exactly like the arrows — no paging, the box closes.
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x74); // 't'
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "the strip is up: {:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, keymap::PAGE_DOWN);
    assert!(!consumed, "the page key belongs to the app");
    let members_seen = members(&out);
    assert!(
        members_seen.contains(&"CommitText".to_string())
            && members_seen.contains(&"HideAuxiliaryText".to_string())
            && members_seen.contains(&"HideLookupTable".to_string())
            && !members_seen.contains(&"UpdateLookupTable".to_string()),
        "the word ends, the box closes, no paging: {:?}",
        members_seen
    );
}

#[test]
fn ctrl_comma_and_ctrl_period_page_the_strip() {
    strip_reset_for_test();
    // Ctrl+. / Ctrl+, are the keyboard flavor of the popup's ◀▶ buttons:
    // page forward (clamped), page back — and with the strip closed the
    // combo passes through untouched.
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert!(members(&out).iter().any(|m| m == "UpdateLookupTable"));
    let (consumed, out) = press_with_state(&mut handler, keymap::PERIOD, keymap::CONTROL_MASK);
    assert!(consumed, "Ctrl+. pages while the strip is up");
    assert_eq!(members(&out), vec!["UpdateLookupTable"]);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 5, "Ctrl+. turns to page 1");
    // The fixture list holds six entries (two pages): the second press
    // CLAMPS at the last page head, and two Ctrl+, walk back to page 0.
    let (consumed, out) = press_with_state(&mut handler, keymap::PERIOD, keymap::CONTROL_MASK);
    assert!(consumed);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 5, "Ctrl+. clamps at the last page head");
    let (consumed, out) = press_with_state(&mut handler, keymap::COMMA, keymap::CONTROL_MASK);
    assert!(consumed);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 0, "Ctrl+, walks back");
    let (consumed, out) = press_with_state(&mut handler, keymap::COMMA, keymap::CONTROL_MASK);
    assert!(consumed);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 0, "Ctrl+, stays on page 0 at the clamp");
    // Without Ctrl, `.` ends the word like any terminator.
    let (consumed, out) = press(&mut handler, keymap::PERIOD);
    assert!(!consumed, "plain `.` belongs to the app");
    let members_seen = members(&out);
    assert!(
        members_seen.contains(&"CommitText".to_string())
            && !members_seen.contains(&"UpdateLookupTable".to_string()),
        "the word ends, no paging: {:?}",
        members_seen
    );
    // With the strip closed the combo passes through untouched.
    let (consumed, out) = press_with_state(&mut handler, keymap::PERIOD, keymap::CONTROL_MASK);
    assert!(!consumed, "a closed strip never sees the combo");
    assert!(members(&out).is_empty(), "{:?}", members(&out));
}

#[test]
fn arrows_fall_through_while_the_strip_is_visible() {
    strip_reset_for_test();
    // The arrows are caret navigation again — the strip never owns them: a
    // LEFT while the strip is up ends the word and moves the caret, exactly
    // the closed-strip flow (commit rides the reply, the strip closes with
    // the word's end).
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x74); // 't'
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "the strip is up: {:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, keymap::LEFT);
    assert!(!consumed, "the arrow belongs to the caret");
    let members_seen = members(&out);
    assert!(
        members_seen.contains(&"CommitText".to_string()),
        "the word ends on the arrow: {:?}",
        members_seen
    );
    assert!(
        !members_seen.contains(&"UpdateLookupTable".to_string()),
        "no paging happened: {:?}",
        members_seen
    );
}

#[test]
fn panel_pager_methods_page_the_strip() {
    strip_reset_for_test();
    // The candidate popup's ◀▶ buttons forward PageUp/PageDown engine
    // methods; they page the strip the same way the keys do (ibus
    // intelligent pinyin's popup paging). A wordless engine has no strip:
    // the method is a silent no-op.
    let (_engine, mut handler) = strip_handler(false);
    let (_, _) = press(&mut handler, 0x74); // 't'
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("PageDown"), "PageDown", &mut out);
    assert!(
        !members(&out).contains(&"UpdateLookupTable".to_string()),
        "no strip, no table: {:?}",
        members(&out)
    );
    // Seed the strip through the funnel the engine's actions ride, then
    // page it through the panel's methods.
    let (engine, mut handler) = strip_handler(false);
    let mut out = Vec::new();
    {
        let mut e = lock_engine(&engine);
        emit(
            &mut e,
            &Injection::new(),
            &[Action::UpdateCandidates {
                word: "th".into(),
                items: strip_items(),
            }],
            &mut out,
        );
    }
    assert_eq!(
        members(&out),
        vec!["UpdateAuxiliaryText", "UpdateLookupTable"]
    );
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("PageDown"), "PageDown", &mut out);
    assert_eq!(members(&out), vec!["UpdateLookupTable"]);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 5, "the panel's next button turns to page 1");
    // The seeded list has seven items: the second PageDown clamps on the
    // partial last page instead of walking past it.
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("PageDown"), "PageDown", &mut out);
    assert_eq!(members(&out), vec!["UpdateLookupTable"]);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 5, "the last page clamps");
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("PageUp"), "PageUp", &mut out);
    assert_eq!(members(&out), vec!["UpdateLookupTable"]);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, ..) = table_parts(&table);
    assert_eq!(cursor, 0, "the panel's prev button walks back");
}

#[test]
fn candidate_clicked_picks_through_the_strip() {
    strip_reset_for_test();
    // A click on a candidate row forwards CandidateClicked(index): the
    // global index commits, same as pressing the entry's digit. The picked
    // text is read back off the rendered table, never a literal.
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x74); // 't'
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert!(members(&out).iter().any(|m| m == "UpdateLookupTable"));
    let table_msg = out
        .iter()
        .find(|m| m.member() == Some("UpdateLookupTable".into()))
        .expect("the strip's table");
    let (table, _) = table_msg
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (.., cands, _) = table_parts(&table);
    assert!(!cands.is_empty(), "candidates on screen: {cands:?}");
    let mut out = Vec::new();
    let msg = engine_msg("CandidateClicked").append3(0u32, 1u32, 0u32);
    handler.engine_call(&msg, "CandidateClicked", &mut out);
    let members_seen = members(&out);
    assert!(
        members_seen.contains(&"HideLookupTable".to_string())
            && members_seen.contains(&"CommitText".to_string()),
        "the click picks: {:?}",
        members_seen
    );
    let committed = out
        .iter()
        .find(|m| m.member() == Some("CommitText".into()))
        .and_then(|m| m.read1::<Variant<Box<dyn RefArg>>>().ok())
        .map(|v| text_from_variant(&v))
        .unwrap_or_else(|| panic!("the clicked entry commits: {:?}", members_seen));
    assert_eq!(
        committed.as_deref(),
        Some(format!("{} ", cands[0]).as_str()),
        "the clicked row commits (plus the pick's space), not the raw keys"
    );
}

#[test]
fn digits_pick_while_the_strip_is_visible() {
    strip_reset_for_test();
    // The real flow: a suggest-enabled engine opens the strip on the FIRST
    // letter, and digit 1 picks its first candidate. The committed text is
    // read back off the rendered table, never a literal — the model builds
    // the list, the label-to-entry mapping is the contract.
    let (_engine, mut handler) = strip_handler(true);
    let (_, out) = press(&mut handler, 0x73); // 's'
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "the first key opens the strip as the floating input box"
    );
    let (table, _) = out[2]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, _, _, _, _, cands, _) = table_parts(&table);
    assert!(
        !cands.is_empty(),
        "the first page offered candidates: {cands:?}"
    );
    // Digit 1 picks: the candidate commits off the emitted list, the box
    // closes, the composition resets.
    let (consumed, out) = press(&mut handler, 0x31); // '1'
    assert!(consumed, "a pick consumes the digit");
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "HideAuxiliaryText",
            "HideLookupTable",
            "CommitText"
        ]
    );
    let committed = out[3]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("CommitText variant");
    assert_eq!(
        text_from_variant(&committed).as_deref(),
        Some(format!("{} ", cands[0]).as_str()),
        "the labeled first candidate commits (plus the pick's space), not the raw keys"
    );
    // The strip is closed: the next word types fresh — `z` matches no model
    // row and no emoji keyword, so the strip stays closed.
    let (_, out) = press(&mut handler, 0x7a); // 'z'
    assert!(
        !members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "{:?}",
        members(&out)
    );
}

#[test]
fn digits_pick_while_the_strip_is_paged() {
    strip_reset_for_test();
    // Page 1 through the popup's ◀▶ button (its PageDown method): digit 1
    // picks that page's first slot — the global index 5 the page mapping
    // promises. The committed text is read back off the rendered table,
    // never a literal (see the visible pick above): paging shifts the
    // mapping, the label-to-entry contract does not move.
    let (_engine, mut handler) = strip_handler(true);
    let (_, out) = press(&mut handler, 0x73); // 's'
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "the strip opens with the word in both places"
    );
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "UpdateAuxiliaryText",
            "UpdateLookupTable"
        ],
        "the second letter updates both surfaces"
    );
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("PageDown"), "PageDown", &mut out);
    assert_eq!(members(&out), vec!["UpdateLookupTable"]);
    let (table, _) = out[0]
        .read2::<Variant<Box<dyn RefArg>>, bool>()
        .expect("table");
    let (_, cursor, .., cands, _) = table_parts(&table);
    assert_eq!(cursor, 5, "the cursor marks the page head");
    assert!(
        cands.len() > 5,
        "page 1 holds entries past the first five: {cands:?}"
    );
    let (consumed, out) = press(&mut handler, 0x31); // '1'
    assert!(consumed, "a pick on page 1 consumes the digit");
    assert_eq!(
        members(&out),
        vec![
            "UpdatePreeditText",
            "HideAuxiliaryText",
            "HideLookupTable",
            "CommitText"
        ]
    );
    let committed = out[3]
        .read1::<Variant<Box<dyn RefArg>>>()
        .expect("CommitText variant");
    assert_eq!(
        text_from_variant(&committed).as_deref(),
        Some(format!("{} ", cands[5]).as_str()),
        "label 1 on page 1 commits the list's sixth entry (global index 5), plus the pick's space"
    );
}

#[test]
fn digits_outside_the_labels_fall_through_while_the_strip_is_visible() {
    strip_reset_for_test();
    // 0 and 6-9 name no strip label: the key falls through unconsumed and
    // ends the word exactly as with the strip closed — the digit enters the
    // composition and the word's end closes the strip server-side (`si` is
    // no model token, so the landing has no next-word list to chain into).
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "the strip is up: {:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, 0x37); // '7'
    assert!(!consumed, "a non-label digit reaches the composition");
    assert!(
        members(&out).iter().any(|m| m == "HideLookupTable"),
        "the digit entered the word, the strip hides: {:?}",
        members(&out)
    );
    let committed = out
        .iter()
        .find(|m| m.member() == Some("CommitText".into()))
        .and_then(|m| m.read1::<Variant<Box<dyn RefArg>>>().ok())
        .map(|v| text_from_variant(&v))
        .unwrap_or_else(|| panic!("the word ends on the digit: {:?}", members(&out)));
    // The raw keys commit and the digit itself follows as the forwarded key
    // (the digit terminator never commits a candidate — no pick happened).
    assert_eq!(
        committed,
        Some("si".into()),
        "the in-flight word commits as typed, the digit follows"
    );
    // 0 rides the same path: the strip reopens with the next word and the
    // non-label digit falls through again.
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, out) = press(&mut handler, 0x69); // 'i'
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "the strip reopens: {:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, 0x30); // '0'
    assert!(!consumed, "0 is no label either");
    assert!(
        members(&out).iter().any(|m| m == "HideLookupTable"),
        "the digit entered the word, the strip hides: {:?}",
        members(&out)
    );
}

#[test]
fn a_digit_beyond_the_list_falls_through_to_the_engine() {
    strip_reset_for_test();
    // The wordless-engine race: the funnel still shows page 1 of a seeded
    // list while the engine has nothing in flight, so every accept is
    // refused — label 5's global index (9) is past any real list.
    let (engine, mut handler) = strip_handler(false);
    let mut out = Vec::new();
    {
        let mut e = lock_engine(&engine);
        emit(
            &mut e,
            &Injection::new(),
            &[Action::UpdateCandidates {
                word: "th".into(),
                items: strip_items(),
            }],
            &mut out,
        );
    }
    let mut page = Vec::new();
    handler.engine_call(&engine_msg("PageDown"), "PageDown", &mut page);
    assert_eq!(
        members(&page),
        vec!["UpdateLookupTable"],
        "paging still works for the seeded strip"
    );
    // Label 5 on page 1 → global index 9 → the engine refuses → today's
    // key path (a wordless digit forwards, nothing commits).
    let (consumed, out) = press(&mut handler, 0x35); // '5'
    assert!(!consumed, "the refused digit falls through");
    assert!(
        members(&out).is_empty(),
        "no pick, no commit, no hide: {:?}",
        members(&out)
    );
}

#[test]
fn strip_keys_type_normally_when_the_strip_is_closed() {
    strip_reset_for_test();
    // Byte-identical to today's engine, captured from the pre-strip
    // behavior: a wordless engine forwards arrows and digits with no
    // signals; a word in flight commits on the arrow and ends on the digit.
    let (_engine, mut handler) = strip_handler(false);
    let (consumed, out) = press(&mut handler, keymap::RIGHT);
    assert!(!consumed);
    assert!(members(&out).is_empty(), "{:?}", members(&out));
    let (consumed, out) = press(&mut handler, 0x31); // '1'
    assert!(!consumed);
    assert!(members(&out).is_empty(), "{:?}", members(&out));
    let (consumed, out) = press(&mut handler, 0x74); // 't'
    assert!(consumed);
    assert_eq!(members(&out), vec!["UpdatePreeditText"]);
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert_eq!(members(&out), vec!["UpdatePreeditText"]);
    // The arrow ends the word and passes through: commit rides the reply.
    let (consumed, out) = press(&mut handler, keymap::RIGHT);
    assert!(!consumed);
    assert_eq!(members(&out), vec!["UpdatePreeditText", "CommitText"]);
    let (consumed, out) = press(&mut handler, 0x31); // '1'
    assert!(!consumed);
    assert!(members(&out).is_empty(), "{:?}", members(&out));
}

#[test]
fn word_end_chains_or_hides_server_side() {
    strip_reset_for_test();
    let (_engine, mut handler) = strip_handler(true);
    // A model-token landing CHAINS: `sinh ` keeps the strip up with the
    // follower list — the space batch carries a fresh table, no hide.
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, _) = press(&mut handler, 0x69); // 'i'
    let (_, _) = press(&mut handler, 0x6e); // 'n'
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable"),
        "strip open: {:?}",
        members(&out)
    );
    let (_, out) = press(&mut handler, 0x20); // space
    assert!(
        members(&out).iter().any(|m| m == "UpdateLookupTable")
            && !members(&out).iter().any(|m| m == "HideLookupTable"),
        "the landing chains into the next-word list: {:?}",
        members(&out)
    );
    // A non-model landing HIDES: `si ` carries no follower list, so the
    // engine's HideCandidates closes the strip server-side with the commit.
    let (_, _) = press(&mut handler, 0x73); // 's'
    let (_, _) = press(&mut handler, 0x69); // 'i'
    let (_, out) = press(&mut handler, 0x20); // space
    assert!(
        members(&out).iter().any(|m| m == "HideLookupTable"),
        "the word end hides the strip: {:?}",
        members(&out)
    );
    // Closed again: the arrow flows as today (forwarded, no table).
    let (consumed, out) = press(&mut handler, keymap::RIGHT);
    assert!(!consumed);
    assert!(!members(&out).iter().any(|m| m == "UpdateLookupTable"));
}

#[test]
fn focus_out_and_reset_teardown_the_strip() {
    strip_reset_for_test();
    let (_engine, mut handler) = strip_handler(true);
    let (_, _) = press(&mut handler, 0x74); // 't'
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert!(members(&out).iter().any(|m| m == "UpdateLookupTable"));
    // FocusOut: the strip rides the picker's teardown — exactly one hide,
    // and the engine's own HideCandidates stays silent behind the taken
    // state instead of doubling it.
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("FocusOut"), "FocusOut", &mut out);
    assert_eq!(
        members(&out)
            .iter()
            .filter(|m| *m == "HideLookupTable")
            .count(),
        1,
        "{:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, keymap::RIGHT);
    assert!(!consumed, "the strip is closed: today's arrow flow");
    assert!(!members(&out).iter().any(|m| m == "UpdateLookupTable"));

    // Same teardown on a client Reset (a click elsewhere dismisses it).
    let (_, out) = press(&mut handler, 0x74); // 't'
    let _ = out;
    let (_, out) = press(&mut handler, 0x68); // 'h'
    assert!(members(&out).iter().any(|m| m == "UpdateLookupTable"));
    let mut out = Vec::new();
    handler.engine_call(&engine_msg("Reset"), "Reset", &mut out);
    assert_eq!(
        members(&out)
            .iter()
            .filter(|m| *m == "HideLookupTable")
            .count(),
        1,
        "{:?}",
        members(&out)
    );
    let (consumed, out) = press(&mut handler, keymap::LEFT);
    assert!(!consumed);
    assert!(!members(&out).iter().any(|m| m == "UpdateLookupTable"));
}

// --- ContentType (InputPurpose) gate ---

/// A `org.freedesktop.DBus.Properties.Set` call for one engine property,
/// the wire shape ibus-daemon uses for ContentType: (interface, name, (uu)).
fn properties_set_msg(prop: &str, purpose: u32, hints: u32) -> Message {
    use dbus::strings::{BusName, Interface, Member, Path};
    let mut msg = Message::method_call(
        &BusName::new(BUS_NAME).unwrap(),
        &Path::new(ENGINE_PATH).unwrap(),
        &Interface::new("org.freedesktop.DBus.Properties").unwrap(),
        &Member::new("Set").unwrap(),
    );
    msg.set_serial(1);
    msg.append3(
        ENGINE_IFACE.to_string(),
        prop.to_string(),
        dbus::arg::Variant(Box::new((purpose, hints))),
    )
}

#[test]
fn content_type_set_gates_password_fields() {
    // The daemon's ContentType write is the only purpose channel: PASSWORD
    // and PIN quiet the engine, TERMINAL (10) and FREE_FORM (0) do not, and
    // a Set of another property leaves the flag alone.
    let engine: SharedEngine = Arc::new(Mutex::new(Engine::new(Method::Telex, Mode::Preedit)));
    let mut handler = EngineHandler {
        engine: Arc::clone(&engine),
        injection: Injection::new(),
        last_config_check: Instant::now(),
        last_config_mtime: None,
        hotkeys: Vec::new(),
        last_fire: Vec::new(),
        picker: None,
        emoji_picker: None,
        burst: BurstWatch::default(),
        burst_buffer: BurstBuffer::default(),
        bounce_deadline: None,
        stash: None,
        focus_ext: None,
    };
    handler.properties_call(&properties_set_msg("ContentType", 8u32, 0u32), "Set");
    assert!(lock_engine(&engine).password_quiet(), "PASSWORD quiets");
    handler.properties_call(&properties_set_msg("ContentType", 9u32, 0u32), "Set");
    assert!(lock_engine(&engine).password_quiet(), "PIN quiets too");
    handler.properties_call(&properties_set_msg("ContentType", 10u32, 0u32), "Set");
    assert!(
        !lock_engine(&engine).password_quiet(),
        "TERMINAL keeps ordinary typing"
    );
    handler.properties_call(&properties_set_msg("ContentType", 0u32, 0u32), "Set");
    assert!(
        !lock_engine(&engine).password_quiet(),
        "FREE_FORM opens again"
    );
    handler.properties_call(&properties_set_msg("Capabilities", 0u32, 0u32), "Set");
    assert!(
        !lock_engine(&engine).password_quiet(),
        "a Set of another property is inert"
    );
}
