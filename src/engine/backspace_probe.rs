//! Test-only probe: what a Backspace really does to a composition under each
//! "Backspace xóa dấu" mode, and what the Flow per-key screen model is
//! before and after it. Research round for the Flow mark-editing design;
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
            "screen={:?} tail={:?} park={:?} out={:?} comp={} raw={:?} fix={:?} ends_word={} plain={} landed={:?}",
            self.screen(),
            self.e.tail,
            self.e.per_key_park.as_ref().map(|(_, t)| t.clone()),
            self.e.wrapper.output(),
            self.e.wrapper.has_composition(),
            self.e.wrapper.raw(),
            self.e.deferred_fix.as_ref().map(|(_, t)| t.clone()),
            self.e.tail_ends_word,
            self.e.region_plain,
            self.e.landed_region
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
        // the Flow Backspace's deferred correction — and land the
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
            ..Config::default()
        },
    )
}

#[test]
fn probe_flow_backspace_screen_model() {
    println!("\n== probe_flow_backspace_screen_model ==");
    let (_, cfg) = per_key(Mode::Flow);
    probe_case("word end then BS", Mode::Flow, cfg.clone(), "tungf ~\u{8}");
    probe_case("BS twice", Mode::Flow, cfg.clone(), "tungf ~\u{8}\u{8}");
    probe_case(
        "mark key after BS",
        Mode::Flow,
        cfg.clone(),
        "tungf ~\u{8}~s",
    );
    probe_case("mid-word BS", Mode::Flow, cfg.clone(), "tieengs\u{8}");
    probe_case(
        "committed word, BS on space",
        Mode::Flow,
        cfg.clone(),
        "chaof ~\u{8}",
    );
    probe_case(
        "committed word, BS twice",
        Mode::Flow,
        cfg.clone(),
        "chaof ~\u{8}\u{8}",
    );
    probe_case(
        "T3: litter key then BS then s",
        Mode::Flow,
        cfg.clone(),
        "chaof ~f\u{8}s",
    );
    probe_case(
        "T3 variant: litter then BS twice",
        Mode::Flow,
        cfg.clone(),
        "chaof ~f\u{8}\u{8}",
    );
    probe_case("dd BS d", Mode::Flow, cfg.clone(), "dd\u{8}d");
    probe_case(
        "second word then BS",
        Mode::Flow,
        cfg.clone(),
        "tooi ~camr ~\u{8}",
    );
    probe_case(
        "url bypass then BS",
        Mode::Flow,
        cfg.clone(),
        "vi-mangrs.com ~\u{8}",
    );
    probe_case(
        "restore then letter",
        Mode::Flow,
        Config {
            experimental_features: true,
            ..Config::default()
        },
        "chaof ~f\u{8}q",
    );
    probe_case(
        "restore then whole-word delete",
        Mode::Flow,
        Config {
            experimental_features: true,
            ..Config::default()
        },
        "chaof ~tungf ~\u{8}\u{8}\u{8}\u{8}\u{8}",
    );
}

// --- target semantics of the Flow Backspace --------------------------------
//
// These pin what the Flow Backspace must do (standard code since the
// experimental switches died): delete a mark, keep editing a mark, and
// restore a word's mark after a stray key is erased — like the inline modes.
// They read the SCREEN, not the action list, so the transport stays free.

fn screen_of(mode: Mode, config: Config, script: &str) -> (String, Engine) {
    let mut sim = Sim::new(mode, config);
    sim.run(script);
    (sim.screen(), sim.e)
}

/// Per-key display plus the user's "Backspace xóa dấu" setting — the
/// standard Flow Backspace behavior.
fn tone(mode: Mode, tone: BackspaceToneConfig) -> (Mode, Config) {
    let (_, cfg) = per_key(mode);
    (
        mode,
        Config {
            backspace_tone: tone,
            ..cfg
        },
    )
}

#[test]
fn target_unikeywalk_backspace_keeps_the_word_live() {
    // Gate off: screen stays `tùng`, the composition is dead and the next
    // mark key starts a new word (`tùngs`).
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::UnikeyWalk);
    let (screen, e) = screen_of(Mode::Flow, cfg, "tungf ~\u{8}");
    assert_eq!(screen, "tùng");
    assert_eq!(e.tail, "tùng");
    assert!(e.wrapper.has_composition(), "word must stay live");
}

