use super::*;
use crate::config::{MethodConfig, ModeConfig};

/// Feed keystrokes like a real client: it reports capabilities and a
/// SetSurroundingText update after every visible change. The first report
/// lands at focus-in (the server sends RequireSurroundingText there), so
/// the walk's protocol-delete row serves the very first word too.
fn type_str(e: &mut Engine, s: &str) -> Vec<Action> {
    if e.client().is_none() {
        // Engine tests exercise the in-process-IM (non-relayed) path; the
        // relayed path has its own tests.
        e.set_client(Some("gtk4-im:test".into()));
    }
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e.set_surrounding(String::new(), 0);
    let mut buf: Vec<char> = Vec::new();
    let mut all = Vec::new();
    for c in s.chars() {
        let actions = e.process_char(c);
        for a in &actions {
            match a {
                Action::DeleteBefore(n) => {
                    buf.truncate(buf.len().saturating_sub(*n as usize));
                }
                Action::DeleteSurrounding { offset, n } => {
                    let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                    let end = (start + *n as usize).min(buf.len());
                    buf.drain(start..end);
                }
                Action::Commit(t) => buf.extend(t.chars()),
                Action::CommitNow(t) => buf.extend(t.chars()),
                _ => {}
            }
        }
        let text: String = buf.iter().collect();
        e.set_surrounding(text, buf.len() as u32);
        all.extend(actions);
    }
    all
}

#[test]
fn surrounding_commits_and_rewrites() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    let actions = type_str(&mut e, "tieengs");
    // Each key is committed as it arrives; the tone key then rewrites the
    // unaccented tail in place.
    assert!(actions.contains(&Action::Commit("e".into())), "{actions:?}");
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::DeleteBefore(n) if *n > 0)));
    assert!(
        actions.contains(&Action::Commit("ếng".into())),
        "{actions:?}"
    );
    // Replaying the actions against a fake application buffer must
    // reproduce the typed word exactly.
    assert_eq!(replay(&actions), "tiếng");
}

/// Apply actions to a stand-in for the application's text field.
fn replay(actions: &[Action]) -> String {
    let mut buf: Vec<char> = Vec::new();
    for a in actions {
        match a {
            Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
            // ForwardBackspace mode: real Backspace key events, same net effect on
            // the buffer as a protocol delete.
            Action::ForwardBackspaces(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
            // IBus semantics: delete n chars starting at cursor+offset.
            Action::DeleteSurrounding { offset, n } => {
                let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                let end = (start + *n as usize).min(buf.len());
                buf.drain(start..end);
            }
            Action::Commit(s) => buf.extend(s.chars()),
            _ => {}
        }
    }
    buf.into_iter().collect()
}

/// Engine with a config whose mode matches the constructor argument.
/// `apply_config` would otherwise silently reset the mode to the config's
/// default (preedit), making a Surrounding test exercise preedit.
fn engine(mode: Mode, config: Config) -> Engine {
    let mut e = Engine::new(Method::Telex, mode);
    e.apply_config(&Config {
        mode: mode.into(),
        ..config
    });
    e
}

fn typed(mode: Mode, keys: &str) -> String {
    let mut e = engine(mode, Config::default());
    type_replay(&mut e, keys)
}

/// Feed a whole string, simulating the application buffer: a key whose
/// actions contain `Forward` inserts the raw character, everything else
/// applies the emitted commit/delete actions.
fn type_replay(e: &mut Engine, s: &str) -> String {
    type_replay_with_reports(e, s, true)
}

/// Like `type_replay`, with `\u{8}` meaning Backspace, and reporting
/// whether any delete asked for more characters than the buffer held —
/// the out-of-range delete that aborts mutter and hangs the desktop.
fn type_replay_backspaces(e: &mut Engine, script: &str) -> (String, bool) {
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    let mut buf: Vec<char> = Vec::new();
    let mut out_of_range = false;
    for step in script.chars() {
        let actions = if step == '\u{8}' {
            match e.backspace() {
                Some(actions) => actions,
                None => {
                    buf.pop();
                    e.set_surrounding(buf.iter().collect(), buf.len() as u32);
                    continue;
                }
            }
        } else {
            e.process_char(step)
        };
        let forwarded = actions.contains(&Action::Forward);
        for action in &actions {
            match action {
                Action::DeleteBefore(n) | Action::DeleteSurrounding { n, .. } => {
                    out_of_range |= *n as usize > buf.len();
                    buf.truncate(buf.len().saturating_sub(*n as usize));
                }
                Action::ForwardBackspaces(n) => {
                    out_of_range |= *n as usize > buf.len();
                    buf.truncate(buf.len().saturating_sub(*n as usize));
                }
                Action::Commit(text) => buf.extend(text.chars()),
                _ => {}
            }
        }
        if forwarded {
            // A forwarded Backspace deletes the application's last char —
            // a ForwardBackspaces action carried its own count above.
            if step == '\u{8}' {
                buf.pop();
            } else {
                buf.push(step);
            }
        }
        e.set_surrounding(buf.iter().collect(), buf.len() as u32);
    }
    (buf.into_iter().collect(), out_of_range)
}

/// Same, with `reports = false` for the no-transport environment
/// (Sublime/JetBrains-style clients that never send
/// SetSurroundingText; uinput is unavailable in tests, so the engine
/// has no way to delete and must keep the raw text).
fn type_replay_with_reports(e: &mut Engine, s: &str, reports: bool) -> String {
    if e.client().is_none() {
        e.set_client(Some("gtk4-im:test".into()));
    }
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    let mut buf: Vec<char> = Vec::new();
    for c in s.chars() {
        let actions = e.process_char(c);
        let forwarded = actions.contains(&Action::Forward);
        for a in &actions {
            match a {
                Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                // ForwardBackspace mode: real Backspace key events, same net effect on
                // the buffer as a protocol delete.
                Action::ForwardBackspaces(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                // IBus semantics: delete n chars starting at cursor+offset.
                Action::DeleteSurrounding { offset, n } => {
                    let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                    let end = (start + *n as usize).min(buf.len());
                    buf.drain(start..end);
                }
                Action::Commit(t) => buf.extend(t.chars()),
                _ => {}
            }
        }
        if forwarded {
            buf.push(c);
        }
        // Real clients report surrounding text after every change.
        if reports {
            let text: String = buf.iter().collect();
            e.set_surrounding(text, buf.len() as u32);
        }
    }
    buf.into_iter().collect()
}

/// Feed timed keystrokes like a real hold: sleep `ms` before each char,
/// replay the actions like `type_replay_with_reports` does. `\u{8}` means
/// Backspace. Returns the committed buffer and the last preedit text.
fn type_timed(e: &mut Engine, script: &[(u64, char)]) -> (String, String) {
    if e.client().is_none() {
        e.set_client(Some("gtk4-im:test".into()));
    }
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    let mut buf: Vec<char> = Vec::new();
    let mut preedit = String::new();
    for (ms, c) in script {
        std::thread::sleep(std::time::Duration::from_millis(*ms));
        let actions = if *c == '\u{8}' {
            match e.backspace() {
                Some(actions) => actions,
                None => {
                    buf.pop();
                    e.set_surrounding(buf.iter().collect(), buf.len() as u32);
                    continue;
                }
            }
        } else {
            e.process_char(*c)
        };
        let forwarded = actions.contains(&Action::Forward);
        for a in &actions {
            match a {
                Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                Action::ForwardBackspaces(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                Action::DeleteSurrounding { offset, n } => {
                    let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                    let end = (start + *n as usize).min(buf.len());
                    buf.drain(start..end);
                }
                Action::Commit(t) | Action::CommitNow(t) => buf.extend(t.chars()),
                Action::UpdatePreedit { text, .. } => preedit = text.clone(),
                Action::HidePreedit => preedit.clear(),
                _ => {}
            }
        }
        if forwarded {
            if *c == '\u{8}' {
                buf.pop();
            } else {
                buf.push(*c);
            }
        }
        let text: String = buf.iter().collect();
        e.set_surrounding(text, buf.len() as u32);
    }
    (buf.into_iter().collect(), preedit)
}

#[test]
fn preedit_never_commits_mid_word() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    let actions = type_str(&mut e, "tieeng");
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Commit(_))),
        "{actions:?}"
    );
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "tiêng")));
}

#[test]
fn preedit_commit_on_reset() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    type_str(&mut e, "vieejt");
    let actions = e.reset();
    assert!(
        actions.contains(&Action::Commit("việt".into())),
        "{actions:?}"
    );
    assert!(actions.contains(&Action::HidePreedit));
}

#[test]
fn preedit_mode_falls_back_without_client_support() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    // No CAP_PREEDIT_TEXT reported (raw loop — no simulated client).
    let actions: Vec<Action> = "as".chars().flat_map(|c| e.process_char(c)).collect();
    assert!(actions.iter().any(|a| matches!(a, Action::Commit(_))));
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { .. })));
}

#[test]
fn stale_surrounding_text_does_not_tear_down_the_word() {
    // Regression: typing fast around Chrome suggestions, a
    // SetSurroundingText report that predates our last commit used to be
    // treated as an app-side edit — the composition was torn down and
    // characters were eaten (`tùng` -> `tungf`). A stale report must be
    // ignored; the fresh one (sent right before the next keystroke) is
    // what verify works on.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    type_str(&mut e, "tie");

    // The tone key turns "tie" into "tiê" (delete 1 + commit); then a
    // STALE report from before that commit arrives, but no fresh one.
    let actions = e.process_char('e');
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Commit(t) if t.contains('ê'))),
        "{actions:?}"
    );
    e.set_surrounding("tie".into(), 3); // stale
    let actions = e.process_char('n');
    // The composition must survive: no resync to the stale "tie".
    assert_eq!(replay_from("tiê", &actions), "tiên");
    assert_eq!(e.wrapper.output(), "tiên");

    // A FRESH report (client acknowledged everything) enables verify
    // again, and a real app-side edit is still adopted.
    e.set_surrounding("tiên".into(), 4);
    let actions = e.process_char('g');
    assert_eq!(replay(&actions), "g");
    assert_eq!(e.wrapper.output(), "tiêng");
}

#[test]
fn no_surrounding_data_never_produces_a_delete() {
    // Regression for the mutter segfault: a client without preedit
    // support that has not reported SetSurroundingText yet must never
    // receive DeleteSurroundingText — mutter crashes applying a delete
    // to a buffer it never saw. The rewrites ride the forwarded
    // key-event transport instead, which needs no report.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    // Raw loop: no capabilities, no SetSurroundingText ever.
    let actions: Vec<Action> = "tieengs".chars().flat_map(|c| e.process_char(c)).collect();
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
        )),
        "{actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "the forwarded transport rewrites: {actions:?}"
    );
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn preedit_commits_the_word_before_the_terminator() {
    // Regression: the terminator used to clear the composition without
    // committing it, so every word typed in preedit mode vanished when the
    // user pressed space.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    type_str(&mut e, "vieejt");
    assert_eq!(
        e.process_char(' '),
        vec![
            Action::HidePreedit,
            Action::Commit("việt".into()),
            Action::Forward
        ]
    );
}

/// Apply actions to a buffer that already holds `prefix` (the client's
/// text before the key).
fn replay_from(prefix: &str, actions: &[Action]) -> String {
    let mut buf: Vec<char> = prefix.chars().collect();
    for a in actions {
        match a {
            Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
            // ForwardBackspace mode: real Backspace key events, same net effect on
            // the buffer as a protocol delete.
            Action::ForwardBackspaces(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
            // IBus semantics: delete n chars starting at cursor+offset.
            Action::DeleteSurrounding { offset, n } => {
                let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                let end = (start + *n as usize).min(buf.len());
                buf.drain(start..end);
            }
            // Real Backspace keys delete from the cursor, then the
            // commit re-types the common suffix.
            Action::RewriteByBackspace { backspaces, commit } => {
                buf.truncate(buf.len().saturating_sub(*backspaces as usize));
                buf.extend(commit.chars());
            }
            Action::Commit(t) => buf.extend(t.chars()),
            _ => {}
        }
    }
    buf.into_iter().collect()
}

#[test]
fn terminator_forwards_and_does_not_delete() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    type_str(&mut e, "vieejt");
    let actions = e.process_char(' ');
    assert!(actions.contains(&Action::Forward));
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "{actions:?}"
    );
}

#[test]
fn backspace_then_type_does_not_eat_a_letter() {
    // Regression: bamboo-core's EditDelta is relative to a stale
    // prev_preedit after remove_last_output_char, so the next keystroke
    // deleted an already-removed character ("tin" came out as "tn").
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    type_str(&mut e, "tie");
    e.backspace().expect("backspace");
    // The client's next SetSurroundingText reflects the deletion.
    e.set_surrounding("ti".into(), 2);
    let actions = e.process_char('n');
    // The app buffer held "ti" after the backspace; the new key must
    // only add its own character, not re-delete an old one.
    assert_eq!(replay_from("ti", &actions), "tin", "{actions:?}");
}

#[test]
fn backspace_then_tone_key_rewrites_correctly() {
    // unikey convention: the Backspace undoes the last KEY — here the
    // tone key "s" — and retyping it restores the word.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    let mut actions = type_str(&mut e, "tieengs"); // tiếng
    actions.extend(e.backspace().expect("backspace")); // tiêng
    e.set_surrounding("tiêng".into(), 5);
    actions.extend(e.process_char('s'));
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn backspace_on_empty_composition_forwards() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert!(e.backspace().is_none());
}

#[test]
fn backspace_deletes_one_char() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        mode: Mode::Surrounding.into(),
        backspace_tone: BackspaceToneConfig::UnikeyWalk,
        ..Config::default()
    });
    type_str(&mut e, "vieejt");
    assert_eq!(e.backspace(), Some(vec![Action::DeleteBefore(1)]));
}

#[test]
fn backspace_replays_when_the_tone_moves() {
    // unikey convention: each Backspace pops the last raw KEY and the
    // word recomposes ("hoàng" -> "hoang" -> "hoan" -> "hoa").
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    let mut actions = type_str(&mut e, "hoangf");
    assert_eq!(replay(&actions), "hoàng");
    for expected in ["hoang", "hoan", "hoa"] {
        actions.extend(e.backspace().expect("backspace"));
        e.set_surrounding(expected.to_string(), expected.chars().count() as u32);
        assert_eq!(replay(&actions), expected);
    }
}

#[test]
fn chromium_falls_back_to_preedit() {
    // Chrome drops DeleteSurroundingText and matches the default app
    // rule, so the word is shown inline instead of being rewritten.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk4-im:chrome".into()));
    let actions = type_str(&mut e, "tieengs");
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "{actions:?}"
    );
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "tiếng")));
    assert_eq!(
        e.reset(),
        vec![Action::HidePreedit, Action::Commit("tiếng".into())]
    );
}

#[test]
fn focus_change_drops_stale_surrounding_data() {
    // Regression for the GNOME Shell search-bar logout: the previous
    // client's surrounding data and caps must not leak into the next
    // context. Without the clear, the engine sent protocol
    // DeleteSurroundingText to a context whose buffer mutter never saw.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    for c in "tieng".chars() {
        e.process_char(c);
    }
    e.set_surrounding("tieng".into(), 5);
    assert!(e.surrounding_data_ok());
    // New focus: FocusIn clears the buffer data, the new context reports
    // caps without SURROUNDING_TEXT (e.g. mutter's own search entry).
    e.clear_buffer_state();
    e.set_capabilities(CAP_PREEDIT_TEXT);
    assert!(!e.surrounding_data_ok());
    // With no uinput the protocol delete must be skipped entirely, never
    // sent. The word rides its locked forwarded transport: the mark
    // rewrite erases with forwarded Backspace key events.
    let actions = e.process_char('s');
    assert_eq!(
        actions,
        vec![Action::ForwardBackspaces(3), Action::Commit("éng".into())],
        "{actions:?}"
    );
}

#[test]
fn fake_client_context_drops_buffer_state_but_keeps_name() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:chrome".into()));
    type_str(&mut e, "tieng");
    assert!(e.surrounding_data_ok());
    e.set_client(Some("fake".into()));
    assert!(
        !e.surrounding_data_ok(),
        "fake context must drop buffer data"
    );
    assert_eq!(e.client(), Some("gtk4-im:chrome"), "app-rule name kept");
}

#[test]
fn stale_echoes_are_dropped_and_deferred_flushed() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.add_injected(2);
    e.defer_commit(2, "word".into());
    // Fresh injection: echoes still expected.
    assert!(!e.take_stale_injection(Duration::from_secs(10)));
    assert!(e.pending_echoes());
    // Past the stale window: pending dropped, commit deliverable.
    assert!(e.take_stale_injection(Duration::ZERO));
    assert!(
        !e.pending_echoes(),
        "real Backspaces must not be eaten as echoes"
    );
    assert_eq!(e.flush_deferred(), vec!["word".to_string()]);
}

#[test]
fn capability_flip_does_not_change_delete_transport() {
    // Regression for the "xin chaà" session: the daemon re-sends caps
    // WITHOUT the surrounding bit on quick refocus (0b1001 in the logs)
    // while the ST reports keep flowing. The report — not the bit —
    // proves mutter has the buffer, so the rewrite stays on the protocol
    // delete; treating the bit as authoritative routed every rewrite to
    // injection, whose backspaces never arrive on a relay.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    // In the real session the report channel works from the first key on.
    e.set_surrounding(String::new(), 0);
    for c in "tun".chars() {
        e.process_char(c);
    }
    e.set_surrounding("tun".into(), 3);
    // The flip seen in the logs: same context, caps now without the bit.
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_surrounding("tung".into(), 4);
    assert!(e.surrounding_data_ok(), "a report makes the transport safe");
    let actions = e.process_char('f');
    assert!(
        actions.iter().any(|a| matches!(a, Action::DeleteBefore(2))),
        "protocol delete stays: {actions:?}"
    );
}

#[test]
fn stale_pending_without_deferred_unsticks_backspace() {
    // A bare injection (Backspace while composing) leaves pending echoes
    // even with nothing deferred; without the stale drop every later
    // real Backspace would be eaten as an echo forever.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.add_injected(1);
    assert!(e.pending_echoes());
    assert!(e.take_stale_injection(Duration::ZERO));
    assert!(!e.pending_echoes());
    assert!(
        !e.take_injected_echo(),
        "the real Backspace is no longer eaten"
    );
}

#[test]
fn digits_terminate_the_word_and_are_forwarded() {
    // Regression: bamboo clears its whole composition when a digit
    // arrives in Telex — the engine replayed that as "delete everything"
    // and the user's characters vanished when typing numbers.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "abc123"), "abc123");
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gtk4-im:test".into()));
    // Preedit: the word commits when the digit ends it, the digit follows.
    assert_eq!(type_replay(&mut e, "x1"), "x1");
    // Vietnamese word + digit.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "tieeng2"), "tiêng2");
}

#[test]
fn lost_echoes_flush_in_the_main_loop_tick() {
    // A uinput rewrite defers its commit behind 2 injected Backspace
    // echoes; one echo is lost. With no further key arriving, the
    // main-loop tick must flush the deferred commit after the stale
    // window — previously it sat until the next keystroke, leaving the
    // erased word as raw latin for as long as the user paused.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    e.defer_commit(2, "tiếng".into());
    e.add_injected(2);
    assert!(e.take_injected_echo(), "first echo arrives");
    let _ = e.note_echo_forwarded();
    // Inside the stale window: nothing to do yet.
    assert!(e.tick().is_empty());
    // The loop must be scheduled to wake for the stale deadline.
    assert!(e.ms_until_tick().is_some());
    // The lost echo goes stale: the tick delivers the commit.
    e.last_inject = Some(Instant::now() - Duration::from_millis(200));
    let actions = e.tick();
    assert_eq!(actions, vec![Action::Commit("tiếng".into())], "{actions:?}");
    assert!(!e.has_deferred() && !e.pending_echoes());
}

#[test]
fn backspace_honors_the_word_mode_lock_across_an_inline_suggestion() {
    // A Surrounding word in flight; the app appends suggestion text after
    // the caret, so effective_mode() now resolves to Preedit. The
    // Backspace must follow the locked mode — a phantom preedit over the
    // per-key-committed text is exactly the mid-word flip the lock
    // exists to prevent.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    assert_eq!(type_replay(&mut e, "tie"), "tie");
    e.set_suggestions(true);
    let actions = e.backspace().expect("backspace over the committed word");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "the locked Surrounding word must not turn into a preedit: {actions:?}"
    );
}

#[test]
fn no_report_no_backend_rewrites_through_forward_backspace() {
    // Surrounding without a surrounding report and without uinput: the
    // walk's terminal transport rewrites the word with forwarded Backspace
    // key events instead of leaving the raw keys on screen.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    assert_eq!(
        type_replay_with_reports(&mut e, "tieengs ", false),
        "tiếng "
    );
}

#[test]
fn no_report_no_backend_macro_expands_through_the_forward_transport() {
    // A macro expansion rewrites committed text; the walk's terminal
    // transport supplies the delete, so the expansion lands instead of
    // being held back.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            time_format: "%H".into(),
            ..Config::default()
        },
    );
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    assert_eq!(type_replay_with_reports(&mut e, "gs ", false), "giờ sáng ");
}

#[test]
fn no_report_no_backend_backspace_rides_the_forward_transport() {
    // Without a report and without a backend the walk still hands the
    // edit a transport: the forwarded Backspace key event erases the
    // character, like any ForwardBackspace word edit.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    assert_eq!(type_replay_with_reports(&mut e, "ab", false), "ab");
    let actions = e.backspace().expect("backspace over a committed word");
    assert!(
        actions.contains(&Action::ForwardBackspaces(1)),
        "the forwarded transport erases: {actions:?}"
    );
}

#[test]
fn no_report_no_backend_legacy_charset_rewrites_forwarded() {
    // Legacy rewrite deletes the whole tail every key; the walk's
    // terminal transport supplies it, so the VNI output lands exactly
    // like on the protocol path.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e.set_charset(Charset::Vni);
    assert_eq!(
        type_replay_with_reports(&mut e, "viee", false),
        encode(Charset::Vni, "viê")
    );
}

#[test]
fn no_report_no_backend_rewrites_em_dash_and_double_space() {
    // The walk's terminal transport gives the rewrite features a working
    // delete: `--` becomes an em dash and the double space inserts the
    // period through forwarded Backspaces.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            em_dash: true,
            double_space_period: true,
            ..Config::default()
        },
    );
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    assert_eq!(type_replay_with_reports(&mut e, "a--", false), "a—");
    assert_eq!(type_replay_with_reports(&mut e, "b  ", false), "b. ");
}

#[test]
fn viqr_marks_apply_in_every_display_mode() {
    // The VIQR mark keys are punctuation keys: they must compose instead
    // of ending the word (BUG: they used to be intercepted as word
    // terminators, so VIQR could not type any mark at all).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(
            mode,
            Config {
                method: crate::config::MethodConfig::Viqr,
                ..Config::default()
            },
        );
        // Preedit only delivers at the terminator.
        let (keys, want) = match mode {
            Mode::Preedit => ("hoi? ", "hỏi "),
            _ => ("hoi?", "hỏi"),
        };
        assert_eq!(type_replay(&mut e, keys), want, "{mode:?}");
    }
}

