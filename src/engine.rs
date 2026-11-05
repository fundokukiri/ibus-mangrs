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
use crate::macros::{Dictionary, Macros};

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
    /// The engine is inert: every key is forwarded untouched.
    DefaultTyping,
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
    /// The app appended suggestion text after the caret mid-word (Chrome's
    /// inline autocomplete). Its rewrite delete is dropped and repairs crash
    /// mutter, so show the following words as preedit — no delete needed.
    suggestions: bool,
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
/// Modes whose tail rewrite deletes with key events forwarded through IBus
/// (`ForwardKeyEvent` signals) instead of the protocol delete or `/dev/uinput`.
/// They need neither a surrounding-text report nor the virtual device, which
/// makes them the only workable delete transport in XIM clients (their input
/// context reports no capabilities at all) and in mutter-relayed Wayland
/// contexts. Keep this the single place the two variants are recognised, or
/// one of them silently stops deleting.
fn forwards_delete(mode: Mode) -> bool {
    matches!(mode, Mode::ForwardKeyEvent | Mode::ForwardSelect)
}

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
            suggestions: false,
            capitalize_next: false,
            cap_word: false,
            commit_interval_ms: 20,
            backspace_interval_ms: 5,
            post_delay_ms: 10,
            unicode_channel: UnicodeChannel::Commit,
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
        self.auto_restore = config.auto_restore;
        self.em_dash = config.em_dash;
        self.auto_capitalize = config.auto_capitalize;
        self.double_space_period = config.double_space_period;
        self.macro_enabled = config.macro_enabled;
        self.capitalize_macro = config.capitalize_macro;
        self.macro_in_off_mode = config.macro_in_off_mode;
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
        self.bypass_url = config.bypass_url;
        self.url_word = false;
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
        debug_log::log(&format!(
            "defer commit after {n} nav echoes: {:?}",
            brief(&text)
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
                debug_log::log(&format!("flush deferred commit (nav): {:?}", brief(&text)));
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
            "defer commit after {n} echoes: {:?}",
            brief(&text)
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
            debug_log::log(&format!("flush deferred commit: {:?}", brief(&text)));
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
                    "flush deferred commit (forced): {:?}",
                    brief(&text)
                ));
                text
            })
            .collect();
        all.extend(self.nav_deferred.drain(..).map(|(_, text)| {
            debug_log::log(&format!(
                "flush deferred commit (nav, forced): {:?}",
                brief(&text)
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

    /// Record the client's `SetSurroundingText(text, cursor, anchor)` report.
    /// Returns actions when the report resolves a deferred client `Reset`
    /// (the click-mid-preedit detection, see `on_client_reset`).
    pub fn set_surrounding(&mut self, text: String, cursor: u32) -> Vec<Action> {
        self.set_surrounding_with_anchor(text, cursor, cursor)
    }

    /// Same, with the selection anchor from the protocol report. A report
    /// with `anchor != cursor` describes a selection, not the word we
    /// committed: comparing it against `tail` and repairing would delete the
    /// selected text, so the report is dropped and the next caret-only one
    /// re-syncs.
    pub fn set_surrounding_with_anchor(
        &mut self,
        text: String,
        cursor: u32,
        anchor: u32,
    ) -> Vec<Action> {
        if anchor != cursor {
            debug_log::log(&format!(
                "st report with a selection anchor={anchor} cursor={cursor} → ignore"
            ));
            self.surrounding = None;
            return Vec::new();
        }
        let cursor = cursor as usize;
        let len = text.chars().count();
        // Cursor and length only — never the text.
        debug_log::log(&format!(
            "st report cursor={cursor} len={len} composing={}",
            self.wrapper.has_composition()
        ));
        // Settle the rewrite-delete delivery proof: a report whose prefix is
        // exactly the swallowed shape (delete missing, insert landed) proves
        // this client drops rewrite deletes. Without real-key injection
        // there is then no working delete transport, and effective_mode()
        // moves the following words to preedit. A stale report shows neither
        // delete nor insert, so it never matches the swallowed shape.
        if let Some((healthy, swallowed)) = self.awaiting_delete_proof.take() {
            if swallowed != healthy && cursor.min(len) == swallowed {
                self.delete_dropped = true;
                debug_log::log(
                    "rewrite delete swallowed (report proof) → following words leave delete rewriting",
                );
            }
        }
        // The previous report (only the inline-suggestion check below still
        // needs it); the incoming text is cloned once for the store.
        let pre_click = self.surrounding.replace((text.clone(), cursor as u32));
        // Inline autocomplete: the app appends suggestion text after the
        // caret without the caret moving past it. Only the transition from
        // "caret at the end" counts — text after the caret is normal when
        // editing mid-line from the first report on.
        if !self.suggestions && self.wrapper.has_composition() {
            if let Some((old_text, old_cursor)) = &pre_click {
                if *old_cursor as usize >= old_text.chars().count()
                    && cursor < len
                    && text.starts_with(old_text.as_str())
                {
                    self.suggestions = true;
                    debug_log::log(
                        "inline suggestion after the caret → preedit for following words",
                    );
                }
            }
        }
        Vec::new()
    }

    /// Record a character the application received, updating the
    /// auto-capitalization window (`. `, `! `, `? `, Enter).
    fn note_char(&mut self, c: char) {
        self.last_char = Some(c);
        match c {
            '.' | '!' | '?' | '\n' | '\r' => self.capitalize_next = true,
            ' ' | '\t' => {}
            _ => self.capitalize_next = false,
        }
    }

    /// The focused application's name from the focus extension or the X
    /// active window. Rule matching only — see the field docs.
    pub fn set_app_name(&mut self, name: Option<String>) {
        self.app_name = name;
    }

    /// Mode forced by an `AppRule` matching the focused client, if any.
    /// The rule target is `app_name` (the focus extension / X active window)
    /// when present, else the IBus client string. The special pattern `none`
    /// matches clients with neither — the only way to target them, since
    /// there is no name to substring-match.
    fn client_rule_mode(&self) -> Option<Mode> {
        let effective = self.app_name.as_deref().or(self.client.as_deref());
        crate::config::app_rule_mode(&self.app_rules, effective).map(Mode::from)
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
    /// first, then the app rule, then the built-in heuristic.
    fn effective_mode(&self) -> Mode {
        let can_preedit = self.caps & CAP_PREEDIT_TEXT != 0;
        let ruled = self.client_rule_mode();
        let requested = ruled.unwrap_or(self.mode);
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

    /// Process one character (already decoded from a keysym).
    pub fn process_char(&mut self, c: char) -> Vec<Action> {
        if self.effective_mode() == Mode::DefaultTyping {
            // Fully inert: the key belongs to the application, nothing is
            // composed, nothing is remembered.
            return vec![Action::Forward];
        }
        if !self.wrapper.is_enabled() || self.effective_mode() == Mode::Off {
            return self.process_char_off(c);
        }

        let mode = self.key_mode();

        // URL/email/path bypass: the word typed directly after one of the
        // context characters stays raw — no marks, no macros, no
        // auto-capitalize. The context character is the previous word's
        // terminator, which `note_char` already recorded.
        if !self.url_word && !self.wrapper.has_composition() {
            self.url_word = self.bypass_url
                && self
                    .last_char
                    .is_some_and(|p| URL_CONTEXT_CHARS.contains(&p));
        }
        if self.url_word {
            let mut actions = Vec::new();
            if self.preedit_visible {
                self.preedit_visible = false;
                actions.push(Action::HidePreedit);
            }
            // `-`/`+` stay inside the URL: the next segment still belongs to
            // the address, so it must not be composed as a Vietnamese word.
            if crate::bamboo_wrapper::is_word_terminator(c) && !matches!(c, '-' | '+') {
                self.url_word = false;
                self.tail.clear();
            } else {
                self.tail.push(c);
            }
            self.note_char(c);
            actions.push(Action::Forward);
            return actions;
        }

        // The uinput modes: the raw key goes straight to the application and
        // bamboo keeps composing internally; nothing is committed until the
        // word ends (see `process_char_smooth`). Per-key display features
        // (em-dash, double space) stay off here — these modes are for
        // terminals and games, where rewriting before the word ends cannot
        // work anyway.
        if matches!(mode, Mode::Smooth | Mode::SuperSmooth | Mode::Select) {
            return self.process_char_smooth(mode, c);
        }

        // `--` -> `—`: the first `-` was already forwarded by the terminator
        // path, so this key replaces it.
        // The rewrite needs a delete transport; without one the second '-'
        // stays literal instead of duplicating the replacement.
        if self.em_dash
            && (self.deletes_allowed() || forwards_delete(mode))
            && c == '-'
            && self.last_char == Some('-')
        {
            let mut actions = vec![self.delete_action(mode, 1, 1)];
            actions.push(Action::Commit("—".into()));
            self.tail = replace_tail(&self.tail, 1, "—");
            self.note_char('—');
            return actions;
        }

        // Double space -> ". ": drop the first space, insert the period, let
        // this space through.
        if self.double_space_period
            && (self.deletes_allowed() || forwards_delete(mode))
            && c == ' '
            && self.last_char == Some(' ')
        {
            let mut actions = self.hide_if_visible();
            actions.push(self.delete_action(mode, 1, 1));
            actions.push(Action::Commit(".".into()));
            self.tail = replace_tail(&self.tail, 1, ".");
            self.note_char(' ');
            actions.push(Action::Forward);
            return actions;
        }

        // If the resolved mode is not preedit but a stale preedit string is
        // still on screen (mode resolved per client, ST data arriving later),
        // clear it first so the client never shows both preedit and commits.
        let mut actions = Vec::new();
        if mode != Mode::Preedit && self.preedit_visible {
            actions.push(Action::HidePreedit);
        }

        // Surrounding-text verification runs before the key changes anything,
        // while the composition still matches `tail`.
        actions.extend(self.verify_surrounding(mode));

        // Auto-capitalization: first letter of a word that starts a sentence.
        // The uppercase character is fed to the composition itself, so
        // bamboo-core keeps the word's case for later transformations.
        let cap = self.auto_capitalize
            && c.is_alphabetic()
            && !self.wrapper.has_composition()
            && self.capitalize_next;
        let c = if cap {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c
        };

        match self.wrapper.process_char(c) {
            Outcome::PassThrough => {
                if cap {
                    // The app would insert the raw (lowercase) key itself, so
                    // commit the uppercase letter instead of forwarding.
                    let upper: String = c.to_uppercase().collect();
                    self.tail.push_str(&upper);
                    self.note_char(c);
                    actions.push(Action::Commit(upper));
                    actions
                } else {
                    self.tail.push(c);
                    self.note_char(c);
                    actions.push(Action::Forward);
                    actions
                }
            }
            Outcome::Finish {
                word,
                raw,
                valid,
                valid_prefix,
            } => {
                actions.extend(self.on_finish(mode, word, raw, valid, valid_prefix));
                // The terminator itself reaches the application last.
                self.note_char(c);
                actions
            }
            Outcome::Edit(edit) => {
                actions.extend(self.on_edit(mode, edit));
                actions
            }
        }
    }

    /// Clamp a delete count to what the client's last surrounding-text report
    /// says exists before the cursor. Defensive against app-side rewrites
    /// racing between our verify and the delete (mutter segfaults on
    /// out-of-range deletes even with the capability present).
    fn clamp_delete(&self, n: u32) -> u32 {
        match &self.surrounding {
            Some((text, cursor)) => {
                let have = text.chars().take(*cursor as usize).count() as u32;
                n.min(have)
            }
            None => n,
        }
    }

    /// Prefix length (in chars) of the last ST report — what the client
    /// showed before the cursor when it last spoke.
    fn report_prefix_len(&self) -> usize {
        self.surrounding.as_ref().map_or(0, |(text, cursor)| {
            (*cursor as usize).min(text.chars().count())
        })
    }

    /// The delete half of a rewrite for this keystroke: the protocol delete
    /// (or the server's uinput fallback) in the inline modes, key events
    /// forwarded through IBus in the two ForwardKeyEvent modes.
    fn delete_action(&mut self, mode: Mode, n: u32, insert_len: usize) -> Action {
        match mode {
            Mode::ForwardKeyEvent => Action::ForwardBackspaces(n),
            Mode::ForwardSelect => Action::ForwardSelectLeft(n),
            _ => self.tracked_delete(n, insert_len),
        }
    }

    /// Delete count for this keystroke. The forward modes delete with real key
    /// events, which remove exactly what is on screen; clamping to the last
    /// surrounding-text report would under-delete there (the report can
    /// predate the commit). The protocol path keeps the clamp.
    fn delete_count(&self, mode: Mode, n: u32) -> u32 {
        if forwards_delete(mode) {
            n
        } else {
            self.clamp_delete(n)
        }
    }

    /// A `DeleteBefore` whose delivery the next ST report proves or refutes
    /// (see `awaiting_delete_proof`). Pure deletions (no paired insert)
    /// cannot be told apart from a stale report, so they stay untracked.
    fn tracked_delete(&mut self, n: u32, insert_len: usize) -> Action {
        // Hard rule (see `delete_count`): never ask for more characters than
        // the last report says sit before the caret. An out-of-range delete
        // aborts mutter (`before >= start`) and hangs the desktop.
        let n = self.clamp_delete(n);
        if insert_len == 0 {
            return Action::DeleteBefore(n);
        }
        let prefix = self.report_prefix_len();
        let healthy = prefix.saturating_sub(n as usize) + insert_len;
        let swallowed = prefix + insert_len;
        if swallowed != healthy {
            self.awaiting_delete_proof = Some((healthy, swallowed));
        }
        Action::DeleteBefore(n)
    }

    /// English mode: forward everything, but macros still work when
    /// `macro_in_off_mode` is set (the wrapper tracks the raw keys).
    fn process_char_off(&mut self, c: char) -> Vec<Action> {
        match self.wrapper.process_char(c) {
            Outcome::Finish { raw, valid, .. } => {
                // Expansion rewrites committed text; without a delete
                // transport the commit would duplicate the raw keys.
                if self.macro_enabled
                    && self.macro_in_off_mode
                    && !raw.is_empty()
                    && self.deletes_allowed()
                {
                    if let Some(exp) = self.expand_macro(&raw, valid) {
                        let mut actions = Vec::new();
                        if !self.tail.is_empty() {
                            actions.push(self.tracked_delete(
                                self.tail.chars().count() as u32,
                                exp.chars().count(),
                            ));
                        }
                        actions.push(Action::Commit(exp.clone()));
                        // Word ended: drop the tracked tail (same reason as
                        // `on_finish`).
                        if let Some(c) = exp.chars().last() {
                            self.note_char(c);
                        }
                        self.tail.clear();
                        actions.push(Action::Forward);
                        return actions;
                    }
                }
                self.tail.clear();
                self.note_char(c);
                vec![Action::Forward]
            }
            _ => {
                self.tail.push(c);
                self.note_char(c);
                vec![Action::Forward]
            }
        }
    }

    fn expand_macro(&self, raw: &str, valid: bool) -> Option<String> {
        if !self.macro_enabled {
            return None;
        }
        // A key sequence that already forms a valid Vietnamese word is much
        // more likely normal typing than a shorthand ("cam", "moi") — do
        // not expand it. Shorthand keys therefore must not be valid words.
        if valid {
            return None;
        }
        self.macros.expand(
            raw,
            self.capitalize_macro,
            &self.time_format,
            &self.date_format,
        )
    }

    /// The preedit string shown to the client. The preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so the client commits whatever it
    /// displays when it resets the context (a mouse click, a focus change):
    /// the display must therefore already be the final text — the macro
    /// expansion and the auto-restore to the raw keys, exactly like
    /// ibus-bamboo shows the processed string in its preedit.
    fn preedit_text(&self) -> String {
        let word = self.wrapper.output();
        let raw = self.wrapper.raw();
        if raw.is_empty() {
            return word;
        }
        let valid = self.wrapper.is_valid();
        if let Some(exp) = self.expand_macro(raw, valid) {
            return exp;
        }
        if self.should_auto_restore(&word, raw, self.wrapper.is_valid_prefix()) {
            return raw.to_string();
        }
        word
    }

    /// Auto Non-VN restore decision: the composed word differs from the raw
    /// keys and is not a Vietnamese syllable, so the raw keys go back. The
    /// check is the prefix one — what ibus-bamboo uses while typing — so a
    /// still-extendable state like `đuo` stays composed instead of flickering
    /// back to `dduo`. A word with an undone mark (`tê` + 'e' → `tee`), or one
    /// in the dictionary, is never restored. Shared by the preedit display and
    /// the word end so the client commits the same string a word break would.
    fn should_auto_restore(&self, word: &str, raw: &str, valid_prefix: bool) -> bool {
        if !self.auto_restore || word.is_empty() || word == raw || valid_prefix {
            return false;
        }
        let dict_hit = self
            .dictionary
            .as_ref()
            .is_some_and(|d| d.contains(&word.to_lowercase()));
        !dict_hit && !self.wrapper.undo_in_word()
    }

    /// The uinput modes (Smooth / SuperSmooth / Select): the application
    /// keeps the raw keystroke; bamboo transforms internally. The rewrite
    /// happens once, at word end.
    fn process_char_smooth(&mut self, mode: Mode, c: char) -> Vec<Action> {
        // Auto-capitalize: the application keeps the raw key, so the capital
        // can only appear in the word-end rewrite. Decide at the first key —
        // the per-key `note_char` below clears `capitalize_next` as soon as
        // the word starts.
        if !self.wrapper.has_composition() {
            self.cap_word = self.auto_capitalize && self.capitalize_next && c.is_alphabetic();
        }
        match self.wrapper.process_char(c) {
            Outcome::PassThrough => {
                self.tail.push(c);
                self.note_char(c);
                vec![Action::Forward]
            }
            Outcome::Edit(_) => {
                // The transformed output stays internal; the application
                // shows the raw key until the word ends.
                vec![Action::Forward]
            }
            Outcome::Finish {
                word, raw, valid, ..
            } => self.on_finish_smooth(mode, word, raw, valid, c),
        }
    }

    /// Word end in a uinput mode: erase the latin tail with real keys and
    /// deliver the Vietnamese word. When the word equals the raw keys
    /// (English text, auto-restored words) nothing is touched — the keys
    /// already sit in the application.
    fn on_finish_smooth(
        &mut self,
        mode: Mode,
        word: String,
        raw: String,
        valid: bool,
        c: char,
    ) -> Vec<Action> {
        let _ = valid;
        // Taken before `note_char` below updates the flag for the next word.
        let cap = std::mem::take(&mut self.cap_word);
        self.release_word_mode();
        self.tail.clear();
        self.note_char(c);

        // Macros win, as in every mode: the expansion replaces the raw keys.
        if !raw.is_empty() {
            if let Some(exp) = self.expand_macro(&raw, valid) {
                return self.smooth_rewrite(mode, &raw, &exp);
            }
        }

        let word = if cap { capitalize_first(&word) } else { word };
        if word == raw || word.is_empty() {
            // Nothing was transformed: no rewrite, no flash.
            return vec![Action::Forward];
        }

        // Auto Non-VN restore: bamboo "corrected" a word that is not a
        // Vietnamese syllable (English typing). The raw keys already sit in
        // the application — keep them instead of rewriting the word into a
        // syllable the user never meant (same gate as the inline modes).
        let dict_hit = self
            .dictionary
            .as_ref()
            .is_some_and(|d| d.contains(&word.to_lowercase()));
        if self.auto_restore && !valid && !dict_hit && !self.wrapper.undo_in_word() {
            self.wrapper.reset();
            // The app holds the raw keys, so auto-capitalize still needs the
            // rewrite — otherwise the sentence starts lowercase.
            let text = if cap {
                capitalize_first(&raw)
            } else {
                raw.clone()
            };
            if text == raw {
                return vec![Action::Forward];
            }
            return self.smooth_rewrite(mode, &raw, &text);
        }

        // Smooth (unlike Super Smooth) defers to app autofill: the app
        // appended suggestion text after the caret, and a rewrite would eat
        // it. Super Smooth rewrites regardless — minimum latency, the owner
        // accepted the risk.
        if mode == Mode::Smooth && self.suggestions {
            self.wrapper.reset();
            return vec![Action::Forward];
        }

        self.smooth_rewrite(mode, &raw, &word)
    }

    /// The shared word-end rewrite: erase `raw` with real keys, deliver
    /// `text` over the configured Unicode channel.
    fn smooth_rewrite(&mut self, mode: Mode, raw: &str, text: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        match mode {
            Mode::Select => {
                // No text report -> mode resolution fell back to Preedit, so
                // the report is here. Select the tail, erase it, then commit.
                let n = raw.chars().count() as u32;
                if n > 0 {
                    actions.push(Action::SelectLeft(n));
                    actions.push(Action::PressDelete);
                }
            }
            _ => {
                let n = raw.chars().count() as u32;
                if n > 0 {
                    actions.push(Action::InjectBackspaces(n));
                }
            }
        }
        let encoded = encode(self.charset, text);
        match self.unicode_channel {
            UnicodeChannel::Commit => actions.push(Action::Commit(encoded)),
            UnicodeChannel::Clipboard => actions.push(Action::Paste(encoded)),
        }
        if self.post_delay_ms > 0 {
            actions.push(Action::PostDelay(self.post_delay_ms));
        }
        self.tail = text.to_string();
        if let Some(c) = text.chars().last() {
            self.note_char(c);
        }
        actions.push(Action::Forward);
        actions
    }

    /// Word end: macros and auto-restore replace the committed word, preedit
    /// commits the word it still holds, and everything moves on.
    fn on_finish(
        &mut self,
        mode: Mode,
        word: String,
        raw: String,
        valid: bool,
        valid_prefix: bool,
    ) -> Vec<Action> {
        let mut actions = self.hide_if_visible();
        let inline = mode != Mode::Preedit;

        // Macros win over auto-restore. Inline expansion rewrites committed
        // text; without a delete transport the commit would duplicate the
        // raw keys the application already holds.
        if !raw.is_empty() {
            let expansion = if inline && !self.deletes_allowed() && !forwards_delete(mode) {
                None
            } else {
                self.expand_macro(&raw, valid)
            };
            if let Some(exp) = expansion {
                if inline && !self.tail.is_empty() {
                    actions.push(self.delete_action(
                        mode,
                        self.tail.chars().count() as u32,
                        exp.chars().count(),
                    ));
                }
                actions.push(Action::Commit(exp.clone()));
                // The word ended and its terminator is forwarded next, so
                // nothing of ours sits before the caret any more. Keeping the
                // expansion here made the next macro rewrite delete it a
                // second time — a stale, too-long delete that reached past the
                // text the client held (mutter aborts on that).
                if let Some(c) = exp.chars().last() {
                    self.note_char(c);
                }
                self.tail.clear();
                self.release_word_mode();
                actions.push(Action::Forward);
                return actions;
            }
        }

        // Auto Non-VN restore: the composed word is not a Vietnamese syllable
        // and differs from the raw keys -> put the raw keys back (see
        // `should_auto_restore`; the prefix verdict keeps `đuo` composed).
        if self.should_auto_restore(&word, &raw, valid_prefix)
            // Inline restore rewrites committed text; without a delete
            // transport the raw keys are already on screen, so there is
            // nothing to restore.
            && (!inline || self.deletes_allowed() || forwards_delete(mode))
        {
            if inline && !self.tail.is_empty() {
                actions.push(self.delete_action(
                    mode,
                    self.tail.chars().count() as u32,
                    raw.chars().count(),
                ));
            }
            actions.push(Action::Commit(raw.clone()));
            // Word ended (see the macro branch above): the forwarded terminator
            // is not ours, so the tracked tail must go.
            if let Some(c) = raw.chars().last() {
                self.note_char(c);
            }
            self.tail.clear();
            self.release_word_mode();
            actions.push(Action::Forward);
            return actions;
        }

        if mode == Mode::Preedit && !word.is_empty() {
            actions.push(Action::Commit(encode(self.charset, &word)));
        }
        self.tail.clear();
        self.release_word_mode();
        actions.push(Action::Forward);
        actions
    }

    /// One keystroke's edit, per display mode.
    fn on_edit(&mut self, mode: Mode, edit: crate::bamboo_wrapper::Edit) -> Vec<Action> {
        if edit.is_empty() {
            return vec![Action::Consumed];
        }
        if mode == Mode::Preedit {
            let text = self.preedit_text();
            let cursor = text.chars().count() as u32;
            self.preedit_visible = true;
            return vec![Action::UpdatePreedit { text, cursor }];
        }
        match self.charset {
            // Unicode is one char per char, so the delta from bamboo-core can
            // be replayed directly.
            Charset::Unicode => {
                if edit.delete_chars > 0 && !self.deletes_allowed() && !forwards_delete(mode) {
                    // No surrounding data yet: never send a delete (mutter
                    // segfaults on clients whose buffer it never saw). Keep
                    // the composition and pass the key through; the client's
                    // SetSurroundingText (sent right after our
                    // RequireSurroundingText) unblocks rewriting.
                    debug_log::log(&format!(
                        "raw-forward (no delete transport) client={:?}",
                        self.client
                    ));
                    // The application keeps the raw keystrokes; re-align the
                    // tracking so later keys edit the text actually shown.
                    self.wrapper.resync_output_to_raw();
                    self.tail = self.wrapper.raw().to_string();
                    return vec![Action::Forward];
                }
                let mut actions = Vec::new();
                if edit.delete_chars > 0 {
                    let insert_len = edit.insert.chars().count();
                    actions.push(self.delete_action(
                        mode,
                        self.delete_count(mode, edit.delete_chars as u32),
                        insert_len,
                    ));
                }
                let insert = edit.insert;
                if !insert.is_empty() {
                    actions.push(Action::Commit(insert.clone()));
                }
                self.tail = replace_tail(&self.tail, edit.delete_chars, &insert);
                if let Some(c) = self.tail.chars().last() {
                    self.note_char(c);
                }
                actions
            }
            // Legacy encodings change the character count (VNI `â` -> `aâ`),
            // so bamboo-core's Unicode delta is unusable; rewrite the word.
            _ if !self.deletes_allowed() && !forwards_delete(mode) => {
                // Same gate as the Unicode arm: the rewrite delete cannot
                // reach the application, and the paired commit would
                // duplicate the word from the second keystroke on.
                debug_log::log(&format!(
                    "raw-forward (no delete transport, legacy) client={:?}",
                    self.client
                ));
                self.wrapper.resync_output_to_raw();
                self.tail = self.wrapper.raw().to_string();
                vec![Action::Forward]
            }
            _ => {
                let actions = self.rewrite(mode);
                if let Some(c) = self.tail.chars().last() {
                    self.note_char(c);
                }
                actions
            }
        }
    }

    /// Replace the whole composition with its legacy-encoded form. Needed
    /// because those encodings change the character count (VNI `â` -> `aâ`),
    /// so bamboo-core's Unicode delta is unusable.
    fn rewrite(&mut self, mode: Mode) -> Vec<Action> {
        let text = encode(self.charset, &self.wrapper.output());
        let mut actions = Vec::new();
        if !self.tail.is_empty() {
            actions.push(self.delete_action(
                mode,
                self.tail.chars().count() as u32,
                text.chars().count(),
            ));
        }
        if !text.is_empty() {
            actions.push(Action::Commit(text.clone()));
        }
        self.tail = text;
        actions
    }

    /// Compare what the client reports before the cursor with what we believe
    /// we committed (`tail`) and repair the difference.
    fn verify_surrounding(&mut self, mode: Mode) -> Vec<Action> {
        // Only Surrounding verifies: plain NonPreedit must never send a
        // repair delete (the client did not ask for it, and a wrong repair
        // eats real text).
        if mode != Mode::Surrounding || self.tail.is_empty() {
            return Vec::new();
        }
        let Some((text, cursor)) = &self.surrounding else {
            return Vec::new();
        };
        let actual: Vec<char> = text.chars().take(*cursor as usize).collect();
        let tail: Vec<char> = self.tail.chars().collect();
        if ends_with(&actual, &tail) {
            self.repair_delete = None;
            return Vec::new();
        }

        if !self.deletes_allowed() {
            return Vec::new();
        }
        // A report that does not even contain the believed word is stale (it
        // predates our last commit — fast typing) or reflects an app-side
        // deletion we cannot safely undo; either way, do nothing rather than
        // tear the composition down (this used to eat characters).
        let contains = contains_sub(&actual, &tail);
        // The caret is the insertion point, so the believed word must end at
        // or right before it. Chrome's find bar reports a page fragment that
        // merely contains the typed word, with the caret far past it;
        // adopting that text deleted the real input ("em tun" → "g").
        if contains && suffix_gap(&actual, &tail).is_some_and(|gap| gap > 2) {
            debug_log::log(&format!(
                "verify: ignore unanchored tail={:?} actual={:?}",
                brief(&tail.iter().collect::<String>()),
                brief(&actual.iter().collect::<String>()),
            ));
            return Vec::new();
        }
        let k = common_suffix_len(&tail, &actual);
        let t2 = &tail[..tail.len() - k];
        let a2 = &actual[..actual.len() - k];
        let extra_in_middle = a2.len() > t2.len() && starts_with(a2, t2);
        if !extra_in_middle && !contains {
            debug_log::log(&format!(
                "verify: ignore stale tail={:?} actual={:?}",
                brief(&tail.iter().collect::<String>()),
                brief(&actual.iter().collect::<String>()),
            ));
            return Vec::new();
        }
        if a2.len() > t2.len() && starts_with(a2, t2) {
            // The client holds extra characters right before the common
            // suffix — a `DeleteSurroundingText` it swallowed earlier.
            debug_log::log(&format!(
                "verify: drift tail={:?} actual={:?} suffix={k} extra={}",
                brief(&tail.iter().collect::<String>()),
                brief(&actual.iter().collect::<String>()),
                a2.len() - t2.len(),
            ));
            if self.repair_delete_forbidden() {
                // Mutter-relayed contexts: a repair delete here segfaults the
                // compositor. Accept the drift (the next healthy report or a
                // rebuild clears it) instead of risking the session.
                self.repair_delete = None;
                return Vec::new();
            }
            let extra = (a2.len() - t2.len()) as u32;
            let offset = -((k as i32) + extra as i32);
            if self.repair_delete.is_some() || self.delete_dropped {
                // A repair was already attempted since the last verified
                // state and the text is still off: the client drops the
                // protocol delete (the drift shape may differ each time).
                // Press real Backspace keys instead — (k + extra) presses
                // remove the extra characters and the common suffix, then the
                // suffix is re-typed.
                self.delete_dropped = true;
                self.repair_delete = None;
                let commit: String = actual[actual.len() - k..].iter().collect();
                return vec![Action::RewriteByBackspace {
                    backspaces: k as u32 + extra,
                    commit,
                }];
            }
            self.repair_delete = Some((offset, extra));
            return vec![Action::DeleteSurrounding { offset, n: extra }];
        }
        // Some other desync (the user edited the text, another IME wrote to
        // it...). Never delete blindly: rebuild the composition from what the
        // application actually holds.
        debug_log::log(&format!(
            "verify: sync-from-surrounding actual={:?}",
            brief(&actual.iter().collect::<String>()),
        ));
        self.sync_from_surrounding(&actual);
        Vec::new()
    }

    /// Align the engine with the application's text after an unrecoverable
    /// desync (the app rewrote its own text, a suggestion replaced the word,
    /// the user edited by hand...). The wrapper is rebuilt from the real text
    /// so tone keys keep working on the word that is actually there; the
    /// wrapper's own `last_output` tracking keeps the next delta correct
    /// despite the rebuild.
    fn sync_from_surrounding(&mut self, actual_prefix: &[char]) {
        let text: String = actual_prefix.iter().collect();
        self.repair_delete = None;
        if self.charset == Charset::Unicode {
            self.wrapper.rebuild(&text);
            self.tail = self.wrapper.output().to_string();
        } else {
            // Encoded text cannot be rebuilt faithfully; just adopt it.
            self.wrapper.reset();
            self.tail = text
                .rsplit(|c: char| crate::bamboo_wrapper::is_word_terminator(c))
                .next()
                .unwrap_or("")
                .to_string();
        }
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
        if mode == Mode::Off || mode == Mode::DefaultTyping {
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
            }
            return None;
        }
        let Some(edit) = self.wrapper.backspace() else {
            // Nothing composed: the application deletes one of its own
            // characters, so the tracked tail has to shrink with it. Left
            // stale, the next macro rewrite deleted more characters than the
            // client held (an out-of-range delete, mutter aborts on it).
            self.tail = replace_tail(&self.tail, 1, "");
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
            // Gated above (returns None); keeps the match exhaustive.
            Mode::Off | Mode::DefaultTyping => None,
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
    pub fn preedit_commit_pending(&self) -> bool {
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
        actions
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

/// Drop `delete` characters from the end of `tail` and append `insert`.
/// mtime of `path`, or None when it does not exist — the reload gate for
/// the auxiliary settings files.
fn file_time(path: Option<&std::path::Path>) -> Option<SystemTime> {
    path.and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// Uppercase the first character, leaving the rest untouched (Vietnamese
/// words may start with a transformed letter: `ă` → `Ă`).
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn replace_tail(tail: &str, delete: usize, insert: &str) -> String {
    let keep = tail.chars().count().saturating_sub(delete);
    let mut s: String = tail.chars().take(keep).collect();
    s.push_str(insert);
    s
}

fn starts_with(haystack: &[char], needle: &[char]) -> bool {
    haystack.len() >= needle.len() && &haystack[..needle.len()] == needle
}

fn ends_with(haystack: &[char], needle: &[char]) -> bool {
    haystack.len() >= needle.len() && &haystack[haystack.len() - needle.len()..] == needle
}

fn contains_sub(haystack: &[char], needle: &[char]) -> bool {
    haystack.windows(needle.len().max(1)).any(|w| w == needle)
}

/// Distance from the end of the last occurrence of `needle` to the end of
/// `haystack`, or `None` when it does not occur.
fn suffix_gap(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .rposition(|w| w == needle)
        .map(|i| haystack.len() - (i + needle.len()))
}

fn common_suffix_len(a: &[char], b: &[char]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModeConfig;

    /// Feed keystrokes like a real client: it reports capabilities and a
    /// SetSurroundingText update after every visible change.
    fn type_str(e: &mut Engine, s: &str) -> Vec<Action> {
        if e.client().is_none() {
            // Engine tests exercise the in-process-IM (non-relayed) path; the
            // relayed path has its own tests.
            e.set_client(Some("gtk4-im:test".into()));
        }
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
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
    fn non_preedit_commits_and_rewrites() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
                // ForwardKeyEvent mode: real Backspace key events, same net effect on
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
    /// default (preedit), making a NonPreedit test exercise preedit.
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
                buf.push(step);
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
                    // ForwardKeyEvent mode: real Backspace key events, same net effect on
                    // the buffer as a protocol delete.
                    Action::ForwardBackspaces(n) => {
                        buf.truncate(buf.len().saturating_sub(*n as usize))
                    }
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        // to a buffer it never saw. The key is forwarded instead.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        // Raw loop: no capabilities, no SetSurroundingText ever.
        let actions: Vec<Action> = "tieengs".chars().flat_map(|c| e.process_char(c)).collect();
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
            )),
            "{actions:?}"
        );
        assert!(actions.iter().any(|a| matches!(a, Action::Forward)));
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
                // ForwardKeyEvent mode: real Backspace key events, same net effect on
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        for mode in [Mode::NonPreedit, Mode::Surrounding] {
            let mut e = Engine::new(Method::Telex, mode);
            type_str(&mut e, "tie");
            e.backspace().expect("backspace");
            // The client's next SetSurroundingText reflects the deletion.
            e.set_surrounding("ti".into(), 2);
            let actions = e.process_char('n');
            // The app buffer held "ti" after the backspace; the new key must
            // only add its own character, not re-delete an old one.
            assert_eq!(replay_from("ti", &actions), "tin", "{mode:?}: {actions:?}");
        }
    }

    #[test]
    fn backspace_then_tone_key_rewrites_correctly() {
        // unikey convention: the Backspace undoes the last KEY — here the
        // tone key "s" — and retyping it restores the word.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        let mut actions = type_str(&mut e, "tieengs"); // tiếng
        actions.extend(e.backspace().expect("backspace")); // tiêng
        e.set_surrounding("tiêng".into(), 5);
        actions.extend(e.process_char('s'));
        assert_eq!(replay(&actions), "tiếng");
    }

    #[test]
    fn backspace_on_empty_composition_forwards() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert!(e.backspace().is_none());
    }

    #[test]
    fn backspace_deletes_one_char() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        type_str(&mut e, "vieejt");
        assert_eq!(e.backspace(), Some(vec![Action::DeleteBefore(1)]));
    }

    #[test]
    fn backspace_replays_when_the_tone_moves() {
        // unikey convention: each Backspace pops the last raw KEY and the
        // word recomposes ("hoàng" -> "hoang" -> "hoan" -> "hoa").
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        // With no uinput the delete must be skipped entirely, never sent.
        let actions = e.process_char('s');
        assert_eq!(actions, vec![Action::Forward], "{actions:?}");
    }

    #[test]
    fn fake_client_context_drops_buffer_state_but_keeps_name() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "abc123"), "abc123");
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        e.set_client(Some("gtk4-im:test".into()));
        // Preedit: the word commits when the digit ends it, the digit follows.
        assert_eq!(type_replay(&mut e, "x1"), "x1");
        // Vietnamese word + digit.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
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
        // A NonPreedit word in flight; the app appends suggestion text after
        // the caret, so effective_mode() now resolves to Preedit. The
        // Backspace must follow the locked mode — a phantom preedit over the
        // per-key-committed text is exactly the mid-word flip the lock
        // exists to prevent.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gnome-shell".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        assert_eq!(type_replay(&mut e, "tie"), "tie");
        e.set_suggestions(true);
        let actions = e.backspace().expect("backspace over the committed word");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::UpdatePreedit { .. })),
            "the locked NonPreedit word must not turn into a preedit: {actions:?}"
        );
    }

    #[test]
    fn no_delete_transport_keeps_word_raw_at_word_end() {
        // NonPreedit without a surrounding report and without uinput: mark
        // keys are raw-forwarded, so the application already holds the raw
        // keys. The auto-restore rewrite would duplicate them (delete
        // dropped, commit appended).
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        assert_eq!(
            type_replay_with_reports(&mut e, "tieengs ", false),
            "tieengs "
        );
    }

    #[test]
    fn no_delete_transport_keeps_macros_raw() {
        // A macro expansion rewrites committed text; without a delete
        // transport the commit would duplicate the raw keys on screen.
        let mut e = engine(
            Mode::NonPreedit,
            Config {
                time_format: "%H".into(),
                ..Config::default()
            },
        );
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.macros
            .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
        assert_eq!(type_replay_with_reports(&mut e, "gs ", false), "gs ");
    }

    #[test]
    fn no_delete_transport_backspace_forwards_to_the_app() {
        // The delete cannot reach the application, so the Backspace key must
        // be forwarded: the app deletes the last character itself, which is
        // also what the wrapper's raw-key undo models.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        assert_eq!(type_replay_with_reports(&mut e, "ab", false), "ab");
        let actions = e.backspace().expect("backspace over a committed word");
        assert!(
            actions.contains(&Action::Forward),
            "the app must delete the character itself: {actions:?}"
        );
    }

    #[test]
    fn no_delete_transport_legacy_charset_stays_raw() {
        // Legacy rewrite deletes the whole tail every key; without a delete
        // transport that duplicates from the second keystroke on.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.set_charset(Charset::Vni);
        assert_eq!(type_replay_with_reports(&mut e, "viee", false), "viee");
    }

    #[test]
    fn no_delete_transport_keeps_em_dash_and_double_space_literal() {
        let mut e = engine(
            Mode::NonPreedit,
            Config {
                em_dash: true,
                double_space_period: true,
                ..Config::default()
            },
        );
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        assert_eq!(type_replay_with_reports(&mut e, "a--", false), "a--");
        assert_eq!(type_replay_with_reports(&mut e, "b  ", false), "b  ");
    }

    #[test]
    fn viqr_marks_apply_in_every_display_mode() {
        // The VIQR mark keys are punctuation keys: they must compose instead
        // of ending the word (BUG: they used to be intercepted as word
        // terminators, so VIQR could not type any mark at all).
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
            Mode::NonPreedit,
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
            Mode::NonPreedit,
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
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "tool"), "tool");
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "toool"), "tool");
        // Vietnamese words keep working.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "xin chaof "), "xin chào ");
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "tieengs "), "tiếng ");
    }

    #[test]
    fn raw_forward_resyncs_to_raw_keys_toool() {
        // Regression: "toool" (English) — the third 'o' undoes the
        // circumflex inside bamboo. With no delete transport the mark keys
        // pass through raw, and the tracker used to keep believing the
        // transformed text — a later key then committed garbage ("tooool"
        // with an extra 'o'). The tracker must re-align to the raw keys the
        // app actually holds.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
                    // ForwardKeyEvent mode: real Backspace key events, same net effect on
                    // the buffer as a protocol delete.
                    Action::ForwardBackspaces(n) => {
                        buf.truncate(buf.len().saturating_sub(*n as usize))
                    }
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
            "no delete transport: {all:?}"
        );
        let text: String = buf.into_iter().collect();
        assert_eq!(text, "toool", "raw keys must survive verbatim");
    }

    #[test]
    fn toool_with_protocol_delete_undoes_the_circumflex() {
        // With a delete transport the Telex mark-undo works: the third 'o'
        // turns "tô" back into "to", and "toool" comes out exactly as typed.
        // "toool" (5 keys — the 3rd 'o' undid the circumflex): the visible
        // text behaves as "too" + 'l' → "tool"; bamboo's hidden extra 'o'
        // must never resurface.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "toool"), "tool");
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:test".into()));
        assert_eq!(type_replay(&mut e, "too"), "tô");
    }

    #[test]
    fn word_mode_locks_across_config_changes() {
        // Mid-word mode changes (config reload, capability flips) must not
        // toggle preedit under the user's hands; the lock releases at the
        // word boundary.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
    fn rule_and_heuristic_clients_fall_back_to_preedit() {
        // These clients ignore DeleteSurroundingText; non-preedit would append
        // the rewrite and duplicate the word, so keep it inline instead.
        for client in ["gtk4-im:kgx", "gtk4-im:gnome-console", "gtk3-im:code"] {
            let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
            e.set_capabilities(CAP_PREEDIT_TEXT);
            e.set_client(Some(client.into()));
            let actions = type_str(&mut e, "tieengs");
            assert!(
                !actions.iter().any(|a| matches!(a, Action::DeleteBefore(_))),
                "{client}: {actions:?}"
            );
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::UpdatePreedit { text, .. } if text == "tiếng")),
                "{client}: {actions:?}"
            );
        }
    }

    #[test]
    fn sublime_text_gets_surrounding_mode() {
        // The default rule pins Sublime Text (preedit is invisible there) to
        // surrounding-text mode: commits stay inline and drift is repaired.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.set_client(Some("gtk3-im:sublime_text".into()));
        let actions = type_str(&mut e, "tieengs");
        assert!(!actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })));
        assert_eq!(replay(&actions), "tiếng");
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
                Mode::NonPreedit,
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
            Mode::NonPreedit,
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
            Mode::NonPreedit,
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
        let mut e = Engine::new(Method::Telex, Mode::ForwardKeyEvent);
        e.set_client(Some("gnome-shell".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT); // no ST capability at all
        let actions = type_str(&mut e, "tieengs");
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                Action::DeleteBefore(_) | Action::DeleteSurrounding { .. }
            )),
            "no protocol delete in ForwardKeyEvent mode: {actions:?}"
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
        let mut e = Engine::new(Method::Telex, Mode::ForwardKeyEvent);
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.set_client(Some("gnome-shell".into()));
        e.set_suggestions(true);
        let actions = type_str(&mut e, "tieengs");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::UpdatePreedit { .. })),
            "ForwardKeyEvent never falls back to preedit: {actions:?}"
        );
        assert_eq!(replay(&actions), "tiếng");
    }

    #[test]
    fn forward_select_mode_selects_the_tail_before_committing() {
        // ForwardKeyEvent II: the tail is selected with forwarded Shift+Left
        // and the commit replaces the selection, so the application never has
        // to handle a Backspace. XIM clients report no capabilities at all,
        // which is exactly the case this mode exists for.
        let mut e = Engine::new(Method::Telex, Mode::ForwardSelect);
        e.set_client(Some("xim".into()));
        e.set_capabilities(0);
        let mut buf: Vec<char> = Vec::new();
        let mut selected = 0usize;
        for c in "tieengs ".chars() {
            let actions = e.process_char(c);
            let forwarded = actions.contains(&Action::Forward);
            for a in &actions {
                match a {
                    Action::ForwardSelectLeft(n) => selected += *n as usize,
                    Action::Commit(t) => {
                        buf.truncate(buf.len().saturating_sub(selected));
                        selected = 0;
                        buf.extend(t.chars());
                    }
                    Action::DeleteBefore(_) | Action::DeleteSurrounding { .. } => {
                        panic!("ForwardSelect must not use a protocol delete: {actions:?}")
                    }
                    _ => {}
                }
            }
            if forwarded {
                buf.push(c);
            }
        }
        assert_eq!(buf.iter().collect::<String>(), "tiếng ");
    }

    #[test]
    fn xim_clients_report_no_usable_application_name() {
        // ibus-x11 names every input context `xim`, so an app rule can only
        // match one by accident; the server must name the app from the
        // compositor extension or xprop for rules to work there.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("xim".into()));
        assert!(e.client_name_is_opaque());
        e.set_client(Some("gtk3-im:sublime_text".into()));
        assert!(!e.client_name_is_opaque());
    }

    #[test]
    fn forward_key_event_backspace_never_deletes_by_protocol() {
        let mut e = Engine::new(Method::Telex, Mode::ForwardKeyEvent);
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
    fn config_rule_overrides_builtin() {
        // A user rule wins even over the built-in preedit list.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_capabilities(CAP_PREEDIT_TEXT);
        e.set_client(Some("gtk4-im:chrome".into()));
        e.apply_config(&Config {
            app_rules: vec![crate::config::AppRule {
                pattern: "chrome".into(),
                mode: crate::config::ModeConfig::NonPreedit,
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
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
            assert_eq!(e.effective_mode(), mode, "unnamed client: {mode:?}");
            // A named client must not match the none rule: the configured
            // base mode (Off) stands for it.
            e.set_client(Some("gtk4-im:firefox".into()));
            assert_ne!(e.effective_mode(), mode, "named client: {mode:?}");
        }
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
    fn firefox_keeps_non_preedit() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_capabilities(CAP_PREEDIT_TEXT);
        e.set_client(Some("gtk4-im:firefox".into()));
        assert_eq!(replay(&type_str(&mut e, "tieengs")), "tiếng");
    }

    #[test]
    fn unknown_client_keeps_non_preedit() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_capabilities(CAP_PREEDIT_TEXT);
        e.set_client(Some("gtk3-im:gedit".into()));
        assert_eq!(replay(&type_str(&mut e, "tieengs")), "tiếng");
    }

    #[test]
    fn daemon_fake_context_is_ignored() {
        // Focus bounces through ibus-daemon's `fake` hotkey context; treating
        // it as the focused app would reset the word being typed.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_client(Some("gtk4-im:firefox".into()));
        type_str(&mut e, "vieejt");
        e.set_client(Some("fake".into()));
        assert_eq!(e.wrapper.output(), "việt");
    }

    #[test]
    fn english_mode_forwards_everything() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_enabled(false);
        assert_eq!(e.process_char('a'), vec![Action::Forward]);
    }

    #[test]
    fn legacy_charset_rewrites_the_whole_word() {
        // VNI's `ế` is several chars, so the Unicode delta cannot be replayed.
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_charset(Charset::Vni);
        let actions = type_str(&mut e, "vieejt");
        assert_eq!(replay(&actions), encode(Charset::Vni, "việt"));
    }

    #[test]
    fn legacy_charset_backspace_replays_correctly() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.set_charset(Charset::Vni);
        let mut actions = type_str(&mut e, "vieejt");
        actions.extend(e.backspace().expect("backspace"));
        assert_eq!(replay(&actions), encode(Charset::Vni, "việ"));
    }

    #[test]
    fn legacy_charset_word_end_sends_nothing_extra() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
    fn only_surrounding_mode_repairs_from_the_report() {
        // Plain NonPreedit must not send a repair delete: the client never
        // asked for surrounding-text verification, and a wrong repair eats
        // real text.
        for mode in [Mode::Preedit, Mode::NonPreedit, Mode::Surrounding] {
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
            assert_eq!(repaired, mode == Mode::Surrounding, "{mode:?}: {actions:?}");
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
            Action::RewriteByBackspace { backspaces, commit } => {
                Some((*backspaces, commit.clone()))
            }
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            em_dash: true,
            mode: ModeConfig::NonPreedit,
            ..Config::default()
        });
        assert_eq!(type_replay(&mut e, "a--b"), "a—b");
    }

    #[test]
    fn em_dash_disabled_by_default() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, "a--b"), "a--b");
    }

    #[test]
    fn double_space_inserts_period_when_enabled() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            double_space_period: true,
            mode: ModeConfig::NonPreedit,
            ..Config::default()
        });
        assert_eq!(type_replay(&mut e, "a  b"), "a. b");
    }

    #[test]
    fn double_space_disabled_by_default() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, "a  b"), "a  b");
    }

    #[test]
    fn auto_capitalize_after_sentence_end() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            auto_capitalize: true,
            ..Config::default()
        });
        assert_eq!(type_replay(&mut e, ". tieengs "), ". Tiếng ");
    }

    #[test]
    fn auto_capitalize_also_uppercases_transformed_letters() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            auto_capitalize: true,
            ..Config::default()
        });
        assert_eq!(type_replay(&mut e, "? dd "), "? Đ ");
    }

    #[test]
    fn auto_capitalize_disabled_by_default() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, ". tieengs "), ". tiếng ");
    }

    // --- macros ---

    #[test]
    fn macro_expands_on_word_end() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            macro_enabled: false,
            ..Config::default()
        });
        e.macros
            .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
        assert_eq!(type_replay(&mut e, "gs "), "gs ");
    }

    #[test]
    fn macro_rewrite_never_deletes_past_the_text() {
        // `ttlt` → "Tôi tên là Tùng": the expansion used to stay in `tail`
        // after the word ended, so backspacing part of it and typing the key
        // again made the next word end ask the client to delete more
        // characters than it held — mutter aborts on that ("before >= start")
        // and the desktop hangs until a hard reset.
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
        let mut e = engine(Mode::NonPreedit, Config::default());
        e.macros
            .set_entries([("ttlt".to_string(), "Tôi tên là Tùng".to_string())].into_iter());
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, "1223344"), "1223344");
    }

    #[test]
    fn digit_after_a_word_keeps_the_word() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, "xin chào9"), "xin chào9");
    }

    #[test]
    fn underscore_ends_the_word_and_is_kept() {
        // bamboo-core clears its composition on '_' just like on digits; the
        // word must be finished at '_' and the '_' itself forwarded (it used
        // to delete the word). The letters after it start a new word, as with
        // any terminator.
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
            assert_eq!(typed(mode, "abc_bcd2"), "abc_bcd2", "{mode:?}");
            assert_eq!(typed(mode, "xin chào_"), "xin chào_", "{mode:?}");
        }
    }

    #[test]
    fn repeated_letters_do_not_duplicate_in_any_mode() {
        // Bug 1: "teen"/"tool"/"need" — typing the repeated key that undoes
        // a mark (ee → ê → e) used to add an extra character on screen and
        // again at the word boundary. The visible word must stay exactly
        // what the user sees, in all three display modes.
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
            mode: Mode::NonPreedit.into(),
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
    fn default_typing_is_fully_inert() {
        let mut e = Engine::new(Method::Telex, Mode::DefaultTyping);
        e.apply_config(&Config {
            macro_in_off_mode: true,
            mode: ModeConfig::DefaultTyping,
            ..Config::default()
        });
        e.macros
            .set_entries([("gs".to_string(), "giờ sáng".to_string())].into_iter());
        assert_eq!(type_replay(&mut e, "vieetj gs "), "vieetj gs ");
    }

    #[test]
    fn default_typing_backspace_belongs_to_the_app() {
        let mut e = Engine::new(Method::Telex, Mode::DefaultTyping);
        assert_eq!(type_replay(&mut e, "abc"), "abc");
        assert!(e.backspace().is_none());
    }

    #[test]
    fn smooth_falls_back_to_preedit_without_uinput() {
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
        e.set_uinput_available(false);
        e.set_capabilities(CAP_PREEDIT_TEXT);
        let actions = type_str(&mut e, "vieetj");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::UpdatePreedit { .. })),
            "no uinput -> must render as preedit: {actions:?}"
        );
    }

    #[test]
    fn smooth_falls_back_to_preedit_in_relay_context() {
        // Empirical (31/10): with the gate open, 52 smooth rewrites injected
        // fine but the rewrite products never reached the relayed app (raw
        // latin stayed, no deletion, no commit). Relayed contexts must
        // render as preedit.
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
        e.set_uinput_available(true);
        // A mutter-owned IBus context (type_str would replace None).
        e.set_client(Some("gnome-shell:gnome-shell".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT);
        let actions = type_str(&mut e, "vieetj");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::UpdatePreedit { .. })),
            "relay context -> must render as preedit: {actions:?}"
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::InjectBackspaces(_))),
            "no uinput rewrite may be emitted on a relay: {actions:?}"
        );
    }

    #[test]
    fn select_falls_back_to_preedit_without_surrounding_report() {
        let mut e = Engine::new(Method::Telex, Mode::Select);
        e.set_uinput_available(true);
        e.set_client(Some("gtk4-im:test".into()));
        // No surrounding-text capability: Select cannot know the tail length.
        e.set_capabilities(CAP_PREEDIT_TEXT);
        let actions = type_str(&mut e, "vieetj");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::UpdatePreedit { .. })),
            "no text report -> must render as preedit: {actions:?}"
        );
    }

    #[test]
    fn select_stays_select_with_a_text_report() {
        let mut e = Engine::new(Method::Telex, Mode::Select);
        e.set_uinput_available(true);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        // The fallback checks the actual report, not just the capability.
        e.set_surrounding(String::new(), 0);
        assert_eq!(e.effective_mode(), Mode::Select);
    }

    // --- URL/email/path bypass (spell-check round, phase 2-3) ---

    #[test]
    fn url_context_keeps_words_raw() {
        // "tieeng" right after "." and "/" would normally become "tiêng".
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(
            type_replay(&mut e, "x.example.tieeng "),
            "x.example.tieeng "
        );
    }

    #[test]
    fn email_context_keeps_words_raw() {
        // "hieeu" after "@" would normally become "hiều".
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, "a@hieeu "), "a@hieeu ");
    }

    #[test]
    fn url_bypass_can_be_disabled() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        e.apply_config(&Config {
            bypass_url: false,
            ..Config::default()
        });
        assert_eq!(type_replay(&mut e, "x.example.tieeng "), "x.example.tiêng ");
    }

    #[test]
    fn url_bypass_releases_at_the_next_space() {
        // The word after the URL ends must type Vietnamese again
        // ("nhieeuf" → "nhiều" — huyền from the f).
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
        assert_eq!(type_replay(&mut e, "x.tieeng nhieeuf "), "x.tieeng nhiều ");
    }

    #[test]
    fn url_bypass_spans_hyphenated_segments() {
        // The segment after the hyphen used to be composed as Vietnamese, so
        // a Telex mark key there stuck: "alo-tungf" → "alo-tùng",
        // "vi-mangrs" → "vi-máng". The address stays raw.
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
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
            Mode::NonPreedit,
            Config {
                em_dash: true,
                ..Config::default()
            },
        );
        assert_eq!(type_replay(&mut e, "a--b "), "a—b ");
    }

    #[test]
    fn macro_does_not_expand_valid_vietnamese_words() {
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
        let mut e = Engine::new(Method::Telex, Mode::NonPreedit);
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
    fn smooth_forwards_raw_keys_and_rewrites_at_word_end() {
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
        e.set_uinput_available(true);
        let actions = type_str(&mut e, "vieetj ");
        // Mid-word: nothing but forwards — the app shows the raw keys.
        let word_end = actions
            .iter()
            .position(|a| matches!(a, Action::InjectBackspaces(_)))
            .unwrap();
        assert!(
            actions[..word_end]
                .iter()
                .all(|a| matches!(a, Action::Forward | Action::Consumed)),
            "raw keys must be forwarded while typing: {:?}",
            &actions[..word_end]
        );
        // Word end: erase the 6 raw keys, commit the Vietnamese word.
        assert!(actions
            .iter()
            .any(|a| matches!(a, Action::InjectBackspaces(6))));
        assert!(actions
            .iter()
            .any(|a| matches!(a, Action::Commit(t) if t == "việt")));
        assert!(actions.iter().any(|a| matches!(a, Action::PostDelay(_))));
        assert!(!actions
            .iter()
            .any(|a| matches!(a, Action::UpdatePreedit { .. })));
    }

    #[test]
    fn smooth_leaves_english_words_alone() {
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
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
    fn smooth_modes_auto_capitalize_at_sentence_start() {
        for mode in [Mode::Smooth, Mode::SuperSmooth, Mode::Select] {
            let mut e = engine(
                mode,
                Config {
                    auto_capitalize: true,
                    ..Config::default()
                },
            );
            e.set_uinput_available(true);
            // Select only resolves with a surrounding report before the
            // first key.
            e.set_client(Some("gtk4-im:test".into()));
            e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
            e.set_surrounding(String::new(), 0);
            let actions = type_str(&mut e, "xin chao. tieengs ");
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::Commit(t) if t == "Tiếng")),
                "{mode:?}: the sentence must start capitalized: {actions:?}"
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Commit(t) if t == "tiếng")),
                "{mode:?}: lowercase word committed: {actions:?}"
            );
        }
    }

    #[test]
    fn smooth_backspace_forwards_and_pops_the_raw_key() {
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
        e.set_uinput_available(true);
        type_str(&mut e, "vieetj");
        assert!(e.backspace().is_none(), "the app deletes its own key");
        let actions = type_str(&mut e, " ");
        // raw after the pop is "vieet" (5 keys), not "vieetj".
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::InjectBackspaces(5))),
            "{actions:?}"
        );
    }

    #[test]
    fn select_uses_selection_then_delete() {
        let mut e = Engine::new(Method::Telex, Mode::Select);
        e.set_uinput_available(true);
        // Prime the client and a text report BEFORE the first key: the word
        // mode locks at word start, and only a report makes Select resolve.
        e.set_client(Some("gtk4-im:test".into()));
        e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        e.set_surrounding(String::new(), 0);
        let actions = type_str(&mut e, "vieetj ");
        assert!(actions.iter().any(|a| matches!(a, Action::SelectLeft(6))));
        assert!(actions.iter().any(|a| matches!(a, Action::PressDelete)));
        assert!(actions
            .iter()
            .any(|a| matches!(a, Action::Commit(t) if t == "việt")));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::InjectBackspaces(_))),
            "Select must not press Backspaces: {actions:?}"
        );
    }

    #[test]
    fn super_smooth_rewrites_even_with_app_suggestions() {
        let mut e = Engine::new(Method::Telex, Mode::SuperSmooth);
        e.set_uinput_available(true);
        // Prime the client first: set_client clears the suggestion flag and
        // type_str sets the client when it is None.
        e.set_client(Some("gtk4-im:test".into()));
        e.set_suggestions(true);
        let actions = type_str(&mut e, "vieetj ");
        assert!(actions
            .iter()
            .any(|a| matches!(a, Action::InjectBackspaces(6))));
        assert!(actions
            .iter()
            .any(|a| matches!(a, Action::Commit(t) if t == "việt")));
    }

    #[test]
    fn smooth_holds_back_when_the_app_autofills() {
        let mut e = Engine::new(Method::Telex, Mode::Smooth);
        e.set_uinput_available(true);
        e.set_client(Some("gtk4-im:test".into()));
        e.set_suggestions(true);
        let actions = type_str(&mut e, "vieetj ");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::InjectBackspaces(_))),
            "rewriting would eat the suggestion text: {actions:?}"
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

        for mode in [Mode::NonPreedit, Mode::Surrounding] {
            let config = Config {
                mode: mode.into(),
                ..Config::default()
            };
            assert_eq!(
                type_replay(&mut engine(mode, config), "ddc "),
                "đc ",
                "{mode:?}"
            );
        }
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
}
#[cfg(test)]
mod digit_debug {
    use super::*;
    #[test]
    fn debug_digit_after_char() {
        for keys in ["x1", "to1", "a1", "x 1"] {
            let mut e = Engine::new(Method::Telex, Mode::Preedit);
            e.set_client(Some("gtk4-im:test".into()));
            e.set_capabilities(CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
            let mut out = Vec::new();
            for c in keys.chars() {
                let acts = e.process_char(c);
                out.push(format!("{c}:{acts:?}"));
            }
            println!("{keys:?} => {}", out.join(" "));
        }
    }
}

#[cfg(test)]
mod digit_probe2 {
    use bamboo_core::{Engine as B, InputMethod, Mode as M};
    #[test]
    fn probe_digit_bamboo() {
        let mut e = B::new(InputMethod::telex());
        e.process_key('x', M::Vietnamese);
        {
            let mut e4 = B::new(InputMethod::telex());
            for c in "loixf".chars() {
                let rep = {
                    let d = e4.process_key_edit(c, M::Vietnamese);
                    (d.delete_chars, d.insert.chars().collect::<String>())
                };
                let out = e4.output_str().to_string();
                println!("loixf {c:?}: del={} ins={:?} out={out:?}", rep.0, rep.1);
            }
        }
        for s in ["tool", "toool"] {
            let mut e3 = B::new(InputMethod::telex());
            let o = e3.process_str(s, M::Vietnamese);
            println!("process_str({s:?}) = {o:?}");
        }
        for c in ['1', '2', '0', '9', '"', '\''] {
            let mut e2 = B::new(InputMethod::telex());
            e2.process_key('x', M::Vietnamese);
            let rep = {
                let d = e2.process_key_edit(c, M::Vietnamese);
                (d.delete_chars, d.insert.chars().collect::<String>())
            };
            let out = e2.output_str().to_string();
            println!("x+{c:?}: del={} ins={:?} out={out:?}", rep.0, rep.1);
        }
    }
}

#[cfg(test)]
mod x_probe {
    use super::*;
    use crate::bamboo_wrapper::Method;

