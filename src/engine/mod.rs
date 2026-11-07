//! Protocol-independent engine state machine.
//!
//! Translates IBus key events into a list of [`Action`]s. Nothing here knows
//! about D-Bus, so the whole layer is unit-testable.
//!
//! # Display modes
//!
//! * **NonPreedit** — no underline at all. Text is committed to the
//!   application immediately and rewritten with `DeleteSurroundingText`
//!   followed by `CommitText`. This is the default.
//! * **Preedit** — the application shows the composition inline with an
//!   underline. Each keystroke sends `UpdatePreeditText`; nothing is
//!   committed until the word ends.
//! * **Surrounding** — non-preedit, but the engine verifies the application's
//!   `SetSurroundingText` reports against what it believes it committed and
//!   repairs drift (swallowed deletes, edits the application made). Useful
//!   for clients that ignore `DeleteSurroundingText` but do report
//!   surrounding text.
//!
//! Per-application rules (`AppRule`) can force a mode for a matched client;
//! clients without a rule fall back to the built-in heuristic.

use std::time::{Duration, Instant, SystemTime};

use crate::bamboo_wrapper::{encode, BambooWrapper, Charset, Method, Outcome};
use crate::config::{AppRule, Config, UnicodeChannel};
use crate::debug_log::{self, brief};
use crate::emoji::UserEmoji;
use crate::macros::{Dictionary, Macros};

mod helpers;
mod process;
mod surrounding;

#[cfg(test)]
mod digit_debug;
#[cfg(test)]
mod digit_probe2;
#[cfg(test)]
mod keys_workload_probe;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod x_probe;

use self::helpers::*;

/// Display mode, selected in the GUI/menu and persisted in the config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Commit as you type, no underline.
    #[default]
    NonPreedit,
    /// Show the composition inline until the word ends.
    Preedit,
    /// Non-preedit with surrounding-text verification.
    Surrounding,
    /// Raw keys are forwarded while typing; the latin tail is erased with
    /// real Backspaces at word end and the Vietnamese word is committed.
    Smooth,
    /// Smooth without the pre-rewrite verification.
    SuperSmooth,
    /// Shift+Left selects the typed tail, Delete erases it, then the word is
    /// committed. Needs a surrounding-text report.
    Select,
    /// Non-preedit whose tail rewrite deletes by forwarding Backspace key
    /// events through IBus instead of `DeleteSurroundingText` — works in
    /// clients that drop the protocol delete and where `/dev/uinput` cannot
    /// reach (mutter-relayed Wayland). ibus-bamboo's ForwardKeyEvent I.
    ForwardKeyEvent,
    /// The same, but the tail is selected with forwarded Shift+Left and the
    /// commit replaces the selection — for apps that mishandle a forwarded
    /// Backspace. ibus-bamboo's ForwardKeyEvent II.
    ForwardSelect,
    /// Vietnamese processing off; macros still run, auto-capitalize does
    /// not (the gap analysis documented this drift — keep the comment true).
    Off,
    /// Not a mode of its own: resolve the real mode per client from what
    /// it reports (see `auto_mode`). App rules still win over the ladder.
    Auto,
}

/// One thing the server must tell IBus to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Insert text at the cursor.
    Commit(String),
    /// Remove `chars` characters immediately before the cursor.
    DeleteBefore(u32),
    /// Remove `n` characters starting `offset` characters before the cursor.
    DeleteSurrounding { offset: i32, n: u32 },
    /// For clients proven to drop the protocol delete: press Backspace
    /// `backspaces` times (real key events through /dev/uinput) and commit
    /// `commit` afterwards. Backspaces delete from the cursor, so the
    /// common suffix inside the word is removed too and re-typed.
    RewriteByBackspace { backspaces: u32, commit: String },
    /// Replace the visible preedit string.
    UpdatePreedit { text: String, cursor: u32 },
    /// Clear the preedit string.
    HidePreedit,
    /// Let the application handle the key (return `false` from ProcessKeyEvent).
    Forward,
    /// The key was consumed; do nothing further.
    Consumed,
    /// Press Backspace `n` times through /dev/uinput, paced — the uinput
    /// modes' word-end rewrite. Emitted only when injection is possible
    /// (mode resolution guarantees it); the server falls back to the
    /// protocol delete if the device died mid-session.
    InjectBackspaces(u32),
    /// Press Backspace `n` times by forwarding the key events through IBus
    /// (`ForwardKeyEvent`) — the ForwardKeyEvent mode's delete transport.
    /// Unlike `/dev/uinput` this reaches every client, including
    /// mutter-relayed Wayland contexts; unlike the protocol delete it works
    /// in clients that drop `DeleteSurroundingText`.
    ForwardBackspaces(u32),
    /// Select `n` characters to the left with Shift+Left forwarded through
    /// IBus (ForwardSelect mode); the commit that follows replaces the
    /// selection. Same wire shape as `ForwardBackspaces`, no `/dev/uinput`.
    ForwardSelectLeft(u32),
    /// Select `n` characters to the left with Shift+Left (uinput, paced).
    SelectLeft(u32),
    /// Press the Delete key once (erases a selection made by `SelectLeft`).
    PressDelete,
    /// Set the clipboard to `text`, paste with injected Ctrl+V, restore the
    /// clipboard — the clipboard Unicode channel of the uinput modes.
    Paste(String),
    /// Idle pause (ms) after a uinput rewrite, before the app sees more keys.
    PostDelay(u64),
}

