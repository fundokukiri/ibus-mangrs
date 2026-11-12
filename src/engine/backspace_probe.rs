//! Test-only probe: what a Backspace really does to a composition under each
//! "Backspace xóa dấu" mode, and what the Flow/Turbo per-key screen model is
//! before and after it. Research round for the Flow/Turbo mark-editing design;
//! prints tables instead of asserting (run with `-- --nocapture`).

use super::*;
use crate::config::BackspaceToneConfig;

/// Word on screen and the Telex keys that type it.
const WORDS: &[(&str, &str)] = &[
    ("tùng", "tungf"),
    ("túng", "tungs"),
    ("tiếng", "tieengs"),
    ("việt", "vieejt"),
    ("chào", "chaof"),
    ("đ", "dd"),
    ("quyển", "quyeenr"),
    ("ươ", "uow"),
];

const TONES: [BackspaceToneConfig; 3] = [
    BackspaceToneConfig::ToneFirst,
    BackspaceToneConfig::WholeChar,
    BackspaceToneConfig::UnikeyWalk,
];

fn type_wrapper(w: &mut BambooWrapper, keys: &str) {
    for c in keys.chars() {
        w.process_char(c);
    }
}

fn wrapper() -> BambooWrapper {
    BambooWrapper::new(Method::Telex, bamboo_core::Config::default())
}

#[test]
fn probe_backspace_tone_matrix() {
    println!("\n== probe_backspace_tone_matrix: wrapper-level, Telex ==");
    for tone in TONES {
        for (word, keys) in WORDS {
            // 1. Live composition, mid-word (every raw key still held).
            let mut w = wrapper();
            type_wrapper(&mut w, keys);
            w.set_backspace_tone(tone);
            let before = w.output();
            let raw = w.raw().to_string();
            let first = w.backspace().is_some();
            let after1 = w.output();
            let comp1 = w.has_composition();
            let raw1 = w.raw().to_string();
            let second = w.backspace().is_some();
            let after2 = w.output();
            println!(
                "live    {tone:?} {word} before={before:?} raw={raw:?} \
                 bs1={first} after1={after1:?} comp={comp1} raw1={raw1:?} \
                 bs2={second} after2={after2:?} comp2={}",
                w.has_composition()
            );

            // 2. Composition rebuilt from the screen (the v1 retrack path).
            let mut w = wrapper();
            w.set_backspace_tone(tone);
            let rebuilt = w.retrack_from_screen(word);
            let after0 = w.output();
            let mut second_part = String::new();
            if rebuilt {
                let _ = w.backspace();
                second_part = format!(
                    "bs={:?} comp={} raw={:?}",
                    w.output(),
                    w.has_composition(),
                    w.raw()
                );
            }
            println!("rebuilt {tone:?} {word} rebuilt={rebuilt} before={after0:?} {second_part}");

            // 3. Dead composition (word already committed by word end).
            let mut w = wrapper();
            type_wrapper(&mut w, keys);
            w.reset();
            w.set_backspace_tone(tone);
            println!("dead    {tone:?} {word} bs={:?}", w.backspace().is_some());
        }
    }
}

/// A stand-in for the application: applies the engine's actions to a text
/// buffer, a forwarded key inserts the raw char and a forwarded Backspace
/// pops one char. Used to read the screen the user would actually see.
struct Sim {
    e: Engine,
    buf: Vec<char>,
    preedit: String,
}

