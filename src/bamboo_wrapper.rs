//! The only module that talks to `bamboo-core` directly.
//!
//! Keeping the dependency here means an upstream API change is a one-file
//! edit. `bamboo-core`'s `EditDelta` borrows the engine, so every method
//! below copies the delta into an owned `Edit` before returning.

use crate::debug_log;
use bamboo_core::{Charset as BambooCharset, Config, Engine, InputMethod, Mode};

/// One keystroke's effect on the text already on screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Edit {
    /// Number of Unicode scalar values to delete before inserting.
    pub delete_chars: usize,
    /// Text to insert after the deletion.
    pub insert: String,
}

impl Edit {
    pub fn is_empty(&self) -> bool {
        self.delete_chars == 0 && self.insert.is_empty()
    }
}

/// Which input method the engine is configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    #[default]
    Telex,
    Vni,
    Viqr,
    MsLayout,
    Telex2,
    TelexVni,
    TelexVniViqr,
    VniFrench,
    TelexW,
}

impl Method {
    fn to_bamboo(self) -> InputMethod {
        match self {
            Method::Telex => InputMethod::telex(),
            Method::Vni => InputMethod::vni(),
            Method::Viqr => InputMethod::viqr(),
            Method::MsLayout => InputMethod::microsoft_layout(),
            Method::Telex2 => InputMethod::telex_2(),
            Method::TelexVni => InputMethod::telex_vni(),
            Method::TelexVniViqr => InputMethod::telex_vni_viqr(),
            Method::VniFrench => InputMethod::vni_french_layout(),
            Method::TelexW => InputMethod::telex_w(),
        }
    }
}

/// Output charset for committed text. `bamboo-core` ships all of these;
/// the menu/config expose them under short names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    #[default]
    Unicode,
    Nfd,
    Tcvn3,
    Vni,
    Viqr,
    Viscii,
    Vps,
    Cp1258,
    Bkhcm,
    VietwareX,
    VietwareFull,
}

impl Charset {
    fn to_bamboo(self) -> BambooCharset {
        match self {
            Charset::Unicode => BambooCharset::Unicode,
            Charset::Nfd => BambooCharset::UnicodeCompound,
            Charset::Tcvn3 => BambooCharset::Tcvn3,
            Charset::Vni => BambooCharset::VniWindows,
            Charset::Viqr => BambooCharset::Viqr,
            Charset::Viscii => BambooCharset::Viscii,
            Charset::Vps => BambooCharset::Vps,
            Charset::Cp1258 => BambooCharset::Windows1258,
            Charset::Bkhcm => BambooCharset::Bkhcm1,
            Charset::VietwareX => BambooCharset::VietwareX,
            Charset::VietwareFull => BambooCharset::VietwareFull,
        }
    }
}

/// When a plain `w` turns into `ư`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum W2u {
    #[default]
    Disabled,
    NonStart,
    Everywhere,
}

/// When brackets type `ơ` and `ư`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Bracket {
    #[default]
    Disabled,
    NonStart,
    Everywhere,
}

/// Encode committed text. Legacy encodings are not one char per char (VNI's
/// `â` is `aâ`), so callers must count characters *after* encoding.
pub fn encode(charset: Charset, text: &str) -> String {
    bamboo_core::encode_charset(charset.to_bamboo(), text).into_owned()
}

/// What the caller should do with a keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The key was consumed and produced this edit.
    Edit(Edit),
    /// The key was not Vietnamese input; pass it to the application unchanged.
    PassThrough,
    /// The key ended the composition (space, punctuation). Carries the word
    /// that was being composed, the raw keys that produced it, and how the
    /// result scores: `valid` is a complete Vietnamese syllable, `valid_prefix`
    /// also accepts a still-extendable one (`đuo`). Both are captured before
    /// the composition is cleared, so a caller that has not committed the word
    /// yet (preedit mode) can still decide.
    Finish {
        word: String,
        raw: String,
        valid: bool,
        valid_prefix: bool,
    },
}

pub struct BambooWrapper {
    engine: Engine,
    method: Method,
    config: Config,
    /// Whether Vietnamese processing is on. English mode forwards every key.
    enabled: bool,
    /// Raw keys of the current word, tracked for macros and auto-restore.
    raw: String,
    /// Composition display override while bamboo's own output is poisoned.
    output_override: Option<String>,
    /// A mark was undone in the current word (`tô` + 'o' → `too`). Bamboo's
    /// internal state keeps the undone transformation hidden and resurfaces
    /// it on the next few keys ("too" + 'l' → "toool"), so later
    /// implausible deltas in this word fall back to plain appends.
    undo_in_word: bool,
    /// The current word is a Vietnamese abbreviation that bamboo's
    /// auto_correct reverted and the wrapper re-fixed (`ddc` → `đc`): not a
    /// valid syllable, so the word-end auto-restore must leave it alone.
    abbrev_fixed: bool,
    /// `abbreviations` config: whether reverted non-syllable compositions
    /// get the dd→đ transform re-applied (đc, đk, đt, đg).
    abbrev: bool,
    /// The current word's composition was kept over bamboo's raw revert (a
    /// VNI-family digit could not extend a complete syllable, `hạo` + '6'):
    /// bamboo sits in its raw-append bypass, the visible output is the kept
    /// composition plus literal keys, and the word counts as valid so the
    /// word-end auto-restore cannot resurrect the consumed digits. Cleared
    /// with the other per-word flags on word end, reset, recomposition.
    kept_composition: bool,
    /// True while a word is being composed (raw belongs to it).
    composing: bool,
    /// The composition output after the last operation (process, backspace,
    /// rebuild). `bamboo-core`'s `EditDelta` is relative to its internal
    /// `prev_preedit`, which `remove_last_output_char` does not refresh — so
    /// a keystroke right after a backspace would report a delete for a
    /// character that is already gone (the "backspace then type eats a
    /// letter" bug). We compute the delta ourselves against `last_output`.
    last_output: String,
}

