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
use crate::config::{AppRule, BackspaceToneConfig, Config, UnicodeChannel};
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
    Flow,
    /// Flow without the pre-rewrite verification.
    Turbo,
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
}

/// What the first navigation key does while a preedit commit is pending on
/// a mutter-relayed context (the experimental `nav_key_while_preedit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavKeyWhilePreedit {
    /// Consume the key: the commit lands while the caret is still in place,
    /// the next press moves the caret. The shipped behavior.
    #[default]
    Swallow,
    /// Send the commit and the key in one reply (ibus-bamboo's sequence):
    /// the caret moves on the first press.
    Forward,
    /// Consume the key and re-emit it as ForwardKeyEvent signals after the
    /// commit; the ordered connection delivers the commit first.
    Replay,
}

impl NavKeyWhilePreedit {
    /// Tolerant parse like the GUI language value: an unknown value keeps
    /// the shipped behavior instead of failing the whole config file.
    pub fn from_config_value(value: &str) -> Self {
        match value {
            "forward" => Self::Forward,
            "replay" => Self::Replay,
            _ => Self::Swallow,
        }
    }
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
    /// Emit CommitText immediately, bypassing the injected-echo deferral —
    /// the per-key batch's parked commit. Its deletes were injected and
    /// their echoes accounted when the batch armed; a normal Commit here
    /// would defer behind that stale count and land ~150 ms late.
    CommitNow(String),
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
    /// Commits parked until a wall-clock deadline. On a mutter relay the
    /// commit channel reaches the client a beat ahead of the key events,
    /// so a rewrite commit must be emitted past the last delete of its
    /// batch even though every echo has already been forwarded; the
    /// server parks them here and the main-loop tick emits them.
    timed_commits: Vec<(Instant, String)>,
    /// The per-key display's single in-flight batch: the parked commit
    /// whose text each new key REPLACES (coalescing). Kept apart from
    /// `timed_commits` so a macro/emoji park from another producer can
    /// never be clobbered by a per-key replace. `None` = no batch in
    /// flight; the next key starts one.
    per_key_park: Option<(Instant, String)>,
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
    /// Experimental: navigation-key behavior while a preedit commit is
    /// pending (the server reads this per keystroke).
    nav_key_while_preedit: NavKeyWhilePreedit,
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
    /// Custom lost-echo stale window (ms); the built-in default applies
    /// while the switch is off. Too small risks flushing the deferred
    /// commit over late echoes (lost characters); too large makes every
    /// word appear late in apps that never route injected keys back.
    echo_stale_enabled: bool,
    echo_stale_ms: u64,
    /// Commit a paused preedit word after `idle_commit_ms` of no keys.
    idle_commit_enabled: bool,
    idle_commit_ms: u64,
    /// The uinput modes show the composed word progressively: every
    /// composition-changing key erases the committed tail and commits the
    /// new one through the injected transport. Mirror of
    /// `config.flow_per_key`, on by default and outside the
    /// experimental gate.
    flow_per_key: bool,
    /// How long the per-key batch stays parked before it lands. Experimental:
    /// apply_config bakes the built-in grace in while the gate is off, so the
    /// shipped behavior cannot drift from the file.
    flow_per_key_delay_ms: u64,
    /// Experimental: a Backspace on a committed uinput word re-tracks the
    /// shortened screen so the next mark key keeps editing. Off keeps the
    /// legacy plain-text behavior byte for byte; apply_config bakes the
    /// flag with the gate.
    flow_backspace_retrack: bool,
    /// How Backspace treats a tone-marked word; pushed into the wrapper per
    /// keystroke because the uinput modes force the unikey undo.
    backspace_tone: BackspaceToneConfig,
    /// When the last key reached the engine — the idle-commit clock. Only
    /// stamped while the feature is on, so the off path costs nothing.
    last_key_at: Option<Instant>,
    /// The word an idle commit just committed: `(raw keys, encoded word)`.
    /// The next word character re-enters the composition (un-commit with a
    /// protocol delete, replay the raw keys) so the word continues as if
    /// the pause never happened. Cleared by any terminator, backspace,
    /// focus loss or state reset.
    idle_committed: Option<(String, String)>,
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
    /// Tail char count when the current raw-forward word armed — the
    /// `url_word`/`emoji_word` span is everything pushed after it. A
    /// Backspace that erases the span releases the bypass (terminators were
    /// the only release before, so deleting a mis-typed word after `:` left
    /// typing raw until space/enter). The per-key tail spans whole words
    /// (`x:abc`), so an empty tail is not the signal — the arm point is.
    raw_word_start: usize,
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
    /// ballpark). One shared value: the key handlers and the main-loop
    /// tick must agree or a lost echo holds the commit for different
    /// lengths of time — both read it through `echo_stale()`.
    pub const DEFAULT_ECHO_STALE_MS: u64 = 150;