impl Sim {
    fn new(mode: Mode, config: Config) -> Self {
        let mut e = Engine::new(Method::Telex, mode);
        let mut config = config;
        config.mode = mode.into();
        e.apply_config(&config);
        e.set_client(Some("gtk4-im:probe".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.set_uinput_available(true);
        Self {
            e,
            buf: Vec::new(),
            preedit: String::new(),
        }
    }

    fn apply(&mut self, actions: &[Action]) {
        let forwarded = actions.contains(&Action::Forward);
        for a in actions {
            match a {
                Action::DeleteBefore(n)
                | Action::DeleteSurrounding { n, .. }
                | Action::InjectBackspaces(n)
                | Action::ForwardBackspaces(n) => {
                    self.buf
                        .truncate(self.buf.len().saturating_sub(*n as usize));
                }
                Action::Commit(t) | Action::CommitNow(t) => {
                    self.buf.extend(t.chars());
                    self.preedit.clear();
                }
                Action::UpdatePreedit { text, .. } => self.preedit = text.clone(),
                Action::HidePreedit => self.preedit.clear(),
                _ => {}
            }
        }
        if forwarded {
            self.buf.pop();
        }
        let text: String = self.buf.iter().collect();
        let len = self.buf.len() as u32;
        self.e.set_surrounding(text, len);
    }

    fn screen(&self) -> String {
        let mut s: String = self.buf.iter().collect();
        s.push_str(&self.preedit);
        s
    }

    fn state(&self) -> String {
        format!(
            "screen={:?} tail={:?} park={:?} out={:?} comp={} raw={:?}",
            self.screen(),
            self.e.tail,
            self.e.per_key_park.as_ref().map(|(_, t)| t.clone()),
            self.e.wrapper.output(),
            self.e.wrapper.has_composition(),
            self.e.wrapper.raw()
        )
    }

    fn step(&mut self, label: &str, step: char) {
        if step == '\u{8}' {
            println!("       before-BS {}", self.state());
        }
        let actions = if step == '\u{8}' {
            self.e.backspace()
        } else {
            Some(self.e.process_char(step))
        };
        match actions {
            Some(actions) => {
                self.apply(&actions);
                println!("  {label:<4} {step:?} {actions:?}");
                if step == '\u{8}' {
                    // The user lets go before the main loop runs again.
                    self.e.note_backspace_released();
                }
            }
            None => {
                self.buf.pop();
                let text: String = self.buf.iter().collect();
                let len = self.buf.len() as u32;
                self.e.set_surrounding(text, len);
                println!("  {label:<4} {step:?} <None: app handles>");
            }
        }
        println!("       {}", self.state());
    }

    fn flush(&mut self) {
        // Wake every parked thing the engine may hold — the per-key batch and
        // the experimental Backspace's deferred correction — and land the
        // commit the correction parks in turn.
        for _ in 0..3 {
            let mut woke = false;
            if let Some((deadline, _)) = self.e.per_key_park.as_mut() {
                *deadline = Instant::now() - Duration::from_millis(1);
                woke = true;
            }
            if let Some((deadline, _)) = self.e.deferred_fix.as_mut() {
                *deadline = Instant::now() - Duration::from_millis(1);
                woke = true;
            }
            if !woke {
                break;
            }
            let actions = self.e.tick();
            self.apply(&actions);
            println!("  flush   {actions:?}");
            println!("       {}", self.state());
        }
    }

    /// Type `script`; `~` flushes the per-key batch, `\u{8}` is a Backspace.
    fn run(&mut self, script: &str) {
        for step in script.chars() {
            if step == '~' {
                self.flush();
            } else {
                self.step("key", step);
            }
        }
        if self.e.per_key_park.is_some() || self.e.deferred_fix.is_some() {
            self.flush();
        }
    }
}

fn probe_case(label: &str, mode: Mode, config: Config, script: &str) {
    println!("\n-- {label} ({mode:?}) script={script:?} --");
    let mut sim = Sim::new(mode, config);
    sim.run(script);
}

fn per_key(mode: Mode) -> (Mode, Config) {
    (
        mode,
        Config {
            experimental_features: true,
            flow_per_key: true,
            ..Config::default()
        },
    )
}

#[test]
fn probe_flow_backspace_screen_model() {
    println!("\n== probe_flow_backspace_screen_model ==");
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        probe_case("word end then BS", mode, cfg.clone(), "tungf ~\u{8}");
        probe_case("BS twice", mode, cfg.clone(), "tungf ~\u{8}\u{8}");
        probe_case("mark key after BS", mode, cfg.clone(), "tungf ~\u{8}~s");
        probe_case("mid-word BS", mode, cfg.clone(), "tieengs\u{8}");
        probe_case(
            "committed word, BS on space",
            mode,
            cfg.clone(),
            "chaof ~\u{8}",
        );
        probe_case(
            "committed word, BS twice",
            mode,
            cfg.clone(),
            "chaof ~\u{8}\u{8}",
        );
        probe_case(
            "T3: litter key then BS then s",
            mode,
            cfg.clone(),
            "chaof ~f\u{8}s",
        );
        probe_case(
            "T3 variant: litter then BS twice",
            mode,
            cfg.clone(),
            "chaof ~f\u{8}\u{8}",
        );
        probe_case("dd BS d", mode, cfg.clone(), "dd\u{8}d");
        probe_case(
            "second word then BS",
            mode,
            cfg.clone(),
            "tooi ~camr ~\u{8}",
        );
        probe_case(
            "url bypass then BS",
            mode,
            cfg.clone(),
            "vi-mangrs.com ~\u{8}",
        );
        probe_case(
            "per-key off",
            mode,
            Config {
                experimental_features: true,
                flow_per_key: false,
                ..Config::default()
            },
            "tungf ~\u{8}s",
        );
        probe_case(
            "v1 retrack ON",
            mode,
            Config {
                experimental_features: true,
                flow_per_key: true,
                flow_backspace_retrack: true,
                ..Config::default()
            },
            "tungf ~\u{8}~s",
        );
        probe_case(
            "v1 retrack OFF",
            mode,
            Config {
                experimental_features: true,
                flow_per_key: true,
                flow_backspace_retrack: false,
                ..Config::default()
            },
            "tungf ~\u{8}~s",
        );
        probe_case(
            "restore then letter",
            mode,
            Config {
                experimental_features: true,
                flow_per_key: true,
                flow_backspace_retrack: true,
                ..Config::default()
            },
            "chaof ~f\u{8}q",
        );
        probe_case(
            "restore then whole-word delete",
            mode,
            Config {
                experimental_features: true,
                flow_per_key: true,
                flow_backspace_retrack: true,
                ..Config::default()
            },
            "chaof ~tungf ~\u{8}\u{8}\u{8}\u{8}\u{8}",
        );
    }
}

// --- target semantics of the experimental feature --------------------------
//
// These pin what the Flow/Turbo Backspace must do with the existing
// experimental switch on (`flow_backspace_retrack`, gate-baked, default off):
// delete a mark, keep editing a mark, and restore a word's mark after a stray
// key is erased — like the inline modes. They read the SCREEN, not the action
// list, so the transport stays free. The ToneFirst row is still the target of
// a later stage and stays ignored.

fn screen_of(mode: Mode, config: Config, script: &str) -> (String, Engine) {
    let mut sim = Sim::new(mode, config);
    sim.run(script);
    (sim.screen(), sim.e)
}

/// The experiment on: per-key display plus both Backspace switches, which is
/// what a user of the "Thử nghiệm" tab has.
fn tone(mode: Mode, tone: BackspaceToneConfig) -> (Mode, Config) {
    let (_, cfg) = per_key(mode);
    (
        mode,
        Config {
            flow_backspace_retrack: true,
            flow_backspace_restore: true,
            backspace_tone: tone,
            ..cfg
        },
    )
}

#[test]
fn target_unikeywalk_backspace_keeps_the_word_live() {
    // Gate off: screen stays `tùng`, the composition is dead and the next
    // mark key starts a new word (`tùngs`).
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, e) = screen_of(mode, cfg, "tungf ~\u{8}");
        assert_eq!(screen, "tùng", "{mode:?}");
        assert_eq!(e.tail, "tùng", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}: word must stay live");
    }
}