impl BambooWrapper {
    pub fn new(method: Method, config: Config) -> Self {
        Self {
            engine: Engine::with_config(method.to_bamboo(), config),
            method,
            config,
            enabled: true,
            raw: String::new(),
            output_override: None,
            undo_in_word: false,
            abbrev_fixed: false,
            abbrev: true,
            kept_composition: false,
            composing: false,
            last_output: String::new(),
        }
    }

    /// Returns whether the engine was rebuilt (the composition was reset).
    pub fn set_method(&mut self, method: Method) -> bool {
        if method != self.method {
            self.method = method;
            self.engine = Engine::with_config(method.to_bamboo(), self.config);
            self.raw.clear();
            self.last_output.clear();
            self.output_override = None;
            self.undo_in_word = false;
            self.kept_composition = false;
            self.composing = false;
            true
        } else {
            false
        }
    }

    /// Returns whether the engine was rebuilt (the composition was reset).
    pub fn set_config(&mut self, config: Config) -> bool {
        if config != self.config {
            self.config = config;
            self.engine = Engine::with_config(self.method.to_bamboo(), config);
            self.raw.clear();
            self.last_output.clear();
            self.output_override = None;
            self.undo_in_word = false;
            self.kept_composition = false;
            self.composing = false;
            true
        } else {
            false
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.engine.reset();
            self.raw.clear();
            self.last_output.clear();
            self.output_override = None;
            self.undo_in_word = false;
            self.kept_composition = false;
            self.composing = false;
        }
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The active input method, for method-specific decisions upstream.
    pub fn method(&self) -> Method {
        self.method
    }

    /// Load raw keystrokes back into the composition (the idle-commit
    /// re-entry): the engine replays them so tone keys can still transform
    /// the word — `rebuild_from_text` cannot, it builds marks without
    /// keystrokes and a following tone key starts a fresh word.
    pub fn replay_raw(&mut self, raw: &str) {
        self.engine.reset();
        self.raw = raw.to_string();
        self.composing = !raw.is_empty();
        self.undo_in_word = false;
        self.abbrev_fixed = false;
        self.kept_composition = false;
        self.output_override = None;
        self.last_output.clear();
        let mut output = String::new();
        for r in raw.chars() {
            output = self.feed_compose(r);
        }
        self.last_output = output;
    }

    /// `abbreviations` config: keep the đ in reverted non-syllable
    /// compositions (`ddc` → `đc`). See the reversion fix in `process_char`.
    pub fn set_abbreviations(&mut self, on: bool) {
        self.abbrev = on;
    }

    /// Feed one in-word key to bamboo-core and return the visible output.
    ///
    /// Shared by `process_char`, the `backspace` recomposition and
    /// `replay_raw`: every path that replays raw keys must run the same
    /// guards, or a word kept in one path resurrects its raw keys through
    /// another (Backspace, the idle-commit re-entry).
    fn feed_compose(&mut self, c: char) -> String {
        // While a kept composition is live, bamboo-core sits in its raw
        // append bypass for this word: the key lands literally on the kept
        // word, and the engine only follows to stay in sync.
        if self.kept_composition {
            let _ = self.engine.process_key_edit(c, Mode::Vietnamese);
            let kept = format!("{}{}", self.output(), c);
            self.output_override = Some(kept.clone());
            return kept;
        }
        let prev = self.output();
        let prev_valid_word = !prev.is_empty() && self.engine.is_valid_word();
        let _ = self.engine.process_key_edit(c, Mode::Vietnamese);
        let mut new_output = self.engine.output_str().to_string();
        // bamboo's auto_correct reverts a composition that is not a valid
        // Vietnamese syllable back to the raw keys — and the common
        // abbreviations are not syllables: typing `ddc` ("đc" for "được")
        // had the dd→đ transform undone and came out as "ddc". Vietnamese
        // abbreviations keep their đ: when the raw keys start with the `dd`
        // rule and the composition was reverted to exactly those raw keys,
        // re-apply the transform. Real words ("ddau" → "đau") never hit
        // this — their compositions are valid and never reverted.
        if self.abbrev && new_output == self.raw && self.raw.starts_with("dd") && self.raw.len() > 2
        {
            debug_log::log("abbreviation reverted by auto_correct → keep the đ");
            new_output = format!("đ{}", &self.raw[2..]);
            self.output_override = Some(new_output.clone());
            self.abbrev_fixed = true;
        }
        // bamboo's auto_correct reverts the WHOLE composition to the raw
        // keys when the total stops being a valid prefix ("hạo" + '6'). The
        // rule layer already refused that key and appended it literally, so
        // the revert resurrects the digits consumed as marks ("hao123456").
        // A complete valid syllable before the key keeps its composition and
        // the key lands literally; a partial one keeps the revert — that is
        // what still returns Telex "street" to its raw keys. Only the
        // digit-mark presets are covered: Telex letter tone keys keep the
        // revert, which English typing leans on.
        if !self.abbrev_fixed
            && new_output == self.raw
            && prev_valid_word
            && self.is_digit_mark_key(c)
            && format!("{prev}{c}") != new_output
        {
            debug_log::log("auto_correct reverted a complete syllable → keep it, key literal");
            new_output = format!("{prev}{c}");
            self.output_override = Some(new_output.clone());
            self.kept_composition = true;
        }
        new_output
    }

    /// Whether `c` is a digit mark key of the VNI-family presets — the keys
    /// that reach bamboo-core as in-word marks there.
    fn is_digit_mark_key(&self, c: char) -> bool {
        c.is_ascii_digit()
            && matches!(
                self.method,
                Method::Vni | Method::TelexVni | Method::TelexVniViqr
            )
    }

    /// Feed one character. The returned `Edit` describes how to rewrite the
    /// text already committed to the application.
    pub fn process_char(&mut self, c: char) -> Outcome {
        // Characters that never take part in Vietnamese words end the word.
        // bamboo-core would keep them in its buffer, which would make the
        // next edit delete them from the application. With a bracket mode on,
        // `[` `]` `{` `}` type vowels instead of ending the word.
        let bracket_key = matches!(c, '[' | ']' | '{' | '}')
            && self.config.bracket_mode != bamboo_core::BracketMode::Disabled;
        // Digits end the word for methods that do not use them as marks.
        // (bamboo-core clears its whole composition on a digit in Telex —
        // feeding it there deleted the word in the application. The Telex 2,
        // Telex W and French-VNI presets clear the same way.) The VNI-based
        // presets keep every digit in the composition — marks there, literal
        // else — so they must all reach bamboo-core.
        let digit_key = c.is_ascii_digit()
            && matches!(
                self.method,
                Method::Telex | Method::Viqr | Method::Telex2 | Method::VniFrench | Method::TelexW
            );
        // VIQR marks are punctuation keys: they must reach bamboo-core
        // instead of ending the word (unikey convention). `0` is VIQR's
        // remove-tone key — the only digit the method uses as a mark, so it
        // is exempt from the digit terminator too.
        let viqr_mark_key = match self.method {
            Method::Viqr => {
                matches!(c, '\'' | '`' | '?' | '~' | '.' | '^' | '+' | '*' | '(') || c == '0'
            }
            // The combined preset drops `+` (literal there, probed) and marks
            // `.` (nặng) — which pure Telex keeps as a sentence separator.
            Method::TelexVniViqr => matches!(c, '\'' | '`' | '?' | '~' | '.' | '^' | '*' | '('),
            // The French-layout VNI preset marks on the AZERTY digit row
            // characters it actually implements: huyền hỏi ngã nặng.
            Method::VniFrench => matches!(c, '"' | '\'' | '(' | '-'),
            _ => false,
        };
        // The Microsoft layout types ư/ơ only through the bracket keys, so
        // they must reach bamboo-core for this method regardless of the
        // bracket option.
        let ms_bracket_key = self.method == Method::MsLayout && matches!(c, '[' | ']' | '{' | '}');
        // Telex 2 is the same idea with only `[`/`]` (its `}` stays a brace).
        let telex2_bracket_key = self.method == Method::Telex2 && matches!(c, '[' | ']');
        if (is_word_terminator(c) || digit_key)
            && !bracket_key
            && !viqr_mark_key
            && !ms_bracket_key
            && !telex2_bracket_key
        {
            // The visible composition, not bamboo's internal string: after an
            // undone mark its state is poisoned ("tool" is "toool" inside)
            // and committing that would re-add the hidden character at the
            // word boundary ("tool" + space -> "toool").
            let word = self.output();
            // Both verdicts are read here: `engine.reset()` below clears the
            // composition, so a caller can no longer ask. `abbrev_fixed` counts
            // as valid, exactly like a real syllable — an abbreviation must not
            // be auto-restored to its raw keys.
            let valid = self.is_valid();
            let valid_prefix = self.is_valid_prefix();
            let raw = std::mem::take(&mut self.raw);
            self.composing = false;
            self.engine.reset();
            self.last_output.clear();
            self.output_override = None;
            self.abbrev_fixed = false;
            self.kept_composition = false;
            return Outcome::Finish {
                word,
                raw,
                valid,
                valid_prefix,
            };
        }

        // First key of the next word clears the leftover raw keys — and the
        // undo flag, which describes exactly one word.
        if !self.composing {
            self.raw.clear();
            self.composing = true;
            self.undo_in_word = false;
            self.abbrev_fixed = false;
            self.kept_composition = false;
        }
        self.raw.push(c);

        if !self.enabled {
            return Outcome::PassThrough;
        }

        let mut new_output = self.feed_compose(c);
        // The composition output can never be longer than the raw keystrokes
        // (marks combine characters, they never add). When it is, bamboo's
        // internal state is poisoned by an undone mark — "too" (undone "tô")
        // + 'l' resurfaces a hidden extra 'o' as "toool" — and replaying the
        // delta would corrupt the application text. Forward raw instead; the
        // resync below keeps the tracker on the raw text.
        if new_output.chars().count() > self.raw.chars().count() {
            debug_log::log(&format!(
                "bamboo output longer than raw keys ({} > {}) → rebuild",
                new_output.chars().count(),
                self.raw.chars().count()
            ));
            self.engine.reset();
            for r in self.raw.chars() {
                let _ = self.engine.process_key_edit(r, Mode::Vietnamese);
            }
            new_output = self.engine.output_str().to_string();
        }
        let mut edit = self.delta_from_last(&new_output);
        // After an undo, one keypress growing the text by more than one
        // character means the hidden state resurfaced ("too" + 'l' →
        // "toool"). Try a clean rebuild from the raw keys; if that is
        // poisoned too, fall back to a plain append of the raw key, which is
        // exactly what the user sees on screen.
        if self.undo_in_word && edit.insert.chars().count() as i64 - edit.delete_chars as i64 > 1 {
            debug_log::log(&format!(
                "bamboo delta grows {} chars net → rebuild",
                edit.insert.chars().count() as i64 - edit.delete_chars as i64
            ));
            let app_text = self.last_output.clone();
            self.engine.reset();
            for r in self.raw.chars() {
                let _ = self.engine.process_key_edit(r, Mode::Vietnamese);
            }
            let rebuilt = self.engine.output_str().to_string();
            let retry = self.delta_between(&app_text, &rebuilt);
            if retry.insert.chars().count() as i64 - retry.delete_chars as i64 <= 1 {
                self.last_output = rebuilt.clone();
                self.output_override = Some(rebuilt);
                edit = retry;
            } else {
                let mut app = app_text;
                app.push(c);
                self.last_output = app.clone();
                self.output_override = Some(app);
                edit = Edit {
                    delete_chars: 0,
                    insert: c.to_string(),
                };
            }
        } else if !self.abbrev_fixed && !self.kept_composition {
            self.output_override = None;
        }
        // Mark removed while the same base letter comes back unmarked = the
        // user undid a mark (`tô` + 'o' → `too`). Bamboo's hidden state then
        // resurfaces on later keys of this word.
        {
            let common = self
                .last_output
                .chars()
                .zip(new_output.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let deleted: Vec<char> = self.last_output.chars().skip(common).collect();
            let inserted: Vec<char> = new_output.chars().skip(common).collect();
            if deleted.iter().any(|d| {
                is_marked(*d)
                    && inserted
                        .iter()
                        .any(|i| base_letter(*i) == base_letter(*d) && *i != *d && !is_marked(*i))
            }) {
                debug_log::log("mark undone in word");
                self.undo_in_word = true;
            }
        }

        if edit.is_empty() {
            // The key is forwarded: the application appends the raw
            // character. Bamboo's own output can lag the raw keys (an undone
            // mark keeps "tooo" at "too"), so track the raw text the app
            // actually holds. An override wins: it is the text on screen.
            self.last_output = self
                .output_override
                .clone()
                .unwrap_or_else(|| self.raw.clone());
            Outcome::PassThrough
        } else {
            // Track what the user sees, not bamboo's poisoned output: after
            // an undone mark the fallback appended the raw key to `app_text`
            // ("toolb"), while bamboo still reports "tooolb". Trusting the
            // latter made the next key grow the preedit to "tooolb".
            self.last_output = self.output_override.clone().unwrap_or(new_output);
            Outcome::Edit(edit)
        }
    }

    /// Grapheme-level backspace over the composition.
    ///
    /// `remove_last_output_char` preserves marks on earlier letters, so the
    /// new output is not always the old one minus its last character:
    /// `hoàng` -> `hòa` moves the tone. When that happens the caller cannot
    /// replay a plain deletion; it must replace the whole word instead.
    pub fn backspace(&mut self) -> Option<Edit> {
        if !self.enabled || self.raw.is_empty() {
            return None;
        }
        let old = self.output();
        // Vietnamese IME convention (unikey): Backspace undoes the last typed
        // KEY and the word recomposes from the remaining raw keys — removing
        // a tone key removes the tone, removing a letter removes its letter
        // ("mười" -> "mươi" -> "mườ"; retyping "if" gives "mười" back).
        // Re-feeding the keys also keeps bamboo's transform stack consistent:
        // remove_last_output_char left it stale, so a later key resurfaced
        // hidden transforms nondeterministically ("toool"+BS stayed "tool",
        // retype-after-BS corrupted marks — open-questions #9/#10).
        self.raw.pop();
        self.engine.reset();
        // The recomposition re-derives every guard from scratch.
        self.output_override = None;
        self.kept_composition = false;
        let mut new = String::new();
        let raw_chars: Vec<char> = self.raw.chars().collect();
        for c in raw_chars {
            new = self.feed_compose(c);
        }
        self.last_output = new.clone();
        // undo_in_word stays: the raw keys may still contain the undo
        // sequence ("tooo"), so the poison guard must keep watching this word
        let old_len = old.chars().count();
        let new_len = new.chars().count();
        if old.starts_with(&new) {
            Some(Edit {
                delete_chars: old_len - new_len,
                insert: String::new(),
            })
        } else {
            Some(Edit {
                delete_chars: old_len,
                insert: new,
            })
        }
    }

    /// Diff between the previous real composition output and `new`, as a
    /// suffix edit the application can replay.
    fn delta_from_last(&self, new: &str) -> Edit {
        self.delta_between(&self.last_output, new)
    }

    /// Suffix edit turning `old` into `new`.
    fn delta_between(&self, old: &str, new: &str) -> Edit {
        let common = old
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .count();
        Edit {
            delete_chars: old.chars().count() - common,
            insert: new.chars().skip(common).collect(),
        }
    }

    /// Current composition text (empty when nothing is being composed).
    pub fn output(&self) -> String {
        match &self.output_override {
            Some(o) => o.clone(),
            None => self.engine.output_str().to_string(),
        }
    }

    /// Whether the current composition is a valid Vietnamese syllable — the
    /// same verdict `Outcome::Finish` carries, for word ends the engine
    /// decides itself (the idle timeout) where `process_char` is not involved.
    /// An abbreviation (`ddc` → `đc`) counts as valid: its đ must survive.
    /// So does a kept composition (`hạo6`): restoring its raw keys would
    /// resurrect the digits consumed as marks.
    pub fn is_valid(&self) -> bool {
        !self.output().is_empty()
            && (self.abbrev_fixed || self.kept_composition || self.engine.is_valid_word())
    }

    /// Valid, possibly incomplete syllable prefix — bamboo-core's
    /// `is_valid_prefix` (`IsValid(false)`), the verdict ibus-bamboo uses
    /// while typing; `is_valid` is the complete-syllable one. An abbreviation
    /// (`ddc` → `đc`) counts as a valid prefix, or the preedit would flash the
    /// raw keys and a click would commit them. Same for a kept composition
    /// (`hạo6`). Only meaningful while a composition is live.
    pub fn is_valid_prefix(&self) -> bool {
        !self.output().is_empty()
            && (self.abbrev_fixed || self.kept_composition || self.engine.is_valid_prefix())
    }

    pub fn has_composition(&self) -> bool {
        !self.engine.output_str().is_empty()
    }

    /// Load `text` as if the last word had been typed with it, so the next
    /// keys edit that word. Used by the surrounding-text repair path.
    pub fn rebuild(&mut self, text: &str) {
        self.engine.rebuild_from_text(text);
        self.last_output = self.engine.output_str().to_string();
        self.raw.clear();
        self.composing = true;
    }

    /// After skipped deletes (no delete transport) the application still
    /// shows the raw keystrokes; re-align the delta tracking to them so the
    /// next key's edit replays against the text the app actually holds.
    /// (Otherwise raw-forwarded mark keys desync the tracker and a later key
    /// commits garbage — the "toool" bug.)
    pub fn resync_output_to_raw(&mut self) {
        self.last_output = self.raw.clone();
    }

    /// Whether a mark was undone in the current word (`tô` + 'o' → `too`,
    /// `tê` + 'e' → `tee`). The raw keys then diverge from the composed word
    /// on purpose — bamboo marks such words invalid, but auto-restore must
    /// not "fix" them back to the raw keys.
    pub fn undo_in_word(&self) -> bool {
        self.undo_in_word
    }

    /// The raw keystrokes of the current word.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn reset(&mut self) {
        self.engine.reset();
        self.last_output.clear();
        self.raw.clear();
        self.output_override = None;
        self.undo_in_word = false;
        self.kept_composition = false;
        self.composing = false;
    }

    pub fn commit(&mut self) {
        self.engine.commit();
    }
}

/// Characters that end a Vietnamese word. bamboo-core keeps punctuation in
/// its buffer, but an IME must commit and let the application see the key.
/// The unmarked base letter of a Vietnamese vowel (`ô` → `o`, `ạ` → `a`).
fn base_letter(c: char) -> char {
    let l = c.to_lowercase().next().unwrap_or(c);
    match l {
        'á' | 'à' | 'ả' | 'ã' | 'ạ' | 'ă' | 'â' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' => {
            'a'
        }
        'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => 'e',
        'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' => 'i',
        'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ớ' | 'ờ' | 'ở'
        | 'ỡ' | 'ợ' => 'o',
        'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => 'u',
        'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
        other => other,
    }
}