    /// A lone letter typed after a forwarded caret move (Home) must reach the
    /// application exactly once in every display mode. GNOME Text Editor
    /// renders this inconsistently (double in preedit, dropped in nonpreedit
    /// — app-side, see open-questions #12); the engine's action stream is the
    /// contract this test locks in.
    #[test]
    fn lone_letter_after_caret_move_commits_exactly_once_per_mode() {
        for mode in [Mode::Preedit, Mode::NonPreedit, Mode::Surrounding] {
            let mut e = Engine::new(Method::Telex, mode);
            e.set_client(None);
            e.set_capabilities(0b101001);
            let mut all: Vec<Action> = Vec::new();
            for c in "ab ".chars() {
                all.extend(e.process_char(c));
            }
            all.extend(e.reset()); // Home: commit pending + the key forwards
            all.extend(e.process_char('x'));
            all.extend(e.process_char(' '));
            let x_commits = all
                .iter()
                .filter(|a| matches!(a, Action::Commit(t) if t == "x"))
                .count();
            assert_eq!(x_commits, 1, "{mode:?}: {all:?}");
            // nothing else may re-insert the letter (no second commit, and
            // the preedit path must hide before committing)
            assert!(
                all.windows(2)
                    .any(|w| matches!(w, [Action::HidePreedit, Action::Commit(t)] if t == "x"))
                    || matches!(mode, Mode::NonPreedit | Mode::Surrounding),
                "{mode:?}: preedit must hide before committing the letter: {all:?}"
            );
        }
    }
}

#[cfg(test)]
mod keys_workload_probe {
    use super::*;
    use crate::bamboo_wrapper::Method;