#[test]
fn target_wholechar_backspace_deletes_the_character() {
    // The first Backspace erases the terminator and gives the word back; the
    // second one follows the screen model (`tùng` -> `tùn`), tone intact.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::WholeChar);
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "tungf ~\u{8}");
    assert_eq!(screen, "tùng");
    assert_eq!(e.tail, "tùng");
    assert!(e.wrapper.has_composition());
    let (screen, e) = screen_of(Mode::Flow, cfg, "tungf ~\u{8}\u{8}");
    assert_eq!(screen, "tùn");
    assert_eq!(e.tail, "tùn");
    assert!(e.wrapper.has_composition());
}

#[test]
fn target_backspace_on_a_terminator_keeps_the_tone() {
    // Owner bug: `chào` + space + Backspace dropped the tone and then ate a
    // letter per press (`chao`, `cha`). Erasing the space belongs to the
    // region path, never to the tone-first rewrite.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, _) = screen_of(Mode::Flow, cfg.clone(), "chaof ~\u{8}");
    assert_eq!(screen, "chào");
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaof ~\u{8} \u{8}");
    assert_eq!(screen, "chào");
}

#[test]
fn target_a_later_letter_does_not_rearm_the_restore() {
    // Live bug from the owner's log: a letter typed after the undo made the
    // wrapper's diff read bamboo's auto_correct revert as another undo, which
    // re-armed the restore window — the next ordinary Backspace then injected
    // deletes and the word lost characters it should not have.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaoffg\u{8}");
    assert_eq!(screen, "chaof");
}

#[test]
fn target_tonefirst_backspace_drops_the_tone() {
    // ToneFirst is the one setting that changes the screen text without a
    // plain delete: `tùng` -> `tung`, like the inline modes. The first
    // Backspace erases the space after the committed word, the second drops
    // the tone; the rewrite is deferred past the key release.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, e) = screen_of(Mode::Flow, cfg, "tungf ~\u{8}\u{8}");
    assert_eq!(screen, "tung");
    assert_eq!(e.tail, "tung");
    assert!(e.wrapper.has_composition());
}

#[test]
fn target_a_mark_key_edits_the_word_after_backspace() {
    // The whole point of keeping the word live: after the terminator erase the
    // mark key edits the word (`tùng` + `s` -> `túng`); after the second
    // Backspace the shortened screen keeps composing (`tùn` + `s` -> `tún`).
    // Gate off: `tùngs` / `tùns`.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::WholeChar);
    let (screen, _) = screen_of(Mode::Flow, cfg, "tungf ~\u{8}~s");
    assert_eq!(screen, "túng");

    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::UnikeyWalk);
    let (screen, _) = screen_of(Mode::Flow, cfg, "tungf ~\u{8}\u{8}~s");
    assert_eq!(screen, "tún");
}

#[test]
fn target_committed_word_comes_back_after_the_litter_is_erased() {
    // Owner trace, one-Backspace-one-character: `chào` committed, a stray `f`
    // litters the next region. The first Backspace erases exactly the stray
    // key (`chào ` — no retraction); the second deletes the terminator and
    // the word behind it comes back to the composition unchanged; `s` then
    // turns it into `cháo`.
    let (_, cfg) = per_key(Mode::Flow);
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "chaof ~f\u{8}");
    assert_eq!(screen, "chào ");
    assert!(
        !e.wrapper.has_composition(),
        "the word is not re-entered yet"
    );
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "chaof ~f\u{8}\u{8}");
    assert_eq!(screen, "chào");
    assert_eq!(e.tail, "chào");
    assert!(e.wrapper.has_composition(), "word must come back");
    let (screen, _) = screen_of(Mode::Flow, cfg.clone(), "chaof ~f\u{8}\u{8}~s");
    assert_eq!(screen, "cháo");
    // Same when the stray key already landed on screen (the batch flushed
    // before the Backspace).
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaof ~f~\u{8}\u{8}");
    assert_eq!(screen, "chào");
}

