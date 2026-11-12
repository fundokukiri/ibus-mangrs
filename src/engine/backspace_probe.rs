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
        Self { e, buf: Vec::new() }
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
                Action::Commit(t) | Action::CommitNow(t) => self.buf.extend(t.chars()),
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
        self.buf.iter().collect()
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
        if let Some((deadline, _)) = self.e.per_key_park.as_mut() {
            *deadline = Instant::now() - Duration::from_millis(1);
        }
        let actions = self.e.tick();
        self.apply(&actions);
        println!("  flush   {actions:?}");
        println!("       {}", self.state());
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
        if self.e.per_key_park.is_some() {
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

/// The experiment on: per-key display plus the Backspace switch, which is what
/// a user of the "Thử nghiệm" tab has.
fn tone(mode: Mode, tone: BackspaceToneConfig) -> (Mode, Config) {
    let (_, cfg) = per_key(mode);
    (
        mode,
        Config {
            flow_backspace_retrack: true,
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
#[ignore = "ToneFirst needs a rewrite (swallow or deferred delete): next stage"]
fn target_tonefirst_backspace_drops_the_tone() {
    // ToneFirst is the only setting that changes the screen text without a
    // plain delete: `tùng` -> `tung`. It needs a rewrite (delete + insert),
    // so it is the expensive row of the design.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::ToneFirst);
        let (screen, e) = screen_of(mode, cfg, "tungf ~\u{8}");
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
fn target_undone_mark_comes_back_on_backspace() {
    // Owner: type `chào`, press `f` again — the tone is undone and the raw
    // keys show (`chaof`); with the unikey walk Backspace gives `chào` back.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, e) = screen_of(mode, cfg, "chaoff\u{8}");
        assert_eq!(screen, "chào", "{mode:?}");
        assert_eq!(e.tail, "chào", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}");
    }
}

#[test]
fn target_undone_mark_follows_the_delete_mode() {
    // The other two settings delete a visible character: the screen shows the
    // raw keys (`chaof`), so Backspace leaves `chao`.
    for t in [
        BackspaceToneConfig::WholeChar,
        BackspaceToneConfig::ToneFirst,
    ] {
        for mode in [Mode::Flow, Mode::Turbo] {
            let (_, cfg) = tone(mode, t);
            let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}");
            assert_eq!(screen, "chao", "{mode:?} {t:?}");
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
        assert_eq!(screen, "chao", "{mode:?}");
        let (screen, _) = screen_of(mode, cfg, "chaoff\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}");
        assert_eq!(screen, "", "{mode:?}");
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