#[test]
fn target_wholechar_backspace_deletes_the_character() {
    // The first Backspace erases the terminator and gives the word back; the
    // second one follows the screen model (`tùng` -> `tùn`), tone intact.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::WholeChar);
        let (screen, e) = screen_of(mode, cfg.clone(), "tungf ~\u{8}");
        assert_eq!(screen, "tùng", "{mode:?}");
        assert_eq!(e.tail, "tùng", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}");
        let (screen, e) = screen_of(mode, cfg, "tungf ~\u{8}\u{8}");
        assert_eq!(screen, "tùn", "{mode:?}");
        assert_eq!(e.tail, "tùn", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}");
    }
}

#[test]
fn target_backspace_on_a_terminator_keeps_the_tone() {
    // Owner bug: `chào` + space + Backspace dropped the tone and then ate a
    // letter per press (`chao`, `cha`). Erasing the space belongs to the
    // region path, never to the tone-first rewrite.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        let (screen, _) = screen_of(mode, cfg.clone(), "chaof ~\u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
        let (screen, _) = screen_of(mode, cfg, "chaof ~\u{8} \u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
    }
}

#[test]
fn target_a_later_letter_does_not_rearm_the_restore() {
    // Live bug from the owner's log: a letter typed after the undo made the
    // wrapper's diff read bamboo's auto_correct revert as another undo, which
    // re-armed the restore window — the next ordinary Backspace then injected
    // deletes and the word lost characters it should not have.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        let (screen, _) = screen_of(mode, cfg, "chaoffg\u{8}");
        assert_eq!(screen, "chaof", "{mode:?}");
    }
}