#[test]
fn target_boundary_backspace_leaves_the_word_unchanged() {
    // The owner's table: a committed word, a terminator, a plain one-letter
    // stray, then two Backspaces that cross the boundary. Each Backspace
    // deletes exactly one character; the word behind the terminator comes
    // back unchanged — its tone included.
    for (word, keys, stray) in [
        ("chịu", "chiuj", "t"),
        ("ê", "ee", "a"),
        ("â", "aa", "e"),
        ("ơ", "ow", "k"),
        ("ớ", "ows", "a"),
        ("ô", "oo", "k"),
        ("ắ", "aws", "a"),
        ("ă", "aw", "a"),
        ("ý", "ys", "a"),
        ("ù", "uf", "a"),
    ] {
        let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
        let script = format!("{keys}~ ~{stray}~\u{8}\u{8}~");
        let (screen, e) = screen_of(Mode::Flow, cfg, &script);
        assert_eq!(screen, word, "{keys}~ ~{stray}~ BS BS");
        assert_eq!(e.tail, word, "{keys}: the word is tracked again");
        assert!(
            e.wrapper.has_composition(),
            "{keys}: the word is live again"
        );
    }
}

#[test]
fn target_multi_key_stray_backspaces_reenter_the_word() {
    // The same boundary over multi-letter strays: the Backspace count the
    // user pressed is the character count deleted (`thùng kia` + 4
    // Backspaces is `thùng`, not a tone-stripped `thung`).
    for (word, keys, stray, backspaces) in [
        ("thùng", "thungf", "kia", 4),
        ("vào", "vaof", "tk", 3),
        ("mệt", "meejt", "qua", 4),
        ("giữa", "giuwax", "ao", 3),
        ("mèo", "meof", "o", 2),
    ] {
        let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
        let script = format!("{keys}~ ~{stray}~{}~", "\u{8}".repeat(backspaces));
        let (screen, _) = screen_of(Mode::Flow, cfg, &script);
        assert_eq!(screen, word, "{keys}~ ~{stray}~ {backspaces} BS");
    }
}

#[test]
fn target_backspaces_into_the_reentered_word_delete_characters() {
    // Past the boundary the word is live and toneless (`lương`): the next
    // Backspaces delete visible characters, one per key.
    for (want, keys, stray, backspaces) in [("lươ", "luowng", "t", 4), ("thươ", "thuwong", "t", 4)]
    {
        let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
        let script = format!("{keys}~ ~{stray}~{}~", "\u{8}".repeat(backspaces));
        let (screen, _) = screen_of(Mode::Flow, cfg, &script);
        assert_eq!(screen, want, "{keys}~ ~{stray}~ {backspaces} BS");
    }
}

#[test]
fn target_marked_stray_reenters_on_the_terminator_backspace() {
    // A stray that composed (the owner's tone-bearing-stray class) is erased
    // by the first Backspace; the second deletes the terminator and re-enters
    // the word; a third pops the tone — ToneFirst on a live word, the
    // setting's own contract.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "aa~ ~is~\u{8}\u{8}~");
    assert_eq!(screen, "â");
    assert!(e.wrapper.has_composition(), "the word is live again");
    let (screen, _) = screen_of(Mode::Flow, cfg, "aa~ ~is~\u{8}\u{8}\u{8}~");
    assert_eq!(screen, "a", "ToneFirst pops on the live word");
}

#[test]
fn target_fundo_backspace_eats_no_extra_character() {
    // Owner trace: `tùng` + the mark key again (the undo shows `tungf`), a
    // space, two Backspaces = `tung` — the terminator and the raw span's `f`,
    // nothing more. The re-track of the post-delete screen must not be popped
    // a second time.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, e) = screen_of(Mode::Flow, cfg, "tung~f~f~ ~\u{8}\u{8}~");
    assert_eq!(screen, "tung");
    assert_eq!(e.tail, "tung");
}