/// Whether the character carries a Vietnamese mark (`ô`, `ạ`, `đ`, …).
fn is_marked(c: char) -> bool {
    base_letter(c) != c.to_lowercase().next().unwrap_or(c) || c == 'đ' || c == 'Đ'
}

pub fn is_word_terminator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '.'
            | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '"'
            | '\''
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '/'
            | '\\'
            | '|'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '-'
            | '+'
            | '='
            | '~'
            | '`'
            | '_'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bamboo_core::{BracketMode, W2uMode};

    fn wrapper(method: Method) -> BambooWrapper {
        BambooWrapper::new(method, Config::default())
    }

    fn type_str(w: &mut BambooWrapper, s: &str) -> Vec<Edit> {
        let mut edits = Vec::new();
        for c in s.chars() {
            if let Outcome::Edit(e) = w.process_char(c) {
                edits.push(e);
            }
        }
        edits
    }

    #[test]
    fn telex_produces_vietnamese() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "tieengs");
        assert_eq!(w.output(), "tiếng");
    }

    #[test]
    fn abbreviation_keeps_the_dd_transform() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "ddc");
        assert_eq!(w.output(), "đc");
        // The word-end auto-restore must not undo the abbreviation: the
        // finished word is "đc", not the raw keys.
        if let Outcome::Finish {
            word, raw, valid, ..
        } = w.process_char(' ')
        {
            assert_eq!(word, "đc");
            assert_eq!(raw, "ddc");
            assert!(valid, "abbrev_fixed must skip auto-restore");
        } else {
            panic!("space must finish the word");
        }
    }

    #[test]
    fn abbreviation_fix_is_togglable() {
        let mut w = wrapper(Method::Telex);
        w.set_abbreviations(false);
        type_str(&mut w, "ddc");
        assert_eq!(w.output(), "ddc", "off: bamboo's raw reversion stands");
        let mut w = wrapper(Method::Telex);
        w.set_abbreviations(true);
        type_str(&mut w, "ddk");
        assert_eq!(w.output(), "đk");
    }

    #[test]
    fn real_dd_words_are_untouched_by_the_abbrev_fix() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "ddau");
        assert_eq!(w.output(), "đau");
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "ddung");
        assert_eq!(w.output(), "đung");
    }

    #[test]
    fn edits_describe_rewrites() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "tieeng");
        // Typing the tone key rewrites the tail rather than appending.
        let Outcome::Edit(e) = w.process_char('s') else {
            panic!("expected edit")
        };
        assert!(e.delete_chars > 0, "tone key should delete: {e:?}");
        assert_eq!(w.output(), "tiếng");
    }

    #[test]
    fn space_ends_the_word() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "vieejt");
        assert_eq!(
            w.process_char(' '),
            Outcome::Finish {
                word: "việt".into(),
                raw: "vieejt".into(),
                valid: true,
                valid_prefix: true
            }
        );
        assert_eq!(w.output(), "", "composition must be cleared");
    }

    #[test]
    fn underscore_ends_the_word() {
        // bamboo-core clears its whole composition on '_' (output becomes
        // empty), which deleted the word in the application; '_' must act as
        // a word terminator instead.
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "tung");
        assert_eq!(
            w.process_char('_'),
            Outcome::Finish {
                word: "tung".into(),
                raw: "tung".into(),
                valid: true,
                valid_prefix: true
            }
        );
    }

    #[test]
    fn finish_carries_validity() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "window");
        let Outcome::Finish { word, valid, .. } = w.process_char(' ') else {
            panic!("expected finish");
        };
        assert_eq!(word, "window");
        assert!(!valid, "english word is not a Vietnamese syllable");
    }

    #[test]
    fn plain_ascii_passes_through_in_english_mode() {
        let mut w = wrapper(Method::Telex);
        w.set_enabled(false);
        assert_eq!(w.process_char('a'), Outcome::PassThrough);
    }

    #[test]
    fn backspace_is_a_pure_deletion() {
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "vieejt");
        assert_eq!(w.output(), "việt");
        let e = w.backspace().expect("edit");
        assert_eq!(e.insert, "", "backspace must not re-insert text");
        assert_eq!(e.delete_chars, 1);
        assert_eq!(w.output(), "việ");
    }

    #[test]
    fn backspace_undoes_the_last_key_like_unikey() {
        // unikey convention: Backspace pops the last raw KEY and the word
        // recomposes — removing the tone key drops the tone, removing a
        // letter shortens the word ("hoàng" -> "hoang" -> "hoan" -> "hoa").
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "hoangf");
        assert_eq!(w.output(), "hoàng");
        assert_eq!(
            w.backspace(),
            Some(Edit {
                delete_chars: 5,
                insert: "hoang".into()
            })
        );
        assert_eq!(w.output(), "hoang");
        assert_eq!(
            w.backspace(),
            Some(Edit {
                delete_chars: 1,
                insert: String::new()
            })
        );
        assert_eq!(w.output(), "hoan");
        assert_eq!(
            w.backspace(),
            Some(Edit {
                delete_chars: 1,
                insert: String::new()
            })
        );
        assert_eq!(w.output(), "hoa");
    }

    #[test]
    fn backspace_rebuild_keeps_later_keys_clean() {
        // The poison class behind open-questions #9/#10: with
        // remove_last_output_char the stack went stale and a later key
        // resurfaced hidden transforms ("toool"+BS stayed "tool",
        // "muowif"+BS+BS+"if" became "mừi", "hieeur"+BS+BS+"u" became "hiu").
        // {b} stands for one backspace.
        for (keys, expected) in [
            ("toool", "tool"),
            ("toool{b}", "too"),
            ("toool{b}l", "tool"),
            ("muowif{b}{b}if", "mười"),
            ("hieeur{b}{b}u", "hiêu"),
        ] {
            let mut w = wrapper(Method::Telex);
            let mut keys = keys.chars().peekable();
            while let Some(c) = keys.next() {
                if c == '{' {
                    let mut name = String::new();
                    for nc in keys.by_ref() {
                        if nc == '}' {
                            break;
                        }
                        name.push(nc);
                    }
                    assert_eq!(name, "b");
                    w.backspace();
                } else {
                    w.process_char(c);
                }
            }
            let got = w.output();
            assert_eq!(got, expected, "keys={keys:?}");
        }
    }

    #[test]
    fn backspace_on_empty_is_none() {
        let mut w = wrapper(Method::Telex);
        assert!(w.backspace().is_none());
    }

    #[test]
    fn vni_uses_digits_for_tones() {
        let mut w = wrapper(Method::Vni);
        type_str(&mut w, "tie6ng1");
        assert_eq!(w.output(), "tiếng");
    }

    #[test]
    fn vni_digit_that_cannot_extend_keeps_the_syllable() {
        // "hạo" + '6': the circumflex cannot apply, bamboo's auto_correct
        // reverts the whole composition to the raw keys and resurrects the
        // digits consumed as tones ("hao123456"). The word keeps its
        // composed form and the key lands literally (unikey convention).
        let mut w = wrapper(Method::Vni);
        type_str(&mut w, "hao12345");
        assert_eq!(w.output(), "hạo");
        let Outcome::Edit(e) = w.process_char('6') else {
            panic!("expected an edit");
        };
        assert_eq!(e.delete_chars, 0, "the word must not be rewritten");
        assert_eq!(e.insert, "6");
        assert_eq!(w.output(), "hạo6");
        // The word counts as valid: no auto-restore to the raw keys.
        if let Outcome::Finish {
            word,
            raw,
            valid,
            valid_prefix,
        } = w.process_char(' ')
        {
            assert_eq!(word, "hạo6");
            assert_eq!(raw, "hao123456");
            assert!(valid && valid_prefix);
        } else {
            panic!("space must finish the word");
        }
    }

    #[test]
    fn kept_word_survives_later_keys_and_backspaces() {
        let mut w = wrapper(Method::Vni);
        type_str(&mut w, "hao123456");
        assert_eq!(w.output(), "hạo6");
        // The next key stays literal on the kept word.
        type_str(&mut w, "x");
        assert_eq!(w.output(), "hạo6x");
        // Backspace recomposes from the raw keys: the guard re-fires.
        w.backspace();
        assert_eq!(w.output(), "hạo6");
        w.backspace();
        assert_eq!(w.output(), "hạo");
        // The tone cycle still works on the recomposed word.
        let _ = w.process_char('1');
        assert_eq!(w.output(), "háo");
    }

    #[test]
    fn combined_digit_presets_keep_the_syllable_too() {
        for m in [Method::TelexVni, Method::TelexVniViqr] {
            let mut w = wrapper(m);
            type_str(&mut w, "hao12345");
            assert_eq!(w.output(), "hạo", "{m:?}");
            type_str(&mut w, "6");
            assert_eq!(w.output(), "hạo6", "{m:?}");
        }
    }

    #[test]
    fn inapplicable_digit_stays_literal_from_scratch() {
        // Without consumed marks the revert equals the literal append —
        // "hao6" comes out the same either way.
        let mut w = wrapper(Method::Vni);
        type_str(&mut w, "hao6");
        assert_eq!(w.output(), "hao6");
    }

    #[test]
    fn english_digit_runs_keep_the_revert() {
        // English typing has no consumed marks: the guard never fires and
        // the raw keys survive verbatim.
        let mut w = wrapper(Method::Vni);
        type_str(&mut w, "abc123");
        assert_eq!(w.output(), "abc123");
        let mut w = wrapper(Method::TelexVni);
        type_str(&mut w, "abc123");
        assert_eq!(w.output(), "abc123");
    }

    #[test]
    fn telex_letter_marks_keep_the_revert() {
        // Out of scope on purpose: a Telex tone key on a complete syllable
        // still reverts (the raw keys are what English typing leans on) —
        // only the digit-mark presets are covered by the guard.
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "vieejt");
        assert_eq!(w.output(), "việt");
        type_str(&mut w, "f");
        assert_eq!(w.output(), "vieejtf");
    }

    #[test]
    fn replay_raw_applies_the_shared_guards() {
        // The idle-commit re-entry replays the raw keys through the same
        // feed: the abbreviation shows composed and a kept word recomposes.
        let mut w = wrapper(Method::Telex);
        w.replay_raw("ddc");
        assert_eq!(w.output(), "đc");
        assert!(w.is_valid());
        let mut w = wrapper(Method::Vni);
        w.replay_raw("hao123456");
        assert_eq!(w.output(), "hạo6");
        assert!(w.is_valid());
    }

    #[test]
    fn microsoft_layout_types_vietnamese() {
        let mut w = wrapper(Method::MsLayout);
        // Microsoft layout appends marked vowels: 3=ê, 8=sắc.
        type_str(&mut w, "ti3ng8");
        assert_eq!(w.output(), "tiếng");
    }

    fn out(m: Method, keys: &str) -> String {
        let mut w = wrapper(m);
        type_str(&mut w, keys);
        w.output()
    }

    /// Does the wrapper end the word at `c` (all methods type `a` first)?
    fn word_ended_at(m: Method, c: char) -> bool {
        let mut w = wrapper(m);
        type_str(&mut w, "a");
        matches!(w.process_char(c), Outcome::Finish { .. })
    }

    #[test]
    fn telex2_horn_brackets_reach_bamboo() {
        // The preset's own horn keys reach bamboo-core even with the bracket
        // option off (ms-layout behaviour).
        assert_eq!(out(Method::Telex2, "t["), "tơ");
        assert_eq!(out(Method::Telex2, "t]"), "tư");
        assert_eq!(out(Method::Telex2, "tieengs"), "tiếng");
        assert_eq!(out(Method::Telex2, "window"), "window");
    }

    #[test]
    fn digit_terminators_per_method() {
        // Presets whose composition a digit clears must end the word first
        // (the VNI-based presets keep every digit in the composition).
        for m in [
            Method::Telex,
            Method::Viqr,
            Method::Telex2,
            Method::VniFrench,
            Method::TelexW,
        ] {
            assert!(word_ended_at(m, '1'), "{m:?}");
        }
        for m in [Method::Vni, Method::TelexVni, Method::TelexVniViqr] {
            assert!(!word_ended_at(m, '1'), "{m:?}");
        }
    }

    #[test]
    fn combined_presets_route_marks_and_literals() {
        assert_eq!(out(Method::TelexVni, "aa1"), "ấ");
        assert_eq!(out(Method::TelexVni, "d9"), "đ");
        assert_eq!(out(Method::TelexVniViqr, "a'"), "á");
        assert_eq!(out(Method::TelexVniViqr, "a."), "ạ");
        assert_eq!(out(Method::TelexVniViqr, "oo*"), "ơ");
        assert_eq!(out(Method::TelexVniViqr, "as0"), "a");
        // `+` is literal in the combined preset, so it ends the word.
        assert!(word_ended_at(Method::TelexVniViqr, '+'));
    }

    #[test]
    fn vni_french_marks_on_azerty_row() {
        assert_eq!(out(Method::VniFrench, "a\""), "à");
        assert_eq!(out(Method::VniFrench, "a'"), "ả");
        assert_eq!(out(Method::VniFrench, "a("), "ã");
        assert_eq!(out(Method::VniFrench, "a-"), "ạ");
        // The preset transforms nothing else — plain Telex keys stay raw.
        assert_eq!(out(Method::VniFrench, "tieengs"), "tieengs");
    }

    #[test]
    fn telex_w_z_removes_tone() {
        assert_eq!(out(Method::TelexW, "asz"), "a");
        assert_eq!(out(Method::TelexW, "tieengs"), "tiếng");
        assert_eq!(out(Method::TelexW, "ew"), "ew");
    }

    #[test]
    fn legacy_charsets_encode_vietnamese() {
        let unicode = "tiếng Việt";
        for charset in [Charset::Tcvn3, Charset::Vni, Charset::Viqr, Charset::Viscii] {
            let encoded = encode(charset, unicode);
            assert_ne!(encoded, unicode, "{charset:?} must change diacritics");
        }
        assert_eq!(encode(Charset::Unicode, unicode), unicode);
        // NFD decomposes instead of changing letters (bamboo keeps the vowel
        // precomposed and splits off the tone).
        assert_eq!(encode(Charset::Nfd, "ề"), "ê\u{300}");
    }

    #[test]
    fn w2u_modes_affect_plain_w() {
        for (config_w2u, expected) in [(W2uMode::NonStart, "như"), (W2uMode::Everywhere, "như")] {
            let config = Config::builder().w2u_mode(config_w2u).build();
            let mut w = BambooWrapper::new(Method::Telex, config);
            type_str(&mut w, "nhw");
            assert_eq!(w.output(), expected);
        }
        // Disabled keeps `w` (telex needs `uw` for `ư`).
        let mut w = wrapper(Method::Telex);
        type_str(&mut w, "nhw");
        assert_eq!(w.output(), "nhw");
    }

    #[test]
    fn bracket_modes_type_vowels() {
        let config = Config::builder()
            .bracket_mode(BracketMode::NonStart)
            .build();
        let mut w = BambooWrapper::new(Method::Telex, config);
        // Not at the start of a word: `t[` -> `tơ`.
        type_str(&mut w, "t[");
        assert_eq!(w.output(), "tơ");
        // At the start of a word the bracket stays a bracket (NonStart).
        let mut w = BambooWrapper::new(
            Method::Telex,
            Config::builder()
                .bracket_mode(BracketMode::NonStart)
                .build(),
        );
        type_str(&mut w, "[");
        assert_eq!(w.output(), "");
    }
}
