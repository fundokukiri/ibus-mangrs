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
    }
}

// --- target semantics (red until the implementation round) -----------------
//
// These pin what the Flow/Turbo Backspace must do once the experiment grows
// the three behaviours the owner asked for: delete a mark, keep editing a
// mark, and restore a word's mark after a stray key is erased — like the
// inline modes. They read the SCREEN, not the action list, so the
// implementation is free to pick its transport (forward + deferred rewrite,
// or a narrowly scoped swallow). Every row was measured with the probes above;
// the numbers in the comments are today's output.

fn screen_of(mode: Mode, config: Config, script: &str) -> (String, Engine) {
    let mut sim = Sim::new(mode, config);
    sim.run(script);
    (sim.screen(), sim.e)
}

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
#[ignore = "target semantics for the Flow/Turbo mark-editing round"]
fn target_unikeywalk_backspace_keeps_the_word_live() {
    // Today (gate off): screen stays `tùng`, the composition is dead and the
    // next mark key starts a new word (`tùngs`). Target: the word survives
    // the Backspace, so a mark key still edits it.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, e) = screen_of(mode, cfg, "tungf ~\u{8}");
        assert_eq!(screen, "tùng", "{mode:?}");
        assert_eq!(e.tail, "tùng", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}: word must stay live");
    }
}

#[test]
#[ignore = "target semantics for the Flow/Turbo mark-editing round"]
fn target_wholechar_backspace_deletes_the_character() {
    // WholeChar keeps the screen model: `tùng` -> `tùn` (one screen
    // character), tone state intact. Today this equals the plain Backspace
    // but the composition dies.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::WholeChar);
        let (screen, e) = screen_of(mode, cfg, "tungf ~\u{8}");
        assert_eq!(screen, "tùn", "{mode:?}");
        assert_eq!(e.tail, "tùn", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}");
    }
}

#[test]
#[ignore = "target semantics for the Flow/Turbo mark-editing round"]
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
#[ignore = "target semantics for the Flow/Turbo mark-editing round"]
fn target_a_mark_key_edits_the_word_after_backspace() {
    // The whole point of keeping the word live: `tùng` BS `s` -> `túng`.
    // Today: `tùngs`.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::WholeChar);
        let (screen, _) = screen_of(mode, cfg, "tungf ~\u{8}~s");
        assert_eq!(screen, "tún", "{mode:?}");
    }
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = tone(mode, BackspaceToneConfig::UnikeyWalk);
        let (screen, _) = screen_of(mode, cfg, "tungf ~\u{8}\u{8}~s");
        assert_eq!(screen, "tú", "{mode:?}");
    }
}

#[test]
#[ignore = "target semantics for the Flow/Turbo mark-editing round"]
fn target_committed_word_comes_back_after_the_litter_is_erased() {
    // Owner trace: `chào` committed, a stray `f` litters the next region,
    // Backspace erases it — the word behind the caret must become editable
    // again, so `s` turns it into `cháo`. Today the Backspace leaves the
    // tracking empty and `s` becomes a new word (`chào s`).
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        let (screen, e) = screen_of(mode, cfg.clone(), "chaof ~f\u{8}");
        assert_eq!(screen, "chào ", "{mode:?}");
        assert_eq!(e.tail, "chào", "{mode:?}");
        assert!(e.wrapper.has_composition(), "{mode:?}: word must come back");
        let (screen, _) = screen_of(mode, cfg, "chaof ~f\u{8}~s");
        assert_eq!(screen, "cháo", "{mode:?}");
    }
}

#[test]
fn target_restore_never_resurrects_a_deleted_word() {
    // Guard (green today, must stay green): deleting into a committed word
    // must NOT bring it back — `chào` BS BS leaves `chà` and no composition.
    for mode in [Mode::Flow, Mode::Turbo] {
        let (_, cfg) = per_key(mode);
        let (screen, e) = screen_of(mode, cfg, "chaof ~\u{8}\u{8}");
        assert_eq!(screen, "chà", "{mode:?}");
        assert_eq!(e.tail, "chà", "{mode:?}");
        assert!(!e.wrapper.has_composition(), "{mode:?}");
    }
}