pub struct Engine {
    wrapper: BambooWrapper,
    mode: Mode,
    /// The focused application's name from sources beyond the IBus client
    /// string (the focus extension for Wayland apps, the X active window for
    /// X11) — used ONLY for rule matching. Transport decisions keep using
    /// `client`, so a relayed client keeps its mutter-safe delete handling
    /// even after it is named.
    app_name: Option<String>,
    /// IBus capabilities reported by the client, from `SetCapabilities`.
    caps: u32,
    /// True while a preedit string is on screen, so we only send `HidePreedit`
    /// when there is something to hide.
    preedit_visible: bool,
    /// `client` from `FocusInId`, e.g. `gtk4-im:chrome`. `None` before the
    /// first focus.
    client: Option<String>,
    /// Charset for text sent to the application.
    charset: Charset,
    /// Encoded text the engine believes sits right before the cursor in the
    /// application (the current word, or the macro/restore replacement).
    tail: String,
    /// Last `SetSurroundingText(text, cursor_pos)` from the client.
    surrounding: Option<(String, u32)>,
    /// The last repair delete sent, to recognize clients that swallow it.
    repair_delete: Option<(i32, u32)>,
    /// Last emitted character, for `--`, double space and auto-capitalization.
    last_char: Option<char>,
    /// True while the next word starts a sentence (after `.`, `!`, `?`,
    /// Enter — spaces in between keep the flag).
    capitalize_next: bool,
    /// The word that just started begins a sentence. The uinput modes only
    /// see the finished word, so the decision is taken at the first key.
    cap_word: bool,
    /// Set once the client has been proven to drop DeleteSurroundingText:
    /// the server then injects real Backspace keys instead of the signal.
    delete_dropped: bool,
    /// Delivery proof for the last rewrite delete: `(expected prefix length
    /// when the delete lands, expected when it is swallowed)`. The next ST
    /// report settles it — a report matching the swallowed shape proves the
    /// client drops rewrite deletes, and (without uinput) the following
    /// words leave delete-based rewriting. Skipped for pure deletions,
    /// where a stale report is indistinguishable from a swallowed one.
    awaiting_delete_proof: Option<(usize, usize)>,
    /// Whether /dev/uinput is usable — deletions for clients without
    /// surrounding-text data go through key injection instead.
    uinput_available: bool,
    /// Backspace keys injected through uinput that will echo back as
    /// ProcessKeyEvent; those must be forwarded untouched.
    pending_injected: u32,
    /// Commits that must wait until their injected Backspaces have been
    /// forwarded to the client: `(flush_after_echoes, text)`. The injected
    /// keys travel through the compositor while a CommitText signal goes
    /// straight over D-Bus — without deferral the commit outruns the
    /// backspace and the echo then deletes the freshly committed word
    /// (seen as "the accent flashes and reverts").
    deferred: Vec<(u32, String)>,
    /// How many injected-backspace echoes have been forwarded so far; drives
    /// `deferred` flush thresholds.
    echoes_forwarded: u32,
    /// When the last batch of injections happened — echoes that have not
    /// come back within the stale window are treated as lost.
    last_inject: Option<Instant>,
    /// Injected non-Backspace keys (arrows, Delete, V) whose echoes are
    /// still expected, in press order.
    pending_nav: Vec<u32>,
    /// When the last non-Backspace injection happened (stale window).
    last_nav_inject: Option<Instant>,
    /// Commits waiting for injected nav-key echoes to be forwarded.
    nav_deferred: Vec<(usize, String)>,
    // --- settings, mirrors of the config file ---
    app_rules: Vec<AppRule>,
    auto_restore: bool,
    em_dash: bool,
    auto_capitalize: bool,
    double_space_period: bool,
    macro_enabled: bool,
    capitalize_macro: bool,
    macro_in_off_mode: bool,
    macro_expand_valid_words: bool,
    time_format: String,
    date_format: String,
    macros: Macros,
    dictionary: Option<Dictionary>,
    /// mtimes of the auxiliary files behind the fields above: apply_config
    /// runs on every focus change, so each file is reloaded only when it
    /// changed on disk.
    macros_mtime: Option<SystemTime>,
    dictionary_mtime: Option<SystemTime>,
    /// Preedit shown without the underline attribute.
    hide_underline: bool,
    /// Opt-in: protocol deletes to direct clients that advertise
    /// surrounding-text support without ever reporting their text.
    x11_delete_without_report: bool,
    /// Opt-in: keep Preedit for XIM clients, whose server draws the preedit.
    preedit_for_xim: bool,
    /// Uinput pacing (ms): text/commit step, Backspace gap, post-rewrite idle.
    commit_interval_ms: u64,
    backspace_interval_ms: u64,
    post_delay_ms: u64,
    /// How the Vietnamese word reaches the app in the uinput modes.
    unicode_channel: UnicodeChannel,
    /// Custom clipboard restore delay (ms); the built-in default applies
    /// while the switch is off.
    clipboard_restore_delay_enabled: bool,
    clipboard_restore_delay_ms: u64,
    /// Display mode locked for the word being typed. The daemon flips
    /// capabilities 1ms apart around focus changes and not at all on quick
    /// refocus; re-resolving the mode mid-word flips preedit↔non-preedit
    /// under the user's hands (duplicated or invisible text). Locked at the
    /// first key of a word, released when the word ends.
    word_mode: Option<Mode>,
    /// The word being typed follows a URL/email/path character: keep it raw
    /// until the next terminator (URL/email/path bypass).
    url_word: bool,
    /// Keep words raw after URL/email/path characters (`bypass_url`).
    bypass_url: bool,
    /// The filtered `extra_url_chars` — empty unless the switch is on
    /// (see `apply_config` for the filter).
    extra_url_chars: Vec<char>,
    /// The filtered `auto_capitalize_after` — empty unless the switch is
    /// on (same filter shape as `extra_url_chars`, without the `-` ban: a
    /// hyphen trigger is legitimate here).
    extra_capitalize_after: Vec<char>,
    /// Emoji shortcodes on: a word glued directly after `:` stays raw and a
    /// closing `:` rewrites `:name:` to the emoji (see `process_char_emoji`).
    emoji_enabled: bool,
    /// The word being typed is an emoji shortcode candidate (`:name:`). The
    /// state starts at the first key after the opening `:` and ends at the
    /// closing one — or at any other terminator, which leaves the text raw.
    emoji_word: bool,
    /// The user's emoji table, reloaded by mtime like the macros.
    emojis: UserEmoji,
    emoji_mtime: Option<SystemTime>,
    /// The app appended suggestion text after the caret mid-word (Chrome's
    /// inline autocomplete). Its rewrite delete is dropped and repairs crash
    /// mutter, so show the following words as preedit — no delete needed.
    suggestions: bool,
    /// Cached `client_rule_mode` result keyed on the name pair that produced
    /// it. Resolution lowercases the pattern and the target and the key path
    /// asks for the mode more than once per keystroke; the cache turns that
    /// into two string compares. The key covers `app_name`/`client` changes
    /// by itself; `apply_config` clears it because the rules changed.
    rule_cache: Option<(Option<String>, Option<String>, Option<Mode>)>,
}

/// `IBUS_CAP_PREEDIT_TEXT` and `IBUS_CAP_SURROUNDING_TEXT` from ibustypes.h.
const CAP_PREEDIT_TEXT: u32 = 1 << 0;
const CAP_SURROUNDING_TEXT: u32 = 1 << 5;

/// Safety net for clients that are neither covered by an `AppRule` nor by the
/// config file but drop `DeleteSurroundingText`. Chromium's
/// `ui/gtk/input_method_context_impl_gtk.cc` connects only `commit` and
/// `preedit-*`, so a `DeleteSurroundingText` sent there is dropped and the
/// next commit appends: `tieengs` comes out as `tieêngs`. Electron inherits
/// the same code.
const FALLBACK_NO_DELETE_SURROUNDING: &[&str] = &[
    "chrome", "chromium", "electron", "code", "brave", "msedge", "vivaldi", "slack", "discord",
];