#[test]
fn target_tonefirst_backspace_drops_the_tone() {
    // ToneFirst is the one setting that changes the screen text without a
    // plain delete: `tùng` -> `tung`, like the inline modes. The first
    // Backspace erases the space after the committed word, the second drops
    // the tone; the rewrite is deferred past the key release.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        let (screen, e) = screen_of(mode, cfg, "tungf ~\u{8}\u{8}");
        assert_eq!(screen, "tung", "{mode:?}");
        assert_eq!(e.tail, "tung", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}");
    }
}

#[test]
fn target_a_mark_key_edits_the_word_after_backspace() {
    // The whole point of keeping the word live: after the terminator erase the
    // mark key edits the word (`tùng` + `s` -> `túng`); after the second
    // Backspace the shortened screen keeps composing (`tùn` + `s` -> `tún`).
    // Gate off: `tùngs` / `tùns`.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::WholeChar);
        let (screen, _) = screen_of(mode, cfg, "tungf ~\u{8}~s");
        assert_eq!(screen, "túng", "{mode:?}");
    }
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, _) = screen_of(mode, cfg, "tungf ~\u{8}\u{8}~s");
        assert_eq!(screen, "tún", "{mode:?}");
    }
}

#[test]
fn target_committed_word_comes_back_after_the_litter_is_erased() {
    // Owner trace: `chào` committed, a stray `f` litters the next region,
    // Backspace erases it — the word behind the terminator comes back to the
    // composition, and the terminator is retracted with it so the word sits at
    // the caret; `s` then turns it into `cháo`.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        let cfg = Config {
            flow_backspace_retrack: true,
            ..cfg
        };
        let (screen, e) = screen_of(mode, cfg.clone(), "chaof ~f\u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
        assert_eq!(e.tail, "chào", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}: word must come back");
        let (screen, _) = screen_of(mode, cfg.clone(), "chaof ~f\u{8}~s");
        assert_eq!(screen, "cháo", "{mode:?}");
        // Same when the stray key already landed on screen (the batch flushed
        // before the Backspace): the injected retract removes the terminator.
        let (screen, e) = screen_of(mode, cfg, "chaof ~f~\u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
        assert_eq!(e.tail, "chào", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}");
    }
}

#[test]
fn probe_inline_reference() {
    for tone in [
        BackspaceToneConfig::ToneFirst,
        BackspaceToneConfig::WholeChar,
        BackspaceToneConfig::UnikeyWalk,
    ] {
        println!("\n-- inline Preedit {tone:?} --");
        let mut sim = Sim::new(
            Mode::Preedit,
            Config {
                backspace_tone: tone,
                ..Config::default()
            },
        );
        sim.run("chaoff\u{8}");
        println!(
            "  final screen={:?} preedit={:?}",
            sim.screen(),
            sim.preedit
        );
    }
}

#[test]
fn target_undone_mark_matches_the_inline_modes() {
    // The owner's requirement: for `chào` + `f` + Backspace the uinput modes
    // must produce what Preedit / NonPreedit / Surrounding produce with the
    // same "Backspace xóa dấu" setting — ToneFirst `chào`, WholeChar `chao`,
    // UnikeyWalk `chào`. The correction is deferred, so the harness settles
    // the main loop before reading the screen.
    for t in [
        BackspaceToneConfig::ToneFirst,
        BackspaceToneConfig::WholeChar,
        BackspaceToneConfig::UnikeyWalk,
    ] {
        let (inline, _) = screen_of(
            Mode::Preedit,
            Config {
                backspace_tone: t,
                ..Config::default()
            },
            "chaoff\u{8}",
        );
        for mode in [Mode::Flow, Mode::Turbo] {
            let (_, cfg) = tone(mode, t);
            let (screen, e) = screen_of(mode, cfg.clone(), "chaoff\u{8}");
            assert_eq!(screen, inline, "{mode:?} {t:?}");
            assert_eq!(e.tail, inline, "{mode:?} {t:?}");
            assert!(e.deferred_fix.is_none(), "{mode:?}: fix must have landed");
        }
    }
}