#[test]
fn target_terminator_backspace_reenters_a_committed_word() {
    // No-regression shape: the terminator alone between the caret and the
    // word — one Backspace re-enters the word, tone kept.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, e) = screen_of(Mode::Flow, cfg, "chiuj~ ~\u{8}~");
    assert_eq!(screen, "chịu");
    assert!(e.wrapper.has_composition());
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
    // must produce what Preedit / Surrounding produce with the
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
        let (_, cfg) = tone(Mode::Flow, t);
        let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "chaoff\u{8}");
        assert_eq!(screen, inline, "{t:?}");
        assert_eq!(e.tail, inline, "{t:?}");
        assert!(e.deferred_fix.is_none(), "fix must have landed");
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
        let (_, cfg) = tone(Mode::Flow, t);
        let (screen, _) = screen_of(Mode::Flow, cfg, "chaoff\u{8}");
        assert_eq!(screen, expected, "{t:?}");
    }
}

#[test]
fn target_repeated_backspace_after_an_undone_mark_never_duplicates() {
    // The live bug: `chaof` + Backspace x5 produced `chaofào`, `chaofàoao`, …
    // Every step must only shorten the screen.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::UnikeyWalk);
    let (screen, _) = screen_of(Mode::Flow, cfg.clone(), "chaoff\u{8}\u{8}");
    assert_eq!(screen, "cha");
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaoff\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}");
    assert_eq!(screen, "");
}

#[test]
fn target_typing_before_the_fix_lands_keeps_the_mark() {
    // The correction is only deferred, not lost: a tone key typed immediately
    // after the Backspace collapses both into one rewrite.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::UnikeyWalk);
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaoff\u{8}s");
    assert_eq!(screen, "cháo");
}

#[test]
fn target_tonefirst_fold_keeps_every_screen_char() {
    // The fold family under ToneFirst: the pop leaves the wrapper one
    // character longer than the screen (`tung` vs `tùn`), so folding the
    // pending tone fix into a fast key's batch must not apply the wrapper's
    // delta to the screen — that erased the `t` (`úng`). The folded rewrite
    // lands the composed word whole.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, e) = screen_of(Mode::Flow, cfg, "tungf ~\u{8}\u{8}s");
    assert_eq!(screen, "túng");
    assert_eq!(e.tail, "túng");
    assert!(e.deferred_fix.is_none(), "the fix folded into the key");
}

#[test]
fn target_tonefirst_fold_after_an_undone_mark() {
    // The equal-length fold sibling under ToneFirst: the undo pop keeps
    // wrapper output and screen the same length, and a fast tone key folds
    // the mark restore into its batch (`cháo`, like the UnikeyWalk pin).
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaoff\u{8}s");
    assert_eq!(screen, "cháo");
}

#[test]
fn target_restore_runs_with_a_zero_delay() {
    // The delay field accepts 0 (test knob): the correction still lands, on
    // the next main-loop wake.
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let cfg = Config {
        flow_backspace_restore_delay_ms: 0,
        ..cfg
    };
    let (screen, e) = screen_of(Mode::Flow, cfg, "chaoff\u{8}");
    assert_eq!(screen, "chào");
    assert!(e.deferred_fix.is_none());
}

#[test]
fn target_the_fix_waits_for_the_backspace_release() {
    // Measured in the owner's log: injected Backspaces sent while the user's
    // Backspace is still down never arrive. The fix must hold until the
    // server reports the release.
    let mut e = Engine::new(Method::Telex, Mode::Flow);
    let cfg = Config {
        experimental_features: true,
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
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    // The undo key is erased: restore.
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "chaoff\u{8}");
    assert_eq!(screen, "chào");
    assert!(e.deferred_fix.is_none());
    // A second Backspace is a delete: the first correction is dropped and
    // the screen only shortens.
    let (screen, _) = screen_of(Mode::Flow, cfg.clone(), "chaoff\u{8}\u{8}");
    assert_eq!(screen, "cha");
    // Typing anything after the undo closes the restore window.
    let (screen, _) = screen_of(Mode::Flow, cfg, "chaoffn\u{8}");
    assert_eq!(screen, "chaof");
}