/// A word typed directly after one of these characters is part of a URL,
/// email address, file path or code (`example.com`, `a@site`, `/etc/hosts`,
/// `get_user_id`) and must stay raw. `-` is deliberately excluded from
/// starting a bypass: it would break the `--` → em-dash feature. Once a
/// bypass is running, `-` and `+` keep it alive so `vi-mangrs.com` is not
/// composed as Vietnamese after the hyphen.
const URL_CONTEXT_CHARS: &[char] = &['/', ':', '@', '.', '_', '~', '=', '#', '&', '%', '?'];

impl Engine {
    /// How long injected-key echoes may be missing before they count as
    /// lost and deferred commits flush anyway (ibus-bamboo's pacing
    /// ballpark). One shared constant: the key handlers and the main-loop
    /// tick must agree or a lost echo holds the commit for different
    /// lengths of time.
    pub const ECHO_STALE: Duration = Duration::from_millis(150);

    pub fn new(method: Method, mode: Mode) -> Self {
        let mut engine = Self {
            wrapper: BambooWrapper::new(method, bamboo_core::Config::default()),
            mode,
            caps: 0,
            preedit_visible: false,
            client: None,
            charset: Charset::Unicode,
            tail: String::new(),
            surrounding: None,
            repair_delete: None,
            last_char: None,
            app_name: None,
            app_rules: Vec::new(),
            auto_restore: true,
            em_dash: false,
            auto_capitalize: false,
            double_space_period: false,
            macro_enabled: true,
            capitalize_macro: true,
            macro_in_off_mode: false,
            macro_expand_valid_words: false,
            time_format: "%H:%M".into(),
            date_format: "%d/%m/%Y".into(),
            macros: Macros::load(),
            dictionary: None,
            macros_mtime: None,
            dictionary_mtime: None,
            hide_underline: false,
            x11_delete_without_report: false,
            preedit_for_xim: false,
            delete_dropped: false,
            awaiting_delete_proof: None,
            uinput_available: false,
            pending_injected: 0,
            deferred: Vec::new(),
            echoes_forwarded: 0,
            last_inject: None,
            pending_nav: Vec::new(),
            last_nav_inject: None,
            nav_deferred: Vec::new(),
            word_mode: None,
            url_word: false,
            bypass_url: true,
            extra_url_chars: Vec::new(),
            extra_capitalize_after: Vec::new(),
            emoji_enabled: false,
            emoji_word: false,
            emojis: UserEmoji::load(),
            emoji_mtime: None,
            suggestions: false,
            rule_cache: None,
            capitalize_next: false,
            cap_word: false,
            commit_interval_ms: 20,
            backspace_interval_ms: 5,
            post_delay_ms: 10,
            unicode_channel: UnicodeChannel::Commit,
            clipboard_restore_delay_enabled: false,
            clipboard_restore_delay_ms: 600,
        };
        engine.apply_config(&Config::default());
        // apply_config applies the config's method/mode; the caller wins here.
        engine.wrapper.set_method(method);
        engine.mode = mode;
        engine
    }