#[test]
fn viqr_non_mark_digits_still_terminate_and_stay_literal() {
    // VIQR uses `0` as its only digit mark (remove tone); the other
    // digits end the word and are kept as data.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            method: crate::config::MethodConfig::Viqr,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "to5i"), "to5i");
}

#[test]
fn viqr_punctuation_stays_literal_where_no_mark_applies() {
    // unikey convention: a mark key that cannot transform anything stays
    // literal, and a doubled mark key yields the literal character.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            method: crate::config::MethodConfig::Viqr,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "ok?"), "ok?");
    assert_eq!(type_replay(&mut e, " "), " ");
    assert_eq!(type_replay(&mut e, "hoi.."), "hoi.");
}

#[test]
fn microsoft_brackets_type_horn_in_every_display_mode() {
    // The Microsoft layout types ư/ơ only on the bracket keys; they must
    // reach the engine even with the bracket option off (BUG: the keys
    // used to end the word, so ư/ơ were untypeable on this layout).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(
            mode,
            Config {
                method: crate::config::MethodConfig::Microsoft,
                ..Config::default()
            },
        );
        let (keys, want) = match mode {
            Mode::Preedit => ("t[ ", "tư "),
            _ => ("t[", "tư"),
        };
        assert_eq!(type_replay(&mut e, keys), want, "{mode:?}");
    }
}

#[test]
fn undo_mark_then_consonant_stays_raw_tool() {
    // Regression: "tô" + 'o' undoes the mark ("too"), then 'l' made
    // bamboo resurface a hidden extra 'o' ("toool") which replayed into
    // the application. The poisoned composition must never win: the
    // output can never be longer than the raw keystrokes.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "tool"), "tool");
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "toool"), "tool");
    // Vietnamese words keep working.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "xin chaof "), "xin chào ");
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "tieengs "), "tiếng ");
}

#[test]
fn forward_backspace_resyncs_the_mark_undo_toool() {
    // Regression: "toool" (English) — the third 'o' undoes the
    // circumflex inside bamboo. Without any delete transport the mark
    // keys used to pass through raw while the tracker kept believing the
    // transformed text — a later key then committed garbage ("tooool"
    // with an extra 'o'). The walk's forwarded transport makes the undo
    // rewrites land, so the visible text behaves as "too" + 'l' → "tool"
    // (bamboo's hidden extra 'o' must never resurface), exactly like on
    // the protocol path.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT); // no ST report, no uinput
    let mut buf: Vec<char> = Vec::new();
    let mut all = Vec::new();
    for c in "toool".chars() {
        let actions = e.process_char(c);
        let forwarded = actions.contains(&Action::Forward);
        for a in &actions {
            match a {
                Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                // ForwardBackspace mode: real Backspace key events, same net effect on
                // the buffer as a protocol delete.
                Action::ForwardBackspaces(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                Action::Commit(t) => buf.extend(t.chars()),
                _ => {}
            }
        }
        if forwarded {
            buf.push(c);
        }
        all.extend(actions);
    }
    assert!(
        !all.iter().any(|a| matches!(
            a,
            Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
        )),
        "no protocol delete transport: {all:?}"
    );
    assert!(
        all.iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "the forwarded transport rewrites: {all:?}"
    );
    let text: String = buf.into_iter().collect();
    assert_eq!(
        text, "tool",
        "the forwarded undo must not resurface raw keys"
    );
}

#[test]
fn toool_with_protocol_delete_undoes_the_circumflex() {
    // With a delete transport the Telex mark-undo works: the third 'o'
    // turns "tô" back into "to", and "toool" comes out exactly as typed.
    // "toool" (5 keys — the 3rd 'o' undid the circumflex): the visible
    // text behaves as "too" + 'l' → "tool"; bamboo's hidden extra 'o'
    // must never resurface.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "toool"), "tool");
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    assert_eq!(type_replay(&mut e, "too"), "tô");
}

#[test]
fn word_mode_locks_across_config_changes() {
    // Mid-word mode changes (config reload, capability flips) must not
    // toggle preedit under the user's hands; the lock releases at the
    // word boundary.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    let first = e.process_char('t');
    assert!(first.contains(&Action::Commit("t".into())), "{first:?}");
    // Mid-word: switch the configured mode to preedit.
    e.apply_config(&Config {
        mode: ModeConfig::Preedit,
        ..Config::default()
    });
    let mid = e.process_char('u');
    assert!(
        mid.contains(&Action::Commit("u".into())) || mid.contains(&Action::Forward),
        "word stays non-preedit: {mid:?}"
    );
    // Word ends: the next word resolves fresh — preedit now.
    let _ = e.process_char(' ');
    let next = e.process_char('c');
    assert!(
        next.iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "new word uses the new mode: {next:?}"
    );
}

#[test]
fn mutter_relayed_clients_skip_repair_deletes_but_rewrite_normally() {
    // The 02:37 gnome-shell segfault came from a REPAIR delete sent after
    // the application rewrote its own buffer (Chrome omnibox autocomplete)
    // — mutter applies it against a buffer it has not caught up with.
    // Normal rewrite deletes work on this path (whole sessions of correct
    // text in the debug log), so only the repair is suppressed; the drift
    // is accepted instead of risking the compositor.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    // A real client reports at focus-in, before the first key: word one
    // resolves through the walk's protocol-delete row.
    e.set_surrounding(String::new(), 0);
    for c in "chan".chars() {
        e.process_char(c);
    }
    e.set_surrounding("chanhdai.com".into(), 12);
    // A normal tone rewrite still uses the protocol delete.
    let actions = e.process_char('s');
    assert!(
        actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "rewrite deletes stay protocol: {actions:?}"
    );
    // The application rewrote its buffer (autocomplete): the report no
    // longer ends with the tail and the extra part is NOT a swallowed
    // delete we can recognise — the repair must be skipped, never sent.
    e.set_surrounding("chanán".into(), 6);
    let actions = e.process_char('x');
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::DeleteSurrounding { .. })),
        "no REPAIR delete may reach a mutter-relayed client: {actions:?}"
    );
}

#[test]
fn heuristic_clients_fall_back_to_preedit() {
    // Clients on the engine's FALLBACK_NO_DELETE_SURROUNDING list ignore
    // DeleteSurroundingText; non-preedit would append the rewrite and
    // duplicate the word, so keep it inline instead.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk3-im:code".into()));
    let actions = type_str(&mut e, "tieengs");
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "{actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "tiếng")),
        "{actions:?}"
    );
}

/// Type through `process_char` without the helpers' forced capabilities.
fn type_raw(e: &mut Engine, s: &str) -> Vec<Action> {
    let mut actions = Vec::new();
    for c in s.chars() {
        actions.extend(e.process_char(c));
    }
    actions
}

#[test]
fn x11_delete_without_report_is_opt_in() {
    // A direct GTK client that advertises surrounding text but never
    // reports it: raw keystrokes by default, protocol deletes once the
    // switch is on (that is what makes Sublime work, like ibus-bamboo).
    for (enabled, want_delete) in [(false, false), (true, true)] {
        let mut e = engine(
            Mode::Surrounding,
            Config {
                x11_delete_without_report: enabled,
                ..Config::default()
            },
        );
        e.set_client(Some("gtk3-im:sublime_text".into()));
        e.set_capabilities(CAP_SURROUNDING_TEXT);
        let actions = type_raw(&mut e, "tieengs");
        assert_eq!(
            actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
            want_delete,
            "switch={enabled}: {actions:?}"
        );
    }
    // A mutter relay still needs a report: the switch must never lift the
    // rule that keeps the compositor alive.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            x11_delete_without_report: true,
            ..Config::default()
        },
    );
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_SURROUNDING_TEXT);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "relay contexts keep the report rule: {actions:?}"
    );
    // Without the surrounding capability there is nothing to delete with.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            x11_delete_without_report: true,
            ..Config::default()
        },
    );
    e.set_client(Some("xim".into()));
    e.set_capabilities(0);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "XIM clients report no capabilities: {actions:?}"
    );
}

#[test]
fn preedit_for_xim_is_opt_in() {
    // XIM clients report no preedit capability even though the XIM server
    // draws the preedit itself; without the switch our preedit is
    // downgraded and every mark key is raw-forwarded ("tungf").
    for (enabled, want_preedit) in [(false, false), (true, true)] {
        let mut e = engine(
            Mode::Preedit,
            Config {
                preedit_for_xim: enabled,
                ..Config::default()
            },
        );
        e.set_client(Some("xim".into()));
        e.set_capabilities(0);
        let actions = type_raw(&mut e, "tieengs");
        assert_eq!(
            actions
                .iter()
                .any(|a| matches!(a, Action::UpdatePreedit { .. })),
            want_preedit,
            "switch={enabled}: {actions:?}"
        );
    }
    // A GTK client that truly cannot show a preedit keeps the fallback.
    let mut e = engine(
        Mode::Preedit,
        Config {
            preedit_for_xim: true,
            ..Config::default()
        },
    );
    e.set_client(Some("gtk3-im:someapp".into()));
    e.set_capabilities(CAP_SURROUNDING_TEXT);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "the switch is XIM-only: {actions:?}"
    );
}

#[test]
fn forward_key_event_mode_deletes_by_forwarding_backspaces() {
    // The mode's point: a mutter-relayed client with no surrounding-text
    // report and no usable /dev/uinput path still rewrites correctly,
    // because the deletes are Backspace key events sent through IBus.
    let mut e = Engine::new(Method::Telex, Mode::ForwardBackspace);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT); // no ST capability at all
    let actions = type_str(&mut e, "tieengs");
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
        )),
        "no protocol delete in ForwardBackspace mode: {actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "the tone rewrite must forward Backspace events: {actions:?}"
    );
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn forward_key_event_mode_survives_the_preedit_heuristics() {
    // The heuristics that push delete-dropping clients back to preedit
    // (inline suggestions, the known-client list, a proven swallowed
    // delete) must not apply: this mode was chosen precisely because
    // preedit is unusable in the app.
    let mut e = Engine::new(Method::Telex, Mode::ForwardBackspace);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e.set_client(Some("gnome-shell".into()));
    e.set_suggestions(true);
    let actions = type_str(&mut e, "tieengs");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "ForwardBackspace never falls back to preedit: {actions:?}"
    );
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn xim_clients_report_no_usable_application_name() {
    // ibus-x11 names every input context `xim`, so an app rule can only
    // match one by accident; the server must name the app from the
    // compositor extension or xprop for rules to work there.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("xim".into()));
    assert!(e.client_name_is_opaque());
    e.set_client(Some("gtk3-im:sublime_text".into()));
    assert!(!e.client_name_is_opaque());
}

#[test]
fn forward_key_event_backspace_never_deletes_by_protocol() {
    let mut e = Engine::new(Method::Telex, Mode::ForwardBackspace);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    type_str(&mut e, "tiee");
    let Some(actions) = e.backspace() else {
        panic!("a composition is in flight");
    };
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
        )),
        "backspace must use the forwarded transport: {actions:?}"
    );
}

#[test]
fn config_rule_bypasses_the_delete_fallback() {
    // An explicit rule pins the mode for its client, skipping the runtime
    // fallback that would otherwise force preedit on delete-dropping
    // clients such as chrome.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk4-im:chrome".into()));
    e.apply_config(&Config {
        app_rules: vec![crate::config::AppRule {
            pattern: "chrome".into(),
            mode: crate::config::ModeConfig::Surrounding,
        }],
        ..Config::default()
    });
    let actions = type_str(&mut e, "tieengs");
    assert_eq!(replay(&actions), "tiếng");
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { .. })));
}

#[test]
fn none_rule_targets_unnamed_clients_in_every_display_mode() {
    // The special `none` pattern targets clients that never reported a
    // name (X11/XIM apps): with no name there is nothing to substring-
    // match, so the rule used to be silently skipped for them.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = Engine::new(Method::Telex, Mode::Off);
        e.set_capabilities(CAP_PREEDIT_TEXT);
        e.set_client(None);
        e.apply_config(&Config {
            // The base mode must be explicit: apply_config resets the
            // mode to the config's, which defaults to preedit.
            mode: crate::config::ModeConfig::Off,
            app_rules: vec![crate::config::AppRule {
                pattern: "none".into(),
                mode: mode.into(),
            }],
            ..Config::default()
        });
        // The none rule matches the unnamed client and its mode then runs
        // the walk. No surrounding report and no backend: the commit-as-
        // you-type requests are served by the terminal forwarded
        // transport, while Preedit stands when the client can render it.
        let unnamed = if mode == Mode::Preedit {
            Mode::Preedit
        } else {
            Mode::ForwardBackspace
        };
        assert_eq!(e.effective_mode(), unnamed, "unnamed client: {mode:?}");
        // A named client must not match the none rule: the configured
        // base mode (Off) stands for it.
        e.set_client(Some("gtk4-im:firefox".into()));
        assert_ne!(e.effective_mode(), unnamed, "named client: {mode:?}");
    }
}

#[test]
fn hidden_modes_are_not_selectable_from_config_or_rules() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk4-im:game".into()));
    e.apply_config(&Config {
        // The global mode is hidden by the explicit subset (no variant
        // ships hidden anymore): the engine moves to the first visible
        // entry instead of running a mode the user cannot see.
        mode: ModeConfig::Flow,
        visible_modes: Some(vec!["preedit".into(), "off".into()]),
        // A rule naming a hidden mode is ignored; the global mode stands.
        app_rules: vec![crate::config::AppRule {
            pattern: "game".into(),
            mode: ModeConfig::Flow,
        }],
        ..Config::default()
    });
    assert_eq!(e.mode(), Mode::Preedit, "hidden global mode coerced");
    assert_eq!(e.effective_mode(), Mode::Preedit, "hidden rule ignored");

    // A visible rule still wins over the global mode (a forward-transport
    // mode, so the missing uinput device does not force a preedit fallback).
    e.apply_config(&Config {
        mode: ModeConfig::Preedit,
        visible_modes: Some(vec!["preedit".into(), "forwardkeyevent".into()]),
        app_rules: vec![crate::config::AppRule {
            pattern: "game".into(),
            mode: ModeConfig::ForwardBackspace,
        }],
        ..Config::default()
    });
    assert_eq!(
        e.effective_mode(),
        Mode::ForwardBackspace,
        "visible rule applies"
    );
}

#[test]
fn hidden_mode_fallback_follows_the_visible_order() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        mode: ModeConfig::Surrounding,
        visible_modes: Some(vec!["off".into(), "preedit".into()]),
        ..Config::default()
    });
    assert_eq!(e.mode(), Mode::Off, "the first visible entry wins");
    // The explicit subset hides Preedit itself: the fallback still follows
    // the list order, not the factory default.
    e.apply_config(&Config {
        mode: ModeConfig::Preedit,
        visible_modes: Some(vec!["forwardbackspace".into(), "flow".into()]),
        ..Config::default()
    });
    assert_eq!(
        e.mode(),
        Mode::ForwardBackspace,
        "the fallback follows the list order, not the factory default"
    );
}

#[test]
fn app_name_feeds_rules_but_not_transport_decisions() {
    // A mutter-relayed client (gnome-shell) is named by the focus
    // extension: rules must now match the name, while every transport
    // decision keeps treating the context as relayed (the mutter-safe
    // delete handling must not be lost just because the app got named).
    let mut e = Engine::new(Method::Telex, Mode::Off);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gnome-shell".into()));
    e.apply_config(&Config {
        mode: crate::config::ModeConfig::Off,
        app_rules: vec![crate::config::AppRule {
            pattern: "firefox".into(),
            mode: crate::config::ModeConfig::Preedit,
        }],
        ..Config::default()
    });
    // Unnamed relay: no match, the base mode stands.
    assert_eq!(e.effective_mode(), Mode::Off);
    e.set_app_name(Some("wayland:Firefox".into()));
    assert_eq!(
        e.effective_mode(),
        Mode::Preedit,
        "rule matches the app name"
    );
    // The transport side is untouched: still a relayed context.
    assert!(e.relay_context());
    assert!(e.repair_delete_forbidden());
    // Named via the extension, the none rule no longer matches it.
    let mut e2 = Engine::new(Method::Telex, Mode::Off);
    e2.set_capabilities(CAP_PREEDIT_TEXT);
    e2.set_client(None);
    e2.set_app_name(Some("x11:Sublime_text".into()));
    e2.apply_config(&Config {
        mode: crate::config::ModeConfig::Off,
        app_rules: vec![crate::config::AppRule {
            pattern: "none".into(),
            mode: crate::config::ModeConfig::Preedit,
        }],
        ..Config::default()
    });
    assert_eq!(e2.effective_mode(), Mode::Off, "named client skips none");
    e2.set_app_name(None);
    assert_eq!(
        e2.effective_mode(),
        Mode::Preedit,
        "unnamed again matches none"
    );
}

#[test]
fn firefox_keeps_surrounding() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk4-im:firefox".into()));
    assert_eq!(replay(&type_str(&mut e, "tieengs")), "tiếng");
}

#[test]
fn unknown_client_keeps_surrounding() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk3-im:gedit".into()));
    assert_eq!(replay(&type_str(&mut e, "tieengs")), "tiếng");
}

#[test]
fn daemon_fake_context_is_ignored() {
    // Focus bounces through ibus-daemon's `fake` hotkey context; treating
    // it as the focused app would reset the word being typed.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:firefox".into()));
    type_str(&mut e, "vieejt");
    e.set_client(Some("fake".into()));
    assert_eq!(e.wrapper.output(), "việt");
}

#[test]
fn english_mode_forwards_everything() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_enabled(false);
    assert_eq!(e.process_char('a'), vec![Action::Forward]);
}

#[test]
fn legacy_charset_rewrites_the_whole_word() {
    // VNI's `ế` is several chars, so the Unicode delta cannot be replayed.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_charset(Charset::Vni);
    let actions = type_str(&mut e, "vieejt");
    assert_eq!(replay(&actions), encode(Charset::Vni, "việt"));
}

#[test]
fn legacy_charset_backspace_replays_correctly() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_charset(Charset::Vni);
    let mut actions = type_str(&mut e, "vieejt");
    actions.extend(e.backspace().expect("backspace"));
    // The default tone-first skip: the tone goes, the letters stay.
    assert_eq!(replay(&actions), encode(Charset::Vni, "viêt"));
}

#[test]
fn legacy_charset_word_end_sends_nothing_extra() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_charset(Charset::Tcvn3);
    let mut actions = type_str(&mut e, "vieejt");
    let end = e.process_char(' ');
    assert!(end.contains(&Action::Forward));
    assert!(!end
        .iter()
        .any(|a| matches!(a, Action::DeleteBefore(_) | Action::Commit(_))));
    actions.extend(end);
    assert_eq!(replay(&actions), encode(Charset::Tcvn3, "việt"));
}

#[test]
fn preedit_commits_encoded_word() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_charset(Charset::Tcvn3);
    type_str(&mut e, "vieejt");
    assert_eq!(
        e.reset(),
        vec![
            Action::HidePreedit,
            Action::Commit(encode(Charset::Tcvn3, "việt"))
        ]
    );
}

// --- surrounding-text verification ---

#[test]
fn selection_reports_never_trigger_a_repair() {
    // anchor != cursor means a selection: the text before the caret is
    // not the word the engine committed, and a repair would delete it.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    type_str(&mut e, "tie");
    let actions = e.set_surrounding_with_anchor("tiex".into(), 4, 0);
    assert!(actions.is_empty(), "{actions:?}");
    let actions = e.process_char('n');
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::DeleteSurrounding { .. } | Action::RewriteByBackspace { .. }
        )),
        "{actions:?}"
    );
}

#[test]
fn surrounding_repairs_from_the_report() {
    // Preedit never sends a repair delete: the composition is still on
    // screen, nothing to verify against. The commit-as-you-type contract
    // verifies and repairs from the report once the walk resolves it
    // through the protocol-delete row (a report proves the client's
    // buffer is known).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = Engine::new(Method::Telex, mode);
        type_str(&mut e, "tie");
        // The client reports text the engine did not write (another
        // writer, a stale report).
        e.set_surrounding("tiex".into(), 4);
        let actions = e.process_char('n');
        let repaired = actions.iter().any(|a| {
            matches!(
                a,
                Action::DeleteSurrounding { .. } | Action::RewriteByBackspace { .. }
            )
        });
        assert_eq!(repaired, mode != Mode::Preedit, "{mode:?}: {actions:?}");
    }
}

#[test]
fn surrounding_repairs_swallowed_delete() {
    // The word "tie" is committed; typing the second `e` turns it into
    // `ê` by deleting 1 char and inserting `ê`. The client drops the
    // delete, so it shows `tieê` while the engine believes `tiê`.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);

    let actions = type_str(&mut e, "tie");
    assert_eq!(replay(&actions), "tie");

    // The client reports the real text (no delete has been dropped yet).
    e.set_surrounding("tie".into(), 3);

    // The tone key: engine deletes 1, commits `ê`; the client drops the
    // delete and ends up with `tieê`.
    let actions = e.process_char('e').into_iter().collect::<Vec<_>>();
    assert!(actions.contains(&Action::Commit("ê".into())), "{actions:?}");

    // The client's next SetSurroundingText reveals the drift — and the
    // swallowed rewrite delete itself (report proof): the client showed
    // `tieê`, exactly the delete-missing shape. The engine therefore
    // skips the doomed protocol repair and presses real Backspace keys
    // right away: delete the extra `e` plus the common suffix `ê`, then
    // re-type `ê`.
    e.set_surrounding("tieê".into(), 4);
    let actions = e.process_char('n');
    assert_eq!(
        actions.first(),
        Some(&Action::RewriteByBackspace {
            backspaces: 2,
            commit: "ê".into()
        }),
        "{actions:?}"
    );
    // After the repair the rest of the key must still land: `tiên`.
    assert_eq!(replay_from("tieê", &actions), "tiên");
}

#[test]
fn swallowed_repairs_switch_to_backspace_injection() {
    // Regression: a client that swallows the protocol delete (Sublime
    // Text, kgx) got stuck — words duplicated and backspace appeared
    // dead. After the repair delete is swallowed too, the engine switches
    // to RewriteByBackspace: the server presses real Backspace keys and
    // re-commits the common suffix.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_SURROUNDING_TEXT);
    type_str(&mut e, "tie");
    e.set_surrounding("tie".into(), 3);
    let _ = e.process_char('e'); // delete 1 + commit ê; client swallows the delete
    e.set_surrounding("tieê".into(), 4);
    // The swallowed rewrite delete is already proven by the report
    // (`tieê` is exactly the delete-missing shape), so the first repair
    // goes straight to Backspace injection instead of trying a protocol
    // delete this client has just proven it drops.
    let first = e.process_char('n');
    assert!(matches!(
        first.first(),
        Some(Action::RewriteByBackspace { backspaces: 2, .. })
    ));
    assert!(e.delete_dropped(), "flag set by the report proof");

    // The client swallowed the delete but applied the commit, so it now
    // shows `tieên` where the engine believes `tiên`. A repair was
    // already attempted without effect: the engine marks the client
    // delete-dropped and switches to Backspace injection.
    e.set_surrounding("tieên".into(), 5);
    let second = e.process_char('n');
    assert!(e.delete_dropped(), "flag set after the second swallow");
    let injected = second.iter().find_map(|a| match a {
        Action::RewriteByBackspace { backspaces, commit } => Some((*backspaces, commit.clone())),
        _ => None,
    });
    assert_eq!(injected, Some((3, "ên".into())), "{second:?}");
    assert!(
        !second
            .iter()
            .any(|a| matches!(a, Action::DeleteSurrounding { .. })),
        "no more protocol deletes: {second:?}"
    );
}