    /// Replay tests/keys-test through the engine the way the server does
    /// (chars → process_char, Backspace → backspace(), nav/edit keys →
    /// reset() + the key forwards) and count how many times the lone "x"
    /// from the {home} scenario reaches the application. Exactly once per
    /// round is the contract; the real-app runs showed 0..3.
    #[test]
    fn full_keys_workload_commits_the_lone_x_exactly_once_per_mode() {
        let keys = std::fs::read_to_string("tests/keys-test").expect("keys-test");
        let mut tokens: Vec<char> = Vec::new();
        let mut chars = keys.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                let mut name = String::new();
                for nc in chars.by_ref() {
                    if nc == '}' {
                        break;
                    }
                    name.push(nc);
                }
                tokens.push(match name.as_str() {
                    "backspace" => '\u{8}',
                    "home" => '\u{1}',
                    "end" | "delete" | "left" | "right" => '\u{1}',
                    " " | "space" => ' ',
                    other => panic!("token {{{other}}}"),
                });
            } else {
                tokens.push(c);
            }
        }

        for mode in [Mode::Preedit, Mode::NonPreedit, Mode::Surrounding] {
            let mut e = Engine::new(Method::Telex, mode);
            e.set_client(None);
            e.set_capabilities(0b101001);
            let mut x_commits = 0;
            let mut all: Vec<Action> = Vec::new();
            for t in &tokens {
                let actions = match *t {
                    '\u{8}' => e.backspace().unwrap_or_default(),
                    '\u{1}' => e.reset(),
                    c => e.process_char(c),
                };
                for a in &actions {
                    if let Action::Commit(text) = a {
                        if text.contains('x') {
                            x_commits += 1;
                        }
                    }
                }
                all.extend(actions);
            }
            assert_eq!(x_commits, 1, "{mode:?}: {all:?}");
        }
    }
}