#[test]
fn target_backspace_on_a_digit_terminator_keeps_the_word() {
    // A digit ends the word for the Telex-family presets, so the Backspace
    // that erases it deletes a character: "Xóa dấu trước" must not read it as
    // an edit of the word and strip the tone (owner: `việt6` + BS gave
    // `viêt`, then one more character went with every later press).
    let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "vieejt6\u{8}");
    assert_eq!(screen, "việt");
    assert_eq!(e.tail, "việt", "tracking must match the screen");
    // The reported cycle: the digit typed again and erased again.
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "vieejt6\u{8}6\u{8}");
    assert_eq!(screen, "việt");
    assert_eq!(e.tail, "việt");
    // A digit terminator behaves exactly like a space one.
    let digit = screen_of(Mode::Flow, cfg.clone(), "vieejt6\u{8}\u{8}").0;
    let space = screen_of(Mode::Flow, cfg, "vieejt \u{8}\u{8}").0;
    assert_eq!(digit, space, "{digit:?} vs {space:?}");
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
    // Guard: deleting into a committed word must NOT bring it back — with
    // the standard ToneFirst handling `chào` BS BS drops the tone (`chao`,
    // the inline result), never resurrects `chào`.
    let (_, cfg) = per_key(Mode::Flow);
    let (screen, e) = screen_of(Mode::Flow, cfg.clone(), "chaof ~\u{8}\u{8}");
    assert_eq!(screen, "chao");
    assert_eq!(e.tail, "chao");
    assert_ne!(e.tail, "chào", "no resurrection");
    // Erasing a whole real word must not restore the region before it
    // either: the deleted region composed, so it was not litter.
    let (screen, e) = screen_of(Mode::Flow, cfg, "chaof ~tungf ~\u{8}\u{8}\u{8}\u{8}\u{8}");
    assert_eq!(screen, "chào ");
    assert_eq!(e.tail, "");
    assert!(!e.wrapper.has_composition());
}