#[test]
fn delete_dropped_marks_protocol_deletes_for_the_server() {
    // The flag survives until the client changes, and every delete-like
    // repair afterwards goes through RewriteByBackspace.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_SURROUNDING_TEXT);
    type_str(&mut e, "tie");
    e.set_surrounding("tie".into(), 3);
    let _ = e.process_char('e');
    e.set_surrounding("tieê".into(), 4);
    let _ = e.process_char('n'); // protocol repair attempted
    e.set_surrounding("tieên".into(), 5);
    let _ = e.process_char('n'); // switches to injection
    e.set_surrounding("tiên".into(), 4);
    let third = e.process_char('g');
    assert!(
        !third
            .iter()
            .any(|a| matches!(a, Action::DeleteSurrounding { .. })),
        "all later deletes are injected, not signalled: {third:?}"
    );
    assert!(e.delete_dropped());
}

#[test]
fn app_side_edit_around_the_word_is_adopted() {
    // Chrome suggestions rewrite the text AROUND the composed word; the
    // report still contains the believed word, so the engine rebuilds
    // from the real text and tone keys keep working.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    type_str(&mut e, "o");
    e.set_surrounding("x o y".into(), 3); // app inserted text around "o"
    let actions = e.process_char('n');
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::DeleteSurrounding { .. })));
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "{actions:?}"
    );
    // Composition rebuilt from the word before the cursor.
    assert_eq!(e.wrapper.output(), "on");
}

#[test]
fn report_missing_the_word_is_ignored() {
    // A report that does not contain the believed word is stale (or the
    // user deleted it); the engine must NOT tear the composition down
    // over it — this is what used to eat characters when typing fast.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    type_str(&mut e, "tie");
    e.set_surrounding("tie".into(), 3);
    let _ = e.process_char('e'); // commit ê; client has not reported yet
    e.set_surrounding("tie".into(), 3); // stale
    let actions = e.process_char('n');
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::DeleteSurrounding { .. })));
    assert_eq!(e.wrapper.output(), "tiên");
}

#[test]
fn surrounding_verify_ignores_preedit_mode() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    type_str(&mut e, "tie");
    e.set_surrounding("garbage".into(), 3);
    let actions = e.process_char('e');
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { .. })));
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::DeleteSurrounding { .. })));
}

// --- typed-text features ---

#[test]
fn em_dash_replaces_double_hyphen() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        em_dash: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "a--b"), "a—b");
}

#[test]
fn em_dash_disabled_by_default() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(type_replay(&mut e, "a--b"), "a--b");
}

#[test]
fn double_space_inserts_period_when_enabled() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        double_space_period: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "a  b"), "a. b");
}

#[test]
fn double_space_disabled_by_default() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(type_replay(&mut e, "a  b"), "a  b");
}

/// Regression: after a forwarded Backspace the engine kept the deleted
/// character in `last_char`, so the next one fired the pair-rewrite against
/// text that was no longer on screen and the paired delete ate the real
/// letter before it ("a-" BS "-" turned the whole "a-" into "—").
#[test]
fn em_dash_after_a_backspace_stays_literal() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        em_dash: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay_backspaces(&mut e, "a-\u{8}-").0, "a-");
    // The healthy pair still rewrites: "a" + "-" + "-" (no backspace).
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        em_dash: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "a--"), "a—");
}

#[test]
fn double_space_after_a_backspace_stays_two_spaces() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        double_space_period: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay_backspaces(&mut e, "a \u{8} ").0, "a ");
    // The healthy pair still inserts the period.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        double_space_period: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "a  b"), "a. b");
}

/// Regression: deleting a fired emoji and retyping the shortcode re-armed
/// the state from the stale `last_char` (':' was gone from the screen) and
/// the rewrite carried a phantom ':' in its delete count — "x:fire:" BS
/// "fire:" ate the "x".
#[test]
fn emoji_shortcode_after_a_backspace_needs_its_colon() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        emoji_enabled: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(
        type_replay_backspaces(&mut e, "x:fire:\u{8}fire:").0,
        "xfire:"
    );
    // A shortcode whose ':' is still on screen keeps rewriting.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        emoji_enabled: true,
        mode: ModeConfig::Surrounding,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "x:fire:"), "x🔥");
}

#[test]
fn auto_capitalize_after_sentence_end() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_capitalize: true,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, ". tieengs "), ". Tiếng ");
}

#[test]
fn auto_capitalize_also_uppercases_transformed_letters() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_capitalize: true,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "? dd "), "? Đ ");
}

#[test]
fn auto_capitalize_disabled_by_default() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(type_replay(&mut e, ". tieengs "), ". tiếng ");
}

// --- macros ---

#[test]
fn macro_expands_on_word_end() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        time_format: "%H".into(),
        ..Config::default()
    });
    // Inject a macro by writing through the same path apply_config reads.
    // (Macros::load reads the user file, so expand through a test hook:
    // use the public expand path with a fake table.)
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "gs "), "giờ sáng ");
}

#[test]
fn macro_can_be_disabled() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        macro_enabled: false,
        ..Config::default()
    });
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "gs "), "gs ");
}

#[test]
fn macro_on_valid_word_is_opt_in() {
    // Default: a shorthand keyed like a real word ("cam") never expands —
    // the word itself wins, in every core mode.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, Config::default());
        e.macros
            .set_entries([("cam".to_string(), "cảm ơn".to_string())].into_iter());
        assert_eq!(type_replay(&mut e, "cam "), "cam ", "{mode:?}");
    }
    // Opt in (`macro_expand_valid_words`): the same key expands.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(
            mode,
            Config {
                experimental_features: true,
                macro_expand_valid_words: true,
                ..Config::default()
            },
        );
        e.macros
            .set_entries([("cam".to_string(), "cảm ơn".to_string())].into_iter());
        assert_eq!(type_replay(&mut e, "cam "), "cảm ơn ", "{mode:?}");
    }
}

#[test]
fn extra_url_char_bypass_is_opt_in() {
    // Default: a backtick is not URL context — `` `vieejt` `` composes.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, Config::default());
        assert_eq!(type_replay(&mut e, "`vieejt` "), "`việt` ", "{mode:?}");
        // With the character added, the word between the backticks stays raw.
        let mut e = engine(
            mode,
            Config {
                experimental_features: true,
                extra_url_chars_enabled: true,
                extra_url_chars: "`".into(),
                ..Config::default()
            },
        );
        assert_eq!(type_replay(&mut e, "`vieejt` "), "`vieejt` ", "{mode:?}");
    }
}

#[test]
fn extra_url_chars_never_accept_a_hyphen() {
    // `-` is filtered out of the start set so `--` keeps becoming an em
    // dash, even when a hand-edited file smuggles it in.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            em_dash: true,
            experimental_features: true,
            extra_url_chars_enabled: true,
            extra_url_chars: "-".into(),
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "a-"), "a-", "the hyphen must stay raw");
    assert_eq!(
        type_replay(&mut e, "-"),
        "—",
        "`--` still becomes an em dash"
    );
}

#[test]
fn experimental_gate_forces_every_feature_off() {
    // Every switch on, master gate closed: the engine must behave exactly
    // like the defaults — the saved switches and values stay in the file
    // but apply nothing.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            macro_expand_valid_words: true,
            extra_url_chars_enabled: true,
            extra_url_chars: "`".into(),
            echo_stale_enabled: true,
            echo_stale_ms: 400,
            ..Config::default()
        },
    );
    e.macros
        .set_entries([("cam".to_string(), "cảm ơn".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "cam "), "cam ", "macro must not expand");
    assert_eq!(
        type_replay(&mut e, "`vieejt` "),
        "`việt` ",
        "no extra bypass"
    );
    assert_eq!(e.echo_stale(), Duration::from_millis(150), "default window");
    assert!(!e.macro_expand_valid_words && !e.idle_commit_enabled);
    assert!(e.extra_url_chars.is_empty() && e.extra_capitalize_after.is_empty());
    // Gate open: the same switches apply again.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            experimental_features: true,
            macro_expand_valid_words: true,
            extra_url_chars_enabled: true,
            extra_url_chars: "`".into(),
            ..Config::default()
        },
    );
    assert!(e.macro_expand_valid_words);
    assert_eq!(
        type_replay(&mut e, "`vieejt` "),
        "`vieejt` ",
        "bypass applies"
    );
}

#[test]
fn echo_stale_window_is_flag_gated() {
    // Default: the built-in 150 ms, whatever a hand edit put in the value.
    let e = engine(Mode::Surrounding, Config::default());
    assert_eq!(e.echo_stale(), Duration::from_millis(150));
    let e = engine(
        Mode::Surrounding,
        Config {
            echo_stale_enabled: false,
            echo_stale_ms: 400,
            ..Config::default()
        },
    );
    assert_eq!(
        e.echo_stale(),
        Duration::from_millis(150),
        "the switch off must ignore the stored value"
    );
    // Switch on: the custom window applies.
    let e = engine(
        Mode::Surrounding,
        Config {
            experimental_features: true,
            echo_stale_enabled: true,
            echo_stale_ms: 400,
            ..Config::default()
        },
    );
    assert_eq!(e.echo_stale(), Duration::from_millis(400));
}

#[test]
fn idle_commit_commits_a_paused_preedit_word() {
    let mut e = engine(
        Mode::Preedit,
        Config {
            experimental_features: true,
            idle_commit_enabled: true,
            idle_commit_ms: 700,
            ..Config::default()
        },
    );
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    for c in "viet".chars() {
        e.process_char(c);
    }
    // Backdate the clock: the pause is far past the configured window.
    e.last_key_at = Some(Instant::now() - Duration::from_millis(2000));
    let actions = e.tick();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Commit(t) if t == "viet")),
        "the tick must commit the paused word: {actions:?}"
    );
    // The client reports the buffer holding the committed word — the proof
    // the re-entry delete needs before it may erase it again.
    e.set_surrounding("viet".into(), 4);
    // Re-entry: the next word character un-commits (the committed word is
    // 4 chars) and restores the composition as preedit.
    let actions = e.process_char('n');
    assert!(
        actions.iter().any(|a| matches!(a, Action::DeleteBefore(4))),
        "the committed word must be erased back: {actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "the word must live again as preedit: {actions:?}"
    );
}

#[test]
fn idle_commit_disabled_by_default_and_terminators_reset_reentry() {
    // Off (the default): the tick never commits.
    let mut e = engine(Mode::Preedit, Config::default());
    for c in "viet".chars() {
        e.process_char(c);
    }
    e.last_key_at = Some(Instant::now() - Duration::from_secs(5));
    assert!(
        e.tick().is_empty(),
        "the default must leave the word pending"
    );
    // A terminator after an idle commit starts a fresh word instead of
    // re-entering the committed one.
    let mut e = engine(
        Mode::Preedit,
        Config {
            experimental_features: true,
            idle_commit_enabled: true,
            idle_commit_ms: 700,
            ..Config::default()
        },
    );
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    for c in "viet".chars() {
        e.process_char(c);
    }
    e.last_key_at = Some(Instant::now() - Duration::from_secs(2));
    let actions = e.tick();
    assert!(actions.iter().any(|a| matches!(a, Action::Commit(_))));
    let actions = e.process_char(' ');
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "a terminator must not un-commit: {actions:?}"
    );
    // A report that never saw the committed word (stale, 1 char): the
    // un-commit delete cannot be proven in range, so no re-entry.
    e.set_surrounding("v".into(), 1);
    e.idle_committed = Some(("viet".into(), "viet".into()));
    let actions = e.process_char('n');
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "a stale report must keep the commit final: {actions:?}"
    );
}

#[test]
fn extra_capitalize_triggers_are_opt_in() {
    // Default: a colon does not start a sentence.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(
            mode,
            Config {
                auto_capitalize: true,
                ..Config::default()
            },
        );
        assert_eq!(type_replay(&mut e, "tra: cam "), "tra: cam ", "{mode:?}");
        // With the colon added, the next word capitalizes.
        let mut e = engine(
            mode,
            Config {
                auto_capitalize: true,
                experimental_features: true,
                auto_capitalize_after_enabled: true,
                auto_capitalize_after: ":".into(),
                ..Config::default()
            },
        );
        assert_eq!(type_replay(&mut e, "tra: cam "), "tra: Cam ", "{mode:?}");
    }
}

#[test]
fn macro_rewrite_never_deletes_past_the_text() {
    // `ttlt` → "Tôi tên là Tùng": the expansion used to stay in `tail`
    // after the word ended, so backspacing part of it and typing the key
    // again made the next word end ask the client to delete more
    // characters than it held — mutter aborts on that ("before >= start")
    // and the desktop hangs until a hard reset.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, Config::default());
        e.macros
            .set_entries([("ttlt".to_string(), "Tôi tên là Tùng".to_string())].into_iter());
        let (text, out_of_range) = type_replay_backspaces(&mut e, "ttlt \u{8}\u{8}ttlt ");
        assert!(!out_of_range, "{mode:?}: delete past the text that exists");
        assert_eq!(text, "Tôi tên là TùnTôi tên là Tùng ", "{mode:?}");
    }
}

#[test]
fn protocol_delete_is_clamped_to_the_last_report() {
    // Hard rule: a protocol delete may never reach past what the last
    // SetSurroundingText says exists, whatever the engine believes.
    let mut e = engine(Mode::Surrounding, Config::default());
    e.macros
        .set_entries([("ttlt".to_string(), "Tôi tên là Tùng".to_string())].into_iter());
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    // A real client reports at focus-in, before the first key: the word
    // resolves through the walk's protocol-delete row, whose delete the
    // clamp below guards.
    e.set_surrounding(String::new(), 0);
    for c in "ttlt".chars() {
        e.process_char(c);
    }
    // The client only reports two characters before the caret.
    e.set_surrounding("tt".into(), 2);
    let actions = e.process_char(' ');
    let deletes: Vec<u32> = actions
        .iter()
        .filter_map(|a| match a {
            Action::DeleteBefore(n) => Some(*n),
            _ => None,
        })
        .collect();
    assert_eq!(deletes, vec![2], "{actions:?}");
}

#[test]
fn macro_in_off_mode() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        macro_in_off_mode: true,
        ..Config::default()
    });
    e.macros
        .set_entries([("ddq".to_string(), "đại đội".to_string())].into_iter());
    e.set_enabled(false);
    assert_eq!(type_replay(&mut e, "ddq "), "đại đội ");
    // English letters still pass through when not a macro.
    assert_eq!(type_replay(&mut e, "ab "), "ab ");
}

// --- auto-restore ---

#[test]
fn auto_restore_returns_english_word() {
    // With auto-correct off, a tone key on an English word bends it; the
    // restore puts the raw keystrokes back.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_correct: false,
        ..Config::default()
    });
    // Whatever bamboo produced mid-word, at the word end the raw keys
    // "windowf" must be what the application ends up with.
    assert_eq!(type_replay(&mut e, "windowf "), "windowf ");
}

#[test]
fn auto_restore_keeps_vietnamese_words() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_restore: true,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "vieejt "), "việt ");
}

#[test]
fn a_valid_prefix_is_kept_in_every_display_mode() {
    // `đuo` is not a complete syllable but is still a valid prefix, so it
    // must not be reverted to the raw keys at the word end. ibus-bamboo
    // keeps it (its preedit fallback uses the prefix check).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut config = Config {
            auto_restore: true,
            ..Config::default()
        };
        config.mode = mode.into();
        assert_eq!(
            type_replay(&mut engine(mode, config), "dduo "),
            "đuo ",
            "{mode:?}"
        );
    }
}

#[test]
fn preedit_shows_a_valid_prefix_without_flicker() {
    // The preedit must not flash the raw keys while a valid prefix grows
    // (`đuo`, `đuoc` stay composed). The client commits what it shows, so
    // a click there commits `đuoc`, not `dduoc`.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    let actions = type_str(&mut e, "dduoc");
    let shown: Vec<&str> = actions
        .iter()
        .filter_map(|a| match a {
            Action::UpdatePreedit { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        shown.contains(&"đuo") && shown.contains(&"đuoc"),
        "{shown:?}"
    );
    assert!(
        !shown.iter().any(|t| t.starts_with("dd")),
        "no raw flicker: {shown:?}"
    );
}

#[test]
fn undone_mark_is_not_restored_at_word_end() {
    // "tê" + 'e' → "tee": the user undid the mark on purpose, so the raw
    // keys ("teee") no longer describe what they want. bamboo marks the
    // result invalid and auto-restore used to put the raw keys back the
    // moment space arrived — "teen" flashed to "teeen". Same for the
    // mark-undo chain "tô" + 'o' + 'l' (internally "toool", visibly
    // "tool"). Check every display mode: preedit used to commit bamboo's
    // poisoned internal string instead of the visible one.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut config = Config {
            auto_restore: true,
            ..Config::default()
        };
        config.mode = mode.into();
        assert_eq!(
            type_replay(&mut engine(mode, config), "teeen "),
            "teen ",
            "{mode:?}"
        );
        let mut config = Config {
            auto_restore: true,
            ..Config::default()
        };
        config.mode = mode.into();
        assert_eq!(
            type_replay(&mut engine(mode, config), "toool "),
            "tool ",
            "{mode:?}"
        );
    }
}

#[test]
fn english_word_without_undo_is_still_restored() {
    // The undo exemption must not disable auto-restore altogether: a
    // plain invalid word still gets its raw keys back.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_correct: false,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "windowf "), "windowf ");
}

#[test]
fn adjacent_digits_are_all_forwarded() {
    // Regression: digits used to delete the text before them (bamboo
    // clears its composition); each digit after another digit must land.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(type_replay(&mut e, "1223344"), "1223344");
}

#[test]
fn digit_after_a_word_keeps_the_word() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(type_replay(&mut e, "xin chào9"), "xin chào9");
}

#[test]
fn underscore_ends_the_word_and_is_kept() {
    // bamboo-core clears its composition on '_' just like on digits; the
    // word must be finished at '_' and the '_' itself forwarded (it used
    // to delete the word). The letters after it start a new word, as with
    // any terminator.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(typed(mode, "abc_bcd2"), "abc_bcd2", "{mode:?}");
        assert_eq!(typed(mode, "xin chào_"), "xin chào_", "{mode:?}");
    }
}

#[test]
fn repeated_letters_do_not_duplicate_in_any_mode() {
    // Bug 1: "teen"/"tool"/"need" — typing the repeated key that undoes
    // a mark (ee → ê → e) used to add an extra character on screen and
    // again at the word boundary. The visible word must stay exactly
    // what the user sees, in every display mode.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(typed(mode, "teeen "), "teen ", "{mode:?}");
        assert_eq!(typed(mode, "teee "), "tee ", "{mode:?}");
        assert_eq!(typed(mode, "toool "), "tool ", "{mode:?}");
        assert_eq!(typed(mode, "tool "), "tool ", "{mode:?}");
        assert_eq!(typed(mode, "need "), "need ", "{mode:?}");
        // Consecutive terminators must not re-commit the finished word
        // from a stale visible-output override.
        assert_eq!(typed(mode, "toool11"), "tool11", "{mode:?}");
        assert_eq!(typed(mode, "toool  "), "tool  ", "{mode:?}");
    }
}

/// Regression for the slice-based verify: a long report whose prefix ends
/// with a fragment containing the tail far from the caret (unanchored) is
/// ignored, never repaired.
#[test]
fn unanchored_tail_in_a_long_report_is_ignored() {
    // Chrome's find bar reports a page fragment containing the word with the
    // caret far past it; adopting that text used to delete the real input.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    type_str(&mut e, "tooi ");
    let mut page = "t".repeat(4000);
    page.push_str("tooi noisy fragment");
    let len = page.chars().count();
    e.set_surrounding(page, len as u32);
    // The next key verifies against a report whose prefix does not end with
    // the tail: the unanchored-tail case must ignore it, not repair.
    let actions = type_str(&mut e, "x");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::DeleteSurrounding { .. })),
        "{actions:?}"
    );
}

/// The verification compares string slices now; a long report with the caret
/// at its end (the normal healthy path) must still classify as healthy.
#[test]
fn long_report_with_the_tail_at_the_caret_is_healthy() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    type_str(&mut e, "tooi ");
    let mut doc = "x".repeat(20_000);
    doc.push_str("tôi ");
    let len = doc.chars().count();
    e.set_surrounding(doc, len as u32);
    let actions = type_str(&mut e, "x");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::DeleteSurrounding { .. })),
        "healthy tail must not trigger a repair: {actions:?}"
    );
    // The replay covers only this key: the plain commit of the typed char.
    assert_eq!(replay(&actions), "x");
}

#[test]
fn report_with_the_word_far_from_the_caret_is_ignored() {
    // Chrome's find bar reports a page fragment that merely contains the
    // typed word, with the caret far past it. Adopting that text synced
    // the wrapper to the page and the next key deleted the real input
    // ("em tun" → "g"). The word must be anchored at the caret.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    for c in "em tun".chars() {
        e.process_char(c);
    }
    e.set_surrounding("em tung tăng lăng xăng nhìn bầu trời".into(), 36);
    let actions = e.process_char('g');
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
        )),
        "must not delete the real input: {actions:?}"
    );
    assert_eq!(
        e.wrapper.output(),
        "tung",
        "composition stays on the typed word"
    );
}

#[test]
fn inline_suggestions_switch_later_words_to_preedit() {
    // Chrome appends inline autocomplete after the caret mid-word. Its
    // rewrite delete is dropped and a repair would crash mutter, so later
    // words are shown as preedit (no rewrite delete needed).
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    let mut text = String::new();
    for c in "em tun".chars() {
        e.process_char(c);
        text.push(c);
        let n = text.chars().count() as u32;
        e.set_surrounding(text.clone(), n);
    }
    // Suggestion appears after the caret: "em tun|g tăng lăng xăng…".
    e.set_surrounding("em tung tăng lăng xăng".into(), 6);
    let _ = e.process_char(' ');
    let next = e.process_char('t');
    assert!(
        next.iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "the next word must use preedit: {next:?}"
    );
}

#[test]
fn unknown_client_without_surrounding_uses_injection() {
    // XIM/XWayland apps (Sublime Text, JetBrains XToolkit) never send
    // FocusInId: client=None and no surrounding report. Injection is the
    // only working delete transport there, and `None` is not a
    // mutter-relayed context.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_uinput_available(true);
    assert_eq!(e.client(), None);
    for c in "tie".chars() {
        e.process_char(c);
    }
    let actions = e.process_char('e');
    assert!(
        actions.iter().any(|a| matches!(a, Action::DeleteBefore(1))),
        "delete allowed through injection: {actions:?}"
    );
}

#[test]
fn mutter_client_without_surrounding_never_deletes() {
    // Same shape as above, but a known mutter relay has no working
    // transport before its first report: raw-forward instead.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_uinput_available(true);
    for c in "tie".chars() {
        e.process_char(c);
    }
    let actions = e.process_char('e');
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "relay must not delete before a report: {actions:?}"
    );
}