#[test]
fn target_undone_mark_follows_the_delete_mode() {
    // The concrete values behind the comparison above.
    for (t, expected) in [
        (BackspaceToneConfig::ToneFirst, "chào"),
        (BackspaceToneConfig::WholeChar, "chao"),
        (BackspaceToneConfig::UnikeyWalk, "chào"),
    ] {
        for mode in [Mode::Flow, Mode::Turbo] {
            let (_, cfg) = tone(mode, t);
            let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}");
            assert_eq!(screen, expected, "{mode:?} {t:?}");
        }
    }
}

#[test]
fn target_repeated_backspace_after_an_undone_mark_never_duplicates() {
    // The live bug: `chaof` + Backspace x5 produced `chaofào`, `chaofàoao`, …
    // Every step must only shorten the screen.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, _) = screen_of(mode, cfg.clone(), "chaoff\u{8}\u{8}");
        assert_eq!(screen, "cha", "{mode:?}");
        let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}");
        assert_eq!(screen, "", "{mode:?}");
    }
}

#[test]
fn target_typing_before_the_fix_lands_keeps_the_mark() {
    // The correction is only deferred, not lost: a tone key typed immediately
    // after the Backspace collapses both into one rewrite.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}s");
        assert_eq!(screen, "cháo", "{mode:?}");
    }
}

#[test]
fn target_gate_off_keeps_the_plain_backspace() {
    // The switch off is today's behavior byte for byte: the undone mark stays
    // undone (`chaof` Backspace gives `chao`).
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}");
        assert_eq!(screen, "chao", "{mode:?}");
    }
}

#[test]
fn target_restore_switch_gates_the_restore() {
    // The restore is its own switch: with it off the Backspace keeps the
    // plain result even though the retrack experiment is on.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        let cfg = Config {
            flow_backspace_retrack: true,
            flow_backspace_restore: false,
            backspace_tone: BackspaceToneConfig::ToneFirst,
            ..cfg
        };
        let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}");
        assert_eq!(screen, "chao", "{mode:?}");
    }
}

#[test]
fn target_restore_runs_with_a_zero_delay() {
    // The delay field accepts 0 (test knob): the correction still lands, on
    // the next main-loop wake.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        let cfg = Config {
            flow_backspace_restore_delay_ms: 0,
            ..cfg
        };
        let (screen, e) = screen_of(mode, cfg, "chaoff\u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
        assert!(e.deferred_fix.is_none(), "{mode:?}");
    }
}

#[test]
fn target_the_fix_waits_for_the_backspace_release() {
    // Measured in the owner's log: injected Backspaces sent while the user's
    // Backspace is still down never arrive. The fix must hold until the
    // server reports the release.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    let cfg = Config {
        experimental_features: true,
        flow_per_key: true,
        flow_backspace_retrack: true,
        flow_backspace_restore: true,
        backspace_tone: BackspaceToneConfig::ToneFirst,
        mode: Mode::Flow.into(),
        ..Config::default()
    };
    e.apply_config(&cfg);
    e.set_client(Some("gtk4-im:probe".into()));
    e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
    e.set_uinput_available(true);
    for c in "chaoff".chars() {
        e.process_char(c);
    }
    let _ = e.backspace();
    assert!(e.deferred_fix.is_some(), "the fix must be scheduled");
    if let Some((deadline, _)) = e.deferred_fix.as_mut() {
        *deadline = Instant::now() - Duration::from_millis(1);
    }
    let actions = e.tick();
    assert!(
        actions.is_empty(),
        "no injection while the key is still down: {actions:?}"
    );
    assert!(e.deferred_fix.is_some(), "the fix must keep waiting");
    e.note_backspace_released();
    if let Some((deadline, _)) = e.deferred_fix.as_mut() {
        *deadline = Instant::now() - Duration::from_millis(1);
    }
    let actions = e.tick();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::InjectBackspaces(_))),
        "{actions:?}"
    );
    assert!(e.deferred_fix.is_none());
}