    /// Apply the on-disk config: input method, tone style, free marking,
    /// display features, macros, dictionary and per-app rules. Returns the
    /// actions needed to keep a visible composition alive: the rebuild and
    /// mode switch below swap the bamboo engine underneath it, so a pending
    /// word must be committed first or it silently disappears (config
    /// hot-reload or a menu change mid-word).
    pub fn apply_config(&mut self, config: &Config) -> Vec<Action> {
        let mut actions = Vec::new();
        let pending = self.wrapper.output().to_string();
        if self.preedit_commit_pending() {
            // Word-end path: a macro expands and an invalid word is restored,
            // exactly as on a terminator. No key is forwarded here.
            let raw = self.wrapper.raw().to_string();
            let valid = self.wrapper.is_valid();
            let valid_prefix = self.wrapper.is_valid_prefix();
            let mut finished =
                self.on_finish(Mode::Preedit, pending.clone(), raw, valid, valid_prefix);
            finished.retain(|a| !matches!(a, Action::Forward));
            actions.extend(finished);
            self.wrapper.reset();
            self.tail.clear();
            self.repair_delete = None;
            self.release_word_mode();
        }
        let (method, mode) = config.engine_settings();
        let mut rebuilt = self.wrapper.set_config(config.bamboo_config());
        rebuilt |= self.wrapper.set_method(method);
        if rebuilt {
            self.tail.clear();
        }
        self.app_rules = config.app_rules.clone();
        // The cached rule resolution belongs to the previous rule set.
        self.rule_cache = None;
        self.auto_restore = config.auto_restore;
        self.em_dash = config.em_dash;
        self.auto_capitalize = config.auto_capitalize;
        self.extra_capitalize_after = if config.auto_capitalize_after_enabled {
            // Same filter shape as `extra_url_chars` minus the `-` ban; a
            // hyphen trigger is legitimate here.
            let mut chars: Vec<char> = Vec::new();
            for c in config.auto_capitalize_after.chars() {
                if c.is_alphanumeric() || c.is_whitespace() || chars.contains(&c) {
                    continue;
                }
                chars.push(c);
                if chars.len() >= 10 {
                    break;
                }
            }
            chars
        } else {
            Vec::new()
        };
        self.double_space_period = config.double_space_period;
        self.macro_enabled = config.macro_enabled;
        self.capitalize_macro = config.capitalize_macro;
        self.macro_in_off_mode = config.macro_in_off_mode;
        self.macro_expand_valid_words = config.macro_expand_valid_words;
        self.x11_delete_without_report = config.x11_delete_without_report;
        self.preedit_for_xim = config.preedit_for_xim;
        self.time_format = config.time_format.clone();
        self.date_format = config.date_format.clone();
        // Reload the auxiliary files only when they changed on disk (see the
        // mtime fields above).
        let macros_time = file_time(Macros::path().as_deref());
        if macros_time != self.macros_mtime {
            self.macros = Macros::load();
            self.macros_mtime = macros_time;
        }
        if config.dictionary_enabled {
            let t = file_time(Dictionary::path().as_deref());
            if self.dictionary.is_none() || t != self.dictionary_mtime {
                self.dictionary_mtime = t;
                self.dictionary = Some(Dictionary::load());
            }
        } else {
            self.dictionary = None;
            self.dictionary_mtime = None;
        };
        self.wrapper.set_abbreviations(config.abbreviations);
        self.hide_underline = config.hide_underline;
        self.commit_interval_ms = config.commit_interval_ms;
        self.backspace_interval_ms = config.backspace_interval_ms;
        self.post_delay_ms = config.post_delay_ms;
        self.unicode_channel = config.unicode_channel;
        self.clipboard_restore_delay_enabled = config.clipboard_restore_delay_enabled;
        self.clipboard_restore_delay_ms = config.clipboard_restore_delay_ms;
        self.bypass_url = config.bypass_url;
        self.extra_url_chars = if config.extra_url_chars_enabled {
            // Defensive filter: `-` stays out of the start set (the `--`
            // em dash would break), letters/digits/spaces are not URL
            // context, and a hand-edited file must not smuggle in a huge
            // list — the hot path scans this vec linearly.
            let mut chars: Vec<char> = Vec::new();
            for c in config.extra_url_chars.chars() {
                if c == '-' || c.is_alphanumeric() || c.is_whitespace() || chars.contains(&c) {
                    continue;
                }
                chars.push(c);
                if chars.len() >= 10 {
                    break;
                }
            }
            chars
        } else {
            Vec::new()
        };
        self.emoji_enabled = config.emoji_enabled;
        self.emoji_word = false;
        self.url_word = false;
        let emoji_time = file_time(UserEmoji::path().as_deref());
        if emoji_time != self.emoji_mtime {
            self.emojis = UserEmoji::load();
            self.emoji_mtime = emoji_time;
        }
        self.set_mode(mode);
        self.set_charset(config.charset.into());
        actions
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            self.wrapper.reset();
            self.tail.clear();
            self.mode = mode;
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_method(&mut self, method: Method) {
        if self.wrapper.set_method(method) {
            self.tail.clear();
        }
    }

    pub fn set_charset(&mut self, charset: Charset) {
        if charset != self.charset {
            self.wrapper.reset();
            self.tail.clear();
            self.charset = charset;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.wrapper.set_enabled(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        self.wrapper.is_enabled()
    }

    pub fn set_capabilities(&mut self, caps: u32) {
        self.caps = caps;
    }

    pub fn has_surrounding_text_cap(&self) -> bool {
        self.caps & CAP_SURROUNDING_TEXT != 0
    }

    /// Whether the preedit string should be sent without the underline.
    pub fn hide_underline(&self) -> bool {
        self.hide_underline
    }

    /// The text we believe is committed right before the cursor.
    pub fn tail(&self) -> &str {
        &self.tail
    }

    /// The focused application identifier, if known (`gtk4-im:chrome`).
    pub fn client(&self) -> Option<&str> {
        self.client.as_deref()
    }

    /// Uinput pacing, from the config (ms between rewrite steps).
    pub fn commit_interval_ms(&self) -> u64 {
        self.commit_interval_ms
    }

    pub fn backspace_interval_ms(&self) -> u64 {
        self.backspace_interval_ms
    }

    pub fn post_delay_ms(&self) -> u64 {
        self.post_delay_ms
    }

    /// How the Vietnamese word is delivered in the uinput modes.
    pub fn unicode_channel(&self) -> UnicodeChannel {
        self.unicode_channel
    }

    /// Effective clipboard restore delay: the custom value when the switch
    /// is on, else the built-in default — the restore itself must always
    /// run, or the user's clipboard stays replaced by the last word.
    pub fn clipboard_restore_delay_ms(&self) -> u64 {
        if self.clipboard_restore_delay_enabled {
            self.clipboard_restore_delay_ms
        } else {
            crate::paste::DEFAULT_RESTORE_DELAY_MS
        }
    }

    /// The client dropped the protocol delete at least once; the server
    /// should inject real key events for deletions from now on.
    pub fn delete_dropped(&self) -> bool {
        self.delete_dropped
    }

    /// Record that `n` Backspace keys were injected; their echoes must be
    /// forwarded, not processed.
    pub fn add_injected(&mut self, n: u32) {
        if n > 0 {
            self.pending_injected += n;
            self.last_inject = Some(Instant::now());
        }
    }

    /// True when injected backspaces have not echoed back within `max` — they
    /// are considered lost (apps that receive injected keys without routing
    /// them through ibus): the pending-echo counter is dropped or real
    /// Backspaces would be eaten as echoes forever, and any deferred commits
    /// become deliverable. Needs no deferred queue — a bare injection (e.g.
    /// Backspace while composing) leaves the counter stuck just the same.
    /// Idempotent: false when nothing is stale.
    pub fn take_stale_injection(&mut self, max: Duration) -> bool {
        let mut stale = false;
        if self.pending_injected > 0 {
            if let Some(t) = self.last_inject {
                if t.elapsed() > max {
                    debug_log::log(&format!(
                        "injection echoes stale ({}ms) → drop {} pending",
                        t.elapsed().as_millis(),
                        self.pending_injected
                    ));
                    self.pending_injected = 0;
                    self.last_inject = None;
                    stale = true;
                }
            }
        }
        if !self.pending_nav.is_empty() {
            if let Some(t) = self.last_nav_inject {
                if t.elapsed() > max {
                    debug_log::log(&format!(
                        "nav injection echoes stale ({}ms) → drop {} pending",
                        t.elapsed().as_millis(),
                        self.pending_nav.len()
                    ));
                    self.pending_nav.clear();
                    self.last_nav_inject = None;
                    stale = true;
                }
            }
        }
        stale
    }

    /// Consume one injected-backspace echo. True when the key is an echo and
    /// must go to the application untouched.
    pub fn take_injected_echo(&mut self) -> bool {
        if self.pending_injected > 0 {
            self.pending_injected -= 1;
            true
        } else {
            false
        }
    }

    /// Injected backspace echoes still expected to come back.
    pub fn pending_echoes(&self) -> bool {
        self.pending_injected > 0 || !self.pending_nav.is_empty()
    }

    /// Record that non-Backspace keys (arrows, Delete, V) were injected; their
    /// echoes must be forwarded in press order, not processed.
    pub fn add_injected_nav(&mut self, keyvals: &[u32]) {
        if keyvals.is_empty() {
            return;
        }
        self.pending_nav.extend_from_slice(keyvals);
        self.last_nav_inject = Some(Instant::now());
    }

    /// Consume an injected nav-key echo when it matches the press order.
    /// True when the key is an echo and must go to the application untouched;
    /// returned commits are those whose nav echoes are all delivered.
    pub fn take_nav_echo(&mut self, keyval: u32) -> Option<Vec<String>> {
        if self.pending_nav.first() == Some(&keyval) {
            self.pending_nav.remove(0);
            Some(self.note_nav_forwarded())
        } else {
            None
        }
    }

    /// Non-Backspace injected keys still awaiting echoes.
    pub fn pending_nav_echoes(&self) -> bool {
        !self.pending_nav.is_empty()
    }

    /// A commit that must wait until `n` more injected nav-key echoes have
    /// been forwarded (same transport race as `defer_commit`).
    pub fn defer_nav_commit(&mut self, n: usize, text: String) {
        // Length only: the queued text is the word being typed; its landing
        // is visible on screen, the queue size is what diagnosis needs.
        debug_log::log(&format!(
            "defer commit after {n} nav echoes len={}",
            text.chars().count()
        ));
        self.nav_deferred.push((n, text));
    }

    /// Record that one injected nav key was forwarded; returns commits whose
    /// nav echoes have all been delivered.
    fn note_nav_forwarded(&mut self) -> Vec<String> {
        let mut ready = Vec::new();
        let mut remaining = Vec::new();
        for (left, text) in self.nav_deferred.drain(..) {
            if left > 1 {
                remaining.push((left - 1, text));
            } else {
                debug_log::log(&format!(
                    "flush deferred commit (nav) len={}",
                    text.chars().count()
                ));
                ready.push(text);
            }
        }
        self.nav_deferred = remaining;
        ready
    }

    /// Queue `text` to be committed only after `n` more injected-backspace
    /// echoes have been forwarded to the client (D-Bus signals and compositor
    /// key events have no shared ordering — see field docs).
    pub fn defer_commit(&mut self, n: u32, text: String) {
        let threshold = self.echoes_forwarded + n;
        debug_log::log(&format!(
            "defer commit after {n} echoes len={}",
            text.chars().count()
        ));
        self.deferred.push((threshold, text));
    }

    /// Record that one injected backspace was forwarded; returns commits whose
    /// Backspaces have all been delivered, in order.
    pub fn note_echo_forwarded(&mut self) -> Vec<String> {
        self.echoes_forwarded += 1;
        let mut ready = Vec::new();
        while self
            .deferred
            .first()
            .is_some_and(|(t, _)| self.echoes_forwarded >= *t)
        {
            let (_, text) = self.deferred.remove(0);
            debug_log::log(&format!(
                "flush deferred commit len={}",
                text.chars().count()
            ));
            ready.push(text);
        }
        ready
    }

    /// Drop the wait and return everything queued (echoes may have been
    /// lost — a focus change or a real key arriving means delivering late is
    /// better than never).
    pub fn flush_deferred(&mut self) -> Vec<String> {
        let mut all: Vec<String> = self
            .deferred
            .drain(..)
            .map(|(_, text)| {
                debug_log::log(&format!(
                    "flush deferred commit (forced) len={}",
                    text.chars().count()
                ));
                text
            })
            .collect();
        all.extend(self.nav_deferred.drain(..).map(|(_, text)| {
            debug_log::log(&format!(
                "flush deferred commit (nav, forced) len={}",
                text.chars().count()
            ));
            text
        }));
        all
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty() || !self.nav_deferred.is_empty()
    }

    /// Whether the server can inject keys (called once at startup).
    pub fn set_uinput_available(&mut self, available: bool) {
        self.uinput_available = available;
    }

    /// Test hook: pretend the app appended suggestion text after the caret.
    #[cfg(test)]
    pub fn set_suggestions(&mut self, suggestions: bool) {
        self.suggestions = suggestions;
    }

    /// Whether real Backspace keys can be injected as the delete transport.
    pub fn uinput_available(&self) -> bool {
        self.uinput_available
    }

    /// Forget everything believed about the focused client's text buffer.
    ///
    /// Called on focus changes. The new context may not support surrounding
    /// text at all; keeping the previous client's data made the engine send
    /// protocol `DeleteSurroundingText` to a context that never reported a
    /// buffer, which segfaults mutter (the GNOME Shell search-bar logout).
    pub fn clear_buffer_state(&mut self) {
        self.surrounding = None;
        self.repair_delete = None;
        self.delete_dropped = false;
        self.suggestions = false;
        self.awaiting_delete_proof = None;
    }

    /// Deletion through the IBus protocol is only safe when the client has
    /// actually **reported** surrounding text in this focus session — mutter
    /// then knows the buffer. The capability bit is NOT usable as the signal:
    /// the daemon flips it (0b1001 ↔ 0b101001 in the logs) without the data
    /// flow changing, and on a quick unfocus/refocus it is not re-sent at
    /// all. A report, by contrast, proves the report channel works.
    pub fn surrounding_data_ok(&self) -> bool {
        self.surrounding.is_some()
    }

    /// True when the IBus client id names no application. Every XIM client is
    /// reported as the hard-coded `xim` (ibus-x11 creates all of its input
    /// contexts with that name — `client/x11/main.c`), so an app rule can only
    /// ever match it by accident; the compositor extension or `xprop` has to
    /// name those apps for rules to work.
    pub fn client_name_is_opaque(&self) -> bool {
        self.client.as_deref() == Some("xim")
    }

    /// Clients whose IBus context is owned by the compositor: every Wayland
    /// client is relayed by mutter and shows up as `gnome-shell`; `fake` is
    /// the daemon's hotkey context; `None` means no FocusInId arrived —
    /// indistinguishable from a relay, so treated as one.
    pub fn relay_context(&self) -> bool {
        match self.client.as_deref() {
            None => true,
            Some(c) => {
                let prgname = c.split_once(':').map_or(c, |(_, p)| p);
                prgname == "gnome-shell" || prgname == "mutter" || prgname == "fake"
            }
        }
    }

    /// Whether real Backspace keys may be injected. **Never for mutter-relayed
    /// contexts**: the injected keys do not reach the application there
    /// ("xin chaà" session — raw text kept, commit appended), while the
    /// protocol delete demonstrably works on that path. A `None` client is an
    /// XIM/XWayland app that never sent `FocusInId`; its keys and injected
    /// backspaces do arrive (Sublime Text, JetBrains XToolkit), so it is not
    /// lumped in with the relay.
    pub fn may_inject(&self) -> bool {
        !self.injection_forbidden() && (self.delete_dropped || self.surrounding.is_none())
    }

    /// Contexts where injected key events are known to be lost.
    fn injection_forbidden(&self) -> bool {
        match self.client.as_deref() {
            None => false,
            Some(c) => {
                let prgname = c.split_once(':').map_or(c, |(_, p)| p);
                prgname == "gnome-shell" || prgname == "mutter" || prgname == "fake"
            }
        }
    }

    /// Clients whose IBus context is owned by the compositor (mutter relays
    /// every Wayland client's text-input; the context id shows up as
    /// `gnome-shell`). Normal rewrite deletes work on this path, but a
    /// **repair** delete sent right after the application rewrote its own
    /// buffer segfaults mutter (coredump 02:37: Chrome omnibox autocomplete,
    /// shell dead 1.1s after the repair) — the compositor applies it against
    /// a buffer it has not caught up with. Repairs are skipped there.
    fn repair_delete_forbidden(&self) -> bool {
        self.relay_context()
    }

    /// Record the focused application (`gtk4-im:chrome`, ...) from `FocusInId`.
    pub fn set_client(&mut self, client: Option<String>) {
        // ibus-daemon reports its hotkey-only context as `fake` between two
        // real focus changes. It never receives typing, but focus can move on
        // to a context without surrounding text (GNOME Shell search) while
        // the last real client's data is still held — stale data re-enables
        // protocol deletes there. Keep the name for app rules, drop the rest.
        if client.as_deref() == Some("fake") {
            self.clear_buffer_state();
            return;
        }
        if client != self.client {
            self.wrapper.reset();
            self.tail.clear();
            self.clear_buffer_state();
            // A new client invalidates the previous app's name; the server
            // re-resolves it right after the focus lookup.
            self.app_name = None;
            self.client = client;
        }
    }

    /// Record a character the application received, updating the
    /// auto-capitalization window (`. `, `! `, `? `, Enter).
    fn note_char(&mut self, c: char) {
        self.last_char = Some(c);
        match c {
            '.' | '!' | '?' | '\n' | '\r' => self.capitalize_next = true,
            ' ' | '\t' => {}
            _ if !self.extra_capitalize_after.is_empty()
                && self.extra_capitalize_after.contains(&c) =>
            {
                self.capitalize_next = true;
            }
            _ => self.capitalize_next = false,
        }
    }

    /// The focused application's name from the focus extension or the X
    /// active window. Rule matching only — see the field docs.
    pub fn set_app_name(&mut self, name: Option<String>) {
        self.app_name = name;
    }

    /// The name set by `set_app_name`, for callers outside the engine (the
    /// app-rule picker needs the same target the rules will match against).
    pub fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    /// Mode forced by an `AppRule` matching the focused client, if any.
    /// The rule target is `app_name` (the focus extension / X active window)
    /// when present, else the IBus client string. The special pattern `none`
    /// matches clients with neither — the only way to target them, since
    /// there is no name to substring-match.
    fn client_rule_mode(&mut self) -> Option<Mode> {
        // Cache hit without cloning: both name sources compare by reference.
        if let Some((cached_app, cached_client, mode)) = &self.rule_cache {
            if self.app_name.as_deref() == cached_app.as_deref()
                && self.client.as_deref() == cached_client.as_deref()
            {
                return *mode;
            }
        }
        let effective = self.app_name.as_deref().or(self.client.as_deref());
        let resolved = crate::config::app_rule_mode(&self.app_rules, effective).map(Mode::from);
        self.rule_cache = Some((self.app_name.clone(), self.client.clone(), resolved));
        resolved
    }

    /// True when the focused app drops `DeleteSurroundingText`, so non-preedit
    /// rewriting would duplicate characters there.
    fn client_cannot_delete_surrounding(&self) -> bool {
        let Some(client) = self.client.as_deref() else {
            return false;
        };
        // `gtk4-im:<prgname>`; the prefix only says which toolkit glued us in.
        let prgname = client.split_once(':').map_or(client, |(_, p)| p);
        FALLBACK_NO_DELETE_SURROUNDING
            .iter()
            .any(|p| prgname.contains(p))
    }

    /// Delete-based rewriting is only safe once the client has actually
    /// reported surrounding text: mutter segfaults applying
    /// DeleteSurroundingText to a client whose buffer it never saw
    /// (g_utf8_offset_to_pointer out of bounds — the "logout while typing a
    /// tone mark" crash). Without a report the only alternative is key
    /// injection — and never on relayed contexts, where injected keys are
    /// lost. Also several clients silently drop the protocol delete, which
    /// used to duplicate words.
    fn deletes_allowed(&self) -> bool {
        self.protocol_delete_ok() || (self.uinput_available && !self.injection_forbidden())
    }

    /// Whether the protocol delete may be sent right now.
    ///
    /// A report is the proof that mutter knows the client's buffer, and a
    /// delete sent to a relayed context without one segfaulted the compositor
    /// (02:37, Chrome omnibox). A DIRECT client applies the delete in its own
    /// process instead (GTK clamps it), so with the opt-in the report is not
    /// required there — ibus-bamboo's surrounding mode deletes that way for
    /// every client, which is what makes X11 apps like Sublime work.
    pub fn protocol_delete_ok(&self) -> bool {
        self.surrounding_data_ok()
            || (self.x11_delete_without_report
                && self.caps & CAP_SURROUNDING_TEXT != 0
                && !self.relay_context())
    }

    /// Resolve the mode actually used for the focused client: hard fallbacks
    /// first, then the app rule, then the built-in heuristic. `&mut self`
    /// because the rule resolution is cached (see `client_rule_mode`).
    fn effective_mode(&mut self) -> Mode {
        let can_preedit = self.caps & CAP_PREEDIT_TEXT != 0;
        let ruled = self.client_rule_mode();
        let requested = ruled.unwrap_or(self.mode);
        // Auto resolves straight from the client's signals — the ladder
        // below never needs a delete transport it cannot have.
        if requested == Mode::Auto {
            return self.auto_mode(can_preedit);
        }
        // Preedit is unusable when the client cannot render it — except for
        // XIM clients, which report no preedit capability even though the XIM
        // server draws the preedit itself (`XIMPreeditPosition`: ibus-x11
        // shows it in its own window). Downgrading them turned every word into
        // raw keystrokes; the opt-in keeps preedit, as ibus-bamboo does.
        if requested == Mode::Preedit
            && !can_preedit
            && !(self.preedit_for_xim && self.client_name_is_opaque())
        {
            return Mode::NonPreedit;
        }
        // The uinput modes press real keys through /dev/uinput, but on
        // mutter-relayed contexts the rewrite products never reach the
        // application (verified 31/10: 52 smooth rewrites injected, echoes
        // forwarded, commits flushed — Text Editor and Firefox showed the
        // untouched latin text, no deletion, no commit, not even the
        // terminator space). Relay contexts therefore fall back to preedit;
        // the uinput modes stay for contexts where injection lands (X11).
        if matches!(requested, Mode::Smooth | Mode::SuperSmooth | Mode::Select)
            && (self.injection_forbidden()
                || !self.uinput_available
                || (requested == Mode::Select && self.surrounding.is_none()))
        {
            return Mode::Preedit;
        }
        // ForwardKeyEvent carries its own delete transport (Backspace key
        // events through IBus), so none of the delete-transport heuristics
        // below apply — falling back to preedit would defeat the mode, which
        // exists for apps where preedit is unusable.
        if forwards_delete(requested) {
            return requested;
        }
        if ruled.is_some() {
            return requested;
        }
        // Inline autocomplete: the app appended suggestion text after the
        // caret, and its rewrite delete is dropped (repairing crashes
        // mutter). Show the following words as preedit so no rewrite delete
        // is needed.
        if self.suggestions && can_preedit && self.relay_context() {
            return Mode::Preedit;
        }
        // Rewriting the tail is impossible for delete-dropping clients, so
        // show the word inline instead of letting the commits pile up.
        if requested == Mode::NonPreedit && can_preedit && self.client_cannot_delete_surrounding() {
            return Mode::Preedit;
        }
        // A rewrite delete was proven swallowed (report proof) and real-key
        // injection cannot take over: without a working delete transport the
        // following words go through preedit, which needs no delete at all.
        // Rule-matched clients keep their configured mode (they returned
        // above); this only heals unlisted clients.
        if matches!(requested, Mode::NonPreedit | Mode::Surrounding)
            && can_preedit
            && self.delete_dropped
            && !self.uinput_available
        {
            return Mode::Preedit;
        }
        requested
    }

    /// The Auto ladder: the display mode this client handles best, from the
    /// capabilities it declares. Ordered so no step needs a channel the
    /// client cannot have:
    /// 1. renders preedit (the XIM opt-in counts — the XIM server draws the
    ///    preedit itself) → Preedit. Preedit needs no delete transport at
    ///    all, so it is right for healthy and delete-dropping clients alike;
    /// 2. XIM (no capabilities reported at all) → ForwardSelect, the mode
    ///    built for them: a forwarded Backspace misbehaves there, Shift+Left
    ///    selection does not;
    /// 3. reports surrounding text → Surrounding (its reports are the only
    ///    repair channel needed);
    /// 4. a mutter relay or a known delete-dropper → ForwardKeyEvent (no
    ///    report needed, works on Wayland);
    /// 5. anything else → NonPreedit — inert rather than guessing a
    ///    transport an unknown client may not have.
    fn auto_mode(&self, can_preedit: bool) -> Mode {
        if can_preedit || (self.preedit_for_xim && self.client_name_is_opaque()) {
            return Mode::Preedit;
        }
        if self.client_name_is_opaque() {
            return Mode::ForwardSelect;
        }
        if self.caps & CAP_SURROUNDING_TEXT != 0 {
            return Mode::Surrounding;
        }
        if self.relay_context() || self.client_cannot_delete_surrounding() {
            return Mode::ForwardKeyEvent;
        }
        Mode::NonPreedit
    }

    /// Mode for the current keystroke: the word-locked mode when a word is in
    /// flight, else the freshly resolved one (and lock it for this word).
    fn key_mode(&mut self) -> Mode {
        match self.word_mode {
            Some(m) => m,
            None => {
                let m = self.effective_mode();
                self.word_mode = Some(m);
                m
            }
        }
    }

    /// Release the word-mode lock (word ended or state reset).
    fn release_word_mode(&mut self) {
        self.word_mode = None;
    }

    /// The Backspace was forwarded: the application's buffer lost its last
    /// character. The per-key features keyed on `last_char` (em-dash,
    /// double space, the emoji/URL arms) must not fire on the deleted
    /// character — their pair-rewrite would delete real text in front of it
    /// ("a-" BS "-" turned the whole "a-" into "—"). `tail` mirrors the
    /// screen for the raw forwarders, so its last char is the new context;
    /// when it holds nothing of ours the context is unknown and stays empty.
    fn note_backspace_forwarded(&mut self) {
        self.last_char = self.tail.chars().last();
        self.capitalize_next = false;
    }

    /// Backspace over the composition. Returns `None` when there is nothing
    /// composed, so the application handles the key itself.
    pub fn backspace(&mut self) -> Option<Vec<Action>> {
        // A word in flight follows the word-mode lock: a mid-word mode
        // re-resolution (capability flip, inline suggestions) must not send
        // the Backspace down a different branch than the keys around it —
        // process_char locks for the same reason. With no composition there
        // is no lock, and resolving here must not create one.
        let mode = if self.wrapper.has_composition() {
            self.key_mode()
        } else {
            self.effective_mode()
        };
        // Vietnamese processing is off; the app owns the Backspace.
        if mode == Mode::Off {
            return None;
        }
        // uinput modes: the application owns the on-screen text — the
        // Backspace is forwarded so it deletes the raw key shown there, and
        // bamboo follows so the composition stays aligned (unikey
        // convention: Backspace undoes the last raw key).
        if matches!(mode, Mode::Smooth | Mode::SuperSmooth | Mode::Select) {
            self.wrapper.backspace();
            if !self.wrapper.has_composition() {
                self.release_word_mode();
                // Nothing composed: the application deletes one of its own
                // characters — the raw forwarders (URL/emoji bypass) track
                // the screen in `tail`, so it has to shrink with the text.
                self.tail = replace_tail(&self.tail, 1, "");
                self.note_backspace_forwarded();
            }
            return None;
        }
        let Some(edit) = self.wrapper.backspace() else {
            // Nothing composed: the application deletes one of its own
            // characters, so the tracked tail has to shrink with it. Left
            // stale, the next macro rewrite deleted more characters than the
            // client held (an out-of-range delete, mutter aborts on it).
            self.tail = replace_tail(&self.tail, 1, "");
            self.note_backspace_forwarded();
            return None;
        };
        if !self.wrapper.has_composition() {
            self.release_word_mode();
        }
        match mode {
            Mode::Preedit => {
                let text = self.preedit_text();
                if text.is_empty() {
                    self.preedit_visible = false;
                    Some(vec![Action::HidePreedit])
                } else {
                    let cursor = text.chars().count() as u32;
                    Some(vec![Action::UpdatePreedit { text, cursor }])
                }
            }
            // Off is gated above; Auto never escapes effective_mode. Keeps
            // the match exhaustive.
            Mode::Off | Mode::Auto => None,
            Mode::Smooth | Mode::SuperSmooth | Mode::Select => None,
            Mode::NonPreedit | Mode::Surrounding | Mode::ForwardKeyEvent | Mode::ForwardSelect => {
                if !self.deletes_allowed() && !forwards_delete(mode) {
                    // The delete cannot reach the application: forward the
                    // Backspace so the app deletes the last character itself.
                    // Without a transport the screen holds the raw keys (mark
                    // keys are raw-forwarded, so no tone can move), which is
                    // exactly what the wrapper's raw-key undo models.
                    debug_log::log("backspace forwarded (no delete transport)");
                    self.tail = replace_tail(&self.tail, 1, "");
                    self.note_backspace_forwarded();
                    return Some(vec![Action::Forward]);
                }
                let actions = if self.charset == Charset::Unicode {
                    let mut actions = Vec::new();
                    if edit.delete_chars > 0 {
                        let insert_len = edit.insert.chars().count();
                        actions.push(self.delete_action(
                            mode,
                            self.delete_count(mode, edit.delete_chars as u32),
                            insert_len,
                        ));
                    }
                    if !edit.insert.is_empty() {
                        actions.push(Action::Commit(edit.insert.clone()));
                        self.tail = replace_tail(&self.tail, edit.delete_chars, &edit.insert);
                    } else {
                        self.tail = replace_tail(&self.tail, edit.delete_chars, "");
                    }
                    if actions.is_empty() {
                        actions.push(Action::Consumed);
                    }
                    actions
                } else {
                    self.rewrite(mode)
                };
                if !actions.is_empty() && actions != vec![Action::Consumed] {
                    if let Some(c) = self.tail.chars().last() {
                        self.note_char(c);
                    }
                }
                Some(actions)
            }
        }
    }

    /// Whether a preedit composition is pending that an editing key would
    /// commit right now. On relayed (mutter) contexts the commit and the
    /// forwarded key travel different transports, so a burst of arrow keys
    /// can move the caret before the commit lands — the word is then
    /// inserted mid-text ("em tên tùng" → "etùngm tên").
    pub fn preedit_commit_pending(&mut self) -> bool {
        // The visible preedit itself is what matters: capabilities can flip
        // mid-word and would otherwise make this report false while the word
        // is still on screen.
        !self.wrapper.output().is_empty()
            && (self.preedit_visible || self.effective_mode() == Mode::Preedit)
    }

    /// Commit whatever is composed and clear state. Used before the
    /// application sees a terminator or an editing key, and on a config
    /// reload. A client `Reset` does NOT come here: the preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so the client committed the word itself
    /// before the Reset arrived (`on_client_reset`).
    pub fn reset(&mut self) -> Vec<Action> {
        // Decide before hiding: a visible preedit must be committed even when
        // a capability flip resolves the mode to non-preedit now.
        let commit_preedit = self.preedit_visible || self.effective_mode() == Mode::Preedit;
        let pending = self.wrapper.output().to_string();
        debug_log::log(&format!(
            "reset: preedit_visible={} pending={} commit_preedit={commit_preedit}",
            self.preedit_visible,
            !pending.is_empty()
        ));
        let mut actions = Vec::new();
        // In non-preedit modes the text is already in the application; only a
        // preedit composition still needs committing, through the word-end
        // path (macros expand, invalid words restore). Must run before
        // `drop_pending_word` resets the wrapper.
        if commit_preedit && !pending.is_empty() {
            let raw = self.wrapper.raw().to_string();
            let valid = self.wrapper.is_valid();
            let valid_prefix = self.wrapper.is_valid_prefix();
            let mut finished =
                self.on_finish(Mode::Preedit, pending.clone(), raw, valid, valid_prefix);
            finished.retain(|a| !matches!(a, Action::Forward));
            actions.extend(finished);
        }
        actions.extend(self.drop_pending_word(true));
        actions
    }

    /// Clear the composition and all word state WITHOUT committing anything.
    /// Used on a client `Reset` and a focus change, where the client has
    /// already committed the preedit itself (`IBUS_ENGINE_PREEDIT_COMMIT`).
    ///
    /// `hide_preedit` is false on a focus change: the client owns the preedit
    /// display there and clears it itself, and our own `HidePreedit` can arrive
    /// before the client's commit in clients that handle `FocusOut` first —
    /// it would wipe the very word they are about to commit. ibus-bamboo's
    /// `FocusOut` is a no-op for the same reason.
    pub fn drop_pending_word(&mut self, hide_preedit: bool) -> Vec<Action> {
        self.wrapper.reset();
        let actions = if hide_preedit {
            self.hide_if_visible()
        } else {
            let _ = std::mem::take(&mut self.preedit_visible);
            Vec::new()
        };
        self.tail.clear();
        self.repair_delete = None;
        self.release_word_mode();
        self.url_word = false;
        self.emoji_word = false;
        actions
    }

    /// End the in-flight word because the app-rule picker is opening (the
    /// pick must never land mid-composition: apply_config on a live word
    /// desynced tail and wrapper — lost letters, duplicated words). Preedit
    /// clients hold nothing on the wire yet — the preedit is only a display —
    /// so the composed text commits here, exactly what is shown; inline
    /// clients already hold their raw characters, so only the bookkeeping
    /// drops and the screen keeps what it shows.
    pub fn end_word_for_picker(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.preedit_visible {
            let text = self.preedit_text();
            if !text.is_empty() {
                actions.push(Action::Commit(text));
            }
        }
        actions.extend(self.drop_pending_word(true));
        actions
    }

    /// Bookkeeping for text the server commits outside the key path (the
    /// pickers' insertions): `last_char` and the capitalize window must
    /// follow what the application received, or the next word mis-reads
    /// the context — a pick right after a literal ':' would leave the emoji
    /// shortcode branch armed and the next word would type raw.
    pub fn note_committed(&mut self, text: &str) {
        if let Some(c) = text.chars().last() {
            self.note_char(c);
        }
    }

    /// The client `Reset` method. GTK fires it when the user clicks in a text
    /// field and on focus changes. The preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so GTK has already committed the pending
    /// word to the field itself before this Reset arrives — committing again
    /// here would duplicate it. Only the internal composition is cleared,
    /// exactly like ibus-bamboo's `Reset`.
    pub fn on_client_reset(&mut self) -> Vec<Action> {
        debug_log::log("client reset → drop the pending word");
        self.drop_pending_word(true)
    }

    /// The client `FocusOut`/`Disable`/`FocusOutId`. The preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so the client commits the pending word
    /// itself on the way out and clears its own preedit display; the engine
    /// only discards its internal composition — no commit, and no HidePreedit,
    /// which in clients that handle `FocusOut` before committing could arrive
    /// first and wipe the very word they are about to commit (ibus-bamboo's
    /// `FocusOut` is a no-op for the same reason).
    pub fn focus_out(&mut self) -> Vec<Action> {
        debug_log::log("focus-out → drop the pending word, no signals");
        self.drop_pending_word(false)
    }

    /// Server-loop tick: flush deferred commits whose injected-key echoes
    /// never came back — the uinput rewrite's commit would otherwise never
    /// land when the user simply stops typing (the word stays raw latin).
    pub fn tick(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.take_stale_injection(Self::ECHO_STALE) {
            for text in self.flush_deferred() {
                actions.push(Action::Commit(text));
            }
        }
        actions
    }

    /// Milliseconds the main loop should wait before the next `tick` — the
    /// lost-echo stale window, or None when nothing is pending (the loop
    /// falls back to its 1 s poll).
    pub fn ms_until_tick(&self) -> Option<u64> {
        // Both echo kinds share the stale window; the earliest deadline wins.
        [
            (self.pending_injected > 0)
                .then_some(self.last_inject)
                .flatten(),
            (!self.pending_nav.is_empty())
                .then_some(self.last_nav_inject)
                .flatten(),
        ]
        .into_iter()
        .flatten()
        .map(|t| {
            Self::ECHO_STALE.as_millis() as u64
                - t.elapsed().as_millis().min(Self::ECHO_STALE.as_millis()) as u64
        })
        .min()
        .map(|ms| ms.max(1))
    }

    fn hide_if_visible(&mut self) -> Vec<Action> {
        if std::mem::take(&mut self.preedit_visible) {
            vec![Action::HidePreedit]
        } else {
            Vec::new()
        }
    }
}