#[test]
fn toolbox_keeps_single_letter_after_an_undone_mark() {
    // Preedit used to show "tooolbox": the undone circumflex left bamboo's
    // internal output longer than the visible text, and the tracker
    // trusted it. Both spellings must come out as typed.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(typed(mode, "tooolbox "), "toolbox ", "{mode:?}");
        assert_eq!(typed(mode, "toolbox "), "toolbox ", "{mode:?}");
        assert_eq!(typed(mode, "tooolb "), "toolb ", "{mode:?}");
    }
}

#[test]
fn every_mode_keeps_text_when_digits_arrive() {
    // Bug 2: a digit used to clear bamboo's composition, which replayed
    // as "delete the whole word". Digits now end the word and are always
    // forwarded — including consecutive digits and digits after a word
    // with no space before them. Same for '_' (Shift+-).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(typed(mode, "1234"), "1234", "{mode:?}");
        assert_eq!(typed(mode, "1223344"), "1223344", "{mode:?}");
        assert_eq!(typed(mode, "xin chào9"), "xin chào9", "{mode:?}");
        assert_eq!(typed(mode, "xin chào1234"), "xin chào1234", "{mode:?}");
        assert_eq!(typed(mode, "tieengs1"), "tiếng1", "{mode:?}");
        assert_eq!(typed(mode, "abc_bcd2"), "abc_bcd2", "{mode:?}");
    }
}

#[test]
fn reset_commits_visible_preedit_after_a_capability_flip() {
    // Bug 3 support: a caret move commits the pending preedit before the
    // key is forwarded. The daemon can flip capabilities mid-word; the
    // word is already visible, so reset must still commit it instead of
    // hiding it (which lost the word) and preedit_commit_pending must
    // still report it.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    for c in "tung".chars() {
        e.process_char(c);
    }
    e.set_capabilities(0); // preedit capability gone mid-word
    assert!(e.preedit_commit_pending());
    assert!(e.reset().contains(&Action::Commit("tung".into())));
}

#[test]
fn apply_config_commits_visible_preedit_instead_of_dropping_it() {
    // A config hot-reload (mode cycle between test rounds, GUI menu
    // change) rebuilds the bamboo engine underneath a pending preedit.
    // The rebuild must commit the word that is on screen first, or the
    // last word of a preedit round disappears ("...và" lost "mùa").
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    for c in "muaf".chars() {
        e.process_char(c);
    }
    // any config change forces the wrapper rebuild (mode switch does too)
    let mut config = Config::default();
    config.free_tone_marking = !config.free_tone_marking;
    let actions = e.apply_config(&config);
    assert!(
        actions.contains(&Action::Commit("mùa".into())),
        "{actions:?}"
    );
    // composition is gone afterwards, so a second apply must not re-commit
    assert!(e.apply_config(&Config::default()).is_empty());
}

#[test]
fn apply_config_mode_switch_commits_pending_preedit() {
    // Changing only the display mode mid-word (the cycle hotkey / the
    // test tool's config rewrite) must also commit, not drop, the word.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    for c in "muaf".chars() {
        e.process_char(c);
    }
    let config = Config {
        mode: Mode::Surrounding.into(),
        ..Config::default()
    };
    let actions = e.apply_config(&config);
    assert!(
        actions.contains(&Action::Commit("mùa".into())),
        "{actions:?}"
    );
}

#[test]
fn auto_restore_can_be_disabled() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_restore: false,
        auto_correct: false,
        ..Config::default()
    });
    let actions = type_str(&mut e, "windowf ");
    let end = replay(&actions);
    // Whatever bamboo produced, it must NOT be rewritten back to raw keys
    // at the word end.
    let restored = actions
        .iter()
        .any(|a| matches!(a, Action::DeleteBefore(n) if *n > 3))
        && actions.contains(&Action::Commit("windowf".into()));
    assert!(!restored, "restore must be off: {end:?}");
}

#[test]
fn dictionary_words_are_never_restored() {
    // The dictionary gate in `on_finish`: a word in the dictionary must
    // not be restored even when it is invalid and differs from the raw
    // keys. The restore condition with a dictionary hit must be false.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        auto_correct: false,
        dictionary_enabled: true,
        ..Config::default()
    });
    let mut dict = Dictionary {
        words: std::collections::HashSet::new(),
    };
    dict.set_words(["wỉndow"].iter().map(|s| s.to_string()));
    e.dictionary = Some(dict);
    let (word, raw) = ("wỉndow".to_string(), "windowf".to_string());
    let dict_hit = e.dictionary.as_ref().is_some_and(|d| d.contains(&word));
    assert!(dict_hit, "dictionary lookup is case-insensitive");
    let would_restore = e.auto_restore && !word.is_empty() && true && word != raw && !dict_hit;
    assert!(!would_restore, "dictionary hit must suppress the restore");
}

#[test]
fn w2u_and_bracket_flow_through_the_config() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        w2u: crate::config::W2uConfig::Everywhere,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "nhw "), "như ");
}

#[test]
fn tail_tracks_forwarded_and_committed_chars() {
    // Regression guard: `tail` must count plain passthrough letters too,
    // or macro/restore deletions would miss them.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    type_str(&mut e, "ab");
    assert_eq!(e.tail, "ab");
}

// --- OFF / Default Typing / uinput-mode fallbacks (phase 6) ---

#[test]
fn off_mode_forwards_raw_keys_but_keeps_macros() {
    let mut e = Engine::new(Method::Telex, Mode::Off);
    // apply_config would reset the mode to the config's, so carry it here.
    e.apply_config(&Config {
        macro_in_off_mode: true,
        mode: ModeConfig::Off,
        ..Config::default()
    });
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "vieetj "), "vieetj ");
    assert_eq!(type_replay(&mut e, "gs "), "giờ sáng ");
}

#[test]
fn off_mode_backspace_belongs_to_the_app() {
    let mut e = Engine::new(Method::Telex, Mode::Off);
    assert_eq!(type_replay(&mut e, "vieetj"), "vieetj");
    assert!(e.backspace().is_none());
}

#[test]
fn flow_without_backend_types_through_forward_backspace() {
    // Flow without /dev/uinput: the walk downgrades to the forwarded
    // transport — commit-as-you-type with the tone rewrites erasing
    // through forwarded Backspaces; preedit never stands in. The guard
    // keeps this test's own fallback log line out of the counting test's
    // redirected log file (they share the process-wide log globals).
    let _guard = crate::debug_log::test_guard();
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_uinput_available(false);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    let actions = type_str(&mut e, "vieetj");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "the downgrade must serve the word: {actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "no backend never renders preedit: {actions:?}"
    );
    assert!(e.flow_needs_backend());
    assert_eq!(replay(&actions), "việt");
}

#[test]
fn flow_runs_on_the_relay() {
    // The 31/10 belief ("injection never reaches a relayed app") was
    // refuted by the 08/11 re-measurement: injection, echoes, deletes and
    // commits all land on a mutter relay — what raced was the ordering,
    // fixed by the parked batch. A relayed context now types through the
    // uinput mode like a direct one.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_uinput_available(true);
    // A mutter-owned IBus context (type_str would replace None).
    e.set_client(Some("gnome-shell:gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    let actions = type_str(&mut e, "vieetj ");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })),
        "{actions:?}"
    );
    let batch = e
        .per_key_park
        .as_ref()
        .map(|(_, t)| t.clone())
        .expect("the relay must run the per-key rewrite");
    assert_eq!(
        batch, "việt ",
        "the word and its terminator coalesce into the parked batch"
    );
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Forward)),
        "no forwarded key races the parked batch: {actions:?}"
    );
}

#[test]
fn parked_commit_flushes_only_past_its_deadline() {
    // Measured on a mutter relay (08/11): the app applies the forwarded
    // Backspaces a beat behind the commit channel, and a commit emitted
    // with the last echo overtook the final deletes ("titiến"). The
    // server parks it; the main-loop tick delivers it past the deadline.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.park_commit(
        "tiếng".into(),
        Duration::from_millis(Engine::ECHO_APPLY_GRACE_MS),
    );
    assert!(e.has_deferred());
    assert!(
        e.tick().is_empty(),
        "before the deadline nothing is deliverable"
    );
    assert!(
        e.ms_until_tick().is_some(),
        "the loop must wake for the deadline"
    );
    // Backdate the park deadline instead of sleeping through it.
    let parked = std::mem::take(&mut e.timed_commits);
    e.timed_commits = parked
        .into_iter()
        .map(|(_, text)| (Instant::now() - Duration::from_millis(10), text))
        .collect();
    assert_eq!(
        e.tick(),
        vec![Action::Commit("tiếng".into())],
        "past the deadline the tick delivers the commit"
    );
    assert!(!e.has_deferred());
}

#[test]
fn parked_commit_flushes_when_forced() {
    // A focus change must not strand the parked commit: delivering late
    // beats never (the same rule as the echo deferral).
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.park_commit("việt".into(), Duration::from_secs(60));
    assert_eq!(e.flush_deferred(), vec!["việt".to_string()]);
    assert!(!e.has_deferred());
}

// --- flow per-key display ---

fn per_key_engine(mode: Mode) -> Engine {
    let mut e = engine(
        mode,
        Config {
            experimental_features: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    e
}

/// Backdate the per-key batch's deadline and run the tick — the parked
/// commit becomes deliverable exactly as the main loop would deliver it.
fn flush_per_key(e: &mut Engine) -> Vec<Action> {
    if let Some((deadline, _)) = e.per_key_park.as_mut() {
        *deadline = Instant::now() - Duration::from_millis(1);
    }
    e.tick()
}

#[test]
fn flow_per_key_commits_across_flush_boundaries() {
    // With the switch on, the keys coalesce into one in-flight batch; the
    // main loop delivers it past the grace deadline. After a flush, the
    // next mark key erases the landed text and re-parks: the screen shows
    // `ê` the moment the second `e` lands (once the batch flushes).
    let mut e = per_key_engine(Mode::Flow);
    let actions = type_str(&mut e, "tie");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Commit(_) | Action::Forward)),
        "a burst commits nothing inline: {actions:?}"
    );
    assert_eq!(e.tail, "tie");
    let actions = flush_per_key(&mut e);
    assert_eq!(
        actions,
        vec![Action::CommitNow("tie".into())],
        "{actions:?}"
    );
    assert!(e.per_key_park.is_none());
    // The mark key: erase the landed `e`, re-park `ê`.
    let actions = type_str(&mut e, "e");
    assert!(
        actions.contains(&Action::InjectBackspaces(1)),
        "{actions:?}"
    );
    let actions = flush_per_key(&mut e);
    assert_eq!(actions, vec![Action::CommitNow("ê".into())], "{actions:?}");
    assert_eq!(e.tail, "tiê");
}

#[test]
fn flow_per_key_burst_coalesces_into_one_commit() {
    // Fast typing (no flush between keys) coalesces: nothing lands
    // mid-burst, no deletes are needed (the app holds nothing of ours),
    // and the whole word arrives as ONE commit when the burst pauses.
    let mut e = per_key_engine(Mode::Flow);
    let actions = type_str(&mut e, "tieeng");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Commit(_) | Action::Forward)),
        "a burst commits nothing inline: {actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::InjectBackspaces(_))),
        "nothing landed, nothing to erase: {actions:?}"
    );
    assert_eq!(e.tail, "tiêng");
    let batch = e
        .per_key_park
        .as_ref()
        .map(|(_, t)| t.clone())
        .expect("the batch is pending");
    assert_eq!(batch, "tiêng");
    let actions = flush_per_key(&mut e);
    assert_eq!(
        actions,
        vec![Action::CommitNow("tiêng".into())],
        "one commit for the whole burst"
    );
}

#[test]
fn per_key_delay_follows_the_configured_value() {
    let mut e = engine(
        Mode::Flow,
        Config {
            experimental_features: true,
            flow_per_key_delay_ms: 200,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    let start = Instant::now();
    type_str(&mut e, "t");
    let (deadline, _) = e.per_key_park.as_ref().expect("per-key batch parked");
    let lag = deadline.duration_since(start);
    assert!(
        lag > Duration::from_millis(150) && lag < Duration::from_millis(250),
        "park deadline must follow the configured 200 ms, got {lag:?}"
    );
}

#[test]
fn per_key_delay_clamps_out_of_range_values() {
    // 0 is allowed by owner decision (the clamp protects against nonsense
    // values, the floor stays open).
    let e = engine(
        Mode::Flow,
        Config {
            flow_per_key_delay_ms: 0,
            ..Config::default()
        },
    );
    assert_eq!(e.per_key_delay(), Duration::from_millis(0));
    let e = engine(
        Mode::Flow,
        Config {
            flow_per_key_delay_ms: 700,
            ..Config::default()
        },
    );
    assert_eq!(e.per_key_delay(), Duration::from_millis(500));
    assert_eq!(
        Engine::new(Method::Telex, Mode::Flow).per_key_delay(),
        Duration::from_millis(20),
        "the engine default follows the config default"
    );
}

#[test]
fn flow_backspace_retrack_works_without_the_switch() {
    // Retrack is standard Flow code, not an experiment: `tungf` ␣ commits
    // `tùng `; Backspace deletes the last screen character — the shortened
    // screen is re-tracked when the pop search cannot (a tone outliving its
    // key), and the next tone key keeps editing the word: `tùn` + `s` lands
    // `tún ` at word end, not the literal `tùns`. No experimental switch
    // involved (default config, gate closed). UnikeyWalk is the setting
    // whose Backspace reaches that shortened screen: ToneFirst drops the
    // tone first (`tùng` -> `tung`, pinned by the probe suite).
    let mut e = engine(
        Mode::Flow,
        Config {
            backspace_tone: BackspaceToneConfig::UnikeyWalk,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    type_str(&mut e, "tungf ");
    let actions = flush_per_key(&mut e);
    assert_eq!(
        actions,
        vec![Action::CommitNow("tùng ".into())],
        "{actions:?}"
    );
    // Backspace over the trailing space: the pop search has no raw keys
    // (the word ended), the re-encoder rebuilds the composition.
    e.backspace();
    assert_eq!(e.tail, "tùng");
    assert!(
        e.wrapper.has_composition(),
        "the word must come back live after the Backspace"
    );
    // Backspace over the `g`: the pop search matches now (unikey walk).
    e.backspace();
    assert_eq!(e.tail, "tùn");
    // The tone key replaces the mark; the word end delivers `tún ` as
    // screen math: the app holds `tùns`, so the rewrite erases the `ns`
    // inline and the parked batch inserts `ún ` right after the `tù`.
    let actions = type_str(&mut e, "s ");
    assert_eq!(actions, vec![Action::InjectBackspaces(2)], "{actions:?}");
    let actions = flush_per_key(&mut e);
    assert_eq!(
        actions,
        vec![Action::CommitNow("ún ".into())],
        "{actions:?}"
    );
}

#[test]
fn flow_backspace_restores_the_undone_mark_without_the_switch() {
    // The restore is standard code too: `chào` + `f` undoes the mark (the
    // screen shows the raw `chaof`); Backspace pops that key and the
    // deferred correction gives the mark back with the user's "Backspace
    // xóa dấu" setting — `chào` returns, like the inline modes. No
    // experimental switch involved (default config, gate closed).
    let mut e = engine(Mode::Flow, Config::default());
    e.set_uinput_available(true);
    type_str(&mut e, "chaoff");
    flush_per_key(&mut e);
    assert_eq!(e.tail, "chaof");
    let actions = e.backspace().expect("backspace");
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert!(
        e.deferred_fix.is_some(),
        "the mark restore must be scheduled"
    );
    // The correction waits for the key release, then lands on the main
    // loop: the rewrite erases the raw key and re-commits the marked word.
    e.note_backspace_released();
    if let Some((deadline, _)) = e.deferred_fix.as_mut() {
        *deadline = Instant::now() - Duration::from_millis(1);
    }
    let actions = e.tick();
    assert!(e.deferred_fix.is_none(), "the fix must have landed");
    assert_eq!(
        actions,
        vec![Action::InjectBackspaces(2)],
        "the fix erases the raw key's screen chars: {actions:?}"
    );
    assert_eq!(e.tail, "chào");
    assert!(
        e.wrapper.has_composition(),
        "the word is live again, ready for the next mark key"
    );
    let actions = flush_per_key(&mut e);
    assert_eq!(actions, vec![Action::CommitNow("ào".into())], "{actions:?}");
}

#[test]
fn flow_per_key_tonefirst_fold_keeps_every_screen_char() {
    // A key typed inside the restore window after a ToneFirst Flow
    // Backspace folds the pending tone fix into its batch. The pop keeps
    // in the wrapper the letter the user's Backspace deleted from the
    // screen (`tung` in the wrapper, `tùn` on screen), so the fold must
    // not apply the wrapper's delta to the screen directly: that erased
    // the leading `t` (`InjectBackspaces(3)`, screen `úng`).
    let mut e = engine(Mode::Flow, Config::default());
    e.set_uinput_available(true);
    type_str(&mut e, "tungf ");
    flush_per_key(&mut e); // "tùng " lands
    assert_eq!(e.tail, "tùng ");
    e.backspace(); // the terminator goes; the word comes back live
    assert_eq!(e.tail, "tùng");
    let actions = e.backspace().expect("backspace");
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert!(e.deferred_fix.is_some(), "the tone fix must be scheduled");
    assert_eq!(e.tail, "tùn");
    // Fast `s` lands inside the window: the fix folds into this key's
    // batch and the screen keeps every character.
    let actions = type_str(&mut e, "s");
    assert_eq!(actions, vec![Action::InjectBackspaces(2)], "{actions:?}");
    assert_eq!(e.tail, "túng");
    assert!(
        e.deferred_fix.is_none(),
        "the fix folded into the key's batch"
    );
    let actions = flush_per_key(&mut e);
    assert_eq!(
        actions,
        vec![Action::CommitNow("úng".into())],
        "{actions:?}"
    );
    assert_eq!(e.tail, "túng");
}

#[test]
fn flow_per_key_word_end_carries_the_terminator_in_the_batch() {
    // The word-end terminator joins the coalesced batch — a forwarded key
    // would race the pending commit on a relay.
    let mut e = per_key_engine(Mode::Flow);
    let actions = type_str(&mut e, "tieengs ");
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Forward)),
        "the terminator must not forward past the batch: {actions:?}"
    );
    let batch = e
        .per_key_park
        .as_ref()
        .map(|(_, t)| t.clone())
        .expect("the batch is pending");
    assert_eq!(batch, "tiếng ");
    let actions = flush_per_key(&mut e);
    assert_eq!(actions, vec![Action::CommitNow("tiếng ".into())]);
    assert_eq!(e.tail, "tiếng ");
}

#[test]
fn flow_per_key_backspace_forwards_and_keeps_tracking() {
    // After a flush the word has landed; the user's real Backspace deletes
    // its last character through the pass-through path — zero injections,
    // the transport that loses echo presses on a relay is gone entirely.
    // The reverse-map pops the mark key so the word stays tracked.
    // WholeChar is the setting whose Backspace IS that screen-character
    // delete; under ToneFirst the Backspace undoes the raw key instead and
    // the screen correction lands deferred (the probe suite pins it).
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Flow,
        BackspaceToneConfig::WholeChar,
    );
    e.set_uinput_available(true);
    type_str(&mut e, "tie");
    flush_per_key(&mut e); // "tie" lands
    let actions = type_str(&mut e, "e"); // the mark: erase the landed `e`
    assert!(
        actions.contains(&Action::InjectBackspaces(1)),
        "{actions:?}"
    );
    flush_per_key(&mut e); // `ê` lands
    assert_eq!(e.tail, "tiê");
    let actions = e.backspace().expect("backspace");
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert!(e.per_key_park.is_none());
    // The screen's last character (`ê`) is gone — the text-editing model:
    // the reverse-map popped both raw `e` keys behind it.
    assert_eq!(e.tail, "ti");
    // Tracking survived: the retyped key appends through the batch.
    type_str(&mut e, "e");
    assert_eq!(
        e.per_key_park.as_ref().map(|(_, t)| t.as_str()),
        Some("e"),
        "the word is still tracked after the backspace"
    );
    flush_per_key(&mut e);
    assert_eq!(e.tail, "tie");
}

#[test]
fn flow_per_key_macro_rewrites_the_landed_tail() {
    // The word-end macro erases what the app actually holds (the landed
    // tail after a flush) and parks the expansion with the terminator.
    let mut e = per_key_engine(Mode::Flow);
    e.macros
        .set_entries([("zz".to_string(), "CLIP".to_string())].into_iter());
    type_str(&mut e, "zz");
    flush_per_key(&mut e); // "zz" lands
    let actions = type_str(&mut e, " "); // word end: the macro expands
    assert!(
        actions.contains(&Action::InjectBackspaces(2)),
        "the landed tail is erased: {actions:?}"
    );
    let actions = flush_per_key(&mut e);
    assert_eq!(actions, vec![Action::CommitNow("CLIP ".into())]);
}