#[test]
fn target_the_restore_fires_only_on_the_undo_key() {
    // Live bug from the owner's log: `undo_in_word` stays set for the whole
    // word, so every later Backspace took the restore branch and re-marked the
    // word while the user was deleting it. Only the key that undid the mark
    // may restore.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        // The undo key is erased: restore.
        let (screen, e) = screen_of(mode, cfg.clone(), "chaoff\u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
        assert!(e.deferred_fix.is_none(), "{mode:?}");
        // A second Backspace is a delete: the first correction is dropped and
        // the screen only shortens.
        let (screen, _) = screen_of(mode, cfg.clone(), "chaoff\u{8}\u{8}");
        assert_eq!(screen, "cha", "{mode:?}");
        // Typing anything after the undo closes the restore window.
        let (screen, _) = screen_of(mode, cfg, "chaoffn\u{8}");
        assert_eq!(screen, "chaof", "{mode:?}");
    }
}

#[test]
fn target_backspace_on_a_digit_terminator_keeps_the_word() {
    // A digit ends the word for the Telex-family presets, so the Backspace
    // that erases it deletes a character: "Xóa dấu trước" must not read it as
    // an edit of the word and strip the tone (owner: `việt6` + BS gave
    // `viêt`, then one more character went with every later press).
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        let (screen, e) = screen_of(mode, cfg.clone(), "vieejt6\u{8}");
        assert_eq!(screen, "việt", "{mode:?}");
        assert_eq!(e.tail, "việt", "{mode:?}: tracking must match the screen");
        // The reported cycle: the digit typed again and erased again.
        let (screen, e) = screen_of(mode, cfg.clone(), "vieejt6\u{8}6\u{8}");
        assert_eq!(screen, "việt", "{mode:?}");
        assert_eq!(e.tail, "việt", "{mode:?}");
        // A digit terminator behaves exactly like a space one.
        let digit = screen_of(mode, cfg.clone(), "vieejt6\u{8}\u{8}").0;
        let space = screen_of(mode, cfg, "vieejt \u{8}\u{8}").0;
        assert_eq!(digit, space, "{mode:?}: {digit:?} vs {space:?}");
    }
}

#[test]
fn probe_bamboo_055_behaviour() {
    // What bamboo-core 0.5.5 changed, measured through the wrapper: the
    // refused-mark syllable and the extended f/j/w onsets.
    println!("\n== probe_bamboo_055_behaviour ==");
    for (method, keys) in [
        (Method::Telex, "ddd"),
        (Method::Telex, "too"),
        (Method::Telex, "chaof"),
        (Method::Telex, "chaoff"),
    ] {
        let mut w = BambooWrapper::new(
            method,
            bamboo_core::Config::builder().auto_correct(true).build(),
        );
        let mut trace = String::new();
        for c in keys.chars() {
            let o = w.process_char(c);
            trace.push_str(&format!(
                "\n  {c:?} ({o:?}) -> out={:?} raw={:?} valid={} undo={}",
                w.output(),
                w.raw(),
                w.is_valid(),
                w.undo_in_word()
            ));
        }
        println!("{method:?} {keys:?}:{trace}");
    }
}

#[test]
fn target_restore_never_resurrects_a_deleted_word() {
    // Guard: deleting into a committed word must NOT bring it back — `chào`
    // BS BS leaves `chà` and no composition.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        let cfg = Config {
            flow_backspace_retrack: true,
            ..cfg
        };
        let (screen, e) = screen_of(mode, cfg.clone(), "chaof ~\u{8}\u{8}");
        assert_eq!(screen, "chà", "{mode:?}");
        assert_eq!(e.tail, "chà", "{mode:?}");
        // The screen model may keep `chà` editable (the experiment re-tracks
        // every shortened screen), but the erased characters must never come
        // back.
        assert_ne!(e.tail, "chào", "{mode:?}: no resurrection");
        // Erasing a whole real word must not restore the region before it
        // either: the deleted region composed, so it was not litter.
        let (screen, e) = screen_of(mode, cfg, "chaof ~tungf ~\u{8}\u{8}\u{8}\u{8}\u{8}");
        assert_eq!(screen, "chào ", "{mode:?}");
        assert_eq!(e.tail, "", "{mode:?}");
        assert!(!e.wrapper.has_composition(), "{mode:?}");
    }
}