    /// How long the server parks a rewrite commit after the last echo of
    /// its batch has been forwarded. Measured on a mutter relay (08/11):
    /// the app applies the forwarded Backspaces a beat behind the commit
    /// channel, and a commit emitted with the last echo overtook the
    /// final deletes (`titiến`). 60 ms clears the observed ~7 ms apply
    /// window with margin while staying imperceptible at word end.
    pub const ECHO_APPLY_GRACE_MS: u64 = 60;

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
            nav_key_while_preedit: NavKeyWhilePreedit::default(),
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
            timed_commits: Vec::new(),
            per_key_park: None,
            word_mode: None,
            url_word: false,
            bypass_url: true,
            extra_url_chars: Vec::new(),
            extra_capitalize_after: Vec::new(),
            emoji_enabled: false,
            emoji_word: false,
            raw_word_start: 0,
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
            echo_stale_enabled: false,
            echo_stale_ms: Self::DEFAULT_ECHO_STALE_MS,
            idle_commit_enabled: false,
            idle_commit_ms: 700,
            flow_per_key: false,
            flow_per_key_delay_ms: Self::ECHO_APPLY_GRACE_MS,
            flow_backspace_retrack: false,
            backspace_tone: BackspaceToneConfig::ToneFirst,
            last_key_at: None,
            idle_committed: None,
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
        // The experimental master gate: with it off every experimental
        // feature is forced off here, no matter what its own switch says —
        // the switches and values stay saved for when the gate reopens.
        let experimental = config.experimental_features;
        self.auto_restore = config.auto_restore;
        self.em_dash = config.em_dash;
        self.auto_capitalize = config.auto_capitalize;
        self.extra_capitalize_after = if experimental && config.auto_capitalize_after_enabled {
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
        self.macro_expand_valid_words = experimental && config.macro_expand_valid_words;
        self.x11_delete_without_report = config.x11_delete_without_report;
        self.preedit_for_xim = config.preedit_for_xim;
        // Experimental: the gate forces the shipped swallow back on while
        // the chosen value stays saved for when the gate reopens.
        self.nav_key_while_preedit = if experimental {
            NavKeyWhilePreedit::from_config_value(&config.nav_key_while_preedit)
        } else {
            NavKeyWhilePreedit::Swallow
        };
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
        self.clipboard_restore_delay_enabled =
            experimental && config.clipboard_restore_delay_enabled;
        self.clipboard_restore_delay_ms = config.clipboard_restore_delay_ms;
        self.echo_stale_enabled = experimental && config.echo_stale_enabled;
        self.echo_stale_ms = config.echo_stale_ms;
        self.idle_commit_enabled = experimental && config.idle_commit_enabled;
        self.idle_commit_ms = config.idle_commit_ms;
        // Not part of the experimental set anymore: the gate must not force
        // it off or the always-visible switch would silently do nothing.
        self.flow_per_key = config.flow_per_key;
        // The delay knob is experimental: gate off applies the built-in 60 ms
        // whatever the file says; the value stays saved for when the gate reopens.
        self.flow_per_key_delay_ms = if experimental {
            config.flow_per_key_delay_ms
        } else {
            Self::ECHO_APPLY_GRACE_MS
        };
        self.flow_backspace_retrack = experimental && config.flow_backspace_retrack;
        self.backspace_tone = config.backspace_tone;
        if !self.idle_commit_enabled {
            self.idle_committed = None;
        }
        self.bypass_url = config.bypass_url;
        self.extra_url_chars = if experimental && config.extra_url_chars_enabled {
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
        self.raw_word_start = 0;
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
            self.idle_committed = None;
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
            self.idle_committed = None;
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

    /// Navigation-key behavior while a preedit commit is pending.
    pub fn nav_key_while_preedit(&self) -> NavKeyWhilePreedit {
        self.nav_key_while_preedit
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

    /// The lost-echo stale window: the custom value when the switch is on,
    /// else the built-in default. The key handlers and the main-loop tick
    /// must read the SAME value or a lost echo holds the commit for
    /// different lengths of time — both go through this getter.
    pub fn echo_stale(&self) -> Duration {
        Duration::from_millis(if self.echo_stale_enabled {
            self.echo_stale_ms
        } else {
            Self::DEFAULT_ECHO_STALE_MS
        })
    }

    /// The per-key display's park delay: the user value clamped to a
    /// sane ceiling. Zero is allowed by owner decision — the knob is
    /// experimental and gate-hidden, and a too-low value can only reorder
    /// text for the user who set it; the two server-side rewrite parks
    /// stay on the built-in constant and are not tunable.
    pub fn per_key_delay(&self) -> Duration {
        Duration::from_millis(self.flow_per_key_delay_ms.clamp(0, 500))
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
        // A forced flush ignores the deadline: the context is going away
        // and nobody will tick for it — delivering late beats never.
        all.extend(self.timed_commits.drain(..).map(|(_, text)| {
            debug_log::log(&format!(
                "flush deferred commit (timed, forced) len={}",
                text.chars().count()
            ));
            text
        }));
        if let Some((_, text)) = self.per_key_park.take() {
            debug_log::log(&format!(
                "flush deferred commit (per-key, forced) len={}",
                text.chars().count()
            ));
            all.push(text);
        }
        all
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
            || !self.nav_deferred.is_empty()
            || !self.timed_commits.is_empty()
            || self.per_key_park.is_some()
    }

    /// Hold `text` back for `delay` before it may be emitted: the relay
    /// applies forwarded keys a beat behind the commit channel, so the
    /// rewrite commit waits past the deletes it depends on.
    pub fn park_commit(&mut self, text: String, delay: Duration) {
        debug_log::log(&format!(
            "park commit for {}ms len={}",
            delay.as_millis(),
            text.chars().count()
        ));
        self.timed_commits.push((Instant::now() + delay, text));
    }

    /// Parked commits whose deadline has passed, in park order.
    fn take_due_commits(&mut self) -> Vec<String> {
        let now = Instant::now();
        let (due, later): (Vec<_>, Vec<_>) = self
            .timed_commits
            .drain(..)
            .partition(|(deadline, _)| *deadline <= now);
        self.timed_commits = later;
        due.into_iter().map(|(_, text)| text).collect()
    }

    /// The per-key batch's parked commit, past its deadline. Emitted as
    /// `CommitNow`: the deferral counter counts this batch's own deletes,
    /// whose echoes have long been forwarded.
    fn take_due_per_key_commit(&mut self) -> Option<Action> {
        let now = Instant::now();
        match &self.per_key_park {
            Some((deadline, _)) if *deadline <= now => {
                let (_, text) = self.per_key_park.take().unwrap();
                Some(Action::CommitNow(text))
            }
            _ => None,
        }
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
        // The uinput modes press real keys through /dev/uinput. On
        // mutter-relayed contexts they were once believed dead (the 31/10
        // session) and fell back to preedit; the 08/11 re-measurement
        // refuted that — injection, echoes, deletes and commits all land
        // on the relay. What actually raced was the ORDER between the
        // commit channel and the key events; the server parks the rewrite
        // commit past the last injected key (ECHO_APPLY_GRACE_MS) and the
        // rewrite batch carries its own terminator key, so the relay runs
        // these modes. Without the virtual device, or Select without a
        // surrounding-text report (it sizes the selection from it),
        // preedit still stands in.
        if matches!(requested, Mode::Flow | Mode::Turbo | Mode::Select)
            && (!self.uinput_available || (requested == Mode::Select && self.surrounding.is_none()))
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
        // A Backspace that erased the whole raw-forward span kills the
        // bypass: terminators were the only release, so deleting a word
        // typed after `:` left every following letter raw until space/enter.
        // A partial delete keeps it — the word is still on screen.
        if (self.url_word || self.emoji_word) && self.tail.chars().count() <= self.raw_word_start {
            self.url_word = false;
            self.emoji_word = false;
            self.raw_word_start = 0;
        }
    }

    /// Backspace over the composition. Returns `None` when there is nothing
    /// composed, so the application handles the key itself.
    pub fn backspace(&mut self) -> Option<Vec<Action>> {
        // The idle-commit clock, like process_char.
        if self.idle_commit_enabled {
            self.last_key_at = Some(Instant::now());
        }
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
        // The tone-skip belongs to committed-text transports: the uinput
        // modes show the raw keys, and a skipped tone would desync the
        // screen from the wrapper. They keep the unikey undo.
        self.wrapper.set_backspace_tone(
            if matches!(mode, Mode::Flow | Mode::Turbo | Mode::Select) {
                BackspaceToneConfig::UnikeyWalk
            } else {
                self.backspace_tone
            },
        );
        // uinput modes: the application owns the on-screen text — the
        // Backspace is forwarded so it deletes the raw key shown there, and
        // bamboo follows so the composition stays aligned (unikey
        // convention: Backspace undoes the last raw key).
        if matches!(mode, Mode::Flow | Mode::Turbo | Mode::Select) {
            // Per-key display: the app holds the composed word, so the
            // Backspace passes through with a reverse-mapped raw buffer —
            // the user's real key, the one transport the relay never drops.
            if self.flow_per_key && !matches!(mode, Mode::Select) {
                return Some(self.backspace_per_key());
            }
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
            // The user is erasing an idle-committed word — re-entering it
            // would resurrect text they are deleting.
            self.idle_committed = None;
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
            // Off is gated above. Keeps the match exhaustive.
            Mode::Off => None,
            Mode::Flow | Mode::Turbo | Mode::Select => None,
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
        self.raw_word_start = 0;
        self.idle_committed = None;
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
    /// With `idle_commit` on, a preedit word left alone past the configured
    /// pause commits here too, through the normal word-end path (macros,
    /// auto-restore, capitalization — exactly like a terminator); the next
    /// word character can re-enter the composition via `idle_committed`.
    pub fn tick(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        // Parked commits past their deadline become deliverable here —
        // the main loop woke for exactly this deadline (ms_until_tick).
        for text in self.take_due_commits() {
            debug_log::log(&format!("timed commit flush len={}", text.chars().count()));
            actions.push(Action::Commit(text));
        }
        if let Some(action) = self.take_due_per_key_commit() {
            debug_log::log("timed commit flush (per-key)");
            actions.push(action);
        }
        if self.take_stale_injection(self.echo_stale()) {
            for text in self.flush_deferred() {
                actions.push(Action::Commit(text));
            }
        }
        // The floor keeps a hand-edited 0 from committing between every
        // keystroke; the GUI range starts at 300 ms.
        let idle_ms = self.idle_commit_ms.max(50);
        if self.idle_commit_enabled
            && self.preedit_commit_pending()
            && self
                .last_key_at
                .is_some_and(|t| t.elapsed().as_millis() as u64 >= idle_ms)
        {
            let pending = self.wrapper.output().to_string();
            let raw = self.wrapper.raw().to_string();
            let valid = self.wrapper.is_valid();
            let valid_prefix = self.wrapper.is_valid_prefix();
            let mut finished = self.on_finish(
                Mode::Preedit,
                pending.clone(),
                raw.clone(),
                valid,
                valid_prefix,
            );
            finished.retain(|a| !matches!(a, Action::Forward));
            self.wrapper.reset();
            self.tail.clear();
            self.repair_delete = None;
            self.release_word_mode();
            self.idle_committed = Some((raw, encode(self.charset, &pending)));
            debug_log::log(&format!("idle commit len={}", pending.chars().count()));
            actions.extend(finished);
        }
        actions
    }

    /// Milliseconds the main loop should wait before the next `tick` — the
    /// lost-echo stale window, the idle-commit deadline, or None when
    /// nothing is pending (the loop falls back to its 1 s poll).
    pub fn ms_until_tick(&self) -> Option<u64> {
        // Both echo kinds share the stale window; the earliest deadline wins.
        let stale_ms = self.echo_stale().as_millis();
        let mut deadlines: Vec<u64> = [
            (self.pending_injected > 0)
                .then_some(self.last_inject)
                .flatten(),
            (!self.pending_nav.is_empty())
                .then_some(self.last_nav_inject)
                .flatten(),
        ]
        .into_iter()
        .flatten()
        .map(|t| stale_ms as u64 - t.elapsed().as_millis().min(stale_ms) as u64)
        .collect();
        // The idle-commit deadline: a paused preedit word must wake the loop.
        if self.idle_commit_enabled && self.preedit_visible && !self.wrapper.output().is_empty() {
            if let Some(t) = self.last_key_at {
                let idle_ms = self.idle_commit_ms.max(50) as u128;
                deadlines.push(idle_ms.saturating_sub(t.elapsed().as_millis()).max(1) as u64);
            }
        }
        // The earliest parked-commit deadline must wake the loop for its
        // timed flush.
        if let Some((deadline, _)) = self.timed_commits.iter().min_by_key(|(d, _)| *d) {
            deadlines.push(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64,
            );
        }
        if let Some((deadline, _)) = &self.per_key_park {
            deadlines.push(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64,
            );
        }
        deadlines.into_iter().min().map(|ms| ms.max(1))
    }

    fn hide_if_visible(&mut self) -> Vec<Action> {
        if std::mem::take(&mut self.preedit_visible) {
            vec![Action::HidePreedit]
        } else {
            Vec::new()
        }
    }
}