#[test]
fn probe_owner_repro_table() {
    // The owner's live corruption table (18/11), typed verbatim: a committed
    // word, a terminator, a plain new word, then Backspaces back onto the old
    // word. Prints the measured per-key trace and asserts nothing — the pin
    // set for the fix comes from here, not from invented shapes.
    // Script conventions: `~` is the main-loop wake (flushes the per-key
    // batch, fires a due deferred fix), `\u{8}` is Backspace; consecutive
    // `\u{8}` run inside the restore window with no wake, like fast typing.
    // Telex raw keys, tone key last (the file's WORDS convention). The
    // "owner-observed" column is the pre-Task-16 build; chains list their
    // intermediate screens — the summary only compares finals.
    let cases: &[(&str, &str, &str, &str)] = &[
        // (label, script, correct screen, owner-observed)
        // Telex tone keys: s=sắc, f=huyền, j=nặng, r=hỏi (the WORDS table's
        // convention: the tone key last).
        ("chịu+t 2bs", "chiuj~ ~t~\u{8}\u{8}~", "chịu", "chịiu"),
        ("ê+a 2bs", "ee~ ~a~\u{8}\u{8}~", "ê", "êe"),
        ("â+e 2bs", "aa~ ~e~\u{8}\u{8}~", "â", "âa"),
        ("ơ+k 2bs", "ow~ ~k~\u{8}\u{8}~", "ơ", "ơo"),
        ("ớ+a 2bs", "ows~ ~a~\u{8}\u{8}~", "ớ", "ớơ"),
        ("ô+k 2bs", "oo~ ~k~\u{8}\u{8}~", "ô", "ôo"),
        ("ắ+a 2bs", "aws~ ~a~\u{8}\u{8}~", "ắ", "ắă"),
        ("ă+a 2bs", "aw~ ~a~\u{8}\u{8}~", "ă", "ăa"),
        ("ý+a 2bs", "ys~ ~a~\u{8}\u{8}~", "ý", "ýy"),
        ("ù+a 2bs", "uf~ ~a~\u{8}\u{8}~", "ù", "ùu"),
        (
            "ố chain",
            "oos~ ~a~\u{8}\u{8}\u{8}\u{8}\u{8}~",
            "",
            "ốô→ốo→ố→",
        ),
        (
            "ớn chain",
            "owns~ ~a~\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}~",
            "",
            "ớơn→ớo→ớ→",
        ),
        ("tĩnh+a 2bs", "tinhx~ ~a~\u{8}\u{8}~", "tĩnh", "tĩinh"),
        (
            "thùng+kia 4bs",
            "thungf~ ~kia~\u{8}\u{8}\u{8}\u{8}~",
            "thùng",
            "thùung",
        ),
        ("vào+tk 3bs", "vaof~ ~tk~\u{8}\u{8}\u{8}~", "vào", "vàao"),
        ("mèo+o 2bs", "meof~ ~o~\u{8}\u{8}~", "mèo", "mèeo"),
        (
            "mệt+qua 4bs",
            "meejt~ ~qua~\u{8}\u{8}\u{8}\u{8}~",
            "mệt",
            "mệêt",
        ),
        (
            "giữa+ao 3bs",
            "giuwax~ ~ao~\u{8}\u{8}\u{8}~",
            "giữa",
            "giữưa",
        ),
        ("lưỡng+a 2bs", "luowngx~ ~a~\u{8}\u{8}~", "lưỡng", "luỡơng"),
        (
            "lương+t 4bs",
            "luowng~ ~t~\u{8}\u{8}\u{8}\u{8}~",
            "lươ",
            "lươo",
        ),
        (
            "thương+t 4bs",
            "thuwong~ ~t~\u{8}\u{8}\u{8}\u{8}~",
            "thươ",
            "thươo",
        ),
        ("hữu+o 2bs", "huwux~ ~o~\u{8}\u{8}~", "hữu", "hữưu"),
        ("hữu+o 4bs", "huwux~ ~o~\u{8}\u{8}\u{8}\u{8}~", "h", "hữu"),
        (
            "thông chain",
            "thoong~ ~o~\u{8}\u{8}~\u{8}\u{8}~",
            "thô",
            "thông→thôo",
        ),
        ("hâm chain", "haam~ ~o~\u{8}\u{8}~\u{8}~", "hâ", "hâm→hâa"),
        ("hăm chain", "hawm~ ~o~\u{8}\u{8}~\u{8}~", "hă", "hăm→hăa"),
        ("êm chain", "eem~ ~o~\u{8}\u{8}~\u{8}~", "ê", "êm→êe"),
        ("ôm chain", "oom~ ~o~\u{8}\u{8}~\u{8}~", "ô", "ôm→ôo"),
        ("â+í 3bs", "aa~ ~is~\u{8}\u{8}\u{8}~", "a", "â"),
        ("mèo+ơ 3bs", "meof~ ~ow~\u{8}\u{8}\u{8}~", "meo", "mèo"),
        (
            "thưởng+tí chain",
            "thuwongf~ ~tis~\u{8}\u{8}\u{8}\u{8}~\u{8}\u{8}~",
            "thươ",
            "thưởng→thưở",
        ),
        ("tùng f-undo 2bs", "tung~f~f~ ~\u{8}\u{8}~", "tung", "tun"),
    ];
    println!("\n== probe_owner_repro_table: owner table through the current build ==");
    let mut mismatches = 0;
    for (label, script, correct, observed) in cases {
        println!(
            "\n--- {label}: script={script:?} correct={correct:?} owner-observed={observed:?}"
        );
        let (_, cfg) = tone(Mode::Flow, BackspaceToneConfig::ToneFirst);
        let mut sim = Sim::new(Mode::Flow, cfg);
        sim.run(script);
        let fix = sim.e.deferred_fix.as_ref().map(|(_, text)| text.clone());
        let verdict = if sim.screen() == *correct {
            "MATCHES-CORRECT"
        } else {
            mismatches += 1;
            "MISMATCH"
        };
        println!(
            "   FINAL screen={:?} tail={:?} deferred_fix={fix:?} [{verdict}]",
            sim.screen(),
            sim.e.tail
        );
    }
    println!(
        "\n== summary: {mismatches}/{} finals mismatch the correct screen ==\n",
        cases.len()
    );
}