#[test]
fn flow_per_key_word_end_never_erases_previous_words() {
    // Regression (owner report): typing the second word's mark erased the
    // FIRST word — the word-end coalesce diffed the bare word against the
    // whole-screen tail and erased everything before it ("tôi cảm" ->
    // "cảm "). The tail is the word's region only: the diff and the erase
    // stay inside the word, and the previous words are never touched.
    let mut e = per_key_engine(Mode::Flow);
    let mut all = Vec::new();
    all.extend(type_str(&mut e, "tôi "));
    all.extend(flush_per_key(&mut e)); // "tôi " lands
    all.extend(type_str(&mut e, "camr ")); // word 2 coalesces
    all.extend(flush_per_key(&mut e)); // "cảm " lands
    let deletes: Vec<u32> = all
        .iter()
        .filter_map(|a| match a {
            Action::InjectBackspaces(n) => Some(*n),
            _ => None,
        })
        .collect();
    assert!(
        deletes.is_empty(),
        "no erase may reach past the current word: {deletes:?} in {all:?}"
    );
    // Two landed commits: the first word, then the second with its mark.
    let commits: Vec<&str> = all
        .iter()
        .filter_map(|a| match a {
            Action::CommitNow(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(commits, vec!["tôi ", "cảm "], "{all:?}");
    assert_eq!(e.tail, "cảm ");
}

#[test]
fn flow_per_key_legacy_charset_rewrites_the_tail() {
    // Legacy encodings change the character count: the per-key batch
    // carries the whole re-encoded tail, diffed against what landed.
    let mut e = per_key_engine(Mode::Flow);
    e.set_charset(Charset::Vni);
    type_str(&mut e, "vie");
    flush_per_key(&mut e); // the encoded "vie" lands
    type_str(&mut e, "e"); // the mark re-encodes the tail
    let actions = flush_per_key(&mut e);
    // The coalesce diffs the encoded tails: only the VNI circumflex
    // marker is new, so the commit carries just that suffix.
    assert_eq!(actions, vec![Action::CommitNow("â".into())], "{actions:?}");
    assert_eq!(e.tail, encode(Charset::Vni, "viê"));
}

#[test]
fn flow_per_key_backspace_lands_the_batch_before_deleting() {
    // Backspace within the grace window: the pending batch must land first
    // (the pass-through key deletes a character of the FINAL screen), then
    // the real Backspace deletes its last character.
    let mut e = per_key_engine(Mode::Flow);
    type_str(&mut e, "vie");
    assert!(e.per_key_park.is_some());
    let actions = e.backspace().expect("backspace");
    assert_eq!(
        actions,
        vec![Action::CommitNow("vie".into()), Action::Forward],
        "{actions:?}"
    );
    assert!(e.per_key_park.is_none());
    assert_eq!(e.tail, "vi");
}

#[test]
fn flow_per_key_backspace_keeps_tracking_across_mark_keys() {
    // `việt` = raw `vieejt`. The Backspace deletes the screen's `t`; the
    // reverse-map pops raw keys until the recomposition matches, so the
    // retype restores the word and a further delete stays tracked too.
    let mut e = per_key_engine(Mode::Flow);
    type_str(&mut e, "vieejt");
    flush_per_key(&mut e);
    assert_eq!(e.tail, "việt");
    let actions = e.backspace().expect("backspace");
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert_eq!(e.tail, "việ");
    // Deleting into the marked vowel: four raw keys (`t`, `j` and both
    // `e`) map to the one screen char this Backspace removes.
    let actions = e.backspace().expect("backspace");
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert_eq!(e.tail, "vi");
    // Retyping the tail re-marks the whole word through one batch.
    type_str(&mut e, "eejt");
    assert_eq!(
        e.per_key_park.as_ref().map(|(_, t)| t.as_str()),
        Some("ệt"),
        "the retype parks through the tracked word"
    );
    flush_per_key(&mut e);
    assert_eq!(e.tail, "việt");
}

#[test]
fn flow_per_key_backspace_releases_words_it_cannot_remap() {
    // A word with no raw keys (the surrounding-text repair rebuilds the
    // composition from text) has no prefix to reverse-map: the word is
    // released. The real Backspace still deletes the last character —
    // zero injections — and the next key starts a fresh word.
    let mut e = per_key_engine(Mode::Flow);
    type_str(&mut e, "tie");
    flush_per_key(&mut e);
    e.wrapper.rebuild("tiê");
    let actions = e.backspace().expect("backspace");
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert_eq!(e.tail, "ti");
    type_str(&mut e, "n");
    assert_eq!(
        e.per_key_park.as_ref().map(|(_, t)| t.as_str()),
        Some("n"),
        "the next key is a fresh word"
    );
}

#[test]
fn flow_per_key_applies_without_the_experimental_gate() {
    // The display is not part of the experimental set: the master gate
    // closed must not force it off (it shapes the typing either way).
    let mut e = engine(Mode::Flow, Config::default());
    e.set_uinput_available(true);
    let _ = type_str(&mut e, "vie");
    let actions = flush_per_key(&mut e);
    assert_eq!(
        actions,
        vec![Action::CommitNow("vie".into())],
        "per-key display must follow the switch with the gate closed: {actions:?}"
    );
}

// --- Mode resolution walk (replaces the effective_mode heuristic chain) ---

#[test]
fn preedit_without_capability_walks_to_surrounding() {
    // caps without CAP_PREEDIT_TEXT, requested Preedit: the walk moves on
    // to the commit-as-you-type contract (Surrounding) instead of dropping
    // to raw keystrokes, and the words still commit — through the protocol
    // delete the report proves, like a real client's report at focus-in.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_SURROUNDING_TEXT);
    e.set_surrounding(String::new(), 0);
    assert_eq!(e.effective_mode(), Mode::Surrounding);
    let mut buf: Vec<char> = Vec::new();
    for c in "tieengs".chars() {
        for a in e.process_char(c) {
            match a {
                Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(n as usize)),
                Action::Commit(t) => buf.extend(t.chars()),
                Action::UpdatePreedit { .. } => {
                    panic!("no preedit without the capability: {a:?}")
                }
                _ => {}
            }
        }
        let text: String = buf.iter().collect();
        e.set_surrounding(text, buf.len() as u32);
    }
    assert_eq!(buf.iter().collect::<String>(), "tiếng");
}

#[test]
fn surrounding_without_report_uses_injected_deletes_when_a_backend_exists() {
    // No ST report but a backend: the walk keeps Surrounding and the
    // rewrites go through the no-ST injection path — the DeleteBefore
    // actions the server delivers as real Backspace key presses.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_uinput_available(true);
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    assert_eq!(e.effective_mode(), Mode::Surrounding);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::DeleteBefore(n) if *n > 0)),
        "the no-ST rewrite path: {actions:?}"
    );
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { .. })));
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn surrounding_on_a_capability_less_client_forwards_backspaces() {
    // No ST report and no backend: the walk terminates on the forwarded
    // transport — Surrounding is served by ForwardBackspaces, it does not
    // fall back to preedit.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0);
    assert_eq!(e.effective_mode(), Mode::ForwardBackspace);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "terminal transport: {actions:?}"
    );
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn caps0_uinput_auto_resolves_to_forward() {
    // A caps-0 context (a real XIM client) under auto, with an injection
    // backend present: the backend proves nothing about an inline rewrite
    // the client renders no preedit for, so the walk still terminates on the
    // forwarded transport — the one that reaches every client by contract.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_capabilities(0);
    e.set_uinput_available(true);
    assert_eq!(e.effective_mode(), Mode::ForwardBackspace);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "the forwarded transport serves the caps-0 client: {actions:?}"
    );
    assert!(
        !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
        "no injected rewrite without a preedit-capable client: {actions:?}"
    );
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn flow_without_backend_serves_forward_backspace_and_logs_once() {
    // Flow without a backend: the walk downgrades to the forwarded
    // transport and logs the fallback once per session — two resolutions,
    // one line.
    let _guard = crate::debug_log::test_guard();
    let dir = std::env::temp_dir().join(format!("ibus-mangrs-walk-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("debug.log");
    let _ = std::fs::remove_file(&path);
    *crate::debug_log::PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
    crate::debug_log::set_enabled(true);
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_uinput_available(false);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    assert_eq!(e.effective_mode(), Mode::ForwardBackspace);
    assert!(e.flow_needs_backend());
    // A second resolution must not log the fallback again.
    assert_eq!(e.effective_mode(), Mode::ForwardBackspace);
    crate::debug_log::test_reset();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        text.lines()
            .filter(|l| l.contains("flow without a backend"))
            .count(),
        1,
        "one fallback line for two resolutions: {text}"
    );
    assert_eq!(
        replay(&type_raw(&mut e, "tieengs")),
        "tiếng",
        "the downgrade serves the word"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn forward_backspace_never_falls_back() {
    // caps 0, no ST report, no backend: the ForwardBackspace request is
    // terminal — the walk keeps it, nothing stands in.
    let mut e = Engine::new(Method::Telex, Mode::ForwardBackspace);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(0);
    e.set_uinput_available(false);
    assert_eq!(e.effective_mode(), Mode::ForwardBackspace);
    let actions = type_raw(&mut e, "tieengs");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ForwardBackspaces(_))),
        "{actions:?}"
    );
    assert!(!actions
        .iter()
        .any(|a| matches!(a, Action::UpdatePreedit { .. })));
    assert_eq!(replay(&actions), "tiếng");
}

#[test]
fn listed_delete_dropping_clients_keep_the_preedit_outcome() {
    // Matrix row: a listed delete-dropping client with preedit capability
    // keeps today's outcome — the word shows inline instead of being
    // rewritten with a delete the client drops. The Surrounding request
    // runs the commit-as-you-type walk, whose first stop is this preedit
    // stand-in.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_capabilities(CAP_PREEDIT_TEXT);
    e.set_client(Some("gtk4-im:chrome".into()));
    assert_eq!(e.effective_mode(), Mode::Preedit);
    let actions = type_str(&mut e, "tieengs");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "tiếng")),
        "{actions:?}"
    );
}

#[test]
fn compositor_routed_backend_is_exempt_from_the_relay_ban() {
    // The relay ban exists because uinput keys never reach a relayed
    // context; a compositor-routed backend (Libei / wlr virtual keyboard)
    // rides the compositor's own routing, so its deletes are allowed —
    // symmetrically on both ban consumers: the deletes_allowed gate and
    // the may_inject gate the server's inject routing reads. Without the
    // exemption the same context forwards instead.
    for (safe, want_delete) in [(false, false), (true, true)] {
        let mut e = Engine::new(Method::Telex, Mode::Surrounding);
        e.set_client(Some("gnome-shell".into()));
        e.set_uinput_available(true);
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.set_injection_relay_safe(safe);
        assert_eq!(e.deletes_allowed(), want_delete, "safe={safe}");
        // No report on this client, so the second may_inject condition
        // (delete proof or no report) holds and the exemption decides.
        assert_eq!(e.may_inject(), want_delete, "safe={safe}");
        let actions = type_raw(&mut e, "tiee");
        assert_eq!(
            actions
                .iter()
                .any(|a| matches!(a, Action::DeleteBefore(n) if *n > 0)),
            want_delete,
            "safe={safe}: {actions:?}"
        );
    }
}

// --- URL/email/path bypass (spell-check round, phase 2-3) ---

#[test]
fn url_bypass_can_be_disabled() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        bypass_url: false,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "https://x.tieeng "), "https://x.tiêng ");
}

#[test]
fn url_bypass_releases_at_the_next_space() {
    // The word after the URL ends must type Vietnamese again
    // ("nhieeuf" → "nhiều" — huyền from the f).
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(
        type_replay(&mut e, "https://x.tieeng nhieeuf "),
        "https://x.tieeng nhiều "
    );
}

#[test]
fn url_bypass_releases_when_backspace_erases_the_word() {
    // `www.abc` types raw inside the URL span; erasing the bypassed word
    // with Backspace must release it — space/enter used to be the only way
    // out (deleting a mis-typed address left every following letter raw).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, Config::default());
        let (out, oor) = type_replay_backspaces(&mut e, "www.abc\u{8}\u{8}\u{8}vieetj ");
        assert!(!oor, "{mode:?}");
        assert_eq!(out, "www.việt ", "{mode:?}");
    }
}

#[test]
fn url_bypass_releases_after_backspace_in_flow() {
    // The per-key tail spans whole words, so the release cannot key on an
    // empty tail — the bypass span is measured from its arm point. The next
    // key must compose, not forward raw. The guard keeps this test's own
    // fallback log line (Flow without a backend, resolved per keystroke
    // here) out of the counting test's redirected log file.
    let _guard = crate::debug_log::test_guard();
    let mut e = engine(Mode::Flow, Config::default());
    let _ = type_replay_backspaces(&mut e, "https://abc\u{8}\u{8}\u{8}\u{8}\u{8}");
    let actions = e.process_char('v');
    assert!(!actions.contains(&Action::Forward), "{actions:?}");
    assert_eq!(e.wrapper.raw(), "v");
}

#[test]
fn url_bypass_survives_a_backspace_inside_the_word() {
    // Deleting part of the address keeps the span: the rest is still on
    // screen and the next keys stay raw (`https://ab\b c ` → `https://ac `).
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, Config::default());
        let (out, oor) = type_replay_backspaces(&mut e, "https://ab\u{8}c ");
        assert!(!oor, "{mode:?}");
        assert_eq!(out, "https://ac ", "{mode:?}");
    }
}

#[test]
fn url_bypass_spans_hyphenated_segments() {
    // The segment after the hyphen used to be composed as Vietnamese, so
    // a Telex mark key there stuck: "alo-tungf" → "alo-tùng",
    // "vi-mangrs" → "vi-máng". The address stays raw.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(
            typed(mode, "https://alo-tungf.com "),
            "https://alo-tungf.com ",
            "{mode:?}"
        );
        assert_eq!(
            typed(mode, "https://vi-mangrs.com "),
            "https://vi-mangrs.com ",
            "{mode:?}"
        );
    }
}

#[test]
fn url_bypass_keeps_the_em_dash_outside_urls() {
    // `-` does not start a bypass, so `--` in normal text still becomes
    // one em dash.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            em_dash: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "a--b "), "a—b ");
}

#[test]
fn prose_punctuation_no_longer_starts_the_bypass() {
    // No character starts a bypass any more: words glued straight onto
    // prose punctuation must compose again (the owner's exact cases).
    assert_eq!(
        typed(Mode::Surrounding, "mucj tieeu:phats trieenr "),
        "mục tiêu:phát triển "
    );
    assert_eq!(typed(Mode::Surrounding, "x=coongj "), "x=cộng ");
    assert_eq!(typed(Mode::Surrounding, "#Bangwf "), "#Bằng ");
    assert_eq!(typed(Mode::Surrounding, "?tungf "), "?tùng ");
    assert_eq!(typed(Mode::Surrounding, "_tungf "), "_tùng ");
    assert_eq!(typed(Mode::Surrounding, "@tungf "), "@tùng ");
    assert_eq!(typed(Mode::Surrounding, ".tungf "), ".tùng ");
    assert_eq!(typed(Mode::Surrounding, "/tungf "), "/tùng ");
}

#[test]
fn url_span_holds_query_and_fragment_raw() {
    // The scheme span keeps the whole address raw — query and fragment
    // included — and the next word after a space types Vietnamese again.
    assert_eq!(
        typed(
            Mode::Surrounding,
            "https://vi-mangrs.com/x?taikhoanf=1&y=2#top nhieeuf "
        ),
        "https://vi-mangrs.com/x?taikhoanf=1&y=2#top nhiều "
    );
}

#[test]
fn url_span_releases_at_sentence_punctuation() {
    // Sentence punctuation ends the span; the URL-internal separators
    // (`. / : @ _ ~ = # & % ? - +`) do not.
    assert_eq!(
        typed(Mode::Surrounding, "https://x.com, nhieeuf "),
        "https://x.com, nhiều "
    );
}

#[test]
fn www_opens_the_url_span() {
    // A scheme-less address still holds the query raw through the `www.`
    // evidence.
    assert_eq!(
        typed(
            Mode::Surrounding,
            "www.vi-mangrs.com/x?taikhoanf=1 nhieeuf "
        ),
        "www.vi-mangrs.com/x?taikhoanf=1 nhiều "
    );
}

#[test]
fn uppercase_scheme_opens_the_url_span() {
    assert_eq!(
        typed(Mode::Surrounding, "HTTPS://x.com?taikhoanf=1 nhieeuf "),
        "HTTPS://x.com?taikhoanf=1 nhiều "
    );
}

#[test]
fn digits_before_the_scheme_do_not_open_the_span() {
    // A scheme starts with a letter: `12:34/…` is a time, not a URL — the
    // word after `?` must compose (the old set blocked it via `?`).
    assert_eq!(
        typed(Mode::Surrounding, "12:34/x?tieengs "),
        "12:34/x?tiếng "
    );
}

#[test]
fn url_span_in_flow_holds_the_query_raw() {
    // The span forwards raw in the uinput mode too: no rewrite batch for
    // the address, and the word after the span composes through the
    // per-key batch.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_uinput_available(true);
    let url_actions = type_str(&mut e, "https://x.com?taikhoanf=1 ");
    assert!(
        !url_actions.iter().any(|a| matches!(
            a,
            Action::Commit(_) | Action::CommitNow(_) | Action::InjectBackspaces(_)
        )),
        "{url_actions:?}"
    );
    type_str(&mut e, "nhieeuf ");
    let flushed = flush_per_key(&mut e);
    assert!(
        flushed
            .iter()
            .any(|a| matches!(a, Action::CommitNow(t) if t == "nhiều ")),
        "the word after the span still composes: {flushed:?}"
    );
}

#[test]
fn macro_does_not_expand_valid_vietnamese_words() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.macros
        .set_entries([("cam".to_string(), "CLIP".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "cam "), "cam ");
    // Raw Telex for a Vietnamese word is normal typing as well.
    e.macros
        .set_entries([("vieetj".to_string(), "CLIP".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "vieetj "), "việt ");
}

#[test]
fn macro_still_expands_nonvietnamese_keys_in_off_mode() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        macro_in_off_mode: true,
        ..Config::default()
    });
    e.set_enabled(false);
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, "gs "), "giờ sáng ");
}

#[test]
fn flow_leaves_english_words_alone() {
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_uinput_available(true);
    let actions = type_str(&mut e, "hello ");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::InjectBackspaces(_) | Action::Commit(_))),
        "word == raw: no rewrite, the keys already sit in the app: {actions:?}"
    );
}

#[test]
fn flow_auto_capitalizes_at_sentence_start() {
    let mut e = engine(
        Mode::Flow,
        Config {
            auto_capitalize: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    type_str(&mut e, "xin chao. ");
    flush_per_key(&mut e); // the first sentence lands
    type_str(&mut e, "tieengs ");
    let flushed = flush_per_key(&mut e);
    assert!(
        flushed
            .iter()
            .any(|a| matches!(a, Action::CommitNow(t) if t == "Tiếng ")),
        "the sentence must start capitalized: {flushed:?}"
    );
    assert_eq!(
        e.tail, "Tiếng ",
        "the capital only exists in the landed batch, never the live tail"
    );
}

#[test]
fn flow_types_through_app_autofill() {
    // No app-autofill hold-back: the per-key rewrites are backward-only (a
    // suggestion sits after the caret), so with the app appending suggestion
    // text the word still composes and lands through the parked batch — and
    // every rewrite delete derives from the tail diff, never past it.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    e.set_uinput_available(true);
    e.set_client(Some("gtk4-im:test".into()));
    e.set_suggestions(true);
    let all = type_str(&mut e, "vieetj ");
    assert!(
        !all.iter().any(|a| matches!(a, Action::Forward)),
        "no forwarded key races the batch: {all:?}"
    );
    let batch = e
        .per_key_park
        .as_ref()
        .map(|(_, t)| t.clone())
        .expect("the batch is pending");
    assert_eq!(batch, "việt ", "the word and its terminator ride the batch");
    // The deletes derive only from the tail diff: the burst landed nothing
    // of ours, so the diff needed no delete at all.
    let deletes: Vec<u32> = all
        .iter()
        .filter_map(|a| match a {
            Action::InjectBackspaces(n) => Some(*n),
            _ => None,
        })
        .collect();
    assert!(
        deletes.is_empty(),
        "the app held nothing of ours while the burst typed: {deletes:?} in {all:?}"
    );
    let flushed = flush_per_key(&mut e);
    assert!(
        flushed
            .iter()
            .any(|a| matches!(a, Action::CommitNow(t) if t == "việt ")),
        "{flushed:?}"
    );
    // With the batch landed, the next rewrite deletes exactly the landed
    // diff — inside the tracked tail, never reaching past it (the
    // suggestion after the caret is out of reach by construction).
    type_str(&mut e, "cam");
    flush_per_key(&mut e); // "cam" lands
    let actions = type_str(&mut e, "r");
    assert_eq!(
        actions,
        vec![Action::InjectBackspaces(2)],
        "the mark delete erases the landed diff inside the tail: {actions:?}"
    );
}

// --- client Reset: the client committed the preedit (PREEDIT_COMMIT) ---

#[test]
fn client_reset_drops_the_pending_word_like_bamboo() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    type_str(&mut e, "chaof");
    assert!(e.preedit_visible);
    let actions = e.on_client_reset();
    assert!(actions.contains(&Action::HidePreedit), "{actions:?}");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Commit(_) | Action::DeleteSurrounding { .. })),
        "a click must not commit, delete or relocate anything: {actions:?}"
    );
    assert!(!e.preedit_visible && !e.wrapper.has_composition());
}

#[test]
fn client_reset_without_a_pending_word_is_clean() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    type_str(&mut e, "xin ");
    let actions = e.on_client_reset();
    assert!(actions.is_empty(), "{actions:?}");
    assert!(!e.preedit_visible);
}

#[test]
fn reset_expands_a_pending_macro() {
    // A focus loss used to commit the raw keys.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    type_str(&mut e, "gs");
    let actions = e.reset();
    assert!(
        actions.contains(&Action::Commit("giờ sáng".into())),
        "{actions:?}"
    );
    assert!(!e.preedit_visible && !e.wrapper.has_composition());
}

#[test]
fn focus_out_drops_the_word_without_touching_the_preedit_signal() {
    // FocusOut/Disable: the client commits the preedit itself
    // (PREEDIT_COMMIT) and clears its own display; our HidePreedit could
    // arrive before that commit in clients that handle FocusOut first and
    // would wipe the word they are about to commit.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    type_str(&mut e, "tung");
    assert!(e.preedit_visible);
    let actions = e.focus_out();
    assert!(actions.is_empty(), "{actions:?}");
    assert!(!e.preedit_visible && !e.wrapper.has_composition());
    // A client Reset still hides the preedit (the client cleared its own
    // display before sending Reset).
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    type_str(&mut e, "tung");
    assert!(e.on_client_reset().contains(&Action::HidePreedit));
}

#[test]
fn abbreviation_keeps_its_d_in_the_preedit_and_on_caret_move() {
    // `ddc` ("đc") is reverted by bamboo's auto_correct and the wrapper
    // re-applies dd→đ. The preedit must show `đc` — a click commits the
    // preedit — and a caret move (`Engine::reset`) must commit `đc`, not
    // the raw `ddc`: both live validity queries used to ignore
    // `abbrev_fixed` and auto-restored the abbreviation to its raw keys.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    let actions = type_str(&mut e, "ddc");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "đc")),
        "{actions:?}"
    );
    let actions = e.reset();
    assert!(
        actions.contains(&Action::Commit("đc".into())),
        "{actions:?}"
    );

    // The commit-as-you-type mode expands abbreviations at the word end
    // like preedit does.
    assert_eq!(
        type_replay(&mut engine(Mode::Surrounding, Config::default()), "ddc "),
        "đc "
    );
}

#[test]
fn config_reload_expands_a_pending_macro() {
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    type_str(&mut e, "gs");
    let actions = e.apply_config(&Config {
        mode: ModeConfig::Preedit,
        ..Config::default()
    });
    assert!(
        actions.contains(&Action::Commit("giờ sáng".into())),
        "{actions:?}"
    );
}

#[test]
fn macro_key_shows_its_expansion_in_the_preedit() {
    // The preedit carries IBUS_ENGINE_PREEDIT_COMMIT, so the client commits
    // whatever it displays when it resets the context (a click, a focus
    // change). Showing the raw keys would commit them raw; the expansion
    // must be visible instead (ibus-bamboo shows it for the same reason).
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.macros
        .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
    let actions = type_str(&mut e, "gs");
    let shown: Vec<&str> = actions
        .iter()
        .filter_map(|a| match a {
            Action::UpdatePreedit { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        shown.contains(&"giờ sáng"),
        "the expansion must be shown in the preedit: {shown:?}"
    );
    // The terminator still commits the expansion, not the raw keys.
    let actions = type_str(&mut e, " ");
    assert!(
        actions.contains(&Action::Commit("giờ sáng".into())),
        "{actions:?}"
    );
}

#[test]
fn preedit_shows_the_restored_raw_keys_for_a_non_vietnamese_word() {
    // The client commits what the preedit shows, so a word that will be
    // restored to its raw keys must already show them (a click would
    // otherwise commit the composed form).
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        auto_correct: false,
        ..Config::default()
    });
    let actions = type_str(&mut e, "windowf");
    let last = actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::UpdatePreedit { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .expect("a preedit update");
    assert_eq!(last, "windowf", "{actions:?}");
}

#[test]
fn app_rule_wins_over_the_global_mode() {
    // A rule pins the client to preedit; the global mode must not run even
    // though the client could not render preedit without the rule. Off is
    // the global mode (the config must carry it — apply_config resets the
    // mode to its own otherwise): an inert engine would leave every key raw.
    let mut e = Engine::new(Method::Telex, Mode::Off);
    e.set_client(Some("gtk4-im:ruleclient".into()));
    e.set_capabilities(0);
    e.apply_config(&Config {
        mode: ModeConfig::Off,
        app_rules: vec![crate::config::AppRule {
            pattern: "ruleclient".into(),
            mode: ModeConfig::Preedit,
        }],
        ..Config::default()
    });
    // The rule requests Preedit; without preedit caps the hard fallback
    // still applies (the rule does not bypass physics) and the word rides
    // the walk's terminal transport — the global mode, on the other hand,
    // is never consulted: raw keys, no commit.
    let actions: Vec<Action> = "vieetj ".chars().flat_map(|c| e.process_char(c)).collect();
    assert_eq!(
        replay(&actions),
        "việt",
        "a pinned rule must bypass the global mode: {actions:?}"
    );
}

/// The rule resolution is cached per client; a config reload with different
/// rules must invalidate it or the old mode sticks until the app refocuses.
#[test]
fn app_rule_cache_follows_a_config_reload() {
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.set_client(Some("gtk4-im:chrome".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    // The built-in safety net pins chrome to Preedit.
    assert_eq!(e.effective_mode(), Mode::Preedit);
    // Rewriting the rules must reach the next resolution, not the cache.
    e.apply_config(&Config {
        app_rules: vec![crate::config::AppRule {
            pattern: "chrome".into(),
            mode: ModeConfig::Off,
        }],
        ..Config::default()
    });
    assert_eq!(e.effective_mode(), Mode::Off);
    // Same client, no rule change: the cache serves the repeated call.
    assert_eq!(e.effective_mode(), Mode::Off);
}

// --- VNI digit marks on complete syllables (the kept-composition guard) ---

#[test]
fn vni_digit_after_a_complete_syllable_keeps_the_word() {
    // "hạo" + 6 used to collapse the word into its raw keys ("hao123456"):
    // bamboo's auto_correct revert resurrected the digits consumed as tone
    // marks. Every core display mode keeps the composed word and lands the
    // digit literally.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let config = Config {
            method: crate::config::MethodConfig::Vni,
            ..Config::default()
        };
        assert_eq!(
            type_replay(&mut engine(mode, config), "hao123456 "),
            "hạo6 ",
            "{mode:?}"
        );
    }
}

#[test]
fn vni_english_digit_runs_stay_raw() {
    // English typing has no consumed marks: the raw keys survive verbatim
    // in every core display mode.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let config = Config {
            method: crate::config::MethodConfig::Vni,
            ..Config::default()
        };
        assert_eq!(
            type_replay(&mut engine(mode, config), "abc123 "),
            "abc123 ",
            "{mode:?}"
        );
    }
}

#[test]
fn vni_backspace_over_a_kept_word_recomposes() {
    // Backspace pops the raw key and recomposes; the kept composition
    // re-derives. Keys after the kept point land literally (bamboo's raw
    // bypass), so tone cycling resumes only once the literal keys are
    // backspaced away — the upstream revert fix lifts that limit.
    let config = Config {
        method: crate::config::MethodConfig::Vni,
        ..Config::default()
    };
    let mut e = engine(Mode::Surrounding, config);
    assert_eq!(type_replay(&mut e, "hao123456x"), "hạo6x");
    let actions = e.backspace().expect("backspace");
    e.set_surrounding("hạo6".into(), 3);
    assert_eq!(replay_from("hạo6x", &actions), "hạo6", "{actions:?}");
    let actions = e.backspace().expect("backspace");
    e.set_surrounding("hạo".into(), 2);
    assert_eq!(replay_from("hạo6", &actions), "hạo", "{actions:?}");
    let actions = e.process_char('1');
    assert_eq!(replay_from("hạo", &actions), "háo", "{actions:?}");
}

#[test]
fn telex_english_word_with_marks_still_returns_raw() {
    // The guard covers only the VNI-family digit keys: Telex tone keys on
    // an incomplete syllable keep the raw revert that English typing leans
    // on ("strê" + t -> "street", never "strêt").
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    assert_eq!(type_replay(&mut e, "street "), "street ");
}

// --- emoji shortcodes ---

/// Emoji tests must not depend on the developer's own `emoji.toml`: clear
/// the user table the engine loaded at startup.
fn with_emoji(mode: Mode, config: Config) -> Engine {
    let mut e = engine(mode, config);
    e.emojis.set_entries(std::iter::empty());
    e
}

#[test]
fn emoji_shortcode_rewrites_in_every_core_mode() {
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = with_emoji(
            mode,
            Config {
                emoji_enabled: true,
                ..Config::default()
            },
        );
        assert_eq!(
            type_replay(&mut e, ":smile:"),
            "😄",
            "{mode:?}: the closing ':' rewrites :name:"
        );
    }
}

#[test]
fn emoji_rewrite_in_flow_injects_backspaces_without_the_closing_colon() {
    let mut e = with_emoji(
        Mode::Flow,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    type_str(&mut e, ":smile");
    let actions = e.process_char(':');
    assert!(
        actions.contains(&Action::InjectBackspaces(6)),
        "the flow rewrite erases ':smile': {actions:?}"
    );
    assert!(
        actions.contains(&Action::Commit("😄".into())),
        "{actions:?}"
    );
    assert!(
        !actions.contains(&Action::Forward),
        "the closing ':' belongs to the shortcode, it must not reach the app: {actions:?}"
    );
}

#[test]
fn emoji_unknown_name_stays_literal() {
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, ":nope:"), ":nope:");
}

#[test]
fn emoji_word_ends_at_a_space_without_a_rewrite() {
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, ":smile x"), ":smile x");
}

#[test]
fn digits_after_a_colon_stay_raw() {
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "3:30"), "3:30");
}

#[test]
fn macro_wins_over_the_emoji_table() {
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    e.macros
        .set_entries([(":smile:".to_string(), "macro wins".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, ":smile:"), "macro wins");
}

#[test]
fn user_emoji_table_overrides_the_embedded_one() {
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    e.emojis
        .set_entries([("smile".to_string(), "🙂".to_string())].into_iter());
    assert_eq!(type_replay(&mut e, ":smile:"), "🙂");
    // Embedded names the user did not list still resolve (one replay, so
    // the app buffer the helper tracks holds both results).
    assert_eq!(type_replay(&mut e, ":smile: :fire:"), "🙂 🔥");
}

#[test]
fn emoji_disabled_types_after_a_colon_normally() {
    // Config::default() has emoji_enabled = false and `:` no longer blocks
    // marks: a word after ':' composes; shortcodes only rewrite with the
    // feature on.
    let mut e = with_emoji(Mode::Surrounding, Config::default());
    assert_eq!(type_replay(&mut e, ":tieeng:"), ":tiêng:");
}

#[test]
fn emoji_candidate_composes_vietnamese_and_releases_on_backspace() {
    // emoji on (the owner's config): a word after ':' composes like any
    // word, and erasing it with Backspace releases the candidate — the old
    // raw-forward candidate kept `x:abc` + 3 Backspace + `vieetj` raw.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = with_emoji(
            mode,
            Config {
                emoji_enabled: true,
                ..Config::default()
            },
        );
        assert_eq!(
            type_replay(&mut e, "mucj tieeu:phats trieenr "),
            "mục tiêu:phát triển ",
            "{mode:?}"
        );
        let (out, oor) = type_replay_backspaces(&mut e, "x:abc\u{8}\u{8}\u{8}vieetj ");
        assert!(!oor, "{mode:?}");
        assert_eq!(out, "x:việt ", "{mode:?}");
    }
}

#[test]
fn emoji_miss_keeps_the_shortcode_as_typed() {
    // A name that matches nothing stays literal — the raw keys, not the
    // composed word (`tungf` composes to `tùng` while typing).
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, ":tungf:"), ":tungf:");
}

#[test]
fn emoji_candidate_composes_live_in_per_key_modes() {
    // The candidate word joins the per-key display — the marks appear
    // while typing (`:tungf` shows `:tùng` before any space) and the word
    // end only appends the terminator.
    let mut e = with_emoji(
        Mode::Flow,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    type_str(&mut e, "x:tungf");
    assert_eq!(e.tail, "tùng", "the word must show live");
    let actions = type_str(&mut e, " ");
    assert_eq!(e.tail, "tùng ");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::InjectBackspaces(_))),
        "no erase is needed at the word end: {actions:?}"
    );
    let flushed = flush_per_key(&mut e);
    assert!(
        flushed.iter().any(|a| matches!(
            a,
            Action::Commit(t) | Action::CommitNow(t) if t == "tùng "
        )),
        "{flushed:?}"
    );
}

#[test]
fn emoji_close_rewrites_with_the_colon_in_per_key_modes() {
    // The committed ':' sits one char before the candidate's region — the
    // close lands the in-flight batch and erases both, or the emoji would
    // keep the leading colon (`:😀`).
    let mut e = with_emoji(
        Mode::Flow,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    e.emojis
        .set_entries([("cuoi".to_string(), "😀".to_string())].into_iter());
    let actions = type_str(&mut e, ":cuoi:");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Commit(t) if t == "😀")),
        "{actions:?}"
    );
    assert_eq!(e.tail, "😀");
}

#[test]
fn emoji_miss_restores_the_raw_keys_in_per_key_modes() {
    // A name that matches nothing types back out of the composed display
    // through the batch (`:tungf:` stays literal).
    let mut e = with_emoji(
        Mode::Flow,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    type_str(&mut e, ":tungf:");
    assert_eq!(e.tail, "tungf:");
    let flushed = flush_per_key(&mut e);
    assert!(
        flushed.iter().any(|a| matches!(
            a,
            Action::Commit(t) | Action::CommitNow(t) if t == "tungf:"
        )),
        "{flushed:?}"
    );
}

#[test]
fn backspaces_inside_the_shortcode_keep_the_rewrite_in_range() {
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    // ':smil' minus two keys, then 'ile:' — the tracked tail must shrink
    // with the backspaces or the rewrite deletes past the typed text.
    let (text, out_of_range) = type_replay_backspaces(&mut e, ":smil\u{8}\u{8}ile:");
    assert_eq!(text, "😄");
    assert!(!out_of_range, "the rewrite may not ask for more than shown");
}

#[test]
fn emoji_pick_after_a_colon_disarms_the_shortcode_branch() {
    // The pickers commit through note_committed; the engine's last-char
    // bookkeeping must follow, or a pick right after a literal ':' leaves
    // the shortcode branch armed and the next word types raw.
    let mut e = with_emoji(
        Mode::Surrounding,
        Config {
            emoji_enabled: true,
            ..Config::default()
        },
    );
    type_str(&mut e, ":");
    e.note_committed("😄");
    let actions = e.process_char('l');
    assert!(
        actions.contains(&Action::Commit("l".into())),
        "the next word must compose normally: {actions:?}"
    );
}

// --- backspace tone-skip (the "Xóa dấu bằng Backspace" dropdown) ---

fn tone_engine(method: MethodConfig, mode: Mode, tone: BackspaceToneConfig) -> Engine {
    let mut e = Engine::new(method.into(), mode);
    e.apply_config(&Config {
        mode: mode.into(),
        method,
        backspace_tone: tone,
        ..Config::default()
    });
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e
}

#[test]
fn backspace_tone_first_strips_the_tone_in_one_step() {
    // `hạ` = raw `hasfxjr` (the tone cycled through every key): one
    // Backspace lands on the tone-free word, the letters stay — the unikey
    // walk would pass through every intermediate tone instead.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::ToneFirst,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hasfxjr\u{8}").0, "ha");
    // The word stays editable: one more Backspace takes the letter.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::ToneFirst,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hasfxjr\u{8}\u{8}").0, "h");
}

#[test]
fn backspace_whole_char_deletes_the_marked_letter() {
    // `hạ` -> `h`: the marked character goes with its tone, one step.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hasfxjr\u{8}").0, "h");
}

#[test]
fn backspace_unikey_walk_undoes_the_last_raw_key() {
    // The UnikeyWalk option: the raw keys pop one by one and the cycled
    // tone walks back through every tone it passed.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::UnikeyWalk,
    );
    // bamboo telex: x=ngã, j=nặng, r=hỏi — popping r lands on j's tone.
    assert_eq!(type_replay_backspaces(&mut e, "hasfxjr\u{8}").0, "hạ");
}

#[test]
fn backspace_tone_skip_leaves_tone_free_words_alone() {
    // `hôm` carries no tone (the horn is a vowel shape, not a tone): the
    // unikey undo pops `m` even with the skip on.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::ToneFirst,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hoom\u{8}").0, "hô");
}

#[test]
fn backspace_tone_first_keeps_the_final_consonant() {
    // `hần` = raw `ha6n2` (VNI): the tone key goes, the circumflex and the
    // final n stay.
    let mut e = tone_engine(
        MethodConfig::Vni,
        Mode::Surrounding,
        BackspaceToneConfig::ToneFirst,
    );
    assert_eq!(type_replay_backspaces(&mut e, "ha6n2\u{8}").0, "hân");
}

#[test]
fn backspace_whole_char_rebuilds_uncomposable_text() {
    // `hần` minus its last character (`hầ`) is composable by no raw
    // prefix — the tone would have to survive its own key. The shortened
    // text is rebuilt from the screen instead: the deletion still takes
    // exactly one character.
    let mut e = tone_engine(
        MethodConfig::Vni,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(type_replay_backspaces(&mut e, "ha6n2\u{8}").0, "hầ");
}

#[test]
fn backspace_whole_char_deletes_the_screen_character() {
    // The owner's case: `hùng` -> `hùn` -> `hù` -> `h` — each Backspace
    // deletes one screen character; the tone survives until its own
    // character is deleted, then goes with it.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hungf\u{8}").0, "hùn");
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(
        type_replay_backspaces(&mut e, "hungf\u{8}\u{8}\u{8}").0,
        "h"
    );
}

#[test]
fn backspace_whole_char_works_on_long_words() {
    // `việt` -> `việ` -> `vi` -> `v`: one screen character per Backspace,
    // no tone-cycle walk on long words.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(type_replay_backspaces(&mut e, "vieejt\u{8}").0, "việ");
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(
        type_replay_backspaces(&mut e, "vieejt\u{8}\u{8}\u{8}").0,
        "v"
    );
}

#[test]
fn backspace_whole_char_on_kept_compositions() {
    // `hạo6`: popping the literal 6 lands exactly on the kept word.
    let mut e = tone_engine(
        MethodConfig::Vni,
        Mode::Surrounding,
        BackspaceToneConfig::WholeChar,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hao123456\u{8}").0, "hạo");
}

#[test]
fn backspace_whole_char_updates_the_preedit() {
    // The rebuilt text replaces the preedit in one step.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Preedit,
        BackspaceToneConfig::WholeChar,
    );
    for c in "hungf".chars() {
        let _ = e.process_char(c);
    }
    let actions = e.backspace().expect("backspace");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "hùn")),
        "{actions:?}"
    );
}

#[test]
fn backspace_tone_skip_leaves_kept_compositions_alone() {
    // `hạo6`'s digits are literal text, not tone keys: the unikey undo
    // pops the 6, the skip must not eat them.
    let mut e = tone_engine(
        MethodConfig::Vni,
        Mode::Surrounding,
        BackspaceToneConfig::ToneFirst,
    );
    assert_eq!(type_replay_backspaces(&mut e, "hao123456\u{8}").0, "hạo");
}

#[test]
fn per_key_backspace_ignores_the_tone_skip() {
    // The per-key reverse-map pass-through runs regardless of the
    // dropdown: the real Backspace deletes the screen's last character.
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Flow,
        BackspaceToneConfig::WholeChar,
    );
    e.set_uinput_available(true);
    e.apply_config(&Config {
        mode: Mode::Flow.into(),
        experimental_features: true,
        backspace_tone: BackspaceToneConfig::WholeChar,
        ..Config::default()
    });
    type_str(&mut e, "hasfxjr");
    flush_per_key(&mut e);
    let actions = e.backspace().expect("backspace");
    // The batch was flushed above: the pass-through is all that is left.
    assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    assert_eq!(e.tail, "h");
}

#[test]
fn backspace_tone_first_updates_the_preedit() {
    let mut e = tone_engine(
        MethodConfig::Telex,
        Mode::Preedit,
        BackspaceToneConfig::ToneFirst,
    );
    for c in "hasfxjr".chars() {
        let _ = e.process_char(c);
    }
    let actions = e.backspace().expect("backspace");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "ha")),
        "{actions:?}"
    );
}

// --- the đ undo (dd -> đ, another d undoes it) ---

#[test]
fn dd_undo_shows_the_composition_in_preedit() {
    // `dd` forms `đ`; the next `d` undoes the rule (bamboo's own
    // semantics): the preedit shows the composition `dd`, not the raw
    // `ddd` the auto-restore would put there.
    let mut e = engine(Mode::Preedit, Config::default());
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    let mut last = String::new();
    for c in "ddd".chars() {
        for a in e.process_char(c) {
            if let Action::UpdatePreedit { text, .. } = a {
                last = text;
            }
        }
    }
    assert_eq!(last, "dd");
}

#[test]
fn dd_undo_keeps_the_composition_at_word_end() {
    // The word end must commit the composition `dd` in every committed-text
    // display mode — the auto-restore must not resurrect `ddd` over an
    // undone đ.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, Config::default());
        assert_eq!(type_replay(&mut e, "ddd "), "dd ", "{mode:?}");
    }
}

#[test]
fn dd_undo_rewrites_flowat_word_end() {
    // The per-key display shows the composition live; the word end parks
    // it with the terminator — the undone đ must not keep `ddd` on screen.
    let mut e = engine(Mode::Flow, Config::default());
    e.set_uinput_available(true);
    type_str(&mut e, "ddd");
    let _ = e.process_char(' ');
    // The word-end terminator travels inside the parked batch.
    let flushed = flush_per_key(&mut e);
    assert!(
        flushed
            .iter()
            .any(|a| matches!(a, Action::CommitNow(t) if t == "dd ")),
        "{flushed:?}"
    );
    assert_eq!(e.tail, "dd ", "the composition replaced the raw keys");
}

// --- bamboo-core 0.6.0 #41: the 32-slot overflow commits the word ---

#[test]
fn overflow_boundary_commits_the_word_in_preedit() {
    // The preedit owns the word; at the 32-slot boundary the engine commits
    // it underneath. The client must receive it as committed text before
    // the fresh preedit replaces it, or the word vanishes from the screen.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.set_client(Some("gnome-shell".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    let mut buf = String::new();
    let mut preedit = String::new();
    for c in "b".repeat(35).chars() {
        for a in e.process_char(c) {
            match a {
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::UpdatePreedit { text, .. } => preedit = text,
                Action::HidePreedit => preedit.clear(),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
    }
    // Mid-word, one boundary in: everything typed is still on screen.
    assert_eq!(buf, "b".repeat(32));
    assert_eq!(preedit, "b".repeat(3));
    for c in " vieejt ".chars() {
        for a in e.process_char(c) {
            match a {
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::UpdatePreedit { text, .. } => preedit = text,
                Action::HidePreedit => preedit.clear(),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
    }
    assert_eq!(buf, format!("{} việt ", "b".repeat(35)));
    assert!(preedit.is_empty(), "the terminators land everything");
}

#[test]
fn overflow_boundary_keeps_every_char_in_surrounding() {
    // The commit-as-you-type display replays deltas literally: a boundary
    // delta that deletes the word erased the committed characters (the
    // "xóa trắng"). Every character must survive the boundary, and the
    // word after it must still compose.
    let mut e = engine(Mode::Surrounding, Config::default());
    // Mid-word, one boundary in: every committed character must still be
    // there — the destructive delta used to white the word out right here
    // (the word-end auto-restore only happened to put the raw keys back).
    let mid = type_replay(&mut e, &"b".repeat(35));
    assert_eq!(mid, "b".repeat(35));
    let rest = type_replay(&mut e, " vieejt ");
    assert_eq!(format!("{mid}{rest}"), format!("{} việt ", "b".repeat(35)));
}

#[test]
fn overflow_boundary_lands_the_word_in_flow() {
    // The per-key display: the boundary word must land as committed text
    // (the fresh word appends after it), and the word-end rewrite may only
    // cover the fresh word — never the committed one.
    let mut e = per_key_engine(Mode::Flow);
    let mut buf = String::new();
    let mut keys = String::new();
    for c in "b".repeat(35).chars() {
        keys.push(c);
        for a in e.process_char(c) {
            match a {
                Action::InjectBackspaces(n) => {
                    let keep = buf.chars().count().saturating_sub(n as usize);
                    buf = buf.chars().take(keep).collect();
                }
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
        for a in flush_per_key(&mut e) {
            if let Action::Commit(t) | Action::CommitNow(t) = a {
                buf.push_str(&t);
            }
        }
    }
    // Mid-word, one boundary in: everything typed is still on screen.
    assert_eq!(buf, "b".repeat(35));
    for c in " chaof ".chars() {
        keys.push(c);
        for a in e.process_char(c) {
            match a {
                Action::InjectBackspaces(n) => {
                    let keep = buf.chars().count().saturating_sub(n as usize);
                    buf = buf.chars().take(keep).collect();
                }
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
        for a in flush_per_key(&mut e) {
            if let Action::Commit(t) | Action::CommitNow(t) = a {
                buf.push_str(&t);
            }
        }
    }
    assert_eq!(buf, format!("{} chào ", "b".repeat(35)));
}

#[test]
fn backspace_after_the_boundary_never_touches_the_committed_word() {
    // After the boundary the wrapper's raw holds only the fresh word's
    // keys: Backspace recomposes that word alone — the committed word
    // keeps every character, and no delete reaches past the buffer.
    let mut e = engine(Mode::Surrounding, Config::default());
    let (text, oor) = type_replay_backspaces(&mut e, &format!("{}\u{8}\u{8}", "b".repeat(40)));
    assert!(!oor, "a delete reached past the buffer");
    assert_eq!(text, "b".repeat(38));
}

#[test]
fn extended_onsets_switch_reaches_the_engine() {
    // bamboo-core 0.6.0: f/j/w can act as onset consonants and take marks
    // (fos -> fó). Off by default: the keys stay as typed — no Vietnamese
    // syllable starts with them, so English words keep raw keys.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(typed(mode, "fos "), "fos ", "{mode:?}");
        assert_eq!(typed(mode, "jus "), "jus ", "{mode:?}");
        assert_eq!(typed(mode, "wos "), "wos ", "{mode:?}");
    }
    let mut e = per_key_engine(Mode::Flow);
    let mut buf = String::new();
    for c in "fos ".chars() {
        for a in e.process_char(c) {
            match a {
                Action::InjectBackspaces(n) => {
                    let keep = buf.chars().count().saturating_sub(n as usize);
                    buf = buf.chars().take(keep).collect();
                }
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
        for a in flush_per_key(&mut e) {
            if let Action::Commit(t) | Action::CommitNow(t) = a {
                buf.push_str(&t);
            }
        }
    }
    assert_eq!(buf, "fos ");
    // On: the tone key marks the extended onset.
    let mut e = engine(
        Mode::Preedit,
        Config {
            extended_onsets: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "fos "), "fó ");
}

#[test]
fn w2u_everywhere_composes_a_word_start_w() {
    // The auto-restore used to force raw keys over every f/j/w word: a
    // word-start `w` under w2u=Everywhere composed `ưo` and then got
    // restored right back to `wo`. The restore only handles invalid words
    // now, so the w2u composition survives.
    let mut e = Engine::new(Method::Telex, Mode::Surrounding);
    e.apply_config(&Config {
        w2u: crate::config::W2uConfig::Everywhere,
        ..Config::default()
    });
    assert_eq!(type_replay(&mut e, "wo "), "ưo ");
    assert_eq!(type_replay(&mut e, "wos "), "ứo ");
}

// --- experimental keep_composed_words ---

#[test]
fn keep_composed_words_off_keeps_today_s_revert() {
    // Default off: the refused letter still reverts the word to its raw
    // keys, and the English revert is untouched — the flag changes nothing.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        assert_eq!(typed(mode, "tungfg "), "tungfg ", "{mode:?}");
        assert_eq!(typed(mode, "isq "), "isq ", "{mode:?}");
        assert_eq!(typed(mode, "soft "), "soft ", "{mode:?}");
        assert_eq!(typed(mode, "sort "), "sort ", "{mode:?}");
    }
}

#[test]
fn keep_composed_words_keeps_a_refused_letter() {
    // On: a plain letter a complete valid syllable cannot extend appends
    // literally, marks intact (`tùng` + `g` -> `tùngg`). The undo behaviors
    // bamboo-core handles itself (base-letter and same-tone repeats) stay
    // correct, and the kept word survives the word-end auto-restore.
    let config = || Config {
        experimental_features: true,
        keep_composed_words: true,
        ..Config::default()
    };
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "tungfg "), "tùngg ", "{mode:?}");
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "haamk "), "hâmk ", "{mode:?}");
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "uocwsq "), "ướcq ", "{mode:?}");
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "isq "), "íq ", "{mode:?}");
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "huongwa "), "hươnga ", "{mode:?}");
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "haama "), "hama ", "{mode:?}");
        let mut e = engine(mode, config());
        assert_eq!(type_replay(&mut e, "tungff "), "tungf ", "{mode:?}");
    }
}

#[test]
fn keep_composed_words_survives_backspace_and_the_word_end() {
    // Backspace after a keep pops the stray key and the mark returns
    // (`tùngg` + BS -> `tùng`), and the kept word is exempt from the
    // word-end auto-restore (`tungfg` must not come back at the space).
    let (text, oor) = type_replay_backspaces(
        &mut engine(
            Mode::Surrounding,
            Config {
                experimental_features: true,
                keep_composed_words: true,
                ..Config::default()
            },
        ),
        "tungfg\u{8}",
    );
    assert!(!oor, "a delete reached past the buffer");
    assert_eq!(text, "tùng");
}

#[test]
fn keep_composed_words_is_baked_off_without_the_gate() {
    // The experimental master gate forces the keep off: with the gate
    // closed the row's value is inert, exactly like the other features.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            experimental_features: false,
            keep_composed_words: true,
            ..Config::default()
        },
    );
    assert_eq!(type_replay(&mut e, "tungfg "), "tungfg ");
}

#[test]
fn keep_composed_words_keeps_in_flow() {
    let mut e = engine(
        Mode::Flow,
        Config {
            experimental_features: true,
            keep_composed_words: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    let mut buf = String::new();
    for c in "tungfg ".chars() {
        for a in e.process_char(c) {
            match a {
                Action::InjectBackspaces(n) => {
                    let keep = buf.chars().count().saturating_sub(n as usize);
                    buf = buf.chars().take(keep).collect();
                }
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
        for a in flush_per_key(&mut e) {
            if let Action::Commit(t) | Action::CommitNow(t) = a {
                buf.push_str(&t);
            }
        }
    }
    assert_eq!(buf, "tùngg ");
}

// --- experimental keep_repeats_raw ---

#[test]
fn keep_repeats_raw_off_a_hold_still_composes() {
    let mut e = engine(
        Mode::Preedit,
        Config {
            experimental_features: true,
            ..Config::default()
        },
    );
    let (buf, preedit) = type_timed(&mut e, &[(0, 'o'), (500, 'o')]);
    assert_eq!(preedit, "ô");
    assert_eq!(buf, "");
}

#[test]
fn keep_repeats_raw_streams_a_hold_raw() {
    // Preedit and Surrounding: the hold appends raw letters, no rewrite
    // fires, the space commits exactly the raw trail, and the next word's
    // fast pair composes — the latch died with the word.
    for mode in [Mode::Preedit, Mode::Surrounding] {
        let mut e = engine(
            mode,
            Config {
                experimental_features: true,
                keep_repeats_raw: true,
                ..Config::default()
            },
        );
        let (buf, preedit) = type_timed(&mut e, &[(0, 'o'), (500, 'o'), (40, 'o')]);
        if mode == Mode::Preedit {
            assert_eq!(preedit, "ooo", "{mode:?}");
            assert_eq!(buf, "", "{mode:?}");
        } else {
            assert_eq!(buf, "ooo", "{mode:?}");
        }
        let (tail, tail_preedit) = type_timed(&mut e, &[(0, ' '), (0, 'o'), (0, 'o')]);
        if mode == Mode::Preedit {
            // The space commits the trail word; the next word's fast pair
            // composes normally — the latch died with the word.
            assert_eq!(format!("{buf}{tail}"), "ooo ", "{mode:?}");
            assert_eq!(tail_preedit, "ô", "{mode:?}");
        } else {
            // The word already landed per key: the space only lands itself.
            assert_eq!(format!("{buf}{tail}"), "ooo ô", "{mode:?}");
        }
    }
}

#[test]
fn keep_repeats_raw_streams_a_hold_in_flow() {
    // The owner's real scenario: per-key flow commits and rewrites per key;
    // without the guard the first repeat fires the injected rewrite that
    // kills the compositor's repeat. With it the hold appends raw and the
    // injected count stays zero.
    let mut e = engine(
        Mode::Flow,
        Config {
            experimental_features: true,
            keep_repeats_raw: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    let mut buf = String::new();
    let mut injected = 0;
    for (ms, c) in [(0u64, 'o'), (500, 'o'), (40, 'o')] {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        for a in e.process_char(c) {
            match a {
                Action::InjectBackspaces(n) => injected += n,
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
        for a in flush_per_key(&mut e) {
            if let Action::Commit(t) | Action::CommitNow(t) = a {
                buf.push_str(&t);
            }
        }
    }
    assert_eq!(buf, "ooo");
    assert_eq!(injected, 0);
}

#[test]
fn keep_repeats_raw_a_hold_survives_the_overflow_boundary() {
    // The owner's live report: past the 32-slot boundary bamboo commits the
    // trail word, and a detector reset made the next repeat compose the
    // pair (`oo` → `ô`) — firing the injected rewrite that killed the
    // compositor's repeat for the rest of the hold. The physical key is
    // still down: the watch must survive the boundary and the fresh word
    // streams raw too.
    let mut e = engine(
        Mode::Flow,
        Config {
            experimental_features: true,
            keep_repeats_raw: true,
            ..Config::default()
        },
    );
    e.set_uinput_available(true);
    let mut buf = String::new();
    let mut injected = 0;
    let mut script: Vec<(u64, char)> = vec![(0, 'o'), (500, 'o')];
    script.extend(std::iter::repeat_n((40u64, 'o'), 34));
    for (ms, c) in script {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        for a in e.process_char(c) {
            match a {
                Action::InjectBackspaces(n) => injected += n,
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
        for a in flush_per_key(&mut e) {
            if let Action::Commit(t) | Action::CommitNow(t) = a {
                buf.push_str(&t);
            }
        }
    }
    assert_eq!(buf.chars().filter(|c| *c == 'o').count(), 36);
    assert_eq!(injected, 0, "the hold must never fire a rewrite");
}

#[test]
fn keep_repeats_raw_fast_double_still_composes() {
    // 150 ms gaps: a deliberate double press under the threshold, with a
    // stall margin — a test-thread pause must eat ~250 ms more before the
    // gap crosses the detector's 400 ms.
    let mut e = engine(
        Mode::Preedit,
        Config {
            experimental_features: true,
            keep_repeats_raw: true,
            ..Config::default()
        },
    );
    let (buf, _) = type_timed(
        &mut e,
        &[(0, 't'), (0, 'o'), (150, 'o'), (150, 'i'), (0, ' ')],
    );
    assert_eq!(buf, "tôi ");
}

#[test]
fn keep_repeats_raw_slow_double_stays_raw() {
    // The documented trade: a deliberate pair slower than the threshold
    // comes out raw ("too" instead of "tô").
    let mut e = engine(
        Mode::Preedit,
        Config {
            experimental_features: true,
            keep_repeats_raw: true,
            ..Config::default()
        },
    );
    let (buf, _) = type_timed(&mut e, &[(0, 't'), (0, 'o'), (500, 'o'), (0, ' ')]);
    assert_eq!(buf, "too ");
}

#[test]
fn keep_repeats_raw_is_baked_off_without_the_gate() {
    let mut e = engine(
        Mode::Preedit,
        Config {
            keep_repeats_raw: true,
            ..Config::default()
        },
    );
    let (_, preedit) = type_timed(&mut e, &[(0, 'o'), (500, 'o')]);
    assert_eq!(preedit, "ô");
}

#[test]
fn keep_repeats_raw_backspace_after_a_hold_pops_one_raw_char() {
    // Recomposition would collapse the trailing pair ("oo" → "ô") and eat
    // a second visible char; the raw trail pops exactly one.
    let mut e = engine(
        Mode::Surrounding,
        Config {
            experimental_features: true,
            keep_repeats_raw: true,
            ..Config::default()
        },
    );
    let (buf, _) = type_timed(&mut e, &[(0, 'o'), (500, 'o'), (40, 'o'), (0, '\u{8}')]);
    assert_eq!(buf, "oo");
}

#[test]
fn keep_repeats_raw_a_mark_key_after_a_hold_keeps_screen_and_word_in_sync() {
    // Viqr punctuation never re-derives the alphabetic-only detector, and
    // bamboo self-commits on it: whatever the transport, the committed
    // word must equal what the screen showed — the raw trail, not some
    // stale internal state.
    let mut e = Engine::new(Method::Viqr, Mode::Preedit);
    e.apply_config(&Config {
        experimental_features: true,
        keep_repeats_raw: true,
        ..Config::default()
    });
    e.set_client(Some("gtk4-im:test".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT);
    let mut buf = String::new();
    for (ms, c) in [(0u64, 'o'), (500, 'o'), (40, 'o'), (0, '\''), (0, ' ')] {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        for a in e.process_char(c) {
            match a {
                Action::Commit(t) | Action::CommitNow(t) => buf.push_str(&t),
                Action::Forward => buf.push(c),
                _ => {}
            }
        }
    }
    // The screen showed "ooo" and "ooo'" — the committed trail word and
    // the literal quote land exactly that.
    assert_eq!(buf, "ooo' ");
}

// --- suggestion strip ---

/// Engine with the strip switch applied; unlike `engine` above this picks
/// the input method too — the digit-tone presets must prove the strip's
/// silence on their digits. The switch rides the experimental master gate
/// (the engine bakes `experimental && suggest_words`), so a strip-on test
/// turns the gate on with it.
fn suggest_engine(method: Method, mode: Mode, suggest_words: bool) -> Engine {
    let mut e = Engine::new(method, mode);
    e.apply_config(&Config {
        mode: mode.into(),
        suggest_words,
        experimental_features: true,
        ..Config::default()
    });
    // apply_config resets the method to the config default, exactly like it
    // resets the mode (see `engine` above); the caller wins again.
    e.set_method(method);
    e
}

/// Feed keys one at a time, keeping each key's action batch apart: the
/// strip assertions are per-key (quiet on the first char, open on the
/// second, hidden on the terminator). Clients here need only the preedit
/// capability — inline modes get their own surrounding-text feeder below.
fn suggest_keys(e: &mut Engine, keys: &str) -> Vec<Vec<Action>> {
    if e.client().is_none() {
        e.set_client(Some("gtk4-im:test".into()));
    }
    e.set_capabilities(CAP_PREEDIT_TEXT);
    keys.chars().map(|c| e.process_char(c)).collect()
}

/// The strip actions in one key's batch, for the zero-leak assertions.
fn strip_actions(actions: &[Action]) -> Vec<&Action> {
    actions
        .iter()
        .filter(|a| matches!(a, Action::UpdateCandidates { .. } | Action::HideCandidates))
        .collect()
}

/// The candidate list a batch carries, if any.
fn candidates_of(actions: &[Action]) -> Option<&Vec<String>> {
    actions.iter().find_map(|a| match a {
        Action::UpdateCandidates { items, .. } => Some(items),
        _ => None,
    })
}

/// The committed sample model for the predict tests. Injected straight into
/// the engine's `model` field: an env override would be process-global and
/// leak into the concurrent legacy strip tests. The Arc matches the field's
/// shared-parse type; the sample bypasses the process-wide cache, so every
/// call parses its own (2 KB) fixture.
pub(crate) fn sample_model() -> std::sync::Arc<crate::predict::PredictModel> {
    // Test-only expect: the committed fixture's absence is the failure.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/model-sample.bin");
    std::sync::Arc::new(
        crate::predict::open(&path)
            .expect("data/model-sample.bin missing — run scripts/build-predict-model.py --sample"),
    )
}

/// Engine with the strip switch on and the committed sample model injected:
/// the model's compounds, single words and next-word lists serve the strip
/// (the merged switch — there is no dictionary path left to layer under).
fn predict_engine(method: Method, mode: Mode) -> Engine {
    let mut e = suggest_engine(method, mode, true);
    e.model = Some(sample_model());
    e
}

/// The candidate a pick at `index` commits, off the list the strip emitted.
/// The list is model-built and deterministic for the fixture, but the pick
/// must still be checked against what the strip actually carried — never a
/// literal.
fn picked_candidate(actions: &[Action], index: usize) -> String {
    let list =
        candidates_of(actions).unwrap_or_else(|| panic!("the strip offered a list: {actions:?}"));
    list.get(index)
        .unwrap_or_else(|| panic!("the list holds the picked index: {list:?}"))
        .clone()
}

/// The expected sentence-start accept-commit: the candidate with its first
/// character uppercased. Spelled out here instead of borrowed from
/// `capitalize_first`, so the check stays independent of the code it tests.
fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[test]
fn suggest_strip_opens_on_the_first_char() {
    // The strip starts at the first letter: the model's rows for the
    // 1-char prefix serve right away (the old below-two-chars silence was
    // the dictionary's full-table scan guard — the model's bounded range
    // scan has no such degenerate case). No other candidate action rides
    // the same key.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "si");
    let first =
        candidates_of(&keys[0]).unwrap_or_else(|| panic!("the strip opens on `s`: {:?}", keys[0]));
    assert!(
        first.first().is_some_and(|w| w.starts_with("s")),
        "{first:?}"
    );
    assert_eq!(strip_actions(&keys[0]).len(), 1, "{:?}", keys[0]);
    let list =
        candidates_of(&keys[1]).unwrap_or_else(|| panic!("the strip rides `si`: {:?}", keys[1]));
    assert!(
        list.first().is_some_and(|w| w.starts_with("si")),
        "{list:?}"
    );
    assert_eq!(strip_actions(&keys[1]).len(), 1, "{:?}", keys[1]);
}

#[test]
fn state_a_ranks_context_followers_first() {
    // After a landed context the very first letter of the next word
    // refreshes the list with the context+prefix fusion: rows that follow
    // the context lead their band even when a stranger's unigram is
    // bigger — `hello ` then `w` keeps `world` (the pair's 6 counts) over
    // the more frequent `want`, `sinh ` then `h` keeps `học` on top. The
    // 1-char pick commits through the state-A path (the held letter is
    // the word the strip replaces).
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    suggest_keys(&mut e, "hello ");
    let keys = suggest_keys(&mut e, "w");
    let items = candidates_of(&keys[0]).expect("the 1-char word refreshes the list");
    assert_eq!(items.first().map(String::as_str), Some("world"));
    let actions = e.accept_candidate(0);
    assert!(
        actions.contains(&Action::Commit("world ".to_string())),
        "{actions:?}"
    );
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    suggest_keys(&mut e, "sinh ");
    let keys = suggest_keys(&mut e, "h");
    let items = candidates_of(&keys[0]).expect("the 1-char word refreshes the list");
    assert_eq!(items.first().map(String::as_str), Some("học"));
}

#[test]
fn suggest_strip_chains_when_the_word_lands() {
    // A word landing (comma terminator here) does not just close the list:
    // the strip chains straight into the landed word's next-word list —
    // the space landing is pinned by `state_b_predicts_next_syllable`.
    // Before the terminator the list stayed open (`sinh` leads with its
    // model compound — `sinh viên` — per `page_one_is_three_singles_then_the_compounds`).
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "sinh,");
    assert!(
        keys[..4].iter().any(|batch| candidates_of(batch).is_some()),
        "the strip is open before the terminator: {keys:?}"
    );
    let items = candidates_of(&keys[4]).expect("the strip chains after the comma");
    assert_eq!(items.first().map(String::as_str), Some("viên"));
}

#[test]
fn suggest_strip_hides_when_nothing_matches_the_word() {
    // `sinhh` extends `sinh` with a final nothing composes: no model row
    // carries the longer prefix, and with the dictionary out of the strip
    // no correction rescues it — the list empties and the strip closes
    // while the word is still in flight.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "sinhh");
    assert!(
        keys[..4].iter().any(|batch| candidates_of(batch).is_some()),
        "the strip opened mid-word: {keys:?}"
    );
    assert!(keys[4].contains(&Action::HideCandidates), "{:?}", keys[4]);
}

#[test]
fn suggest_accept_commits_the_candidate_and_closes() {
    // Selection rides the server (the picker's digit precedent); the
    // engine's accept path commits the candidate off the emitted list,
    // resets the composition like a word end, and records the context
    // word for the strip's next round. The committed text is the emitted
    // entry at the picked index, checked against that list — never a
    // literal (see `picked_candidate`).
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "si");
    let picked = picked_candidate(&keys[1], 0);
    let actions = e.accept_candidate(0);
    assert_eq!(
        actions,
        vec![
            Action::HidePreedit,
            Action::HideCandidates,
            Action::Commit(format!("{} ", picked.clone())),
        ]
    );
    assert!(!e.suggest_open);
    assert_eq!(e.last_committed_word.as_deref(), Some(picked.as_str()));
    assert!(!e.preedit_commit_pending());
}

#[test]
fn suggest_accept_ignores_a_closed_strip_and_an_out_of_range_index() {
    // The server maps page + label to a global index; a race between its
    // strip state and the engine's (the word moved on between the last
    // update and the key) must commit nothing rather than a stale word.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    assert!(e.accept_candidate(0).is_empty());
    suggest_keys(&mut e, "si");
    assert!(e.accept_candidate(25).is_empty());
    // State untouched: the strip is still up, the word still composing.
    assert!(e.suggest_open);
    assert!(e.preedit_commit_pending());
    assert!(e.last_committed_word.is_none());
}

#[test]
fn suggest_accept_capitalizes_a_sentence_start_candidate() {
    // The accepted candidate gets the same auto_capitalize treatment as the
    // word it replaces: the composition path uppercases the first key of a
    // sentence-start word (the capital rides the composition), so the
    // candidate committing in its place carries the capital too — first
    // character only. The commit is checked against the emitted list
    // entry, not a literal (see `picked_candidate`); the capitalization is
    // the invariant under test.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        mode: Mode::Preedit.into(),
        auto_capitalize: true,
        suggest_words: true,
        experimental_features: true,
        ..Config::default()
    });
    e.set_method(Method::Telex);
    e.model = Some(sample_model());
    // `.` arms the capitalize window; the strip answers with the model's
    // list for the word. A mid-list index keeps the index-to-entry mapping
    // under test the way the original position lookup did.
    let keys = suggest_keys(&mut e, ".si");
    let picked = picked_candidate(&keys[2], 1);
    let actions = e.accept_candidate(1);
    assert_eq!(
        actions,
        vec![
            Action::HidePreedit,
            Action::HideCandidates,
            Action::Commit(format!("{} ", capitalized(&picked))),
        ]
    );
    // The context word follows the commit as it landed: the capitalized form.
    assert_eq!(
        e.last_committed_word.as_deref(),
        Some(capitalized(&picked).as_str())
    );
    assert!(!e.suggest_open);
}

#[test]
fn suggest_accept_keeps_the_model_form_without_auto_capitalize() {
    // The switch off: even at a sentence start the candidate commits in
    // its model form verbatim — capitalization is opt-in. The commit is
    // checked against the emitted list entry, not a literal (see
    // `picked_candidate`).
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        mode: Mode::Preedit.into(),
        auto_capitalize: false,
        suggest_words: true,
        experimental_features: true,
        ..Config::default()
    });
    e.set_method(Method::Telex);
    e.model = Some(sample_model());
    let keys = suggest_keys(&mut e, ".si");
    let picked = picked_candidate(&keys[2], 0);
    let actions = e.accept_candidate(0);
    assert_eq!(
        actions,
        vec![
            Action::HidePreedit,
            Action::HideCandidates,
            Action::Commit(format!("{} ", picked.clone())),
        ]
    );
    assert_eq!(e.last_committed_word.as_deref(), Some(picked.as_str()));
}

#[test]
fn suggest_accept_stays_lowercase_mid_sentence() {
    // The capitalize condition is sentence start, not global: with the
    // switch on, a word typed after a plain space still commits the model
    // form verbatim. The commit is checked against the emitted list entry,
    // not a literal (see `picked_candidate`).
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        mode: Mode::Preedit.into(),
        auto_capitalize: true,
        suggest_words: true,
        experimental_features: true,
        ..Config::default()
    });
    e.set_method(Method::Telex);
    e.model = Some(sample_model());
    suggest_keys(&mut e, "sinh ");
    let keys = suggest_keys(&mut e, "si");
    let picked = picked_candidate(&keys[1], 0);
    let actions = e.accept_candidate(0);
    assert_eq!(
        actions,
        vec![
            Action::HidePreedit,
            Action::HideCandidates,
            Action::Commit(format!("{} ", picked.clone())),
        ]
    );
    assert_eq!(e.last_committed_word.as_deref(), Some(picked.as_str()));
}

#[test]
fn suggest_digit_still_ends_the_word_and_hides() {
    // The engine never intercepts digits (the server owns selection while
    // its strip is up): on a Telex preset a digit is a word terminator, so
    // `si1` commits the raw keys and closes the strip, and the next digit
    // types as content — no candidate action leaks on either.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "si1");
    assert!(candidates_of(&keys[1]).is_some(), "{:?}", keys[1]);
    assert!(keys[2].contains(&Action::HideCandidates), "{:?}", keys[2]);
    assert!(candidates_of(&keys[2]).is_none(), "{:?}", keys[2]);
    let after = suggest_keys(&mut e, "1");
    assert!(strip_actions(&after[0]).is_empty(), "{:?}", after[0]);
}

// --- prediction tier (state A) ---

#[test]
fn page_one_is_three_singles_then_the_compounds() {
    // Page 1 has the fixed shape the owner asked for, so the digits 1-5
    // always mean the same thing: three single words, then two compounds.
    // `h` is the full house — `học`, `họ`, the EN `hello`, then
    // `học sinh` and `học sinh giỏi` — and the rest of the list keeps
    // coming for the strip's pages.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "h");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on h");
    let page: Vec<&str> = items.iter().take(5).map(String::as_str).collect();
    assert_eq!(
        page,
        ["học", "họ", "hello", "học sinh", "học sinh giỏi"],
        "{items:?}"
    );
    assert!(items.len() > 5, "the leftovers still page: {items:?}");
}

#[test]
fn page_one_gives_the_last_slot_to_a_keyword_emoji() {
    // A keyword-matching emoji holds slot 5, and the compounds give up
    // their second seat to it: `sinh` shows `sinh viên` at slot 4 and the
    // glyph at slot 5. A thin prefix pads nothing — `si` has one single,
    // so its page is shorter instead of gap-filled.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "sinh");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on sinh");
    assert_eq!(items[0], "sinh viên", "{items:?}");
    assert!(
        !items[1].chars().any(|c| c.is_ascii_alphanumeric()),
        "the emoji holds slot 5: {items:?}"
    );
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "si");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on si");
    assert_eq!(items[0], "sinh", "{items:?}");
    assert_eq!(items[1], "sinh viên", "{items:?}");
}

#[test]
fn strip_without_a_model_serves_emoji_only() {
    // The dictionary left the strip entirely: with the model slot cleared
    // the word bands are empty — `zz` matches no model row and no emoji
    // keyword, so nothing opens — and only the emoji keywords still light
    // the strip up (`meo`). (The slot is cleared directly: apply_config
    // would auto-load whatever file the machine carries, and an env
    // override is process-global across parallel tests.)
    let mut e = suggest_engine(Method::Telex, Mode::Preedit, true);
    e.model = None;
    let zz = suggest_keys(&mut e, "zz");
    assert!(zz.iter().all(|b| candidates_of(b).is_none()), "{zz:?}");
    let mut e = suggest_engine(Method::Telex, Mode::Preedit, true);
    e.model = None;
    let meo = suggest_keys(&mut e, "meo");
    let items = candidates_of(&meo[meo.len() - 1]).expect("emoji still open the strip");
    assert!(
        items
            .iter()
            .all(|w| !w.chars().any(|c| c.is_ascii_alphanumeric())),
        "emoji-only list without a model: {items:?}"
    );
}

#[test]
fn predict_state_a_leads_model_completions() {
    // A partial prefix finishes the syllable first: the model's single
    // words lead the list, its compounds (what could continue the
    // endings) follow, the emoji tail trails.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "si");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on si");
    assert!(
        !items.is_empty(),
        "the model's VN rows fill the list on a live prefix"
    );
    assert!(!items.first().unwrap().contains(' '), "{items:?}");
    assert!(
        items.iter().any(|item| item.contains(' ')),
        "the compounds follow the singles: {items:?}"
    );
}

// --- prediction tier (state B) ---

#[test]
fn state_b_predicts_next_syllable() {
    // `sinh ` lands the word and the strip re-opens with the model's
    // next-word list for the new context, strongest follower first.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "sinh ");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens after the space");
    assert_eq!(items.first().map(String::as_str), Some("viên"));
}

#[test]
fn state_b_english_context_predicts_english() {
    // The unified id space: an EN context finds its EN followers in the
    // same bigram table, and the unigram backoff tier fills the list
    // behind them (0.4 * count / total) with no VN syllable leaking into
    // the head — the fixture's own rows first, global frequent fillers after.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "hello ");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens after the space");
    assert_eq!(&items[..2], &["world".to_string(), "there".to_string()]);
    assert_eq!(
        &items[2..5],
        &["hello".to_string(), "want".to_string(), "help".to_string()],
        "the unigram tier fills by global frequency (hello is the top row)"
    );
    assert!(
        items[..5].iter().all(|w| w.is_ascii()),
        "no VN syllable leaks into the EN head"
    );
}

#[test]
fn state_b_pick_inserts_word_and_space() {
    // The state-B pick commits the candidate wholesale (nothing is held —
    // no delete transport) and chains the new context's list in place.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    suggest_keys(&mut e, "sinh ");
    let actions = e.accept_candidate(0);
    assert_eq!(replay(&actions), "viên ");
    let chained = actions
        .iter()
        .find_map(|a| match a {
            Action::UpdateCandidates { items, .. } => Some(items),
            _ => None,
        })
        .expect("the strip chains into the next context");
    assert_eq!(chained.first().map(String::as_str), Some("học"));
}

#[test]
fn state_b_confined_to_strip_modes() {
    // The state-B hook lives on the common-path word landing only: Flow's
    // uinput word ends never reach it (its state-A strip still runs — the
    // space batch just hides without offering a next-word list) and OFF
    // turns the strip off outright.
    for mode in [Mode::Flow, Mode::Off] {
        let mut e = predict_engine(Method::Telex, mode);
        let keys = suggest_keys(&mut e, "sinh ");
        assert!(
            candidates_of(keys.last().unwrap()).is_none(),
            "no state-B list after the space in {mode:?}: {:?}",
            keys.last().unwrap()
        );
    }
}

#[test]
fn state_b_without_a_model_stays_silent() {
    // No model, no next-word list: the space batch hides the strip and
    // offers nothing — the dictionary that once served this surface is out
    // of the strip entirely. (The slot is cleared directly — see
    // `strip_without_a_model_serves_emoji_only`.)
    let mut e = suggest_engine(Method::Telex, Mode::Preedit, true);
    e.model = None;
    let keys = suggest_keys(&mut e, "sinh ");
    assert!(
        candidates_of(keys.last().unwrap()).is_none(),
        "the state-B surface stays silent without a model: {:?}",
        keys.last().unwrap()
    );
}

#[test]
fn state_b_without_context_stays_silent() {
    // A bare space with no word before it: no context, no strip.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, " ");
    assert!(keys.iter().all(|b| candidates_of(b).is_none()));
}

// --- prediction tier (bands: EN + emoji) ---

#[test]
fn english_completions_on_ascii_prefix() {
    // `hel` is pure ASCII: the EN band joins the list with the model's
    // EN rows on that prefix, frequency-first among themselves.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "hel");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on hel");
    assert!(items.contains(&"hello".to_string()), "{items:?}");
    let en: Vec<&String> = items
        .iter()
        .filter(|w| ["hello", "help", "hen"].contains(&w.as_str()))
        .collect();
    assert_eq!(
        en,
        [&"hello", &"help"],
        "EN rows on the typed prefix, frequency-first: {items:?}"
    );
}

#[test]
fn vietnamese_letters_exclude_english() {
    // A Vietnamese letter or tone mark anywhere excludes EN: `hè` never
    // offers `hello`.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "hè");
    if let Some(items) = candidates_of(keys.last().unwrap()) {
        assert!(!items.contains(&"hello".to_string()), "{items:?}");
    }
}

#[test]
fn emoji_reach_the_strip_tail() {
    // `meo` folds to the mèo keywords: the model carries no VN row for the
    // prefix and the dictionary left the strip, so the glyphs serve alone,
    // capped at STRIP_EMOJI_CAP.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    let keys = suggest_keys(&mut e, "meo");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on meo");
    assert_eq!(items.first().map(String::as_str), Some("😸"), "{items:?}");
    let count = items
        .iter()
        .filter(|w| !w.chars().any(|c| c.is_ascii_alphanumeric()))
        .count();
    assert!(
        count > 0 && count <= crate::emoji::STRIP_EMOJI_CAP,
        "{items:?}"
    );
}

#[test]
fn suggest_digit_tone_presets_stay_silent() {
    // The presets whose digits are tone keys never run the strip: the
    // selection digits would fight the method's own marks. Zero candidate
    // actions for any key run, marks and literal digits included.
    for method in [Method::Vni, Method::TelexVni] {
        let mut e = suggest_engine(method, Mode::Preedit, true);
        let keys = suggest_keys(&mut e, "viet6 1");
        let all: Vec<Action> = keys.into_iter().flatten().collect();
        assert!(strip_actions(&all).is_empty(), "{method:?}: {all:?}");
    }
}

#[test]
fn suggest_stays_silent_while_the_experimental_gate_is_closed() {
    // The row lives behind "Bật thử nghiệm" like the keeps: the engine bakes
    // the master gate into the mirror (`experimental && suggest_words`), so
    // the saved switch alone — gate closed — must keep every strip action
    // off the wire. Config::default() carries the closed gate on purpose.
    let mut e = Engine::new(Method::Telex, Mode::Preedit);
    e.apply_config(&Config {
        suggest_words: true,
        ..Config::default()
    });
    let keys = suggest_keys(&mut e, "th");
    for (i, batch) in keys.iter().enumerate() {
        assert!(
            strip_actions(batch).is_empty(),
            "key {i} emitted a strip action: {batch:?}"
        );
    }
}

#[test]
fn suggest_off_and_flow_stay_silent() {
    // OFF forwards raw keys — nothing composes, so no candidate action may
    // appear. (Flow composes through the injection backend and serves the
    // strip like the other modes now; its own pin follows.)
    let mut e = suggest_engine(Method::Telex, Mode::Off, true);
    let keys = suggest_keys(&mut e, "tieng ");
    let all: Vec<Action> = keys.into_iter().flatten().collect();
    assert!(strip_actions(&all).is_empty(), "{all:?}");
}

#[test]
fn suggest_strip_opens_in_flow_and_rides_the_word_end() {
    // Flow's per-key path now carries the strip like the common path: the
    // list follows the composed word while it grows, and the word-end
    // rewrite closes it with the commit.
    let mut e = suggest_engine(Method::Telex, Mode::Flow, true);
    e.set_uinput_available(true);
    let keys = suggest_keys(&mut e, "thooi ");
    assert!(
        keys[..5].iter().any(|batch| candidates_of(batch).is_some()),
        "the strip is open before the terminator: {keys:?}"
    );
    assert!(keys[5].contains(&Action::HideCandidates), "{:?}", keys[5]);
    assert!(candidates_of(&keys[5]).is_none(), "{:?}", keys[5]);
}

/// The word an `UpdateCandidates` action carries — the box's input line
/// source (the inline modes have no preedit signal to convert).
fn word_of(actions: &[Action]) -> Option<&str> {
    actions.iter().find_map(|a| match a {
        Action::UpdateCandidates { word, .. } => Some(word.as_str()),
        _ => None,
    })
}

/// Feed keys like `suggest_keys` but with surrounding-text reports after
/// every key — the inline modes' delete transports (the protocol delete,
/// clamped to the report) need a client that reports its buffer.
fn suggest_keys_st(e: &mut Engine, keys: &str) -> Vec<Vec<Action>> {
    if e.client().is_none() {
        e.set_client(Some("gtk4-im:test".into()));
    }
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e.set_surrounding(String::new(), 0);
    let mut buf: Vec<char> = Vec::new();
    keys.chars()
        .map(|c| {
            let actions = e.process_char(c);
            for a in &actions {
                match a {
                    Action::DeleteBefore(n) => {
                        buf.truncate(buf.len().saturating_sub(*n as usize));
                    }
                    Action::DeleteSurrounding { offset, n } => {
                        let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                        let end = (start + *n as usize).min(buf.len());
                        buf.drain(start..end);
                    }
                    Action::Commit(t) => buf.extend(t.chars()),
                    _ => {}
                }
            }
            let text: String = buf.iter().collect();
            e.set_surrounding(text, buf.len() as u32);
            actions
        })
        .collect()
}

#[test]
fn suggest_strip_carries_the_word_for_the_box_line() {
    // The action is the word's only vehicle to the box's input line: the
    // inline modes have no preedit signal to convert, so the word rides
    // every `UpdateCandidates` — and never collides with its own list (the
    // completions exclude the word itself).
    let mut e = suggest_engine(Method::Telex, Mode::Preedit, true);
    let keys = suggest_keys(&mut e, "th");
    assert_eq!(word_of(&keys[1]), Some("th"), "{:?}", keys[1]);
    let items = candidates_of(&keys[1]).expect("the strip opens on `th`");
    assert!(
        !items.iter().any(|w| w == "th"),
        "completions exclude the word: {items:?}"
    );
}

#[test]
fn suggest_accept_rewrites_the_held_word_in_surrounding() {
    // Commit-as-you-type: `th` already sits in the application's field, so
    // the pick erases it through the protocol delete (clamped to the
    // report) before the candidate commits — the old shape appended the
    // candidate after the held word (`chon` + pick → `chonchọn`).
    let mut e = suggest_engine(Method::Telex, Mode::Surrounding, true);
    let keys = suggest_keys_st(&mut e, "th");
    let picked = picked_candidate(&keys[1], 0);
    let actions = e.accept_candidate(0);
    assert_eq!(
        actions,
        vec![
            Action::HideCandidates,
            Action::DeleteBefore(2),
            Action::Commit(format!("{} ", picked)),
        ],
        "{actions:?}"
    );
    assert!(!e.suggest_open);
}

#[test]
fn suggest_accept_uses_forwarded_backspaces_in_forward_mode() {
    // ForwardBackspace holds the composed text in the application too; its
    // transport is Backspace key events sent through IBus — they reach
    // every client, so the pick erases through them.
    let mut e = suggest_engine(Method::Telex, Mode::ForwardBackspace, true);
    let keys = suggest_keys(&mut e, "th");
    let picked = picked_candidate(&keys[1], 0);
    let actions = e.accept_candidate(0);
    assert_eq!(
        actions,
        vec![
            Action::HideCandidates,
            Action::ForwardBackspaces(2),
            Action::Commit(format!("{} ", picked)),
        ],
        "{actions:?}"
    );
}

#[test]
fn suggest_accept_rewrites_through_injection_in_flow() {
    // Flow holds the word on screen through the per-key batch: the pick
    // lands any parked commit first (a fast digit must not let the stale
    // park re-insert the old text), erases through the injected Backspaces
    // and commits — the emoji rewrite's exact sequence.
    let mut e = suggest_engine(Method::Telex, Mode::Flow, true);
    e.set_uinput_available(true);
    let keys = suggest_keys(&mut e, "th");
    let picked = picked_candidate(&keys[1], 0);
    let actions = e.accept_candidate(0);
    assert_eq!(
        actions,
        vec![
            Action::HideCandidates,
            Action::CommitNow("th".into()),
            Action::InjectBackspaces(2),
            Action::PostDelay(10),
            Action::Commit(format!("{} ", picked)),
        ],
        "{actions:?}"
    );
}

#[test]
fn suggest_backspace_follows_the_word_in_inline_modes() {
    // The Backspace arms' contract, inline flavor: the list follows the
    // shorter word all the way down to the single letter — from the first
    // letter on the model serves (the old floor that closed the strip
    // below two characters was the dictionary's scan guard).
    let mut e = suggest_engine(Method::Telex, Mode::Surrounding, true);
    let keys = suggest_keys_st(&mut e, "tha");
    assert!(
        candidates_of(&keys[2]).is_some(),
        "the strip is up: {:?}",
        keys[2]
    );
    let actions = e.backspace().expect("the word is in flight");
    assert!(
        candidates_of(&actions).is_some(),
        "the list follows the shorter word: {actions:?}"
    );
    let actions = e.backspace().expect("the word is still in flight");
    assert!(
        candidates_of(&actions).is_some(),
        "the list follows the single letter too: {actions:?}"
    );

    let mut e = suggest_engine(Method::Telex, Mode::ForwardBackspace, true);
    let keys = suggest_keys(&mut e, "tha");
    assert!(candidates_of(&keys[2]).is_some(), "{:?}", keys[2]);
    let actions = e.backspace().expect("the word is in flight");
    assert!(
        candidates_of(&actions).is_some(),
        "the list follows the shorter word: {actions:?}"
    );
    let actions = e.backspace().expect("the word is still in flight");
    assert!(
        candidates_of(&actions).is_some(),
        "the list follows the single letter too: {actions:?}"
    );

    let mut e = suggest_engine(Method::Telex, Mode::Flow, true);
    e.set_uinput_available(true);
    suggest_keys(&mut e, "tha");
    let actions = e.backspace().expect("the word is in flight");
    assert_eq!(
        strip_actions(&actions).len(),
        1,
        "exactly one strip action rides the backspace: {actions:?}"
    );
    let actions = e.backspace().expect("the word is still in flight");
    assert!(
        candidates_of(&actions).is_some(),
        "the list follows the single letter too: {actions:?}"
    );
}

#[test]
fn suggest_last_committed_word_follows_the_word_end() {
    // The word a Finish commits becomes the strip's context word; focus
    // loss drops it with the rest of the word state.
    let mut e = suggest_engine(Method::Telex, Mode::Preedit, true);
    suggest_keys(&mut e, "tieengs");
    assert!(e.last_committed_word.is_none());
    suggest_keys(&mut e, " ");
    assert_eq!(e.last_committed_word.as_deref(), Some("tiếng"));
    e.focus_out();
    assert!(e.last_committed_word.is_none());
}

// --- pick learning (tier 3) ---

/// A strip engine with pick learning on, redirected away from the real
/// config dir: behavioral data from a test run must never touch the
/// owner's files.
fn learn_engine(method: Method, mode: Mode) -> Engine {
    let mut e = suggest_engine(method, mode, true);
    e.apply_config(&Config {
        mode: mode.into(),
        suggest_words: true,
        experimental_features: true,
        ..Config::default()
    });
    e.set_method(method);
    e.model = Some(sample_model());
    e.learn_dir = learn_tmp("engine");
    e.picks = Some(crate::learn::UserPicks::load(&e.learn_dir));
    e
}

fn learn_tmp(tag: &str) -> std::path::PathBuf {
    // A fresh dir per call: the tests run on parallel threads and a shared
    // directory would let one test's wipe/delete race another's asserts.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "mangrs-learn-test-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp learn dir");
    dir
}

#[test]
fn learning_off_writes_nothing() {
    // The pick path is shared with the legacy strip: with the switch off,
    // picking a candidate must not create the behavioral files — they
    // exist only when the owner opted in.
    let mut e = suggest_engine(Method::Telex, Mode::Preedit, true);
    let dir = learn_tmp("off");
    e.learn_dir = dir.clone();
    suggest_keys(&mut e, "sinh");
    e.accept_candidate(0);
    assert!(!dir.join(crate::learn::PICKS_FILE).exists());
    assert!(!dir.join(crate::learn::PAIRS_FILE).exists());
}

#[test]
fn pick_flushes_on_tick_not_on_key() {
    // The pick lands in memory at accept time and on disk at the engine's
    // tick — the key path never touches the filesystem. `sin` completes the
    // fixture's `sinh` row (single words only now — the model compound band
    // is gone).
    let mut e = learn_engine(Method::Telex, Mode::Preedit);
    let dir = e.learn_dir.clone();
    let keys = suggest_keys(&mut e, "sin");
    let items = candidates_of(keys.last().unwrap()).expect("the strip opens on sin");
    let word = items[0].clone();
    e.accept_candidate(0);
    assert!(
        !dir.join(crate::learn::PICKS_FILE).exists(),
        "no write on the key path"
    );
    e.tick();
    let text = std::fs::read_to_string(dir.join(crate::learn::PICKS_FILE))
        .expect("the tick flushed the pick");
    assert!(text.contains(&format!("{word}\t1\n")), "{text}");
}

#[test]
fn user_bonus_stays_inside_its_band() {
    // 25 picks of `help` after `world` (both frequency and pair bonus)
    // lift help over the more frequent hello INSIDE the EN band — and
    // never over the Vietnamese rows: the sample fixture has no VN row
    // under `he`, so the band check rides the `h` prefix, whose VN
    // completions must still lead the boosted EN band.
    let mut e = learn_engine(Method::Telex, Mode::Preedit);
    e.last_committed_word = Some("world".into());
    for _ in 0..25 {
        e.picks
            .as_mut()
            .expect("learning is on")
            .record_pick("help", Some("world"));
    }
    let list = e.state_a_candidates("he");
    let pos_help = list.iter().position(|w| w == "help").expect("help offered");
    let pos_hello = list
        .iter()
        .position(|w| w == "hello")
        .expect("hello offered");
    assert!(pos_help < pos_hello, "picks lift help over hello: {list:?}");
    let list_h = e.state_a_candidates("h");
    let first_vn = list_h
        .iter()
        .position(|w| !w.chars().all(|c| c.is_ascii_alphabetic()))
        .expect("the h prefix serves VN rows");
    let first_en = list_h
        .iter()
        .position(|w| w.chars().all(|c| c.is_ascii_alphabetic()))
        .expect("the h prefix serves EN rows too");
    assert!(
        first_vn < first_en,
        "the bonus never crosses the band: {list_h:?}"
    );
}

#[test]
fn password_purpose_gates_strip_and_learning() {
    // PASSWORD (8) and PIN (9) quiet the strip and the learning files;
    // typing itself is untouched, and a normal field (0) re-opens the
    // strip on the in-flight word.
    let mut e = learn_engine(Method::Telex, Mode::Preedit);
    let dir = e.learn_dir.clone();
    e.set_content_purpose(PURPOSE_PASSWORD);
    let keys = suggest_keys(&mut e, "sinh");
    assert!(
        candidates_of(keys.last().unwrap()).is_none(),
        "the strip stays quiet in a password field"
    );
    // Typing itself is untouched: the in-flight word composes exactly as
    // before (Preedit holds it — no commit exists mid-word).
    assert_eq!(e.wrapper.output(), "sinh");
    e.accept_candidate(0);
    e.tick();
    assert!(
        !dir.join(crate::learn::PICKS_FILE).exists(),
        "a password field learns nothing"
    );
    e.set_content_purpose(PURPOSE_PIN);
    assert!(
        candidates_of(&e.suggest_step()).is_none(),
        "PIN stays quiet too"
    );
    e.set_content_purpose(0);
    let reopen = e.suggest_step();
    assert!(
        candidates_of(&reopen).is_some(),
        "a normal field re-opens the strip on the in-flight word"
    );
}

// --- fused state-B scoring (trigram + stupid backoff, task 6) ---

#[test]
fn prev_committed_word_shifts() {
    // Two typed words land: the register holds the pair (before, last) the
    // trigram tier keys on.
    let mut e = predict_engine(Method::Telex, Mode::Preedit);
    suggest_keys(&mut e, "sinh viên ");
    assert_eq!(e.last_committed_word.as_deref(), Some("viên"));
    assert_eq!(e.prev_committed_word.as_deref(), Some("sinh"));
}

#[test]
fn pick_lifts_a_state_b_candidate() {
    // Ten picks of (hello, there) triple there's context score — it
    // overtakes the stronger world inside the same list.
    let mut e = learn_engine(Method::Telex, Mode::Preedit);
    suggest_keys(&mut e, "hello ");
    for _ in 0..10 {
        e.picks
            .as_mut()
            .expect("learning is on")
            .record_pick("there", Some("hello"));
    }
    let items = e.state_b_candidates(None, "hello");
    assert_eq!(items.first().map(String::as_str), Some("there"));
}
